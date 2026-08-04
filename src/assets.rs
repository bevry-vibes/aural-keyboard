//! Embedded soundfont loading: OGG decode (symphonia) → mono downmix → silence trim
//! → resample to the device rate (rubato). Runs once at startup; the render path
//! only ever reads the leaked `&'static SampleBank`, so it never allocates.

use anyhow::Result;

/// A decoded, trimmed, device-rate, mono sample.
pub struct Sample {
    pub data: Vec<f32>,
}

/// All mapped notes, indexed by MIDI number. Only the 37 mapped notes are populated.
pub struct SampleBank {
    pub piano: [Option<Sample>; 128],
    pub drums: [Option<Sample>; 128],
}

impl SampleBank {
    pub fn get(&self, instrument: crate::mapping::Instrument, midi: u8) -> Option<&Sample> {
        match instrument {
            crate::mapping::Instrument::Piano => self.piano[midi as usize].as_ref(),
            crate::mapping::Instrument::Drums => self.drums[midi as usize].as_ref(),
        }
    }

    /// Tiny synthetic bank for tests and `bench --synthetic`: a short decaying sine per note.
    #[doc(hidden)]
    pub fn synthetic(sample_rate: u32) -> &'static SampleBank {
        let mut bank = Box::new(SampleBank {
            piano: std::array::from_fn(|_| None),
            drums: std::array::from_fn(|_| None),
        });
        let frames = (sample_rate * 2) as usize; // 2 s, so release ramps are observable
        for (midi, _) in PIANO_OGG.iter().chain(DRUM_OGG.iter()) {
            let freq = 440.0 * 2f32.powf((*midi as f32 - 69.0) / 12.0);
            let data: Vec<f32> = (0..frames)
                .map(|i| {
                    let t = i as f32 / sample_rate as f32;
                    (t * freq * std::f32::consts::TAU).sin() * (1.0 - i as f32 / frames as f32)
                })
                .collect();
            if PIANO_OGG.iter().any(|(m, _)| m == midi) {
                bank.piano[*midi as usize] = Some(Sample { data });
            } else {
                bank.drums[*midi as usize] = Some(Sample { data });
            }
        }
        Box::leak(bank)
    }
}

// The 37 mapped notes (DESIGN.md §5), embedded into the binary.
static PIANO_OGG: &[(u8, &[u8])] = &[
    (62, include_bytes!("../assets/soundfonts/piano/D4.ogg")),
    (64, include_bytes!("../assets/soundfonts/piano/E4.ogg")),
    (65, include_bytes!("../assets/soundfonts/piano/F4.ogg")),
    (67, include_bytes!("../assets/soundfonts/piano/G4.ogg")),
    (69, include_bytes!("../assets/soundfonts/piano/A4.ogg")),
    (70, include_bytes!("../assets/soundfonts/piano/Bb4.ogg")),
    (72, include_bytes!("../assets/soundfonts/piano/C5.ogg")),
    (74, include_bytes!("../assets/soundfonts/piano/D5.ogg")),
    (76, include_bytes!("../assets/soundfonts/piano/E5.ogg")),
    (77, include_bytes!("../assets/soundfonts/piano/F5.ogg")),
    (79, include_bytes!("../assets/soundfonts/piano/G5.ogg")),
    (81, include_bytes!("../assets/soundfonts/piano/A5.ogg")),
    (82, include_bytes!("../assets/soundfonts/piano/Bb5.ogg")),
    (84, include_bytes!("../assets/soundfonts/piano/C6.ogg")),
    (86, include_bytes!("../assets/soundfonts/piano/D6.ogg")),
    (88, include_bytes!("../assets/soundfonts/piano/E6.ogg")),
    (89, include_bytes!("../assets/soundfonts/piano/F6.ogg")),
    (91, include_bytes!("../assets/soundfonts/piano/G6.ogg")),
    (93, include_bytes!("../assets/soundfonts/piano/A6.ogg")),
    (94, include_bytes!("../assets/soundfonts/piano/Bb6.ogg")),
    (96, include_bytes!("../assets/soundfonts/piano/C7.ogg")),
    (98, include_bytes!("../assets/soundfonts/piano/D7.ogg")),
    (100, include_bytes!("../assets/soundfonts/piano/E7.ogg")),
    (101, include_bytes!("../assets/soundfonts/piano/F7.ogg")),
];

static DRUM_OGG: &[(u8, &[u8])] = &[
    (36, include_bytes!("../assets/soundfonts/drums/C2.ogg")),
    (37, include_bytes!("../assets/soundfonts/drums/Db2.ogg")),
    (38, include_bytes!("../assets/soundfonts/drums/D2.ogg")),
    (39, include_bytes!("../assets/soundfonts/drums/Eb2.ogg")),
    (41, include_bytes!("../assets/soundfonts/drums/F2.ogg")),
    (45, include_bytes!("../assets/soundfonts/drums/A2.ogg")),
    (49, include_bytes!("../assets/soundfonts/drums/Db3.ogg")),
    (50, include_bytes!("../assets/soundfonts/drums/D3.ogg")),
    (54, include_bytes!("../assets/soundfonts/drums/Gb3.ogg")),
    (56, include_bytes!("../assets/soundfonts/drums/Ab3.ogg")),
    (57, include_bytes!("../assets/soundfonts/drums/A3.ogg")),
    (58, include_bytes!("../assets/soundfonts/drums/Bb3.ogg")),
    (61, include_bytes!("../assets/soundfonts/drums/Db4.ogg")),
];

/// Decode, trim, and resample every mapped note; leak the bank for the program lifetime.
pub fn load(sample_rate: u32) -> Result<&'static SampleBank> {
    let mut bank = Box::new(SampleBank {
        piano: std::array::from_fn(|_| None),
        drums: std::array::from_fn(|_| None),
    });
    for (midi, bytes) in PIANO_OGG {
        bank.piano[*midi as usize] = Some(load_one(bytes, sample_rate)?);
    }
    for (midi, bytes) in DRUM_OGG {
        bank.drums[*midi as usize] = Some(load_one(bytes, sample_rate)?);
    }
    Ok(Box::leak(bank))
}

fn load_one(bytes: &'static [u8], device_rate: u32) -> Result<Sample> {
    let (mono, rate) = decode_ogg(bytes)?;
    let mono = trim_silence(mono);
    let data = resample_to(mono, rate, device_rate)?;
    Ok(Sample { data })
}

fn decode_ogg(bytes: &'static [u8]) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow::anyhow!("no audio track"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| anyhow::anyhow!("no audio codec params"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())?;

    let mut mono: Vec<f32> = Vec::new();
    let mut rate = 0u32;
    while let Some(packet) = format.next_packet()? {
        if packet.track_id != track_id {
            continue;
        }
        let Ok(buf) = decoder.decode(&packet) else {
            continue;
        };
        rate = buf.spec().rate();
        let channels = buf.spec().channels().count().max(1);
        let mut interleaved = vec![0.0f32; buf.samples_interleaved()];
        buf.copy_to_slice_interleaved(&mut interleaved);
        for frame in interleaved.chunks(channels) {
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }
    if mono.is_empty() || rate == 0 {
        anyhow::bail!("decoded zero frames");
    }
    Ok((mono, rate))
}

/// Trim codec/encoder silence. Head trim is tight (attack latency matters);
/// tail padding is generous (keep the natural decay).
fn trim_silence(mut data: Vec<f32>) -> Vec<f32> {
    const THRESHOLD: f32 = 1e-4;
    const HEAD_PAD: usize = 64;
    const TAIL_PAD: usize = 8192;
    let start = data
        .iter()
        .position(|s| s.abs() > THRESHOLD)
        .unwrap_or(0)
        .saturating_sub(HEAD_PAD);
    let end = data
        .iter()
        .rposition(|s| s.abs() > THRESHOLD)
        .map(|p| (p + TAIL_PAD).min(data.len()))
        .unwrap_or(data.len());
    data.drain(..start);
    data.truncate(end.saturating_sub(start));
    data
}

/// One-shot startup resampler: Catmull-Rom cubic interpolation. We resample 37
/// short samples exactly once at load, so a tiny interpolator beats pulling in
/// an FFT/sinc library (rubato) — and is transparent for 44.1→48 kHz material.
fn resample_to(data: Vec<f32>, from: u32, to: u32) -> Result<Vec<f32>> {
    if from == to || data.is_empty() {
        return Ok(data);
    }
    let ratio = from as f64 / to as f64;
    let out_len = (data.len() as f64 / ratio).round() as usize;
    let last = (data.len() - 1) as isize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as isize;
        let t = (pos - idx as f64) as f32;
        let s = |j: isize| data[(idx + j).clamp(0, last) as usize];
        let (p0, p1, p2, p3) = (s(-1), s(0), s(1), s(2));
        out.push(
            p1 + 0.5
                * t
                * (p2 - p0
                    + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0))),
        );
    }
    Ok(out)
}
