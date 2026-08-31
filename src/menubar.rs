//! macOS menubar (NSStatusItem) shell — `aural menubar`.
//!
//! A tiny menu-bar app that hosts the engine on a worker thread and exposes:
//! `Mute` (check), `Enable at Login` (check), `Open Doctor`, `Quit`. Uses the
//! `tray-icon` crate (thin around AppKit) so nothing here touches the engine;
//! the engine stays shell-agnostic per DESIGN.md D8 ("add tray/GUI without
//! touching any real code").
//!
//! Threading: the engine (`crate::engine::run`) runs on a worker thread, while
//! the main thread runs `NSApplication.run()` (which services all run-loop
//! modes, including the event-tracking mode that NSStatusItem menus run in).
//! Menu clicks are delivered to a `MenuEvent::set_event_handler` closure that
//! runs on the main thread, so every `CheckMenuItem`/`TrayIcon` call stays on
//! the main thread.
//!
//! Note on dependencies: only compiled/used on macOS. The `png` crate decodes
//! the embedded menubar icon once at startup into RGBA for `Icon::from_rgba`.

use anyhow::{Context, Result};

use tray_icon::menu::{
    CheckMenuItemBuilder, Menu, MenuEvent, MenuId, MenuItemBuilder, PredefinedMenuItem,
};
use tray_icon::TrayIconBuilder;

/// Menu item ids handled in `handle_event`.
const ID_MUTE: &str = "mute";
const ID_LOGIN: &str = "login";
const ID_DOCTOR: &str = "doctor";
const ID_QUIT: &str = "quit";

/// Run the engine on a worker thread and drive the menubar on the main thread.
///
/// Blocks the calling (main) thread in `NSApplication.run()` until Quit.
pub fn run() -> Result<()> {
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
    let menu = build_menu()?;

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
    MenuEvent::set_event_handler(Some(|event| {
        if let Err(e) = handle_event(&event) {
            eprintln!("aural menubar: {e:#}");
        }
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

/// Handle one menu event. On macOS a `CheckMenuItem` auto-toggles itself, so we
/// read its new state and persist it (config for Mute, LaunchAgent for Login).
fn handle_event(event: &MenuEvent) -> Result<()> {
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
            // `stop:` makes `NSApplication.run()` return so we can clean up.
            use objc2_app_kit::NSApplication;
            use objc2_foundation::MainThreadMarker;
            if let Some(mtm) = MainThreadMarker::new() {
                NSApplication::sharedApplication(mtm).stop(None);
            }
        }
        other => {
            eprintln!("aural menubar: unknown menu id {other:?}");
        }
    }
    Ok(())
}

/// Open `aural doctor` in a new Terminal window so the user actually sees the
/// diagnostics. `aural doctor` is a one-shot that exits, so the script keeps
/// the window open after it finishes. Uses `open -a Terminal <script>` (plain
/// LaunchServices) rather than AppleScript, so no Automation ("control
/// Terminal") permission is required.
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

/// True when the running binary lives inside a `.app` bundle (i.e. the packaged
/// Aural.app), so the menubar only appears for the GUI agent, not a bare CLI.
fn in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.ends_with("MacOS")))
        .unwrap_or(false)
}

/// Point this process's stderr at the aural log file. When launched via
/// `open Aural.app` (LaunchServices), stderr is discarded, so the engine's
/// diagnostics would be invisible; redirecting lets us read them from the log.
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

// --- Cocoa helpers (objc2) ---

/// `NSApplication sharedApplication`; build once per process main thread.
fn app_main() -> objc2::rc::Retained<objc2_app_kit::NSApplication> {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;
    let mt = MainThreadMarker::new().expect("menubar must run on the main thread");
    let app = NSApplication::sharedApplication(mt);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app
}

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

/// The parent `Menu` (kept alive so its items' ids stay valid for the session).
struct AppMenu {
    menu: Menu,
}

fn build_menu() -> Result<AppMenu> {
    let menu = Menu::new();
    let cfg = crate::config::load();

    let mute = CheckMenuItemBuilder::new()
        .id(MenuId(ID_MUTE.to_string()))
        .text("Mute")
        .enabled(true)
        .checked(cfg.muted)
        .build();
    menu.append(&mute)?;

    let login = CheckMenuItemBuilder::new()
        .id(MenuId(ID_LOGIN.to_string()))
        .text("Enable at Login")
        .enabled(true)
        .checked(crate::daemon::login_enabled())
        .build();
    menu.append(&login)?;

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

    Ok(AppMenu { menu })
}
