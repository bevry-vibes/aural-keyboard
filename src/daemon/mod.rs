//! Daemon lifecycle: detached background process, PID file, single-instance,
//! login autostart. Platform backends: `windows` (CreateProcessW detach, mutex,
//! Registry Run key) and `unix` (setsid detach, flock; LaunchAgent on macOS,
//! XDG autostart on Linux).

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;
