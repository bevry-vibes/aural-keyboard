//! POSIX daemon backend: spawn-detached engine with stdio redirected to the
//! log file, PID file, `kill(pid, …)` for alive/stop, `flock` for
//! single-instance. Login autostart lives in `system` (LaunchAgent on macOS,
//! XDG autostart on Linux). All state lives under the platform config dir
//! (`config::dir`).

use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn pid_path() -> PathBuf {
    crate::config::pid_path()
}

fn log_path() -> PathBuf {
    crate::config::dir().join("aural.log")
}

fn lock_path() -> PathBuf {
    crate::config::dir().join("aural.lock")
}

/// Read the recorded engine pid, or None.
fn read_pid() -> Option<u32> {
    fs::read_to_string(pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Process `pid` exists (any owner). EPERM counts as alive: the process is
/// there but we may not signal it (e.g. a daemon running as another user in
/// dedicated-user mode).
fn alive(pid: u32) -> bool {
    match unsafe { libc::kill(pid as i32, 0) } {
        0 => true,
        _ => std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM),
    }
}

/// Spawn the engine detached from this terminal (setsid), stdio → log file.
/// `args` selects the mode: the background daemon (`run --daemon`) or the
/// Linux tray app (`menubar`) started by `aural system install`.
pub fn spawn_detached(args: &[&str]) -> Result<u32> {
    let exe = std::env::current_exe().context("current_exe")?;
    fs::create_dir_all(crate::config::dir())?;
    let out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())?;
    let err = out.try_clone()?;
    let dev_null = OpenOptions::new().read(true).open("/dev/null")?;
    // SAFETY (pre_exec): runs in the forked child before exec; `setsid` is
    // async-signal-safe. Detaches from the controlling terminal so the
    // process outlives the launcher and its terminal.
    let child = unsafe {
        Command::new(exe)
            .args(args)
            // The child must re-evaluate self-disclaim for itself (it is
            // responsible for the event tap); never inherit a stale guard.
            .env_remove("AURAL_DISCLAIMED")
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

/// Launch the background daemon unless one is already recorded as running.
pub fn start() -> Result<()> {
    if let Some(pid) = read_pid().filter(|&p| alive(p)) {
        bail!("already running (pid {pid})");
    }
    let pid = spawn_detached(&["system", "daemon"])?;
    println!("aural: started (pid {pid}, log {})", log_path().display());
    Ok(())
}

/// SIGTERM the recorded engine instance, then wait for the process to exit so
/// an immediate restart (`aural system install`) doesn't race the
/// single-instance lock.
pub fn stop() -> Result<()> {
    match read_pid() {
        Some(pid) if alive(pid) => {
            let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            if rc != 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::EPERM) {
                    bail!(
                        "cannot stop pid {pid}: it runs as another user (dedicated-user mode)\n  \
                         → use systemd instead: sudo systemctl stop aural"
                    );
                }
                return Err(e).context("signaling daemon");
            }
            for _ in 0..30 {
                if !alive(pid) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = fs::remove_file(pid_path());
            println!("aural: stopped (pid {pid})");
            Ok(())
        }
        _ => {
            let _ = fs::remove_file(pid_path());
            println!("aural: not running");
            Ok(())
        }
    }
}

/// Some(pid) if the recorded engine process is alive.
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
