//! aural — system-wide melodic keyboard sounds (CLI entry point).

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aural",
    version,
    about = "System-wide melodic keyboard sounds (aural-coding, ported to the OS)",
    long_about = "System-wide melodic keyboard sounds (aural-coding, ported to the OS).\n\n\
                  Install the app once with `aural system install` — it handles the\n\
                  permissions, starts now, and starts at every login. `aural stdin` plays\n\
                  from stdin for quick testing (no permissions needed)."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the installed app: install/uninstall, login autostart, mute,
    /// diagnostics
    #[command(subcommand_required = true, arg_required_else_help = true)]
    System {
        #[command(subcommand)]
        action: SystemAction,
    },
    /// Read keys from stdin instead of the OS hook (testing; no permissions needed)
    Stdin,
}

#[derive(Subcommand)]
enum SystemAction {
    /// Install the app (menu-bar/tray entry, or the background daemon on
    /// Windows), handling the permissions it needs, then start it now and at
    /// every login
    Install,
    /// Stop the app and remove everything the install created
    Uninstall,
    /// Start the app automatically at login
    Enable,
    /// Don't start the app automatically at login
    Disable,
    /// Diagnostics: engine, device, buffer, hook, input access, install state
    Doctor,
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
    /// Version and sound attribution
    About,

    // Hidden internal commands — the install machinery invokes these across a
    // process boundary (login entries, the detached spawner, systemd units);
    // they are not for users.
    /// (internal) the engine in background-daemon mode
    #[command(hide = true)]
    Daemon,
    /// (internal) the menu-bar/tray app
    #[command(hide = true)]
    Tray,
    /// (internal) wait for the session's pipewire socket, then grant the
    /// dedicated engine user audio access (Linux)
    #[command(hide = true)]
    AclWait {
        user: String,
        uid: u32,
        timeout: u64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Re-exec disclaimed (macOS) before anything else so TCC attributes the
    // Input Monitoring grant to aural itself, not the launching terminal —
    // only for commands that install the keyboard hook.
    #[cfg(target_os = "macos")]
    if disclaim_needed(cli.command.as_ref()) {
        aural::macos::disclaim()?;
    }
    match cli.command {
        None => run_default(),
        Some(Command::Stdin) => {
            aural::engine::install_ctrlc();
            aural::engine::run(false, true, None)
        }
        Some(Command::System { action }) => match action {
            SystemAction::Install => aural::system::install(),
            SystemAction::Uninstall => aural::system::uninstall(),
            SystemAction::Enable => aural::system::enable(),
            SystemAction::Disable => aural::system::disable(),
            SystemAction::Doctor => doctor(),
            SystemAction::Mute => set_muted(true),
            SystemAction::Unmute => set_muted(false),
            SystemAction::Toggle => toggle_muted(),
            SystemAction::Volume { value } => {
                let v = (value / 100.0).clamp(0.0, 1.0);
                aural::config::update(|c| c.volume = v)?;
                println!("aural: volume {}%", (v * 100.0).round() as u32);
                Ok(())
            }
            SystemAction::Bench { synthetic } => match synthetic {
                Some(n) => aural::bench::synthetic(n),
                None => {
                    aural::engine::install_ctrlc();
                    aural::bench::live()
                }
            },
            SystemAction::About => {
                println!("aural {}", env!("CARGO_PKG_VERSION"));
                println!(
                    "Melodic keyboard sounds, system-wide. Port of aural-coding (Atom/VSCode)."
                );
                println!(
                    "Samples: FluidR3_GM soundfont (acoustic_grand_piano, synth_drum), CC BY 3.0."
                );
                println!("See README.md (Attribution) and DESIGN.md.");
                Ok(())
            }
            SystemAction::Daemon => {
                aural::engine::install_ctrlc();
                aural::engine::run(true, false, None)
            }
            SystemAction::Tray => {
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                {
                    aural::menubar::run()
                }
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                {
                    eprintln!("aural system tray: not supported on this platform");
                    std::process::exit(1);
                }
            }
            SystemAction::AclWait { user, uid, timeout } => {
                #[cfg(target_os = "linux")]
                {
                    aural::system::linux::acl_wait(&user, uid, timeout)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let _ = (user, uid, timeout);
                    eprintln!("aural system acl-wait: Linux only");
                    std::process::exit(1);
                }
            }
        },
    }
}

/// Bare `aural`: the help text — except inside the packaged Aural.app, where
/// a no-argument launch is how Finder/`open` starts the app (the tray).
fn run_default() -> Result<()> {
    #[cfg(target_os = "macos")]
    if aural::system::in_app_bundle() {
        return aural::menubar::run();
    }
    // render_long_help so this matches `aural --help` exactly (print_help
    // renders the short form).
    print!("{}", Cli::command().render_long_help());
    Ok(())
}

/// Whether the command installs the keyboard hook (needs Input Monitoring) or
/// reports on it, so it is worth re-exec'ing disclaimed. Bare `aural` is the
/// help text (or, inside the app bundle, the tray — guarded below), and the
/// control commands (`mute`/`volume`/`about`/…) never touch the hook.
///
/// When launched from within a `.app` bundle, the responsible process is
/// already "Aural" (a stable, grantable identity), so disclaiming would only
/// create a *new* TCC identity (the raw binary path) that the user hasn't
/// granted. Skip the re-exec in that case.
#[cfg(target_os = "macos")]
fn disclaim_needed(cmd: Option<&Command>) -> bool {
    if aural::system::in_app_bundle() {
        return false;
    }
    match cmd {
        None => false,
        Some(Command::Stdin) => false,
        Some(Command::System { action }) => matches!(
            action,
            SystemAction::Bench { synthetic: None }
                | SystemAction::Doctor
                | SystemAction::Daemon
                | SystemAction::Tray
        ),
    }
}

/// Shared by `aural system mute` / `unmute`.
fn set_muted(muted: bool) -> Result<()> {
    aural::config::update(|c| c.muted = muted)?;
    println!("aural: {}", if muted { "muted" } else { "unmuted" });
    Ok(())
}

/// `aural system toggle` (and the hotkey's CLI equivalent).
fn toggle_muted() -> Result<()> {
    let c = aural::config::update(|c| c.muted = !c.muted)?;
    println!("aural: {}", if c.muted { "muted" } else { "unmuted" });
    Ok(())
}

fn doctor() -> Result<()> {
    use cpal::traits::DeviceTrait;
    println!("aural {}", env!("CARGO_PKG_VERSION"));
    println!("config: {}", aural::config::path().display());
    match aural::daemon::status() {
        Some(pid) => println!("engine: running (pid {pid})"),
        None => println!("engine: not running"),
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
    println!("hook: WH_KEYBOARD_LL (installs when aural runs; no admin required)");
    #[cfg(windows)]
    println!(
        "autostart: {}",
        if aural::system::login_enabled() {
            "registered (registry Run key)"
        } else {
            "not registered (`aural system enable`)"
        }
    );
    #[cfg(target_os = "macos")]
    {
        if aural::hook::listen_access_granted() {
            println!("hook: CGEventTap listen-only; Input Monitoring permission: granted");
        } else {
            println!(
                "hook: CGEventTap listen-only; Input Monitoring permission: NOT granted\n  \
                 → grant aural (or Aural, if the app is installed) in System Settings →\n    \
                 Privacy & Security → Input Monitoring, then restart aural\n    \
                 (`aural system install` relaunches the app)."
            );
        }
        println!(
            "autostart: {}",
            if aural::system::login_enabled() {
                "registered (LaunchAgent)"
            } else {
                "not registered (`aural system enable`)"
            }
        );
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
        if aural::system::linux::dedicated_installed() {
            println!(
                "install: dedicated-user mode — {}\n  \
                 → a missing item below usually just needs one log out & back in\n    \
                 (group and environment changes only apply to new sessions)",
                aural::system::linux::verify_chain()
            );
        } else if aural::hook::listen_access_granted() {
            println!("hook: evdev listen-only; /dev/input access: granted");
        } else {
            println!(
                "hook: evdev listen-only; /dev/input access: NOT granted\n  \
                 → `aural system install` sets up the dedicated-user mode (it asks for sudo)"
            );
        }
        if let Some(dir) = std::env::var_os("AURAL_CONFIG_DIR") {
            println!(
                "config dir: {} (AURAL_CONFIG_DIR override — dedicated-user mode)",
                std::path::PathBuf::from(&dir).display()
            );
        }
        println!(
            "autostart: {}",
            if aural::system::login_enabled() {
                if aural::system::linux::dedicated_installed() {
                    "registered (aural.service)"
                } else {
                    "registered (XDG autostart)"
                }
            } else {
                "not registered (`aural system enable`)"
            }
        );
    }
    Ok(())
}
