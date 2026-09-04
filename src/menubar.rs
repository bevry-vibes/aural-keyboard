//! menubar — `aural menubar`: a tiny tray/menubar shell that hosts the engine
//! on a worker thread and exposes: `Mute` (check), `Enable at Login` (check),
//! `Open Doctor`, `Quit`.
//!
//! Both platform shells use the `tray-icon` crate (thin around AppKit on
//! macOS, GTK + libappindicator on Linux), so the menu construction and event
//! handling below is shared and nothing here touches the engine; the engine
//! stays shell-agnostic per DESIGN.md D8 ("add tray/GUI without touching any
//! real code").
//!
//! Threading: the engine (`crate::engine::run`) runs on a worker thread, while
//! the platform UI loop runs on the main thread. Menu clicks are delivered to
//! the shared [`handle_event`] from the UI thread on both platforms:
//!
//! - macOS: the main thread runs `NSApplication.run()` (which services all
//!   run-loop modes, including the event-tracking mode that NSStatusItem menus
//!   run in); clicks arrive via `MenuEvent::set_event_handler`.
//! - Linux: the main thread runs `gtk::main()`; clicks are polled from muda's
//!   `MenuEvent::receiver()` crossbeam channel via a glib timeout (50 ms).
//!
//! Note on dependencies: only compiled/used on macOS and Linux. The `png`
//! crate decodes the embedded menubar icon once at startup into RGBA for
//! `Icon::from_rgba`.

use anyhow::{Context, Result};

use tray_icon::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuEvent, MenuId, MenuItemBuilder,
    PredefinedMenuItem,
};
use tray_icon::TrayIconBuilder;

/// Menu item ids handled in `handle_event`.
const ID_MUTE: &str = "mute";
const ID_LOGIN: &str = "login";
const ID_DOCTOR: &str = "doctor";
const ID_QUIT: &str = "quit";

// --- macOS shell (NSStatusItem via AppKit) ---

#[cfg(target_os = "macos")]
pub fn run(no_engine: bool) -> Result<()> {
    // `--no-engine` is a Linux dedicated-user-mode flag; the macOS menubar
    // always hosts the engine inside the Aural.app bundle.
    let _ = no_engine;
    // The menubar is a UI shell that only makes sense inside the packaged
    // Aural.app bundle (LSUIElement agent). Refuse to run from a bare binary so
    // the status item doesn't appear for terminal/CLI usage.
    if !in_app_bundle() {
        anyhow::bail!(
            "aural menubar: only runs from within the Aural.app bundle\n  \
             → build with `cargo build --release`, then `./scripts/package-app.sh`\n    \
             and `open target/release/Aural.app --args menubar`."
        );
    }

    // Engine on a worker thread; it installs the keyboard hook, needs the TCC
    // disclaim, and hot-reloads `config.json` for mute/volume within 500 ms.
    // Redirect the engine's stderr to the log file (when launched via `open`
    // stderr is discarded) and log its result so a hook/audio failure isn't
    // silent.
    let engine = std::thread::spawn(|| {
        redirect_stderr_to_log();
        if let Err(e) = crate::engine::run(false, false, None) {
            eprintln!("aural menubar: engine error: {e:#}");
        }
    });

    let icon = load_icon().context("loading menubar icon")?;
    let menu = build_menu(true)?;

    // Set up AppKit on the main thread.
    let app = app_main();

    // Create the status item now that the app is running. tray-icon requires the
    // icon to be created on the main thread once the event loop is active.
    let _tray = TrayIconBuilder::new()
        .with_tooltip("aural — melodic keyboard sounds")
        .with_icon(icon)
        .with_menu(Box::new(menu.menu.clone()))
        .build()
        .context("failed to create the macOS menu bar icon")?;

    // Route menu clicks to a main-thread handler. muda auto-toggles the check
    // visuals on macOS, so we only persist the new state to config/plist.
    MenuEvent::set_event_handler(Some(|event| match handle_event(&event) {
        Ok(true) => stop_ui(),
        Ok(false) => {}
        Err(e) => eprintln!("aural menubar: {e:#}"),
    }));

    // Run the AppKit main loop. This services every run-loop mode (including
    // NSEventTrackingRunLoopMode, which NSStatusItem menu tracking requires),
    // so the status item's menu is fully interactive.
    app.run();

    // The run loop returned (Quit → `stop:`). Stop the engine and wait for it.
    crate::engine::request_stop();
    let _ = engine.join();
    Ok(())
}

// --- Linux shell (StatusNotifierItem via GTK + libappindicator) ---

#[cfg(target_os = "linux")]
pub fn run(no_engine: bool) -> Result<()> {
    // With `--no-engine` (dedicated-user mode) the tray is a control surface
    // only: the engine runs as the `aural` system user via systemd
    // (scripts/setup-dedicated-user.sh), and Mute works through the shared
    // AURAL_CONFIG_DIR. stderr stays visible in this mode (no log redirect).
    let engine = if no_engine {
        None
    } else {
        Some(std::thread::spawn(|| {
            redirect_stderr_to_log();
            if let Err(e) = crate::engine::run(false, false, None) {
                eprintln!("aural menubar: engine error: {e:#}");
            }
        }))
    };

    let icon = load_icon().context("loading tray icon")?;
    let menu = build_menu(!no_engine)?;

    // GTK must be initialized (on the main thread) before building the icon.
    gtk::init().context("initializing GTK (a display server / Wayland session is required)")?;

    // The tray registers as a StatusNotifierItem via libappindicator. GNOME
    // shows it only with the "AppIndicator and KStatusNotifierItem Support"
    // extension enabled; `aural doctor` reports when the host is missing.
    let _tray = TrayIconBuilder::new()
        .with_tooltip("aural — melodic keyboard sounds")
        .with_icon(icon)
        .with_menu(Box::new(menu.menu.clone()))
        .build()
        .context("failed to create the tray icon (no StatusNotifier host? — GNOME needs the AppIndicator extension enabled)")?;

    // Poll muda's menu-event channel from the GTK main loop. (Menu clicks are
    // delivered on the main thread; a 50 ms poll is imperceptible for a menu.)
    let menu_rx = MenuEvent::receiver();
    let last_mtime = std::cell::Cell::new(crate::config::mtime());
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        while let Ok(event) = menu_rx.try_recv() {
            match handle_event(&event) {
                Ok(true) => {
                    gtk::main_quit();
                    return gtk::glib::ControlFlow::Break;
                }
                Ok(false) => {}
                Err(e) => eprintln!("aural menubar: {e:#}"),
            }
        }
        // Keep the Mute checkbox in sync with the shared config: the CLI and
        // the mute hotkey toggle config.json outside this process (the daemon
        // itself hot-reloads it within 500 ms — mirror that here). Gated on
        // mtime so the tick is a stat, not a JSON parse.
        let mt = crate::config::mtime();
        if mt != last_mtime.get() {
            last_mtime.set(mt);
            let muted = crate::config::load().muted;
            if menu.mute.is_checked() != muted {
                menu.mute.set_checked(muted);
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();

    // The loop returned (Quit). Stop the engine (if hosted) and wait for it.
    crate::engine::request_stop();
    if let Some(engine) = engine {
        let _ = engine.join();
    }
    Ok(())
}

// --- shared menu logic ---

/// Handle one menu event; returns `true` when Quit was requested. On both
/// platforms a `CheckMenuItem` auto-toggles itself, so we read the new state
/// and persist it (config for Mute, autostart entry for Login).
fn handle_event(event: &MenuEvent) -> Result<bool> {
    match event.id().0.as_str() {
        ID_MUTE => {
            let now_checked = crate::config::load().muted;
            crate::config::update(|c| c.muted = !now_checked)?;
            // The engine hot-reloads config within 500 ms.
        }
        ID_LOGIN => {
            if crate::daemon::login_enabled() {
                crate::daemon::uninstall()?;
            } else {
                crate::daemon::install()?;
            }
        }
        ID_DOCTOR => {
            spawn_doctor();
        }
        ID_QUIT => {
            return Ok(true);
        }
        other => {
            eprintln!("aural menubar: unknown menu id {other:?}");
        }
    }
    Ok(false)
}

/// True when the running binary lives inside a `.app` bundle (i.e. the packaged
/// Aural.app), so the menubar only appears for the GUI agent, not a bare CLI.
#[cfg(target_os = "macos")]
fn in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.ends_with("MacOS")))
        .unwrap_or(false)
}

/// Point this process's stderr at the aural log file. When launched via
/// `open Aural.app` (macOS) or the XDG autostart entry (Linux), stderr is
/// discarded, so the engine's diagnostics would be invisible; redirecting
/// lets us read them from the log.
fn redirect_stderr_to_log() {
    use std::os::unix::io::AsRawFd;
    let log = crate::config::dir().join("aural.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    else {
        return;
    };
    unsafe {
        libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
    }
}

// --- Cocoa helpers (macOS) ---

/// `NSApplication sharedApplication`; build once per process main thread.
#[cfg(target_os = "macos")]
fn app_main() -> objc2::rc::Retained<objc2_app_kit::NSApplication> {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;
    let mt = MainThreadMarker::new().expect("menubar must run on the main thread");
    let app = NSApplication::sharedApplication(mt);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app
}

/// Make `NSApplication.run()` return (Quit menu item).
#[cfg(target_os = "macos")]
fn stop_ui() {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).stop(None);
    }
}

// --- icon ---

/// Decode the embedded PNG (32x32 RGBA) into a `tray_icon::Icon`.
fn load_icon() -> Result<tray_icon::Icon> {
    let bytes = include_bytes!("../assets/aural-menubar.png");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // EXPAND converts grayscale/palette to RGB and adds an alpha channel, so
    // the output is always 4 bytes/pixel RGBA regardless of the source format.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().context("reading menubar icon PNG")?;
    let mut frame = vec![0; reader.output_buffer_size().expect("raw PNG output")];
    let info = reader
        .next_frame(&mut frame)
        .context("decoding menubar icon PNG")?;
    let width = info.width;
    let height = info.height;
    Ok(tray_icon::Icon::from_rgba(frame, width, height)?)
}

/// The parent `Menu` plus the Mute checkbox handle — the checkbox is kept in
/// sync with the shared config (the CLI and mute hotkey change it outside
/// this process). `include_login` hides "Enable at Login" in dedicated-user
/// mode, where login persistence belongs to the systemd service.
struct AppMenu {
    menu: Menu,
    mute: CheckMenuItem,
}

fn build_menu(include_login: bool) -> Result<AppMenu> {
    let menu = Menu::new();
    let cfg = crate::config::load();

    let mute = CheckMenuItemBuilder::new()
        .id(MenuId(ID_MUTE.to_string()))
        .text("Mute")
        .enabled(true)
        .checked(cfg.muted)
        .build();
    menu.append(&mute)?;

    // "Enable at Login" toggles the XDG autostart entry — meaningful only when
    // this process hosts the engine. In dedicated-user mode login persistence
    // is owned by the systemd service instead.
    if include_login {
        let login = CheckMenuItemBuilder::new()
            .id(MenuId(ID_LOGIN.to_string()))
            .text("Enable at Login")
            .enabled(true)
            .checked(crate::daemon::login_enabled())
            .build();
        menu.append(&login)?;
    }

    menu.append(&PredefinedMenuItem::separator())?;

    let doctor = MenuItemBuilder::new()
        .id(MenuId(ID_DOCTOR.to_string()))
        .text("Open Doctor")
        .enabled(true)
        .build();
    menu.append(&doctor)?;

    let quit = MenuItemBuilder::new()
        .id(MenuId(ID_QUIT.to_string()))
        .text("Quit aural")
        .enabled(true)
        .build();
    menu.append(&quit)?;

    Ok(AppMenu { menu, mute })
}

// --- doctor window (Open Doctor menu item) ---

/// Open `aural doctor` in a visible terminal window so the user actually sees
/// the diagnostics. `aural doctor` is a one-shot that exits, so the script
/// keeps the window open after it finishes.
#[cfg(target_os = "macos")]
fn spawn_doctor() {
    let Some(exe) = std::env::current_exe().ok() else {
        return;
    };
    let exe = exe.display().to_string();
    // A tiny script that runs doctor and keeps the window open afterwards.
    let script = format!(
        "#!/bin/sh\n\"{exe}\" doctor\necho\necho \"--- aural doctor finished (press any key to close) ---\"\nread -r _\n"
    );
    let dir = crate::config::dir();
    let script_path = dir.join("aural-doctor.sh");
    if std::fs::write(&script_path, script).is_err() {
        return;
    }
    // `open -a Terminal <script>` executes the file, so it needs the exec bit.
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
    let _ = std::process::Command::new("open")
        .arg("-a")
        .arg("Terminal")
        .arg(&script_path)
        .spawn();
}

/// Linux: write the same doctor script, then try the common terminal
/// emulators in order (GNOME Terminal first — Fedora/GNOME covers it).
/// When none is found, run doctor directly with output appended to the log so
/// the diagnostics are at least not lost.
#[cfg(target_os = "linux")]
fn spawn_doctor() {
    let Some(exe) = std::env::current_exe().ok() else {
        return;
    };
    let exe = exe.display().to_string();
    let script = format!(
        "#!/bin/sh\n\"{exe}\" doctor\necho\necho \"--- aural doctor finished (press any key to close) ---\"\nread -r _\n"
    );
    let dir = crate::config::dir();
    let _ = std::fs::create_dir_all(&dir);
    let script_path = dir.join("aural-doctor.sh");
    if std::fs::write(&script_path, script).is_err() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
    let script_path = script_path.display().to_string();
    // (terminal binary, arg-prefix before the command) — Ptyxis first (the
    // Fedora 40+ default; its `--` passthrough matches gnome-terminal's)
    for (term, prefix) in [
        ("ptyxis", vec!["--"]),
        ("gnome-terminal", vec!["--"]),
        ("konsole", vec!["-e"]),
        ("xfce4-terminal", vec!["-e"]),
        ("xterm", vec!["-e"]),
    ] {
        if which(term) {
            let mut cmd = std::process::Command::new(term);
            cmd.args(prefix).arg("bash").arg(&script_path);
            if cmd.spawn().is_ok() {
                return;
            }
        }
    }
    // No terminal emulator found: append the diagnostics to the log.
    if let Ok(out) = std::process::Command::new(&exe).arg("doctor").output() {
        let _ = std::fs::write(dir.join("aural-doctor.out"), &out.stdout);
    }
}

/// Is `bin` on PATH? (no external `which` dependency)
#[cfg(target_os = "linux")]
fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(bin).is_file()))
}
