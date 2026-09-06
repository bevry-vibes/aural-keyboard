//! Global keyboard capture: platform backends (`windows`, `macos`, `linux`) over the shared
//! key→trigger logic in this module. The callback only translates keys and pushes
//! `Trigger`s — it never touches the audio API (DESIGN.md D6). Also owns the global
//! mute hotkey on the same thread.
//!
//! Key identity is the Windows VK code (see `mapping`); each backend translates its
//! native keycodes to VKs before calling [`handle_key`].

use crate::mapping;
use crate::mixer::{SharedFlags, Trigger};
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::{listen_access_granted, spawn, stop, HookHandle};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{spawn, stop, HookHandle};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::{listen_access_granted, spawn, stop, HookHandle};

static HOOK_TX: OnceLock<Sender<Trigger>> = OnceLock::new();
static HOOK_FLAGS: OnceLock<Arc<SharedFlags>> = OnceLock::new();
static LOG_KEYS: AtomicBool = AtomicBool::new(false);
/// Whether caps lock is currently engaged. The sound must reflect the capital
/// actually written, so this inverts the letter register (see
/// `mapping::effective_shift`). Backends seed it from ground truth at spawn
/// (macOS session flags, Linux `EVIOCGLED`, Windows `GetKeyState`) and keep it
/// synced where the OS reports the state (macOS `flagsChanged`, Linux `EV_LED`);
/// the toggle in [`handle_key`] covers platforms/sessions that report neither.
static CAPS_LOCK: AtomicBool = AtomicBool::new(false);

thread_local! {
    // Pressed-key table (VK codes are bytes): shift-state tracking + autorepeat dedup.
    static PRESSED: std::cell::Cell<[bool; 256]> = const { std::cell::Cell::new([false; 256]) };
}

/// Called once by each backend's hook thread before installing the platform hook.
pub(crate) fn init(tx: Sender<Trigger>, flags: Arc<SharedFlags>) {
    let _ = HOOK_TX.set(tx);
    let _ = HOOK_FLAGS.set(flags);
    LOG_KEYS.store(std::env::var_os("AURAL_LOG").is_some(), Ordering::Relaxed);
}

pub(crate) fn log_keys_enabled() -> bool {
    LOG_KEYS.load(Ordering::Relaxed)
}

/// Whether the shared pressed-table currently tracks `vk` as held
/// (macOS uses it to reconcile caps lock; Linux for the hotkey chord check).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn is_pressed(vk: u8) -> bool {
    PRESSED.with(|p| p.get()[vk as usize])
}

/// Sync caps-lock state to the OS-reported truth (macOS `flagsChanged`, Linux
/// `EV_LED`, Windows `GetKeyState` at spawn). Beats a desynced toggle.
pub(crate) fn set_caps_lock(on: bool) {
    if CAPS_LOCK.swap(on, Ordering::Relaxed) != on && log_keys_enabled() {
        eprintln!("aural: caps lock {}", if on { "on" } else { "off" });
    }
}

/// Flip caps-lock state (fallback where the OS does not report it).
fn toggle_caps_lock() {
    let on = !CAPS_LOCK.load(Ordering::Relaxed);
    set_caps_lock(on);
}

pub(crate) fn caps_lock_on() -> bool {
    CAPS_LOCK.load(Ordering::Relaxed)
}

/// Shared key handling: pressed-table upkeep, autorepeat dedup, mapping, trigger send.
pub(crate) fn handle_key(vk: u8, is_up: bool) {
    // Track pressed keys (shift state + OS autorepeat dedup) before mapping.
    let (already_down, shift_down) = PRESSED.with(|p| {
        let mut keys = p.get();
        let already = if is_up {
            keys[vk as usize] = false;
            false
        } else {
            std::mem::replace(&mut keys[vk as usize], true)
        };
        p.set(keys);
        let shift = keys[mapping::VK_SHIFT as usize]
            || keys[mapping::VK_LSHIFT as usize]
            || keys[mapping::VK_RSHIFT as usize];
        (already, shift)
    });

    if is_up {
        // NoteOff for any non-modifier key; the mixer ignores keys with no voice.
        if !mapping::is_modifier(vk) {
            if let Some(tx) = HOOK_TX.get() {
                let _ = tx.send(Trigger::NoteOff { key: vk });
            }
        }
        return;
    }
    if already_down {
        return; // held-key autorepeat: original ignores these
    }
    // Caps lock toggles on its key-down (deduped above, so holds/repeats flip once).
    // Backends override this with OS-reported truth when they have it.
    if vk == mapping::VK_CAPITAL {
        toggle_caps_lock();
    }
    // The sound reflects the capital actually written: caps lock swaps the letter
    // registers (plain → high, shift → plain); non-letters keep their shift sounds.
    let caps = caps_lock_on();
    let shift = mapping::effective_shift(vk, shift_down, caps);
    if let Some(note) = mapping::map_key(vk, shift) {
        if log_keys_enabled() {
            eprintln!(
                "aural: key {vk:#04x} (shift={shift_down}, caps={caps}) → {:?} midi {} vel {}",
                note.instrument, note.midi, note.velocity
            );
        }
        if let Some(tx) = HOOK_TX.get() {
            let _ = tx.send(Trigger::NoteOn {
                key: vk,
                instrument: note.instrument,
                midi: note.midi,
                velocity: note.velocity,
                at: Instant::now(),
            });
        }
    }
}

/// Testing aid (`aural run --stdin`): turn stdin characters into key presses
/// — no OS hook, no permissions. On a TTY, stdin is switched to
/// character-at-a-time mode so keys play as typed; piped input is read by
/// line (Enter plays Return after each). Uppercase and shifted punctuation
/// are sent as shift chords, exercising both registers. EOF stops the engine.
pub fn spawn_stdin_reader(tx: Sender<Trigger>, flags: Arc<SharedFlags>) {
    init(tx, flags);
    std::thread::spawn(move || {
        #[cfg(unix)]
        let char_mode = stdin_char_mode();
        #[cfg(not(unix))]
        let char_mode = false;
        if char_mode {
            eprintln!("aural: terminal detected — keys play as you type (Ctrl+C to quit)");
            read_stdin_bytes();
        } else {
            read_stdin_lines();
        }
        crate::engine::request_stop(); // EOF (piped input): end the run
    });
}

fn play_char(c: char) {
    if c == '\n' || c == '\r' {
        handle_key(mapping::VK_RETURN, false);
        handle_key(mapping::VK_RETURN, true);
        return;
    }
    let Some((vk, shift)) = mapping::vk_for_char(c) else {
        return;
    };
    if shift {
        handle_key(mapping::VK_LSHIFT, false);
    }
    handle_key(vk, false);
    handle_key(vk, true);
    if shift {
        handle_key(mapping::VK_LSHIFT, true);
    }
}

fn read_stdin_lines() {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        for c in line.chars() {
            play_char(c);
        }
        play_char('\n');
    }
}

fn read_stdin_bytes() {
    use std::io::Read;
    let stdin = std::io::stdin();
    for b in stdin.lock().bytes() {
        let Ok(b) = b else { break };
        if b < 0x80 {
            play_char(b as char);
        }
    }
}

#[cfg(unix)]
static ORIG_TERMIOS: std::sync::OnceLock<libc::termios> = std::sync::OnceLock::new();

#[cfg(unix)]
extern "C" fn restore_termios() {
    if let Some(t) = ORIG_TERMIOS.get() {
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, t) };
    }
}

/// Switch a TTY stdin to character-at-a-time mode (canonical input off; echo
/// and Ctrl+C signalling kept; original settings restored via `atexit`).
/// Returns false when stdin is not a TTY (e.g. piped) → caller uses line mode.
#[cfg(unix)]
fn stdin_char_mode() -> bool {
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
            return false;
        }
        let _ = ORIG_TERMIOS.set(t);
        libc::atexit(restore_termios);
        t.c_lflag &= !(libc::ICANON as libc::tcflag_t);
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t) == 0
    }
}

/// Shared mute toggle (Windows `WM_HOTKEY` arm / macOS in-tap chord match).
pub(crate) fn toggle_mute() {
    if let Some(f) = HOOK_FLAGS.get() {
        let now = !f.muted.load(Ordering::Relaxed);
        f.muted.store(now, Ordering::Relaxed);
        let _ = crate::config::update(|c| c.muted = now);
        eprintln!("aural: {}", if now { "muted" } else { "unmuted" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::SharedFlags;

    fn note_ons(rx: &crossbeam_channel::Receiver<Trigger>) -> Vec<Trigger> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, Trigger::NoteOn { .. }) {
                out.push(ev);
            }
        }
        out
    }

    fn midi_of(t: &Trigger) -> u8 {
        match t {
            Trigger::NoteOn { midi, .. } => *midi,
            _ => panic!("expected NoteOn, got {t:?}"),
        }
    }

    #[test]
    fn caps_lock_swaps_letter_registers_end_to_end() {
        let (tx, rx) = crossbeam_channel::unbounded();
        init(tx, Arc::new(SharedFlags::new(1.0, false)));
        set_caps_lock(false); // isolate from any prior state
        let _ = note_ons(&rx);

        // Caps on (its own key press plays the default drum, like the original).
        handle_key(mapping::VK_CAPITAL, false);
        assert_eq!(midi_of(&note_ons(&rx)[0]), 45);
        handle_key(mapping::VK_CAPITAL, true);
        let _ = note_ons(&rx);

        // Plain letter under caps → the shifted (high) register.
        handle_key(mapping::VK_A, false);
        handle_key(mapping::VK_A, true);
        assert_eq!(midi_of(&note_ons(&rx)[0]), 82);

        // Shift+letter under caps → the plain (low) register.
        handle_key(mapping::VK_LSHIFT, false);
        handle_key(mapping::VK_A, false);
        handle_key(mapping::VK_A, true);
        handle_key(mapping::VK_LSHIFT, true);
        assert_eq!(midi_of(&note_ons(&rx)[0]), 62);

        // Non-letters keep their shift sounds under caps ('!' still crashes).
        handle_key(mapping::VK_1, false);
        handle_key(mapping::VK_1, true);
        let _ = note_ons(&rx);
        handle_key(mapping::VK_LSHIFT, false);
        handle_key(mapping::VK_1, false);
        handle_key(mapping::VK_1, true);
        handle_key(mapping::VK_LSHIFT, true);
        assert_eq!(midi_of(&note_ons(&rx)[0]), 54);

        // Caps off restores the original registers.
        handle_key(mapping::VK_CAPITAL, false);
        handle_key(mapping::VK_CAPITAL, true);
        let _ = note_ons(&rx);
        handle_key(mapping::VK_A, false);
        handle_key(mapping::VK_A, true);
        assert_eq!(midi_of(&note_ons(&rx)[0]), 62);
        set_caps_lock(false);
    }
}
