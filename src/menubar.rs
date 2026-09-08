//! menubar — the menu-bar (macOS) / system-tray (Linux) app: the internal
//! `aural system tray` command (installed by `aural system install`, not
//! user-facing). Hosts the engine on a worker thread and exposes `Mute`
//! (check), `Enable at Login` (check), the install items,
//! `Install Aural` (greyed when installed), `Uninstall Aural` (greyed when
//! not), `Open Doctor`, and `Quit` — unless the single-instance lock is held by
//! another instance (Linux dedicated-user mode, where the engine runs as the
//! `aural` system user under systemd), in which case it is a control surface
//! (Mute/Open Doctor/Quit) sharing the daemon's config.
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
const ID_INSTALL: &str = "install";
const ID_UNINSTALL: &str = "uninstall";
const ID_DOCTOR: &str = "doctor";
const ID_QUIT: &str = "quit";

/// Should this tray host the engine? Probes the single-instance lock: free →
/// we host; held by another instance (the dedicated-user daemon) → control
/// surface only. The probe acquires and immediately releases — the engine
/// thread does the real acquire moments later; if something slips in between,
/// the engine fails with "already running" and the tray degrades to a control
/// surface anyway.
#[cfg(target_os = "linux")]
fn probe_hostable() -> bool {
    crate::daemon::acquire_single_instance()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}

/// Spawn the engine on its worker thread (stderr → log, failures logged —
/// when launched via `open`/autostart, stderr is otherwise discarded).
fn spawn_engine() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        redirect_stderr_to_log();
        if let Err(e) = crate::engine::run(false, false, None) {
            eprintln!("aural tray: engine error: {e:#}");
        }
    })
}

// --- macOS shell (NSStatusItem via AppKit) ---

#[cfg(target_os = "macos")]
pub fn run() -> Result<()> {
    // The menubar is a UI shell that only makes sense inside the packaged
    // Aural.app bundle (LSUIElement agent). Refuse to run from a bare binary so
    // the status item doesn't appear for terminal/CLI usage.
    if !in_app_bundle() {
        anyhow::bail!(
            "aural system tray: only runs from the installed Aural.app\n  \
             → install it with `aural system install`"
        );
    }
    let engine = spawn_engine();

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
        Err(e) => eprintln!("aural tray: {e:#}"),
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
pub fn run() -> Result<()> {
    // Dedicated-user mode auto-detect: when another instance holds the
    // single-instance lock (the engine running as the `aural` system user via
    // systemd), this tray is a control surface only — Mute works through the
    // shared AURAL_CONFIG_DIR, and the lifecycle menu items hide (systemd
    // owns them).
    let hosted = probe_hostable();
    let engine = if hosted {
        Some(spawn_engine())
    } else {
        eprintln!("aural tray: another aural instance is running — control surface only");
        None
    };

    let icon = load_icon().context("loading tray icon")?;
    let menu = build_menu(hosted)?;

    // GTK must be initialized (on the main thread) before building the icon.
    gtk::init().context("initializing GTK (a display server / Wayland session is required)")?;

    // The tray registers as a StatusNotifierItem via libappindicator. GNOME
    // shows it only with the "AppIndicator and KStatusNotifierItem Support"
    // extension enabled; `aural system doctor` reports when the host is missing.
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
            let was_install = event.id().0 == ID_INSTALL || event.id().0 == ID_UNINSTALL;
            match handle_event(&event) {
                Ok(true) => {
                    gtk::main_quit();
                    return gtk::glib::ControlFlow::Break;
                }
                Ok(false) => {}
                Err(e) => eprintln!("aural tray: {e:#}"),
            }
            // muda auto-toggled the clicked checkbox; reality gets the final
            // say (an uninstall here usually kills the process anyway).
            if was_install {
                menu.sync_install_state();
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
            if crate::system::login_enabled() {
                crate::system::disable_login()?;
            } else {
                crate::system::enable_login()?;
            }
        }
        ID_INSTALL => {
            // Self-aware: when this tray is the running instance (the usual
            // case), install reduces to ensuring the login registration.
            crate::system::install()?;
        }
        ID_UNINSTALL => {
            // Removes the login registration and app files, then stops the
            // running instance — this process — so we usually never return;
            // Ok(true) quits the UI if we somehow do.
            crate::system::uninstall()?;
            return Ok(true);
        }
        ID_DOCTOR => {
            spawn_doctor();
        }
        ID_QUIT => {
            return Ok(true);
        }
        other => {
            eprintln!("aural tray: unknown menu id {other:?}");
        }
    }
    Ok(false)
}

/// True when the running binary lives inside a `.app` bundle (i.e. the packaged
/// Aural.app), so the menubar only appears for the GUI agent, not a bare CLI.
#[cfg(target_os = "macos")]
fn in_app_bundle() -> bool {
    crate::system::in_app_bundle()
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

/// The parent `Menu` plus the checkbox handles kept in sync with the outside
/// world: `mute` follows the shared config (the CLI and mute hotkey change it
/// outside this process), and the install items follow the install state
/// (muda auto-toggles a clicked checkbox, so reality gets the final say via
/// `sync_install_state`). `hosted` = this tray hosts the engine, so the whole
/// app lifecycle belongs in the menu; in dedicated-user mode the systemd
/// service owns install/login, so those items are hidden.
struct AppMenu {
    menu: Menu,
    // macOS never reads these handles (AppKit auto-toggles, and Uninstall
    // kills the process); they are kept so both platforms share the
    // construction below and Linux can reconcile after clicks.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    mute: CheckMenuItem,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    install: CheckMenuItem,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    uninstall: CheckMenuItem,
}

impl AppMenu {
    /// Re-check/grey the install items to match reality: the checked-and-
    /// greyed one names the current state (Install ✓ = installed, Uninstall ✓
    /// = uninstalled), the enabled unchecked one is the actionable transition.
    /// (Linux only: muda auto-toggles a clicked checkbox; macOS's AppKit
    /// toggles too, but there the items can't change state — Install is
    /// disabled whenever installed, and Uninstall quits the process.)
    #[cfg(target_os = "linux")]
    fn sync_install_state(&self) {
        let installed = crate::system::installed();
        self.install.set_checked(installed);
        self.install.set_enabled(!installed);
        self.uninstall.set_checked(!installed);
        self.uninstall.set_enabled(installed);
    }
}

fn build_menu(hosted: bool) -> Result<AppMenu> {
    let menu = Menu::new();
    let cfg = crate::config::load();

    let mute = CheckMenuItemBuilder::new()
        .id(MenuId(ID_MUTE.to_string()))
        .text("Mute")
        .enabled(true)
        .checked(cfg.muted)
        .build();
    menu.append(&mute)?;

    // "Enable at Login" toggles the autostart entry — meaningful only when
    // this process hosts the engine. In dedicated-user mode login persistence
    // is owned by the systemd service instead.
    if hosted {
        let login = CheckMenuItemBuilder::new()
            .id(MenuId(ID_LOGIN.to_string()))
            .text("Enable at Login")
            .enabled(true)
            .checked(crate::system::login_enabled())
            .build();
        menu.append(&login)?;
    }

    menu.append(&PredefinedMenuItem::separator())?;

    let (install, uninstall) = if hosted {
        // Check items doubling as state indicators (see `sync_install_state`).
        let installed = crate::system::installed();
        let install = CheckMenuItemBuilder::new()
            .id(MenuId(ID_INSTALL.to_string()))
            .text("Install Aural")
            .enabled(!installed)
            .checked(installed)
            .build();
        menu.append(&install)?;

        let uninstall = CheckMenuItemBuilder::new()
            .id(MenuId(ID_UNINSTALL.to_string()))
            .text("Uninstall Aural")
            .enabled(installed)
            .checked(!installed)
            .build();
        menu.append(&uninstall)?;
        (install, uninstall)
    } else {
        // Dedicated-user mode: the items are hidden; placeholder handles keep
        // the construction uniform (never read).
        let placeholder = || {
            CheckMenuItemBuilder::new()
                .id(MenuId(String::new()))
                .text("")
                .enabled(false)
                .build()
        };
        (placeholder(), placeholder())
    };

    let doctor = MenuItemBuilder::new()
        .id(MenuId(ID_DOCTOR.to_string()))
        .text("Open Doctor")
        .enabled(true)
        .build();
    menu.append(&doctor)?;

    let quit = MenuItemBuilder::new()
        .id(MenuId(ID_QUIT.to_string()))
        .text("Quit Aural")
        .enabled(true)
        .build();
    menu.append(&quit)?;

    Ok(AppMenu {
        menu,
        mute,
        install,
        uninstall,
    })
}

// --- doctor window (Open Doctor menu item) ---

/// Open `aural system doctor` in a visible terminal window so the user actually sees
/// the diagnostics. `aural system doctor` is a one-shot that exits, so the script
/// keeps the window open after it finishes.
#[cfg(target_os = "macos")]
fn spawn_doctor() {
    let Some(exe) = std::env::current_exe().ok() else {
        return;
    };
    let exe = exe.display().to_string();
    // A tiny script that runs doctor and keeps the window open afterwards.
    let script = format!(
        "#!/bin/sh\n\"{exe}\" system doctor\necho\necho \"--- aural system doctor finished (press any key to close) ---\"\nread -r _\n"
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
        "#!/bin/sh\n\"{exe}\" system doctor\necho\necho \"--- aural system doctor finished (press any key to close) ---\"\nread -r _\n"
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
    if let Ok(out) = std::process::Command::new(&exe)
        .args(["system", "doctor"])
        .output()
    {
        let _ = std::fs::write(dir.join("aural-doctor.out"), &out.stdout);
    }
}

/// Is `bin` on PATH? (no external `which` dependency)
#[cfg(target_os = "linux")]
fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(bin).is_file()))
}
