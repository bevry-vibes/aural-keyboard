//! cpal output stage: the shortest path from the OS audio callback to `AuralMixer`
//! (DESIGN.md D2 refinement — cpal directly, no intermediate mixer thread/channel).
//!
//! Buffer negotiation: try 128 then 256 fixed frames (WASAPI shared mode accepts
//! small periods), falling back to the device default. The negotiated value is
//! reported for `doctor`/`bench`.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Stream, StreamConfig};

use crate::mixer::AuralMixer;

pub struct OutputInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_desc: String,
}

pub fn default_output() -> Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("no default output device")?;
    let supported = device
        .default_output_config()
        .context("no default output config")?;
    Ok((device, supported))
}

pub fn sample_rate_of(supported: &cpal::SupportedStreamConfig) -> u32 {
    supported.sample_rate()
}

pub fn start_stream(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    mixer: AuralMixer,
) -> Result<(Stream, OutputInfo)> {
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let sample_rate = sample_rate_of(supported);
    let mut last_err = None;
    for candidate in [Some(128u32), Some(256), None] {
        // Use the device's default config untouched (channel count included —
        // forcing stereo breaks multi-channel WASAPI endpoints).
        let mut config: StreamConfig = supported.config();
        config.buffer_size = match candidate {
            Some(n) => BufferSize::Fixed(n),
            None => BufferSize::Default,
        };
        match try_build(device, &config, &mixer) {
            Ok(stream) => {
                stream.play().context("starting stream")?;
                let buffer_desc = match candidate {
                    Some(n) => format!("{n} frames (requested)"),
                    None => "device default".to_string(),
                };
                return Ok((
                    stream,
                    OutputInfo {
                        device_name,
                        sample_rate,
                        channels: supported.channels(),
                        buffer_desc,
                    },
                ));
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap()).context("opening output stream")
}

// cpal moves the mixer into the callback; we only need a fresh closure per attempt.
fn try_build(device: &cpal::Device, config: &StreamConfig, mixer: &AuralMixer) -> Result<Stream> {
    // Reconstructing is cheap and keeps `start_stream` retry-friendly: the mixer
    // state is empty at startup anyway (voices are all Free).
    let mut mixer = mixer.clone_fresh();
    let stream = device.build_output_stream(
        *config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| mixer.fill(data),
        |e| eprintln!("audio stream error: {e}"),
        None,
    )?;
    Ok(stream)
}
