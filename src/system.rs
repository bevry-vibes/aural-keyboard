//! system — `aural system …`: the installed-app lifecycle.
//! The daemon module owns the *process* lifecycle (spawn/stop/status, PID file); this one owns the *app*: the menu-bar/tray entry, starting at login, and — on macOS — the Aural.app bundle that gives the app a stable, grantable identity.
//!
//! What "installed" means per platform:
//!
//! - **macOS** — `~/Applications/Aural.app`: a copy of this binary, bundled (`LSUIElement`, so no Dock icon) and code-signed.
//! The menu-bar app only runs from a bundle, and macOS's Input Monitoring grant keys to the bundle's "Aural" identity — a stable signature keeps the grant valid across reinstalls (DESIGN.md §12).
//! A LaunchAgent starts it at login.
//! - **Linux** — the tray app (`menubar`: engine + StatusNotifierItem icon), started at login by an XDG autostart entry.
//! - **Windows** — the background daemon, started at login by the registry Run key (no tray yet).

#[cfg(any(target_os = "macos", target_os = "windows"))]
use anyhow::bail;
use anyhow::{Context, Result};
use std::path::PathBuf;

#[cfg(target_os = "linux")]
pub mod linux;

// --- install / uninstall ---

/// `aural system install`: stop whatever is running, put the app in place — permissions included — then start it now and at every login.
pub fn install() -> Result<()> {
    // Invoked from within the running app (its menu's Install item): the app obviously exists and is running, so stopping/starting ourselves would be self-destructive — the only thing left is the login registration.
    if crate::daemon::status() == Some(std::process::id()) {
        return enable();
    }
    #[cfg(target_os = "linux")]
    return linux::install();
    #[cfg(target_os = "macos")]
    {
        // The single-instance guard means the app we install must take over from any engine that is already running.
        if crate::daemon::status().is_some() {
            crate::daemon::stop()?;
        }
        println!(
            "aural: installed the app ({})",
            install_app_bundle()?.display()
        );
        enable_login()?;
        // LaunchServices (`open`) attributes the TCC prompt to "Aural" and honors the bundle's LSUIElement (menu-bar agent, no Dock icon).
        let app = app_bundle_path()?;
        let status = std::process::Command::new("open")
            .arg(&app)
            .status()
            .with_context(|| format!("opening {}", app.display()))?;
        if !status.success() {
            bail!("open {} failed", app.display());
        }
        println!("aural: started the app — approve \"Aural\" if macOS asks for Input Monitoring");
        println!("aural:   (System Settings → Privacy & Security → Input Monitoring)");
        println!(
            "aural: if sounds stay silent after granting, rerun `aural system install` \
             (it relaunches the app)"
        );
        Ok(())
    }
    #[cfg(windows)]
    {
        if crate::daemon::status().is_some() {
            crate::daemon::stop()?;
        }
        let exe = install_exe_copy()?;
        println!("aural: installed the app ({})", exe.display());
        enable_login()?;
        crate::daemon::start(&exe)
    }
}

/// `aural system uninstall`: remove the app — the login registration first, so nothing resurrects at the next login.
/// Safe to invoke from the tray's own Uninstall item (macOS/Linux): files are removed before the stop, which kills this process (on macOS the bundle is removed while still executing from it — unlink only).
/// Windows cannot delete a running exe, so it stops first (the tray doesn't run there).
pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "linux")]
    if linux::dedicated_installed() {
        return linux::uninstall();
    }
    #[cfg(target_os = "linux")]
    if !linux::dedicated_installed() {
        // Fallback mode: the autostart entry and engine are user-level.
        // The binary copy needs the elevated pass — remove it as root, hint at it otherwise (the input-group membership stays; other tools may share it).
        // The stop stays last: the tray invokes this on itself.
        if disable_login()? {
            println!("aural: no longer starts at login");
        } else {
            println!("aural: not registered to start at login");
        }
        if std::path::Path::new(linux::INSTALL_BIN).exists() {
            if linux::is_root() {
                std::fs::remove_file(linux::INSTALL_BIN)
                    .with_context(|| format!("removing {}", linux::INSTALL_BIN))?;
                println!("aural: removed {}", linux::INSTALL_BIN);
                println!(
                    "aural: (your input-group membership stays — drop it with: \
                     sudo gpasswd -d $USER input)"
                );
            } else {
                println!("aural: to also remove the installed binary, rerun with sudo:");
                println!("aural:   sudo {} system uninstall", linux::INSTALL_BIN);
            }
        }
        crate::daemon::stop()?;
        return Ok(());
    }
    if disable_login()? {
        println!("aural: no longer starts at login");
    } else {
        println!("aural: not registered to start at login");
    }
    #[cfg(target_os = "macos")]
    if let Ok(app) = app_bundle_path() {
        if app.exists() {
            std::fs::remove_dir_all(&app).with_context(|| format!("removing {}", app.display()))?;
            println!("aural: removed {}", app.display());
        }
    }
    #[cfg(windows)]
    {
        // Windows cannot delete a running exe, so stop before removing.
        crate::daemon::stop()?;
        if let Ok(exe) = installed_exe() {
            if let Some(dir) = exe.parent().filter(|d| d.exists()) {
                std::fs::remove_dir_all(dir)
                    .with_context(|| format!("removing {}", dir.display()))?;
                println!("aural: removed {}", dir.display());
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        crate::daemon::stop()?;
        Ok(())
    }
}

/// `aural system enable`: start the app automatically at login.
pub fn enable() -> Result<()> {
    #[cfg(target_os = "linux")]
    if linux::dedicated_installed() {
        return linux::set_enabled(true);
    }
    enable_login()
}

/// `aural system disable`: don't start the app automatically at login.
pub fn disable() -> Result<()> {
    #[cfg(target_os = "linux")]
    if linux::dedicated_installed() {
        return linux::set_enabled(false);
    }
    if disable_login()? {
        println!("aural: no longer starts at login");
    } else {
        println!("aural: not registered to start at login");
    }
    Ok(())
}

// --- start at login (macOS LaunchAgent / Linux XDG autostart / Windows Run key) ---

/// Register the app to start at login. Also used by the tray's "Enable at Login" checkbox.
pub(crate) fn enable_login() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let program = app_program()?;
        let plist = format!(
            concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
                "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
                "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
                "<plist version=\"1.0\">\n",
                "<dict>\n",
                "    <key>Label</key>\n",
                "    <string>com.bevry.aural</string>\n",
                "    <key>ProgramArguments</key>\n",
                "    <array>\n",
                "        <string>{exe}</string>\n",
                "        <string>system</string>\n",
                "        <string>tray</string>\n",
                "    </array>\n",
                "    <key>RunAtLoad</key>\n",
                "    <true/>\n",
                "</dict>\n",
                "</plist>\n"
            ),
            exe = xml_escape(&program.display().to_string())
        );
        let path = autostart_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, plist)?;
        println!("aural: starts at login ({})", path.display());
    }
    #[cfg(target_os = "linux")]
    {
        // Prefer the stable copy the install made (/usr/local/bin); before installing (or from a manual binary) the current binary stands in.
        let exe = if std::path::Path::new(linux::INSTALL_BIN).exists() {
            linux::INSTALL_BIN.to_string()
        } else {
            std::env::current_exe()?.display().to_string()
        };
        let desktop = desktop_entry(&exe);
        let path = autostart_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, desktop)?;
        println!("aural: starts at login ({})", path.display());
    }
    #[cfg(windows)]
    {
        let exe = installed_exe()?;
        if !exe.exists() {
            bail!("the app is not installed — run `aural system install` first");
        }
        let value = format!("\"{}\" system daemon", exe.display());
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let key = open_run_key()?;
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let r = RegSetValueExW(key, w!("AuralKeyboard"), None, REG_SZ, Some(bytes));
            let _ = RegCloseKey(key);
            r.ok().context("writing Run value")?;
        }
        println!("aural: starts at login (registry Run key)");
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    bail!("starting at login is not implemented for this platform yet");
    Ok(())
}

/// Remove the start-at-login registration. Ok(false) when nothing was registered (uninstall stays idempotent).
pub(crate) fn disable_login() -> Result<bool> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let removed = {
        let path = autostart_path()?;
        if path.exists() {
            std::fs::remove_file(&path).context("removing the autostart entry")?;
            true
        } else {
            false
        }
    };
    #[cfg(windows)]
    let removed = unsafe {
        let key = open_run_key()?;
        let r = RegDeleteValueW(key, w!("AuralKeyboard"));
        let _ = RegCloseKey(key);
        if r == ERROR_FILE_NOT_FOUND {
            false
        } else {
            r.ok().context("deleting Run value")?;
            true
        }
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let removed = bail!("starting at login is not implemented for this platform yet");
    Ok(removed)
}

/// Whether the app is registered to start at login (`aural system doctor`, the tray's "Enable at Login" checkbox state).
pub fn login_enabled() -> bool {
    #[cfg(target_os = "linux")]
    let enabled = {
        if linux::dedicated_installed() {
            linux::service_enabled()
        } else {
            autostart_path().map(|p| p.exists()).unwrap_or_default()
        }
    };
    #[cfg(target_os = "macos")]
    let enabled = autostart_path().map(|p| p.exists()).unwrap_or_default();
    #[cfg(windows)]
    let enabled = unsafe {
        let Ok(key) = open_run_key() else {
            return false;
        };
        let r = RegQueryValueExW(key, w!("AuralKeyboard"), None, None, None, None);
        let _ = RegCloseKey(key);
        r.is_ok()
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let enabled = false;
    enabled
}

/// Whether the app's install artifacts exist — the binary/tray sense of "properly installed" (drives the tray's Install/Uninstall enabled state; the login registration has its own checkbox).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        // The macOS tray only runs from a bundle — ours, or one the user drag-installed from a release zip elsewhere.
        in_app_bundle() || app_bundle_path().map(|p| p.exists()).unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        linux::dedicated_installed()
            || std::path::Path::new(linux::INSTALL_BIN).exists()
            || autostart_path().map(|p| p.exists()).unwrap_or(false)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn autostart_path() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").context("HOME is not set")?;
        Ok(PathBuf::from(home).join("Library/LaunchAgents/com.bevry.aural.plist"))
    }
    #[cfg(target_os = "linux")]
    {
        let config = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })
            .context("neither XDG_CONFIG_HOME nor HOME is set")?;
        Ok(config.join("autostart/com.bevry.aural.desktop"))
    }
}

/// Minimal XML text escaping for the plist (`&` and `<` in a path would break parsing otherwise; paths with spaces need none).
#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape a path for a Desktop Entry `Exec` value (double-quoted per the spec); only `"` and `\` can appear in a path and need escaping.
#[cfg(target_os = "linux")]
fn exec_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The XDG autostart entry contents — the tray app (hosting the engine). Shared by `enable_login` and the Linux install.
#[cfg(target_os = "linux")]
pub(crate) fn desktop_entry(exe: &str) -> String {
    format!(
        concat!(
            "[Desktop Entry]\n",
            "Type=Application\n",
            "Name=aural\n",
            "Comment=System-wide melodic keyboard sounds\n",
            "Exec={exe} system tray\n",
            "Terminal=false\n",
            "X-GNOME-Autostart-enabled=true\n",
            "Categories=Utility;Audio;\n"
        ),
        exe = exec_quote(exe)
    )
}

// --- the app bundle (macOS) ---

/// True when the running binary lives inside a `.app` bundle — the packaged Aural.app (`aural system install`'s, or one drag-installed from a release zip).
#[cfg(target_os = "macos")]
pub fn in_app_bundle() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.ends_with("MacOS")))
        .unwrap_or(false)
}

/// The install location: `~/Applications/Aural.app` — per-user, user-writable, and `open`/LaunchServices find it there.
#[cfg(target_os = "macos")]
fn app_bundle_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Applications").join("Aural.app"))
}

/// What the LaunchAgent should launch: this bundle when we already run inside one (wherever the user mounted it — e.g. /Applications from a release zip), otherwise the installed `~/Applications/Aural.app`.
#[cfg(target_os = "macos")]
fn app_program() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("current_exe")?;
    if in_app_bundle() {
        return Ok(exe);
    }
    let installed = app_bundle_path()?.join("Contents/MacOS/aural");
    if !installed.exists() {
        bail!("the app is not installed — run `aural system install` first");
    }
    Ok(installed)
}

/// Wrap this binary as `~/Applications/Aural.app`.
/// The copy decouples the app from wherever cargo put the binary (so it survives rebuilds/clean), the stable bundle identity keeps the Input Monitoring grant, and the menu-bar app only runs bundled.
/// Same layout `scripts/package-app.sh` produces.
#[cfg(target_os = "macos")]
fn install_app_bundle() -> Result<PathBuf> {
    let app = app_bundle_path()?;
    let contents = app.join("Contents");
    let exe = std::env::current_exe().context("current_exe")?;
    let dest = contents.join("MacOS/aural");
    std::fs::create_dir_all(contents.join("MacOS"))?;
    std::fs::create_dir_all(contents.join("Resources"))?;
    let same = std::fs::canonicalize(&exe).ok() == std::fs::canonicalize(&dest).ok();
    if !same {
        // Unlink before copy: replacing a running binary in place can crash it.
        let _ = std::fs::remove_file(&dest);
        std::fs::copy(&exe, &dest).context("copying the binary into the app")?;
    }
    std::fs::write(contents.join("Info.plist"), info_plist()).context("writing Info.plist")?;
    std::fs::write(
        contents.join("Resources/AppIcon.icns"),
        include_bytes!("../assets/aural-icon.icns"),
    )
    .context("writing the app icon")?;
    codesign(&app)?;
    Ok(app)
}

/// The bundle's Info.plist — `LSUIElement` keeps it a menu-bar agent (no Dock icon).
/// With no command-line arguments the bundled binary runs the menu-bar app, so both `open Aural.app` and the LaunchAgent start the app.
#[cfg(target_os = "macos")]
fn info_plist() -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
            "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
            "<plist version=\"1.0\">\n",
            "<dict>\n",
            "    <key>CFBundleDevelopmentRegion</key>\n    <string>en</string>\n",
            "    <key>CFBundleDisplayName</key>\n    <string>Aural</string>\n",
            "    <key>CFBundleExecutable</key>\n    <string>aural</string>\n",
            "    <key>CFBundleIconFile</key>\n    <string>AppIcon</string>\n",
            "    <key>CFBundleIdentifier</key>\n    <string>com.bevry.aural</string>\n",
            "    <key>CFBundleInfoDictionaryVersion</key>\n    <string>6.0</string>\n",
            "    <key>CFBundleName</key>\n    <string>Aural</string>\n",
            "    <key>CFBundlePackageType</key>\n    <string>APPL</string>\n",
            "    <key>CFBundleShortVersionString</key>\n    <string>{version}</string>\n",
            "    <key>CFBundleVersion</key>\n    <string>{version}</string>\n",
            "    <key>LSMinimumSystemVersion</key>\n    <string>11.0</string>\n",
            "    <key>LSUIElement</key>\n    <true/>\n",
            "</dict>\n",
            "</plist>\n"
        ),
        version = env!("CARGO_PKG_VERSION")
    )
}

/// Code-sign the bundle: prefer the stable "Aural Code Signing" self-signed identity (keeps the TCC grant valid across reinstalls — DESIGN.md §12), then `AURAL_SIGN_IDENTITY`, else ad-hoc (`-`; free, but the grant must be re-made after every reinstall).
#[cfg(target_os = "macos")]
fn codesign(app: &std::path::Path) -> Result<()> {
    let identity = std::env::var("AURAL_SIGN_IDENTITY")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(find_aural_signing_identity)
        .unwrap_or_else(|| "-".to_string());
    let out = std::process::Command::new("codesign")
        .arg("--force")
        .arg("--sign")
        .arg(&identity)
        .arg(app)
        .output()
        .context("running codesign")?;
    if !out.status.success() {
        bail!(
            "codesign failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Is the "Aural Code Signing" identity present in the keychain? (Mirrors `scripts/package-app.sh`.)
#[cfg(target_os = "macos")]
fn find_aural_signing_identity() -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .ok()?;
    let list = String::from_utf8_lossy(&out.stdout);
    list.lines()
        .any(|l| l.contains("Aural Code Signing"))
        .then(|| "Aural Code Signing".to_string())
}

// --- the installed app (Windows): a stable exe copy ---

/// The install location: `%LOCALAPPDATA%\Programs\aural\aural.exe` — decoupled from wherever cargo put the binary, so the login entry survives `cargo clean` and rebuilds.
#[cfg(windows)]
fn installed_exe() -> Result<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local)
        .join("Programs")
        .join("aural")
        .join("aural.exe"))
}

/// Copy this binary to the install location (unlink first — replacing a running exe fails on Windows; running from the copy itself is a no-op).
#[cfg(windows)]
fn install_exe_copy() -> Result<PathBuf> {
    let dest = installed_exe()?;
    let exe = std::env::current_exe().context("current_exe")?;
    let same = std::fs::canonicalize(&dest).is_ok_and(|d| d == exe);
    if !same {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let _ = std::fs::remove_file(&dest);
        std::fs::copy(&exe, &dest).context("copying the binary into place")?;
    }
    Ok(dest)
}

// --- the registry Run key (Windows) ---

#[cfg(windows)]
use windows::core::w;
#[cfg(windows)]
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
#[cfg(windows)]
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

#[cfg(windows)]
fn open_run_key() -> Result<HKEY> {
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            None,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut key,
        )
        .ok()
        .context("opening Run key")?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "macos")]
    fn xml_escape_works() {
        assert_eq!(super::xml_escape("a&b<c>d"), "a&amp;b&lt;c&gt;d");
    }
}
