//! The engine: wires config → assets → mixer → cpal stream → keyboard hook,
//! then supervises (config hot-reload, device-stall watchdog, graceful stop).

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::assets::{self, SampleBank};
use crate::audio;
use crate::config;
use crate::mixer::{AuralMixer, SharedFlags, Trigger};

pub struct Engine {
    pub flags: Arc<SharedFlags>,
    pub trigger_tx: Sender<Trigger>,
    rx: Receiver<Trigger>, // clonable (MPMC): stream rebuilds re-plug into the same queue
    pub bank: &'static SampleBank,
    pub sample_rate: u32,
    channels: usize,
}

static STOP: AtomicBool = AtomicBool::new(false);

pub fn request_stop() {
    STOP.store(true, Ordering::Relaxed);
}

/// Install the Ctrl+C handler for foreground runs (idempotent).
pub fn install_ctrlc() {
    let _ = ctrlc::set_handler(request_stop);
}

impl Engine {
    /// Open the default output device and decode the soundfont bank at its rate.
    pub fn new() -> Result<(Self, cpal::Device, cpal::SupportedStreamConfig)> {
        let (device, supported) = audio::default_output()?;
        let sample_rate = audio::sample_rate_of(&supported);
        let channels = supported.channels() as usize;
        let started = Instant::now();
        let bank = assets::load(sample_rate).context("loading soundfont assets")?;
        eprintln!("aural: 37 notes decoded in {:?}", started.elapsed());
        let (tx, rx) = crossbeam_channel::unbounded();
        let c = config::load();
        let flags = Arc::new(SharedFlags::new(c.volume, c.muted));
        Ok((
            Engine {
                flags,
                trigger_tx: tx,
                rx,
                bank,
                sample_rate,
                channels,
            },
            device,
            supported,
        ))
    }

    /// A fresh mixer plugged into the engine's trigger queue (voices all Free).
    pub fn mixer(&self, bench_tx: Option<Sender<u64>>) -> AuralMixer {
        AuralMixer::new(
            self.rx.clone(),
            self.bank,
            self.flags.clone(),
            self.sample_rate,
            self.channels,
            bench_tx,
        )
    }
}

/// Foreground/daemon supervision: stream, hook, config watch, stall watchdog.
// device/supported are only read again on watchdog rebuilds, which the
// flow-insensitive lint can't see.
#[allow(unused_assignments)]
pub fn run(daemon: bool, stdin_keys: bool, bench_tx: Option<Sender<u64>>) -> Result<()> {
    if daemon {
        crate::daemon::redirect_stdio_to_log();
    }
    let (engine, mut device, mut supported) = Engine::new()?;
    let _single = crate::daemon::acquire_single_instance()?;

    let mut stream: Option<cpal::Stream> = {
        let (s, info) = audio::start_stream(&device, &supported, engine.mixer(bench_tx))?;
        eprintln!(
            "aural: output '{}' @ {} Hz, buffer {}",
            info.device_name, info.sample_rate, info.buffer_desc
        );
        Some(s)
    };
    let config = config::load();
    let mut hook = None;
    if stdin_keys {
        eprintln!("aural: reading keys from stdin (each char = a key, Enter = Return)");
        crate::hook::spawn_stdin_reader(engine.trigger_tx.clone(), engine.flags.clone());
    } else {
        let h = crate::hook::spawn(
            engine.trigger_tx.clone(),
            engine.flags.clone(),
            config::parse_hotkey(&config.hotkey),
        )
        .context("keyboard hook")?;
        eprintln!(
            "aural: keyboard hook installed ({h}), hotkey {}",
            config.hotkey
        );
        hook = Some(h);
    }
    if daemon {
        std::fs::write(config::pid_path(), std::process::id().to_string()).ok();
    }
    eprintln!("aural: running — type anywhere");

    let mut last_config_mtime = config::mtime();
    let mut last_frames = 0u64;
    let mut stalled_checks = 0u32;

    while !STOP.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(500));

        // Config hot-reload (CLI mute/volume land here within 500 ms).
        let mt = config::mtime();
        if mt != last_config_mtime {
            last_config_mtime = mt;
            let c = config::load();
            engine.flags.muted.store(c.muted, Ordering::Relaxed);
            engine
                .flags
                .volume_bits
                .store(c.volume.to_bits(), Ordering::Relaxed);
        }

        // Watchdog: render counter stalled ~2 s (device unplug/switch) → rebuild
        // the stream on the current default device (KeyEcho lesson, DESIGN.md D2).
        let frames = engine.flags.frames_rendered.load(Ordering::Relaxed);
        stalled_checks = if frames == last_frames {
            stalled_checks + 1
        } else {
            0
        };
        last_frames = frames;
        if stalled_checks >= 4 {
            eprintln!("aural: output stalled; reopening default device");
            drop(stream.take());
            while stream.is_none() && !STOP.load(Ordering::Relaxed) {
                match audio::default_output().and_then(|(d, s)| {
                    audio::start_stream(&d, &s, engine.mixer(None))
                        .map(|(st, info)| (d, s, st, info))
                }) {
                    Ok((d, s, st, info)) => {
                        device = d;
                        supported = s;
                        stream = Some(st);
                        eprintln!(
                            "aural: output recovered on '{}' ({})",
                            info.device_name, info.buffer_desc
                        );
                    }
                    Err(e) => {
                        eprintln!("aural: reopen failed: {e:#}; retrying in 2 s");
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }
            }
            stalled_checks = 0;
        }
    }

    if let Some(hook) = hook {
        crate::hook::stop(hook);
    }
    drop(stream);
    if daemon {
        let _ = std::fs::remove_file(config::pid_path());
    }
    Ok(())
}
