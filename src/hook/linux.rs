//! Linux backend: listen-only evdev readers over `/dev/input/event*`
//! (DESIGN.md D3 — native capture, no rdev). One reader per keyboard-class
//! device, driven by `poll(2)` for low latency; devices are *not* grabbed
//! (pass-through parity with the Windows LL hook / macOS listen-only tap), so
//! nothing is ever blocked or consumed.
//!
//! evdev sits below the display server, so the hook works identically under
//! X11 and Wayland — the only global-capture route that does. The price is
//! permission: reading `/dev/input/event*` requires the `input` group (checked
//! by `aural doctor` and reported by [`spawn`] on failure).
//!
//! Hotplug: the device set is rescanned every 5 s, so keyboards connected
//! later are picked up without a restart.

use super::{handle_key, init, is_pressed, toggle_mute};
use crate::keycodes;
use crate::mapping;
use crate::mixer::{SharedFlags, Trigger};
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use evdev::{Device, EventSummary, KeyCode};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// evdev key-event values.
const KEY_VALUE_UP: i32 = 0;
const KEY_VALUE_DOWN: i32 = 1;
// value 2 = kernel autorepeat; `handle_key`'s pressed-table dedups these too,
// but they are skipped explicitly for clarity/zero cost.

/// Config hotkey, packed `(mods << 32) | vk` — 0 disables (parity with macOS).
static HOTKEY: AtomicU64 = AtomicU64::new(0);
/// Set by [`stop`] so the poll loop exits within one timeout tick.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Poll timeout (ms): bounds both shutdown latency and hotplug rescan delay.
const POLL_TIMEOUT_MS: libc::c_int = 250;
/// Hotplug rescan cadence (keyboards plugged in later are picked up here).
const RESCAN_EVERY: Duration = Duration::from_secs(5);

pub struct HookHandle {
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Display for HookHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("evdev")
    }
}

/// True when this process can read evdev devices (i.e. it is in the `input`
/// group, or root). Used by `aural doctor` and mirrors the macOS TCC preflight.
pub fn listen_access_granted() -> bool {
    first_keyboard_device().is_some()
}

/// Open the first keyboard-class device we can, or None (used for the
/// permission preflight; the real hook opens every keyboard device).
fn first_keyboard_device() -> Option<(PathBuf, Device)> {
    for path in event_device_paths() {
        if let Ok(device) = Device::open(&path) {
            if is_keyboard(&device) {
                return Some((path, device));
            }
        }
    }
    None
}

/// All `/dev/input/event*` paths.
fn event_device_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("event") && name[5..].bytes().all(|b| b.is_ascii_digit()) {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    out
}

/// Only real keyboards: devices that can produce both A and Space keys.
/// Excludes power buttons, headphone jacks (KEY_PLAYPAUSE), etc.
fn is_keyboard(device: &Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|k| k.contains(KeyCode::KEY_A) && k.contains(KeyCode::KEY_SPACE))
}

/// Open every accessible keyboard-class device. Open failures are tolerated
/// and reported (e.g. unreadable nodes when the user is not in the `input`
/// group); the caller decides what to do with zero devices.
fn open_keyboards() -> (Vec<(PathBuf, Device)>, Vec<String>) {
    let mut opened = Vec::new();
    let mut failures = Vec::new();
    for path in event_device_paths() {
        match Device::open(&path) {
            Ok(device) => {
                if is_keyboard(&device) {
                    opened.push((path, device));
                }
            }
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    (opened, failures)
}

// --- spawn / stop (parity with the macOS backend's signature) ---

/// Spawn the hook thread: open all keyboard evdev devices and run the poll
/// loop until [`stop`] is called. Fails fast if no keyboard device can be
/// opened — almost always missing `input` group membership.
pub fn spawn(
    tx: Sender<Trigger>,
    flags: Arc<SharedFlags>,
    hotkey: Option<(u32, u32)>,
) -> Result<HookHandle> {
    HOTKEY.store(
        hotkey
            .map(|(mods, vk)| ((mods as u64) << 32) | vk as u64)
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    SHUTDOWN.store(false, Ordering::Relaxed);

    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<()>>(0);
    let thread = std::thread::Builder::new()
        .name("aural-hook-evdev".into())
        .spawn(move || {
            init(tx, flags);
            let (mut devices, failures) = open_keyboards();
            if devices.is_empty() {
                let mut msg = String::from(
                    "no evdev keyboard devices could be opened (checked /dev/input/event*)",
                );
                if let Some(first) = failures.first() {
                    msg.push_str(&format!("\n  first failure: {first}"));
                }
                msg.push_str(
                    "\n  → either add your user to the input group (log out & back in):\n    \
                     sudo usermod -aG input $USER\n  \
                     or run the daemon as a dedicated `aural` user instead (anything running\n    \
                     as you stays locked out of input):\n    \
                     sudo ./scripts/setup-dedicated-user.sh   (see README: Dedicated-user mode)",
                );
                ready_tx.send(Err(anyhow::anyhow!(msg))).ok();
                return;
            }
            for (_, device) in &mut devices {
                let _ = device.set_nonblocking(true);
            }
            eprintln!(
                "aural: watching {} keyboard device{} via evdev (listen-only){}",
                devices.len(),
                if devices.len() == 1 { "" } else { "s" },
                if failures.is_empty() {
                    String::new()
                } else {
                    format!(", {} other device(s) unreadable", failures.len())
                }
            );
            ready_tx.send(Ok(())).ok();
            poll_loop(&mut devices);
        })
        .context("spawning evdev hook thread")?;

    ready_rx
        .recv()
        .context("evdev hook thread exited before signaling readiness")?
        .context("opening evdev keyboard devices")?;

    Ok(HookHandle {
        thread: Some(thread),
    })
}

pub fn stop(mut handle: HookHandle) {
    SHUTDOWN.store(true, Ordering::Relaxed);
    if let Some(thread) = handle.thread.take() {
        let _ = thread.join();
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        // Safety net: never leak a running hook thread (the engine stops first).
        if self.thread.is_some() {
            SHUTDOWN.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

// --- poll loop ---

/// Drive all open devices from one thread via `poll(2)`, rescanning for
/// hotplug devices every [`RESCAN_EVERY`]. Runs until [`SHUTDOWN`] is set.
fn poll_loop(devices: &mut Vec<(PathBuf, Device)>) {
    let mut last_rescan = Instant::now();
    while !SHUTDOWN.load(Ordering::Relaxed) {
        // Raw fd copies (no borrow held across the mutable event fetches).
        let mut fds: Vec<libc::pollfd> = devices
            .iter()
            .map(|(_, d)| libc::pollfd {
                fd: d.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        let rc =
            unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, POLL_TIMEOUT_MS) };
        if rc > 0 {
            let dead_fds: Vec<i32> = fds
                .iter()
                .filter(|p| p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0)
                .map(|p| p.fd)
                .collect();
            for (i, pfd) in fds.iter().enumerate() {
                if pfd.revents & libc::POLLIN != 0 {
                    // Copy the path out first: `fetch_events` mutably borrows
                    // the device and the `Result` temporary holds that borrow
                    // through the match arms.
                    let path = devices[i].0.display().to_string();
                    match devices[i].1.fetch_events() {
                        Ok(events) => {
                            for event in events {
                                process(event);
                            }
                        }
                        // EAGAIN (drained) is benign; other errors mark the
                        // device gone (unplugged) for removal below.
                        Err(e) if e.raw_os_error() != Some(libc::EAGAIN) => {
                            eprintln!("aural: keyboard device {path} gone ({e}); will rescan");
                        }
                        Err(_) => {}
                    }
                }
            }
            if !dead_fds.is_empty() {
                devices.retain(|(_, d)| !dead_fds.contains(&d.as_raw_fd()));
            }
        }
        if last_rescan.elapsed() >= RESCAN_EVERY {
            last_rescan = Instant::now();
            rescan(devices);
        }
    }
}

/// Open any newly-appeared keyboard devices (hotplug).
fn rescan(devices: &mut Vec<(PathBuf, Device)>) {
    let known: std::collections::HashSet<PathBuf> =
        devices.iter().map(|(p, _)| p.clone()).collect();
    for path in event_device_paths() {
        if known.contains(&path) {
            continue;
        }
        if let Ok(device) = Device::open(&path) {
            if is_keyboard(&device) {
                let _ = device.set_nonblocking(true);
                eprintln!("aural: keyboard device {} attached", path.display());
                devices.push((path, device));
            }
        }
    }
}

/// One evdev event → shared key handling (and the mute-hotkey chord check).
fn process(event: evdev::InputEvent) {
    let (code, value) = match event.destructure() {
        EventSummary::Key(_key_event, code, value) => (code.0, value),
        _ => return, // SYN/REL/ABS/etc. — only KEY events matter to us
    };
    if !keycodes::is_evdev_key(code) {
        return; // mouse buttons / touch / misc — stay silent like LL hooks do
    }
    let vk = keycodes::vk_for_evdev(code);
    match value {
        KEY_VALUE_DOWN => {
            if is_hotkey(vk) {
                return; // chord matched: toggle fired, key stays silent (macOS parity)
            }
            handle_key(vk, false);
        }
        KEY_VALUE_UP => handle_key(vk, true),
        _ => {} // kernel autorepeat: the original ignores held keys
    }
}

/// Mute-hotkey chord check: matching key down with exactly the configured
/// modifier mask, resolved from the shared pressed-table (evdev reports the
/// modifier presses themselves, like the macOS tap's flagsChanged).
fn is_hotkey(vk: u8) -> bool {
    let encoded = HOTKEY.load(Ordering::Relaxed);
    if encoded == 0 {
        return false;
    }
    let (want_mods, want_vk) = ((encoded >> 32) as u32, encoded as u8);
    if vk != want_vk {
        return false;
    }
    let mut have = 0u32;
    if is_pressed(mapping::VK_LSHIFT) || is_pressed(mapping::VK_RSHIFT) {
        have |= 0x0004; // MOD_SHIFT
    }
    if is_pressed(mapping::VK_LCONTROL) || is_pressed(mapping::VK_RCONTROL) {
        have |= 0x0002; // MOD_CONTROL
    }
    if is_pressed(mapping::VK_LMENU) || is_pressed(mapping::VK_RMENU) {
        have |= 0x0001; // MOD_ALT
    }
    if is_pressed(mapping::VK_LWIN) || is_pressed(mapping::VK_RWIN) {
        have |= 0x0008; // MOD_WIN
    }
    if have != want_mods {
        return false;
    }
    toggle_mute();
    true
}
