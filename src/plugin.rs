//! The plugin itself: voices, background instrument loading, and the audio process cycle.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use nice_plug::prelude::*;
use nice_plug_egui::{
    EguiEditor, EguiEditorState, EguiNiceSettings, RepaintNotifier, create_egui_editor,
};

use crate::editor::{OgziEditor, WINDOW_SIZE};
use crate::instrument::{LoadedInstrument, load_instrument};
use crate::library::Library;
use crate::params::OgziParams;
use crate::voice::Voice;

/// The number of simultaneous voices.
const NUM_VOICES: usize = 16;

/// Background tasks this plugin can run.
pub enum Task {
    /// Decode all samples of the given instrument.
    LoadInstrument { group: usize, instrument: usize },
}

/// A (group, instrument) index pair.
type Selection = (usize, usize);

/// The result a background task delivers back to the audio thread.
type Delivery = (Selection, Arc<LoadedInstrument>);

pub struct Ogzi {
    params: Arc<OgziParams>,
    lib: Arc<Library>,

    /// The instrument that has finished loading, ready to be played.
    loaded: Option<Arc<LoadedInstrument>>,
    /// Which selection `loaded` corresponds to. `(usize::MAX, usize::MAX)` means nothing has
    /// been loaded yet.
    loaded_selection: Selection,
    /// The selection currently being loaded in the background, if any.
    loading_selection: Option<Selection>,

    /// Where the background task leaves its result. Shared with the task executor.
    deliveries: Arc<Mutex<Option<Delivery>>>,
    /// Whether a selection is currently loading. Shared with the GUI.
    loading: Arc<AtomicBool>,
    /// The group currently selected, as seen when formatting parameter values for the host.
    current_group: Arc<AtomicI32>,

    repaint_notifier: RepaintNotifier,
    editor_state: Arc<EguiEditorState>,
    /// Held until the editor is first created.
    initial_editor: Option<OgziEditor>,

    sample_rate: f32,
    voices: Vec<Voice>,
    /// Round-robin voice stealing.
    next_voice: usize,
}

impl Default for Ogzi {
    fn default() -> Self {
        let lib = Arc::new(Library::load());

        let current_group = Arc::new(AtomicI32::new(0));
        let params = Arc::new(OgziParams::new(lib.clone(), current_group.clone()));

        let loading = Arc::new(AtomicBool::new(false));
        let editor = OgziEditor::new(params.clone(), lib.clone(), loading.clone());

        Self {
            params,
            lib,
            loaded: None,
            loaded_selection: (usize::MAX, usize::MAX),
            loading_selection: None,
            deliveries: Arc::new(Mutex::new(None)),
            loading,
            current_group,
            repaint_notifier: RepaintNotifier::new(),
            editor_state: EguiEditorState::from_size(WINDOW_SIZE, 1.0),
            initial_editor: Some(editor),
            sample_rate: 1.0,
            voices: (0..NUM_VOICES).map(|_| Voice::inactive()).collect(),
            next_voice: 0,
        }
    }
}

impl Ogzi {
    /// The currently selected (group, instrument), clamped to the library's contents.
    fn selection(&self) -> Option<Selection> {
        let group_idx = self.params.group.value() as usize;
        let group = self.lib.group(group_idx)?;
        let instrument_idx = (self.params.instrument.value() as usize)
            .min(group.instruments.len().saturating_sub(1));
        Some((group_idx, instrument_idx))
    }

    fn note_on(&mut self, note: u8, velocity: u8, channel: u8) {
        let velocity = velocity.max(1);
        let Some(sample) = self
            .loaded
            .as_ref()
            .and_then(|inst| inst.pick(note, velocity))
            .cloned()
        else {
            if self.loaded.is_some() {
                nice_log!("Ogzi: no sample for note {note} at velocity {velocity}");
            }
            return;
        };

        // Retriggering the same note releases the old voice first so looped samples don't stack
        // up forever.
        for voice in &mut self.voices {
            if voice.active && voice.note == note && voice.channel == channel {
                voice.start_release(self.sample_rate);
            }
        }

        let free_slot = self.voices.iter().position(|v| !v.active);
        let slot = match free_slot {
            Some(slot) => slot,
            None => {
                let slot = self.next_voice;
                self.next_voice = (self.next_voice + 1) % NUM_VOICES;
                slot
            }
        };
        self.voices[slot].start(sample, note, channel, self.sample_rate, velocity);
    }

    fn note_off(&mut self, note: u8, channel: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note && voice.channel == channel && voice.is_looping()
            {
                // One-shot samples play out in full, looped samples follow the note off.
                voice.start_release(self.sample_rate);
            }
        }
    }

    fn choke(&mut self, note: u8, channel: u8) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note && voice.channel == channel {
                voice.start_release(self.sample_rate);
            }
        }
    }

    /// Take the instrument delivered by a background task, if it isn't stale.
    fn pick_up_delivery(&mut self) {
        let Some(delivery) = self.deliveries.lock().unwrap().take() else {
            return;
        };
        let (selection, instrument) = delivery;
        if Some(selection) == self.loading_selection || Some(selection) == self.selection() {
            self.loaded = Some(instrument);
            self.loaded_selection = selection;
            self.loading_selection = None;
            self.loading.store(false, Ordering::Relaxed);
        }
    }

    /// Post a background task for the current selection if it hasn't been loaded yet.
    fn request_selection(&mut self, context: &mut impl ProcessContext<Self>) {
        let Some(selection) = self.selection() else {
            return;
        };
        if selection != self.loaded_selection && self.loading_selection != Some(selection) {
            self.loading_selection = Some(selection);
            self.loading.store(true, Ordering::Relaxed);
            context.execute_background(Task::LoadInstrument {
                group: selection.0,
                instrument: selection.1,
            });
        }
    }
}

impl Plugin for Ogzi {
    const NAME: &'static str = "Ogzi";
    const VENDOR: &'static str = "ogzi-music";
    const URL: &'static str = "https://github.com/ogzi-music/lib";
    const EMAIL: &'static str = "";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;

    type Editor = EguiEditor<OgziEditor>;
    type SysExMessage = ();
    type BackgroundTask = Task;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let lib = self.lib.clone();
        let deliveries = self.deliveries.clone();
        let loading = self.loading.clone();
        let repaint_notifier = self.repaint_notifier.clone();

        Box::new(move |task| match task {
            Task::LoadInstrument { group, instrument } => {
                let loaded = load_instrument(&lib, group, instrument).unwrap_or_else(|e| {
                    nice_error!("Ogzi: failed to load instrument: {e}");
                    LoadedInstrument {
                        samples: Vec::new(),
                    }
                });
                *deliveries.lock().unwrap() = Some(((group, instrument), Arc::new(loaded)));
                loading.store(false, Ordering::Relaxed);
                repaint_notifier.request_repaint();
            }
        })
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_egui_editor(
            self.editor_state.clone(),
            self.repaint_notifier.clone(),
            EguiNiceSettings::new()
                .with_tile("Ogzi")
                .with_resize_hint(ResizeHint::NON_RESIZABLE),
            self.initial_editor.take()?,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        true
    }

    fn reset(&mut self) {
        for voice in &mut self.voices {
            *voice = Voice::inactive();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.pick_up_delivery();
        self.request_selection(context);

        // Keep the group used to format the instrument parameter's value in sync.
        self.current_group
            .store(self.params.group.value(), Ordering::Relaxed);

        let mut next_event = context.next_event();
        for (sample_idx, mut channel_samples) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {
                if event.timing() > sample_idx as u32 {
                    break;
                }

                match event {
                    NoteEvent::NoteOn {
                        key,
                        velocity,
                        channel,
                        ..
                    } => {
                        let note = key.number_or_middle_c();
                        let velocity = (velocity.clamp(0.0, 1.0) * 127.0).round() as u8;
                        let channel = channel.number().unwrap_or(0);
                        self.note_on(note, velocity, channel);
                    }
                    NoteEvent::NoteOff { key, channel, .. } => {
                        let note = key.number_or_middle_c();
                        let channel = channel.number().unwrap_or(0);
                        self.note_off(note, channel);
                    }
                    NoteEvent::Choke { key, channel, .. } => {
                        let note = key.number_or_middle_c();
                        let channel = channel.number().unwrap_or(0);
                        self.choke(note, channel);
                    }
                    _ => (),
                }

                next_event = context.next_event();
            }

            for voice in &mut self.voices {
                if voice.active {
                    voice.render(&mut channel_samples);
                }
            }
        }

        // Report the remaining tail so hosts won't cut off ringing samples.
        let tail = Voice::max_remaining(&self.voices);
        if tail.is_infinite() {
            ProcessStatus::KeepAlive
        } else if tail > 0.0 {
            ProcessStatus::Tail(tail as u32 + 1)
        } else {
            ProcessStatus::Normal
        }
    }
}

impl ClapPlugin for Ogzi {
    const CLAP_ID: &'static str = "music.ogzi.ogzi-sampler";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A sampler for the Ogzi sample library");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_plays_an_instrument() {
        let lib = Library::load();
        let group = lib
            .groups
            .iter()
            .position(|g| g.slug == "brass")
            .expect("brass group");
        let instrument = lib.groups[group]
            .instruments
            .iter()
            .position(|i| i.slug == "trumpet-jazz")
            .expect("trumpet-jazz");

        let loaded = load_instrument(&lib, group, instrument).expect("instrument loads");
        assert!(!loaded.samples.is_empty());

        let sample_rate = loaded
            .pick(60, 100)
            .expect("sample for middle C")
            .sample_rate;

        // Render a short note through a real buffer and check that we actually get sound.
        let mut ogzi = Ogzi::default();
        ogzi.sample_rate = sample_rate;
        ogzi.loaded = Some(Arc::new(loaded));
        ogzi.note_on(60, 100, 0);

        let mut real_buffers = vec![vec![0.0_f32; 1000]; 2];
        let mut buffer = Buffer::default();
        unsafe {
            buffer.set_slices(1000, |slices| {
                let (first, rest) = real_buffers.split_at_mut(1);
                *slices = vec![&mut first[0], &mut rest[0]];
            });
        }
        for mut channel_samples in buffer.iter_samples() {
            for voice in &mut ogzi.voices {
                if voice.active {
                    voice.render(&mut channel_samples);
                }
            }
        }
        assert!(
            real_buffers[0].iter().any(|s| s.abs() > 1e-6),
            "voice produced silence"
        );
    }
}
