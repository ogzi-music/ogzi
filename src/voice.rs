//! Voices: playing instances of loaded samples.

use nice_plug::buffer::ChannelSamples;
use std::sync::Arc;

use crate::instrument::{LoadedSample, PlayMode};

/// How long the release fade after a note off lasts, in seconds.
const RELEASE_SEC: f32 = 0.03;

pub(crate) struct Voice {
    pub(crate) active: bool,
    pub(crate) sample: Option<Arc<LoadedSample>>,
    pub(crate) note: u8,
    pub(crate) channel: u8,
    /// The playback position, in sample frames.
    pos: f64,
    /// The playback rate, in sample frames per output sample.
    rate: f64,
    /// The velocity and level gain.
    gain: f32,
    /// The current release gain multiplier, `1.0` while the note is held.
    release_gain: f32,
    /// The per-sample decrement of `release_gain` while releasing.
    release_step: Option<f32>,
}

impl Voice {
    pub(crate) fn inactive() -> Self {
        Self {
            active: false,
            sample: None,
            note: 0,
            channel: 0,
            pos: 0.0,
            rate: 1.0,
            gain: 0.0,
            release_gain: 1.0,
            release_step: None,
        }
    }

    /// Start a new note on this voice.
    pub(crate) fn start(
        &mut self,
        sample: Arc<LoadedSample>,
        note: u8,
        channel: u8,
        sample_rate: f32,
        velocity: u8,
    ) {
        *self = Voice {
            active: true,
            sample: Some(sample.clone()),
            note,
            channel,
            pos: match sample.mode {
                PlayMode::Reverse => sample.end as f64,
                _ => sample.start as f64,
            },
            // Pitch shift relative to the sample's key, plus the sample's tuning and the
            // difference between the sample's and the host's sample rates.
            rate: 2f64.powf((note as f64 - sample.key as f64 + sample.tune as f64) / 12.0)
                * (sample.sample_rate as f64 / sample_rate as f64),
            gain: velocity as f32 / 127.0 * sample.gain,
            release_gain: 1.0,
            release_step: None,
        };
    }

    /// Begin the release fade, if the voice isn't already releasing.
    pub(crate) fn start_release(&mut self, sample_rate: f32) {
        if self.active && self.release_step.is_none() {
            self.release_step = Some(1.0 / (RELEASE_SEC * sample_rate).max(1.0));
        }
    }

    /// Whether this voice plays a sample that loops until the note is released.
    pub(crate) fn is_looping(&self) -> bool {
        self.sample
            .as_deref()
            .is_some_and(|s| s.mode == PlayMode::Loop)
    }

    /// The number of output samples left before this voice goes silent, used for the tail.
    fn remaining(&self) -> f64 {
        let Some(s) = self.sample.as_deref() else {
            return 0.0;
        };
        match self.release_step {
            Some(step) => (self.release_gain / step) as f64,
            None => match s.mode {
                PlayMode::SingleShot => ((s.end as f64 - self.pos) / self.rate).max(0.0),
                PlayMode::Reverse => ((self.pos - s.start as f64) / self.rate).max(0.0),
                PlayMode::Loop => f64::INFINITY,
            },
        }
    }

    /// The longest any active voice will still be audible.
    pub(crate) fn max_remaining(voices: &[Voice]) -> f64 {
        voices
            .iter()
            .filter(|v| v.active)
            .map(|v| v.remaining())
            .fold(0.0_f64, f64::max)
    }

    /// Render one output sample frame into `out`, one element per output channel.
    pub(crate) fn render(&mut self, out: &mut ChannelSamples<'_, '_>) {
        let Some(s) = self.sample.as_deref() else {
            self.active = false;
            return;
        };

        match s.mode {
            PlayMode::Reverse => {
                if self.pos <= s.start as f64 {
                    self.active = false;
                    return;
                }
                self.pos -= self.rate;
                let frame = (self.pos as usize).max(s.start);
                self.mix(s, frame * s.channels, 0.0, out);
            }
            _ => {
                if self.pos >= s.end as f64 {
                    match s.loop_start {
                        // Loop back to the loop start point, wrapping the remainder.
                        Some(loop_start) if loop_start < s.end => {
                            while self.pos >= s.end as f64 {
                                self.pos -= (s.end - loop_start) as f64;
                            }
                        }
                        _ => {
                            self.active = false;
                            return;
                        }
                    }
                }
                let frame = self.pos as usize;
                let frac = (self.pos - frame as f64) as f32;
                self.mix(s, frame * s.channels, frac, out);
                self.pos += self.rate;
            }
        }

        // The release fade after a note off.
        if let Some(step) = self.release_step {
            self.release_gain -= step;
            if self.release_gain <= 0.0 {
                self.active = false;
            }
        }
    }

    /// Mix one interpolated frame of `s` starting at `base` into `out`.
    fn mix(&self, s: &LoadedSample, base: usize, frac: f32, out: &mut ChannelSamples<'_, '_>) {
        let gain = self.gain * self.release_gain;
        for (out_ch, out_sample) in out.iter_mut().enumerate() {
            // Mono samples feed every output channel; panning always follows the output channel.
            let src_ch = out_ch.min(s.channels - 1);
            let a = s.data[base + src_ch];
            let b = s.data[(base + s.channels + src_ch).min(s.data.len() - 1)];
            let value = a + (b - a) * frac;
            let pan = if out_ch == 0 { s.pan_l } else { s.pan_r };
            *out_sample += value * gain * pan;
        }
    }
}
