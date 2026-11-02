//! Loading instruments: the per-instrument TOML metadata and the FLAC samples it describes.

use std::path::Path;
use std::sync::Arc;

use nice_plug::prelude::*;
use serde::Deserialize;

use crate::library::Library;

/// The per-instrument TOML file, e.g. `lib/brass/trumpet-jazz.toml`.
#[derive(Deserialize)]
struct InstrumentToml {
    #[serde(default)]
    samples: Vec<SampleToml>,
}

/// A single `[[samples]]` entry. See the library's README for the full field reference.
#[derive(Deserialize)]
struct SampleToml {
    file: String,
    key: u8,
    #[serde(default)]
    key_from: Option<u8>,
    #[serde(default)]
    key_to: Option<u8>,
    #[serde(default)]
    velocity_from: Option<u8>,
    #[serde(default)]
    velocity_to: Option<u8>,
    #[serde(default = "default_level")]
    level: u8,
    #[serde(default = "default_center")]
    pan: u8,
    #[serde(default = "default_center")]
    tune_coarse: u8,
    #[serde(default = "default_center")]
    tune_fine: u8,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    start: Option<usize>,
    #[serde(default)]
    end: Option<usize>,
    /// The loop start point, only used in loop mode.
    #[serde(rename = "loop")]
    loop_start: Option<usize>,
}

fn default_level() -> u8 {
    127
}

fn default_center() -> u8 {
    64
}

fn default_mode() -> String {
    "single-shot".to_string()
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PlayMode {
    SingleShot,
    Loop,
    Reverse,
}

/// A single decoded sample, ready for playback.
pub(crate) struct LoadedSample {
    /// Interleaved sample data.
    pub(crate) data: Vec<f32>,
    pub(crate) channels: usize,
    pub(crate) sample_rate: f32,
    /// The key this sample was recorded at.
    pub(crate) key: u8,
    /// The inclusive range of keys covered by this sample.
    pub(crate) key_from: u8,
    pub(crate) key_to: u8,
    /// The inclusive range of velocities covered by this sample.
    pub(crate) vel_from: u8,
    pub(crate) vel_to: u8,
    /// The sample's playback gain, from `level`.
    pub(crate) gain: f32,
    /// Constant power panning gains for the left and right output channels, from `pan`.
    pub(crate) pan_l: f32,
    pub(crate) pan_r: f32,
    /// The sample's tuning in semitones, from `tune-coarse` and `tune-fine`.
    pub(crate) tune: f32,
    pub(crate) mode: PlayMode,
    pub(crate) start: usize,
    /// The exclusive end of the playback range, in frames.
    pub(crate) end: usize,
    /// The loop start point, in loop mode.
    pub(crate) loop_start: Option<usize>,
}

/// A fully loaded instrument: all of its samples decoded into memory.
pub(crate) struct LoadedInstrument {
    pub(crate) samples: Vec<Arc<LoadedSample>>,
}

impl LoadedInstrument {
    /// Pick the sample covering the given note and velocity.
    pub(crate) fn pick(&self, note: u8, velocity: u8) -> Option<&Arc<LoadedSample>> {
        self.samples.iter().find(|s| {
            note >= s.key_from && note <= s.key_to && velocity >= s.vel_from && velocity <= s.vel_to
        })
    }
}

/// Decode an instrument's TOML metadata and all of its FLAC samples. Samples that fail to
/// decode are skipped so one broken file doesn't take out the whole instrument.
pub(crate) fn load_instrument(
    lib: &Library,
    group: usize,
    instrument: usize,
) -> Result<LoadedInstrument, String> {
    let group = lib
        .group(group)
        .ok_or_else(|| "group index out of range".to_string())?;
    let inst = group
        .instruments
        .get(instrument)
        .ok_or_else(|| "instrument index out of range".to_string())?;

    let dir = lib.root.join(&group.slug).join(&inst.slug);
    let meta_path = lib
        .root
        .join(&group.slug)
        .join(format!("{}.toml", inst.slug));
    let meta_raw =
        std::fs::read_to_string(&meta_path).map_err(|e| format!("{}: {e}", meta_path.display()))?;
    let meta: InstrumentToml =
        toml::from_str(&meta_raw).map_err(|e| format!("{}: {e}", meta_path.display()))?;

    let mut samples = Vec::new();
    for s in meta.samples {
        let path = dir.join(&s.file);
        match load_sample(&path, s) {
            Ok(sample) => samples.push(Arc::new(sample)),
            Err(e) => nice_error!("Ogzi: skipping sample {}: {e}", path.display()),
        }
    }

    Ok(LoadedInstrument { samples })
}

/// Decode a single FLAC file described by its metadata entry.
fn load_sample(path: &Path, s: SampleToml) -> Result<LoadedSample, String> {
    let mut reader =
        claxon::FlacReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    if channels == 0 || channels > 2 {
        return Err(format!("unsupported channel count {channels}"));
    }
    let norm = (1u64 << (info.bits_per_sample - 1)) as f32;
    let mut data = Vec::with_capacity(info.samples.unwrap_or(0) as usize * channels);
    for sample in reader.samples() {
        let sample = sample.map_err(|e| e.to_string())?;
        data.push(sample as f32 / norm);
    }

    let frames = data.len() / channels;
    let start = s.start.unwrap_or(0).min(frames.saturating_sub(1));
    // The metadata's end point is inclusive, ours is exclusive.
    let end = s.end.map(|e| e + 1).unwrap_or(frames).min(frames);
    let mode = match s.mode.as_str() {
        "loop" => PlayMode::Loop,
        "reverse" => PlayMode::Reverse,
        _ => PlayMode::SingleShot,
    };
    if end <= start {
        return Err("empty playback range".to_string());
    }

    // Constant power panning, `64` being the center.
    let pan = ((s.pan as f32 - 64.0) / 63.0).clamp(-1.0, 1.0);
    let pan_angle = (pan + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    let tune = (s.tune_coarse as f32 - 64.0) + (s.tune_fine as f32 - 64.0) / 100.0;
    let key_from = s.key_from.unwrap_or(s.key);
    let key_to = s.key_to.unwrap_or(s.key);

    Ok(LoadedSample {
        data,
        channels,
        sample_rate: info.sample_rate as f32,
        key: s.key,
        key_from: key_from.min(key_to),
        key_to: key_to.max(key_from),
        vel_from: s.velocity_from.unwrap_or(1),
        vel_to: s.velocity_to.unwrap_or(127),
        gain: s.level as f32 / 127.0,
        pan_l: pan_angle.cos(),
        pan_r: pan_angle.sin(),
        tune,
        mode,
        start,
        end,
        loop_start: s.loop_start.filter(|l| *l < end),
    })
}
