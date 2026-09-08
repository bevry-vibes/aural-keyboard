//! Daemon lifecycle: detached background process, PID file, single-instance
//! guard. Platform backends: `windows` (CreateProcessW detach, named mutex)
//! and `unix` (setsid detach, flock). Login autostart lives in `system`
//! (registry Run key / LaunchAgent / XDG autostart).

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;
