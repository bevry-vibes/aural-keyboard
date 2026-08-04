//! aural — system-wide melodic keyboard sounds (CLI entry point).

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aural",
    version,
    about = "System-wide melodic keyboard sounds (aural-coding, ported to the OS)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run in the foreground (Ctrl+C to quit)
    Run {
        #[arg(long, hide = true)]
        daemon: bool,
    },
    /// Start as a background daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Is the daemon running?
    Status,
    /// Start automatically at login (Windows Run key)
    Install,
    /// Remove from login autostart
    Uninstall,
    /// Mute all sounds
    Mute,
    /// Unmute
    Unmute,
    /// Toggle mute (same as the global hotkey)
    Toggle,
    /// Set volume (0–100)
    Volume { value: f32 },
    /// Measure press→sound latency; type, then Ctrl+C for the report
    Bench {
        /// Fire N synthetic triggers instead of using the keyboard hook
        #[arg(long)]
        synthetic: Option<usize>,
    },
    /// Diagnostics: device, buffer, assets, daemon state
    Doctor,
    /// Version and sound attribution
    About,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Run { daemon } => {
            aural::engine::install_ctrlc();
            aural::engine::run(daemon, None)
        }
        Command::Start => aural::daemon::start(),
        Command::Stop => aural::daemon::stop(),
        Command::Status => {
            match aural::daemon::status() {
                Some(pid) => println!("aural: running (pid {pid})"),
                None => println!("aural: not running"),
            }
            Ok(())
        }
        Command::Install => aural::daemon::install(),
        Command::Uninstall => aural::daemon::uninstall(),
        Command::Mute => {
            aural::config::update(|c| c.muted = true)?;
            println!("aural: muted");
            Ok(())
        }
        Command::Unmute => {
            aural::config::update(|c| c.muted = false)?;
            println!("aural: unmuted");
            Ok(())
        }
        Command::Toggle => {
            let c = aural::config::update(|c| c.muted = !c.muted)?;
            println!("aural: {}", if c.muted { "muted" } else { "unmuted" });
            Ok(())
        }
        Command::Volume { value } => {
            let v = (value / 100.0).clamp(0.0, 1.0);
            aural::config::update(|c| c.volume = v)?;
            println!("aural: volume {}%", (v * 100.0).round() as u32);
            Ok(())
        }
        Command::Bench { synthetic: Some(n) } => aural::bench::synthetic(n),
        Command::Bench { synthetic: None } => {
            aural::engine::install_ctrlc();
            aural::bench::live()
        }
        Command::Doctor => doctor(),
        Command::About => {
            println!("aural {}", env!("CARGO_PKG_VERSION"));
            println!("Melodic keyboard sounds, system-wide. Port of aural-coding (Atom/VSCode).");
            println!(
                "Samples: FluidR3_GM soundfont (acoustic_grand_piano, synth_drum), CC BY 3.0."
            );
            println!("See README.md (Attribution) and DESIGN.md.");
            Ok(())
        }
    }
}

fn doctor() -> Result<()> {
    use cpal::traits::DeviceTrait;
    println!("aural {}", env!("CARGO_PKG_VERSION"));
    println!("config: {}", aural::config::path().display());
    match aural::daemon::status() {
        Some(pid) => println!("daemon: running (pid {pid})"),
        None => println!("daemon: not running"),
    }
    match aural::audio::default_output() {
        Ok((device, supported)) => {
            println!(
                "device: {}",
                device
                    .description()
                    .map(|d| d.name().to_string())
                    .unwrap_or_else(|_| "unknown".into())
            );
            println!(
                "default config: {} Hz, {} ch, {:?}",
                supported.sample_rate(),
                supported.channels(),
                supported.sample_format()
            );
        }
        Err(e) => println!("output: ERROR {e:#}"),
    }
    let started = std::time::Instant::now();
    match aural::assets::load(48_000) {
        Ok(_) => println!("assets: 37 notes decode OK in {:?}", started.elapsed()),
        Err(e) => println!("assets: ERROR {e:#}"),
    }
    println!("hook: WH_KEYBOARD_LL (installs on `aural run`; no admin required)");
    Ok(())
}
