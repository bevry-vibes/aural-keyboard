//! The real-time voice-pool mixer (DESIGN.md §5, decision D6).
//!
//! Lives entirely inside the audio callback: no allocation, no locks, no I/O.
//! Triggers arrive over a crossbeam channel from the keyboard-hook thread and are
//! drained lazily at frame boundaries. Voices: velocity gain + 0.5 s release ramp,
//! replicating aural-coding's Web Audio semantics (`noteOn`/`noteOff`).

use crate::assets::SampleBank;
use crate::mapping::Instrument;
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const MAX_VOICES: usize = 32;
/// The original noteOff: linear ramp to 0 over 0.5 s, stop at 0.6 s.
pub const RELEASE_SECONDS: f32 = 0.5;

/// A keyboard event translated into a musical command.
#[derive(Debug, Clone, Copy)]
pub enum Trigger {
    NoteOn {
        key: u8,
        instrument: Instrument,
        midi: u8,
        velocity: f32,
        at: Instant,
    },
    NoteOff {
        key: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VoiceState {
    Free,
    Playing,
    Releasing,
}

#[derive(Clone, Copy)]
struct Voice {
    state: VoiceState,
    key: u8,
    sample: &'static [f32],
    pos: usize,
    gain: f32,
    release_step: f32,
    age: u64,
}

impl Voice {
    const fn free() -> Self {
        Voice {
            state: VoiceState::Free,
            key: 0,
            sample: &[],
            pos: 0,
            gain: 0.0,
            release_step: 0.0,
            age: 0,
        }
    }
}

/// Flags shared between the control plane (CLI/config/hotkey) and the render thread.
pub struct SharedFlags {
    pub muted: AtomicBool,
    pub volume_bits: AtomicU32,
    pub frames_rendered: AtomicU64,
}

impl SharedFlags {
    pub fn new(volume: f32, muted: bool) -> Self {
        SharedFlags {
            muted: AtomicBool::new(muted),
            volume_bits: AtomicU32::new(volume.to_bits()),
            frames_rendered: AtomicU64::new(0),
        }
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }
}

pub struct AuralMixer {
    rx: Receiver<Trigger>,
    bank: &'static SampleBank,
    flags: Arc<SharedFlags>,
    sample_rate: u32,
    channels: usize, // device channel count; we render to L/R, silence elsewhere
    voices: [Voice; MAX_VOICES],
    clock: u64,
    bench_tx: Option<Sender<u64>>, // hook→voice-start latency, nanoseconds (bench mode)
}

impl AuralMixer {
    pub fn new(
        rx: Receiver<Trigger>,
        bank: &'static SampleBank,
        flags: Arc<SharedFlags>,
        sample_rate: u32,
        channels: usize,
        bench_tx: Option<Sender<u64>>,
    ) -> Self {
        AuralMixer {
            rx,
            bank,
            flags,
            sample_rate,
            channels: channels.max(1),
            voices: [Voice::free(); MAX_VOICES],
            clock: 0,
            bench_tx,
        }
    }

    /// A fresh mixer over the same inputs (all voices Free). Used when (re)building
    /// the output stream — e.g. buffer-size retry or device-loss recovery.
    pub fn clone_fresh(&self) -> Self {
        AuralMixer::new(
            self.rx.clone(),
            self.bank,
            self.flags.clone(),
            self.sample_rate,
            self.channels,
            self.bench_tx.clone(),
        )
    }
}

impl AuralMixer {
    /// cpal output callback body: fill one interleaved multi-channel buffer.
    /// The mix goes to channels 0/1; any extra device channels stay silent.
    pub fn fill(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let muted = self.flags.muted.load(Ordering::Relaxed);
        let volume = self.flags.volume();
        let stride = self.channels;
        let frames = out.len() / stride;
        for frame in 0..frames {
            if frame & 0x3F == 0 {
                self.drain_triggers(muted);
            }
            let mut acc = 0.0f32;
            for v in &mut self.voices {
                if v.state == VoiceState::Free {
                    continue;
                }
                if v.pos < v.sample.len() {
                    acc += v.sample[v.pos] * v.gain;
                    v.pos += 1;
                } else {
                    v.state = VoiceState::Free;
                    continue;
                }
                if v.state == VoiceState::Releasing {
                    v.gain -= v.release_step;
                    if v.gain <= 0.0 {
                        v.state = VoiceState::Free;
                    }
                }
            }
            let s = (acc * volume).clamp(-1.0, 1.0);
            let base = frame * stride;
            out[base] = s;
            if stride > 1 {
                out[base + 1] = s;
            }
        }
        self.flags
            .frames_rendered
            .fetch_add(frames as u64, Ordering::Relaxed);
    }

    fn drain_triggers(&mut self, discard: bool) {
        for _ in 0..MAX_VOICES * 2 {
            let Ok(t) = self.rx.try_recv() else { break };
            if !discard {
                self.apply(t);
            }
        }
    }

    fn apply(&mut self, t: Trigger) {
        match t {
            Trigger::NoteOn {
                key,
                instrument,
                midi,
                velocity,
                at,
            } => {
                // Original dedup: skip if this key's voice is still playing.
                if self
                    .voices
                    .iter()
                    .any(|v| v.key == key && v.state == VoiceState::Playing)
                {
                    return;
                }
                let Some(sample) = self.bank.get(instrument, midi) else {
                    return;
                };
                if let Some(tx) = &self.bench_tx {
                    let _ = tx.try_send(at.elapsed().as_nanos() as u64);
                }
                let idx = self.find_slot();
                let v = &mut self.voices[idx];
                *v = Voice {
                    state: VoiceState::Playing,
                    key,
                    sample: &sample.data,
                    pos: 0,
                    gain: velocity,
                    release_step: velocity / (RELEASE_SECONDS * self.sample_rate as f32),
                    age: self.clock,
                };
                self.clock += 1;
            }
            Trigger::NoteOff { key } => {
                // Release detaches the voice from the key; the tail keeps ringing,
                // so an immediate re-press starts a fresh voice (original behavior).
                for v in &mut self.voices {
                    if v.key == key && v.state == VoiceState::Playing {
                        v.state = VoiceState::Releasing;
                    }
                }
            }
        }
    }

    fn find_slot(&mut self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| v.state == VoiceState::Free) {
            return i;
        }
        // Steal the oldest releasing voice, else the oldest voice overall.
        let oldest = |state: Option<VoiceState>, voices: &[Voice; MAX_VOICES]| {
            voices
                .iter()
                .enumerate()
                .filter(|(_, v)| state.is_none_or(|s| v.state == s))
                .min_by_key(|(_, v)| v.age)
                .map(|(i, _)| i)
        };
        oldest(Some(VoiceState::Releasing), &self.voices)
            .or_else(|| oldest(None, &self.voices))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::SampleBank;
    use crate::mapping;
    use crossbeam_channel::unbounded;

    const RATE: u32 = 48_000;

    fn setup() -> (Sender<Trigger>, AuralMixer) {
        let (tx, rx) = unbounded();
        let flags = Arc::new(SharedFlags::new(1.0, false));
        let mixer = AuralMixer::new(rx, SampleBank::synthetic(RATE), flags, RATE, 2, None);
        (tx, mixer)
    }

    fn press(tx: &Sender<Trigger>, key: u8) {
        let n = mapping::map_key(key, false).unwrap();
        tx.send(Trigger::NoteOn {
            key,
            instrument: n.instrument,
            midi: n.midi,
            velocity: n.velocity,
            at: Instant::now(),
        })
        .unwrap();
    }

    #[test]
    fn note_on_produces_sound() {
        let (tx, mut mixer) = setup();
        press(&tx, mapping::VK_A);
        let mut buf = vec![0.0; 512 * 2];
        mixer.fill(&mut buf);
        assert!(buf.iter().any(|s| s.abs() > 1e-6));
    }

    #[test]
    fn silence_without_triggers() {
        let (_tx, mut mixer) = setup();
        let mut buf = vec![0.0; 256 * 2];
        mixer.fill(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn release_fades_over_half_second() {
        // Synthetic samples are 2 s long, so the voice outlives the release ramp:
        // output must go silent ≈ 0.5 s (24000 frames) after NoteOff.
        let (tx, mut mixer) = setup();
        press(&tx, mapping::VK_A);
        tx.send(Trigger::NoteOff { key: mapping::VK_A }).unwrap();
        let mut buf = vec![0.0; 128 * 2];
        let mut sounding_frames = 0usize;
        for _ in 0..400 {
            mixer.fill(&mut buf);
            if buf.iter().any(|s| s.abs() > 1e-6) {
                sounding_frames += 128;
            } else {
                break;
            }
        }
        let expected = (RELEASE_SECONDS * RATE as f32) as usize;
        let drift = sounding_frames.abs_diff(expected);
        assert!(
            drift <= 2 * 128,
            "release lasted {sounding_frames} frames, expected ≈{expected}"
        );
    }

    #[test]
    fn held_key_does_not_retrigger() {
        let (tx, mut mixer) = setup();
        press(&tx, mapping::VK_A);
        press(&tx, mapping::VK_A); // OS autorepeat arrives as another NoteOn
        let mut buf = vec![0.0; 128 * 2];
        mixer.fill(&mut buf);
        let playing = mixer
            .voices
            .iter()
            .filter(|v| v.key == mapping::VK_A && v.state == VoiceState::Playing)
            .count();
        assert_eq!(playing, 1);
    }

    #[test]
    fn note_off_detaches_voice_for_repress() {
        let (tx, mut mixer) = setup();
        press(&tx, mapping::VK_A);
        tx.send(Trigger::NoteOff { key: mapping::VK_A }).unwrap();
        press(&tx, mapping::VK_A);
        let mut buf = vec![0.0; 128 * 2];
        mixer.fill(&mut buf);
        let releasing = mixer
            .voices
            .iter()
            .filter(|v| v.key == mapping::VK_A && v.state == VoiceState::Releasing)
            .count();
        let playing = mixer
            .voices
            .iter()
            .filter(|v| v.key == mapping::VK_A && v.state == VoiceState::Playing)
            .count();
        assert_eq!(
            (releasing, playing),
            (1, 1),
            "old tail rings while new voice plays"
        );
    }

    #[test]
    fn mute_outputs_silence_but_keeps_rendering() {
        let (tx, mut mixer) = setup();
        mixer.flags.muted.store(true, Ordering::Relaxed);
        press(&tx, mapping::VK_A);
        let mut buf = vec![0.0; 256 * 2];
        mixer.fill(&mut buf);
        assert!(buf.iter().all(|s| *s == 0.0));
        assert_eq!(mixer.flags.frames_rendered.load(Ordering::Relaxed), 256);
    }
}
