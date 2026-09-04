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
        /// Read keys from stdin instead of the system hook (testing; no permissions needed)
        #[arg(long)]
        stdin: bool,
    },
    /// Start as a background daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Is the daemon running?
    Status,
    /// Start automatically at login (Windows Run key / macOS LaunchAgent / Linux XDG autostart)
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
    /// (macOS, Linux) run as a tray/menubar app with mute / login / doctor / quit
    Menubar {
        /// Don't host the engine — Linux dedicated-user mode only: the engine
        /// runs as the `aural` system user via systemd; this tray becomes a
        /// control surface (Mute via the shared config dir, Open Doctor, Quit)
        #[arg(long)]
        no_engine: bool,
    },
    /// Version and sound attribution
    About,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Re-exec disclaimed (macOS) before anything else so TCC attributes the
    // Input Monitoring grant to aural itself, not the launching terminal —
    // only for commands that install the keyboard hook.
    #[cfg(target_os = "macos")]
    if disclaim_needed(&cli.command) {
        aural::macos::disclaim()?;
    }
    match cli.command {
        Command::Run { daemon, stdin } => {
            aural::engine::install_ctrlc();
            aural::engine::run(daemon, stdin, None)
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
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        Command::Menubar { no_engine } => aural::menubar::run(no_engine),
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        Command::Menubar { .. } => {
            eprintln!("aural menubar: not supported on this platform");
            std::process::exit(1);
        }
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

/// Whether the command installs the keyboard hook (needs Input Monitoring) or
/// reports on it, so it is worth re-exec'ing disclaimed. `--stdin`,
/// `--synthetic`, and the control commands (`start`/`stop`/`status`/`mute`/
/// `volume`/`about`/`install`/`uninstall`) never touch the hook and need no TCC.
///
/// When launched from within the packaged `Aural.app` bundle, the responsible
/// process is already "Aural" (a stable, grantable identity), so disclaiming
/// would only create a *new* TCC identity (the raw binary path) that the user
/// hasn't granted. Skip the re-exec in that case.
#[cfg(target_os = "macos")]
fn disclaim_needed(cmd: &Command) -> bool {
    if in_app_bundle() {
        return false;
    }
    matches!(
        cmd,
        Command::Run { stdin: false, .. }
            | Command::Bench { synthetic: None }
            | Command::Doctor
            | Command::Menubar { .. }
    )
}

/// True when the running binary lives inside a `.app` bundle (the packaged
/// Aural.app), so TCC already attributes the grant to "Aural".
#[cfg(target_os = "macos")]
fn in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.ends_with("MacOS")))
        .unwrap_or(false)
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
    #[cfg(windows)]
    println!("hook: WH_KEYBOARD_LL (installs on `aural run`; no admin required)");
    #[cfg(target_os = "macos")]
    {
        if aural::hook::listen_access_granted() {
            println!("hook: CGEventTap listen-only; Input Monitoring permission: granted");
        } else {
            println!(
                "hook: CGEventTap listen-only; Input Monitoring permission: NOT granted\n  \
                 → grant this binary (or your terminal) in System Settings →\n    \
                 Privacy & Security → Input Monitoring, then run `aural run`."
            );
        }
    }
    #[cfg(target_os = "macos")]
    println!("{}", aural::macos::secure_input_check());
    #[cfg(target_os = "linux")]
    {
        println!(
            "display: {} session on {}",
            std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into()),
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into())
        );
        if aural::hook::listen_access_granted() {
            println!("hook: evdev listen-only; /dev/input access: granted");
        } else {
            println!(
                "hook: evdev listen-only; /dev/input access: NOT granted\n  \
                 → either add your user to the input group (log out & back in):\n    \
                 sudo usermod -aG input $USER\n  \
                 or run the daemon as a dedicated `aural` user (better isolation):\n    \
                 sudo ./scripts/setup-dedicated-user.sh   (see README: Dedicated-user mode)"
            );
        }
        if let Some(dir) = std::env::var_os("AURAL_CONFIG_DIR") {
            println!(
                "config dir: {} (AURAL_CONFIG_DIR override — dedicated-user mode)",
                std::path::PathBuf::from(&dir).display()
            );
        }
        if std::path::Path::new("/etc/systemd/system/aural.service").exists() {
            println!(
                "dedicated-user service: installed (daemon runs as `aural`; \
                 control with `systemctl status aural`)"
            );
        }
        println!(
            "autostart: {}",
            if aural::daemon::login_enabled() {
                "registered (XDG autostart)"
            } else {
                "not registered (`aural install`)"
            }
        );
    }
    Ok(())
}
