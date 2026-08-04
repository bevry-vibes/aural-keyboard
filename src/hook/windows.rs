//! Windows backend: `WH_KEYBOARD_LL` on a dedicated message-loop thread. The
//! callback only translates keys and pushes `Trigger`s via `super::handle_key`.
//! Also owns the global mute hotkey (`RegisterHotKey`) on the same thread's
//! message loop.

use super::{handle_key, init, toggle_mute};
use crate::mixer::{SharedFlags, Trigger};
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use std::sync::Arc;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const HOTKEY_ID: i32 = 1;

/// Opaque hook handle: the hook thread's Win32 thread id.
pub struct HookHandle {
    thread_id: u32,
}

impl std::fmt::Display for HookHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "thread {}", self.thread_id)
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = kb.vkCode as u8;
        let is_up = kb.flags.0 & LLKHF_UP.0 != 0;
        handle_key(vk, is_up);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Spawn the hook thread. The thread installs the LL hook + mute hotkey and
/// pumps messages; the returned handle stops it.
pub fn spawn(
    tx: Sender<Trigger>,
    flags: Arc<SharedFlags>,
    hotkey: Option<(u32, u32)>,
) -> Result<HookHandle> {
    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<u32>>(0);
    std::thread::spawn(move || {
        init(tx, flags);
        unsafe {
            let hook = match SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                None, // LL hooks run in our own thread context
                0,
            ) {
                Ok(h) => h,
                Err(e) => {
                    ready_tx
                        .send(Err(e).context("SetWindowsHookExW(WH_KEYBOARD_LL)"))
                        .ok();
                    return;
                }
            };
            if let Some((mods, vk)) = hotkey {
                let _ = RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(mods), vk);
            }
            ready_tx
                .send(Ok(windows::Win32::System::Threading::GetCurrentThreadId()))
                .ok();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                    toggle_mute();
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnregisterHotKey(None, HOTKEY_ID);
            let _ = UnhookWindowsHookEx(hook);
        }
    });
    let thread_id = ready_rx
        .recv()
        .context("hook thread exited before signaling readiness")??;
    Ok(HookHandle { thread_id })
}

pub fn stop(handle: HookHandle) {
    unsafe {
        let _ = PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
    }
}
