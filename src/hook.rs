//! Windows global keyboard capture: `WH_KEYBOARD_LL` on a dedicated message-loop
//! thread. The callback only translates keys and pushes `Trigger`s — it never
//! touches the audio API (DESIGN.md D6). Also owns the global mute hotkey on the
//! same thread's message loop.

use crate::mapping;
use crate::mixer::{SharedFlags, Trigger};
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::*;

static HOOK_TX: OnceLock<Sender<Trigger>> = OnceLock::new();
static HOOK_FLAGS: OnceLock<Arc<SharedFlags>> = OnceLock::new();
static LOG_KEYS: AtomicBool = AtomicBool::new(false);

thread_local! {
    // Pressed-key table (VK codes are bytes): shift-state tracking + autorepeat dedup.
    static PRESSED: std::cell::Cell<[bool; 256]> = const { std::cell::Cell::new([false; 256]) };
}

const HOTKEY_ID: i32 = 1;

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = kb.vkCode as u8;
        let is_up = kb.flags.0 & LLKHF_UP.0 != 0;
        handle_key(vk, is_up);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn handle_key(vk: u8, is_up: bool) {
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
    if let Some(note) = mapping::map_key(vk, shift_down) {
        if LOG_KEYS.load(Ordering::Relaxed) {
            eprintln!(
                "aural: key {vk:#04x} (shift={shift_down}) → {:?} midi {} vel {}",
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

/// Spawn the hook thread and return its Win32 thread id (for `stop`).
/// The thread installs the LL hook + mute hotkey and pumps messages.
pub fn spawn(tx: Sender<Trigger>, flags: Arc<SharedFlags>, hotkey: Option<(u32, u32)>) -> u32 {
    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<u32>(0);
    LOG_KEYS.store(std::env::var_os("AURAL_LOG").is_some(), Ordering::Relaxed);
    std::thread::spawn(move || {
        let _ = HOOK_TX.set(tx);
        let _ = HOOK_FLAGS.set(flags);
        unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                None, // LL hooks run in our own thread context
                0,
            )
            .expect("SetWindowsHookExW(WH_KEYBOARD_LL) failed");
            if let Some((mods, vk)) = hotkey {
                let _ = RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(mods), vk);
            }
            ready_tx
                .send(windows::Win32::System::Threading::GetCurrentThreadId())
                .ok();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                    if let Some(f) = HOOK_FLAGS.get() {
                        let now = !f.muted.load(Ordering::Relaxed);
                        f.muted.store(now, Ordering::Relaxed);
                        let _ = crate::config::update(|c| c.muted = now);
                        eprintln!("aural: {}", if now { "muted" } else { "unmuted" });
                    }
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnregisterHotKey(None, HOTKEY_ID);
            let _ = UnhookWindowsHookEx(hook);
        }
    });
    ready_rx.recv().expect("hook thread failed to start")
}

pub fn stop(thread_id: u32) {
    unsafe {
        let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
}
