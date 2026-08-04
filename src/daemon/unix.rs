//! POSIX daemon backend: spawn-detached `run --daemon` with stdio redirected to
//! the log file, PID file, `kill(pid, …)` for alive/stop, `flock` for
//! single-instance. Autostart uses a macOS LaunchAgent (Linux will plug systemd
//! here later). All state lives under the platform config dir (`config::dir`).

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::{bail, Result};
use std::fs::{self, File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn pid_path() -> PathBuf {
    crate::config::pid_path()
}

fn log_path() -> PathBuf {
    crate::config::dir().join("aural.log")
}

fn lock_path() -> PathBuf {
    crate::config::dir().join("aural.lock")
}

#[cfg(target_os = "macos")]
fn plist_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents/com.bevry.aural.plist"))
}

/// Read the recorded daemon pid, or None.
fn read_pid() -> Option<u32> {
    fs::read_to_string(pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Process `pid` exists (any owner).
fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn spawn_detached() -> Result<u32> {
    let exe = std::env::current_exe().context("current_exe")?;
    fs::create_dir_all(crate::config::dir())?;
    let out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())?;
    let err = out.try_clone()?;
    let dev_null = OpenOptions::new().read(true).open("/dev/null")?;
    // SAFETY (pre_exec): runs in the forked child before exec; `setsid` is
    // async-signal-safe. Detaches from the controlling terminal so the daemon
    // outlives the launcher and its terminal.
    let child = unsafe {
        Command::new(exe)
            .args(["run", "--daemon"])
            .stdin(Stdio::from(dev_null))
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            })
            .spawn()
            .context("failed to spawn daemon process")?
    };
    Ok(child.id())
}

/// Launch the daemon unless one is already recorded as running.
pub fn start() -> Result<()> {
    if let Some(pid) = read_pid().filter(|&p| alive(p)) {
        bail!("already running (pid {pid})");
    }
    let pid = spawn_detached()?;
    println!("started (pid {pid}, log {})", log_path().display());
    Ok(())
}

/// SIGTERM the recorded daemon.
pub fn stop() -> Result<()> {
    match read_pid() {
        Some(pid) if alive(pid) => {
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            let _ = fs::remove_file(pid_path());
            println!("stopped (pid {pid})");
            Ok(())
        }
        _ => {
            let _ = fs::remove_file(pid_path());
            println!("not running");
            Ok(())
        }
    }
}

/// Some(pid) if the recorded daemon process is alive.
pub fn status() -> Option<u32> {
    read_pid().filter(|&p| alive(p))
}

/// Single-instance guard: `flock(LOCK_EX|LOCK_NB)` on `aural.lock`, held for
/// the process lifetime (auto-released on exit/crash). Returns a kill-proof
/// handle (the fd is kept in an Mmap). Unlike the Windows named mutex, this is
/// per-file not per-name — users with separate config dirs are unaffected.
pub struct SingleInstance {
    _file: File,
}

pub fn acquire_single_instance() -> Result<Option<SingleInstance>> {
    fs::create_dir_all(crate::config::dir())?;
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(lock_path())?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        Ok(Some(SingleInstance { _file: file }))
    } else {
        Ok(None) // EWOULDBLOCK: another instance holds the lock
    }
}

/// No-op on unix: stdio is redirected to the log file at spawn time
/// (`spawn_detached`), unlike Windows where the detached process inherits
/// the console and needs a later redirect.
pub fn redirect_stdio_to_log() {}
// --- autostart (macOS LaunchAgent) ---

/// Register `aural start` as a LaunchAgent (RunAtLoad) — parity with the
/// Windows Run key: starts at login, not immediately.
#[cfg(target_os = "macos")]
pub fn install() -> Result<()> {
    let exe = std::env::current_exe()?.display().to_string();
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
            "        <string>start</string>\n",
            "    </array>\n",
            "    <key>RunAtLoad</key>\n",
            "    <true/>\n",
            "    <key>ProcessType</key>\n",
            "    <string>Background</string>\n",
            "</dict>\n",
            "</plist>\n"
        ),
        exe = xml_escape(&exe)
    );
    let path = plist_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, plist)?;
    println!(
        "installed ({}; will start at login via launchd)",
        path.display()
    );
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<()> {
    let path = plist_path()?;
    if path.exists() {
        fs::remove_file(&path)?;
        println!("removed {}", path.display());
    } else {
        println!("not installed");
    }
    Ok(())
}

/// Minimal XML text escaping for the plist (paths with spaces need none; `&`
/// and `<` in a path would break parsing otherwise).
#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(not(target_os = "macos"))]
pub fn install() -> Result<()> {
    bail!("autostart is not implemented for this platform yet")
}

#[cfg(not(target_os = "macos"))]
pub fn uninstall() -> Result<()> {
    bail!("autostart is not implemented for this platform yet")
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "macos")]
    fn xml_escape_works() {
        assert_eq!(super::xml_escape("a&b<c>d"), "a&amp;b&lt;c&gt;d");
    }
}
