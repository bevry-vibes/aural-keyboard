//! macOS backend: a listen-only `CGEventTap` on a dedicated `CFRunLoop` thread
//! (DESIGN.md D3 — hand-rolled CoreGraphics/CoreFoundation FFI like
//! KeyEcho/TickeysRedux; no rdev). Listen-only taps never block events
//! (pass-through parity with the Windows LL hook) and need only the Input
//! Monitoring permission, not Accessibility.

use super::{handle_key, init, is_pressed, log_keys_enabled, toggle_mute};
use crate::keycodes;
use crate::mixer::{SharedFlags, Trigger};
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::Arc;

// --- CoreGraphics / CoreFoundation FFI (no crates; DESIGN.md D3) ---

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;
type CFIndex = isize;

// CGEventTapLocation / CGEventTapPlacement / CGEventTapOptions
const CG_HID_EVENT_TAP: u32 = 0;
const CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

// CGEventType
const CG_EVENT_KEY_DOWN: u32 = 10;
const CG_EVENT_KEY_UP: u32 = 11;
const CG_EVENT_FLAGS_CHANGED: u32 = 12;
const CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

// CGEventField
const CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

// CGEventFlags
const CG_EVENT_FLAG_MASK_ALPHA_SHIFT: u64 = 0x0001_0000;
const CG_EVENT_FLAG_MASK_SHIFT: u64 = 0x0002_0000;
const CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x0004_0000;
const CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 0x0008_0000;
const CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x0010_0000;

type CGEventTapCallBack =
    extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
}

// Input Monitoring permission (TCC), introduced in macOS 10.15 as
// `CG*ListenEventTapAccess` and renamed to `CG*ListenEventAccess` in the
// macOS 26 SDK (same runtime entry points).
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    #[allow(non_upper_case_globals)]
    static kCFRunLoopCommonModes: CFStringRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    fn CFMachPortInvalidate(port: CFMachPortRef);
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRunLoopWakeUp(rl: CFRunLoopRef);
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

/// The tap mach port, so the callback can re-enable it after an OS timeout
/// disable (single-instance process, so a plain static is fine).
static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// The mute hotkey as `(MOD_* << 32) | vk` (same encoding as
/// `config::parse_hotkey`); 0 = disabled. Detected in-tap (no Carbon hotkey API).
static HOTKEY: AtomicU64 = AtomicU64::new(0);

/// Opaque hook handle: the hook thread's (retained) `CFRunLoop` plus its join
/// handle, so `stop` can stop the loop from any thread and wait for teardown
/// (`CFRunLoopStop`/`CFRunLoopWakeUp` are documented thread-safe).
pub struct HookHandle {
    run_loop: usize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Display for HookHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("event tap")
    }
}
// --- tap callback ---

extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    if log_keys_enabled() {
        eprintln!("aural: tap event type {event_type}");
    }
    match event_type {
        CG_EVENT_TAP_DISABLED_BY_TIMEOUT | CG_EVENT_TAP_DISABLED_BY_USER_INPUT => {
            let port = TAP_PORT.load(Ordering::Relaxed);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
            }
        }
        CG_EVENT_KEY_DOWN => {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, CG_KEYBOARD_EVENT_KEYCODE) } as u16;
            let flags = unsafe { CGEventGetFlags(event) };
            let vk = keycodes::vk_for_keycode(keycode);
            if log_keys_enabled() {
                eprintln!("aural: keycode {keycode:#06x} → vk {vk:#04x} (flags {flags:#x})");
            }
            if !is_hotkey(keycode, flags) {
                handle_key(vk, false);
            }
        }
        CG_EVENT_KEY_UP => {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, CG_KEYBOARD_EVENT_KEYCODE) } as u16;
            let vk = keycodes::vk_for_keycode(keycode);
            handle_key(vk, true);
        }
        CG_EVENT_FLAGS_CHANGED => {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, CG_KEYBOARD_EVENT_KEYCODE) } as u16;
            let flags = unsafe { CGEventGetFlags(event) };
            let pressed = modifier_pressed(keycode, flags);
            let vk = keycodes::vk_for_keycode(keycode);
            if !pressed {
                sync_caps_lock(flags);
            }
            handle_key(vk, !pressed);
        }
        _ => {}
    }
    event // listen-only: always pass through
}

/// Whether a flagsChanged event is the modifier being pressed (vs released),
/// from the event's post-transition flags.
fn modifier_pressed(keycode: u16, flags: u64) -> bool {
    let mask = match keycode {
        0x38 | 0x3C => CG_EVENT_FLAG_MASK_SHIFT,
        0x3B | 0x3E => CG_EVENT_FLAG_MASK_CONTROL,
        0x3A | 0x3D => CG_EVENT_FLAG_MASK_ALTERNATE,
        0x37 | 0x36 => CG_EVENT_FLAG_MASK_COMMAND,
        0x39 => CG_EVENT_FLAG_MASK_ALPHA_SHIFT,
        _ => 0,
    };
    mask != 0 && flags & mask != 0
}

/// Reconcile caps lock: releases only arrive when it turns off, so a down with
/// no matching up (e.g. granted mid-hold) is dropped to avoid a stuck key.
fn sync_caps_lock(flags: u64) {
    let on = flags & CG_EVENT_FLAG_MASK_ALPHA_SHIFT != 0;
    let tracked = is_pressed(0x14); // VK_CAPITAL
    if on != tracked {
        handle_key(0x14, !on);
    }
}

/// Mute-hotkey chord check: matching key down with exactly the configured
/// modifier mask (F-keys need no Fn qualifier here — macOS emits the F-key
/// keycode directly, and Fn is not part of `CGEventFlags` matching).
fn is_hotkey(keycode: u16, flags: u64) -> bool {
    let encoded = HOTKEY.load(Ordering::Relaxed);
    if encoded == 0 {
        return false;
    }
    let (want_mods, want_vk) = ((encoded >> 32) as u32, encoded as u8);
    if keycode != keycodes::keycode_for_vk(want_vk).unwrap_or(u16::MAX) {
        return false;
    }
    let mut have = 0u32;
    if flags & CG_EVENT_FLAG_MASK_SHIFT != 0 {
        have |= 0x0004; // MOD_SHIFT
    }
    if flags & CG_EVENT_FLAG_MASK_CONTROL != 0 {
        have |= 0x0002; // MOD_CONTROL
    }
    if flags & CG_EVENT_FLAG_MASK_ALTERNATE != 0 {
        have |= 0x0008; // MOD_ALT
    }
    if flags & CG_EVENT_FLAG_MASK_COMMAND != 0 {
        have |= 0x0010; // MOD_WIN (Command)
    }
    if have != want_mods {
        return false;
    }
    toggle_mute();
    true
}

/// True if Input Monitoring permission is already granted to this binary
/// (no prompt; macOS 10.15+).
pub fn listen_access_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

// --- spawn / stop ---

/// Spawn the hook thread: create the listen-only tap, attach it to the
/// thread's run loop, and run until [`stop`] is called. Fails fast (from the
/// thread, before the run loop starts) if the tap cannot be created — almost
/// always missing Input Monitoring permission.
pub fn spawn(
    tx: Sender<Trigger>,
    flags: Arc<SharedFlags>,
    hotkey: Option<(u32, u32)>,
) -> Result<HookHandle> {
    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<usize>>(0);
    HOTKEY.store(
        hotkey
            .map(|(mods, vk)| ((mods as u64) << 32) | vk as u64)
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    let thread = std::thread::spawn(move || {
        init(tx, flags);
        // Gate on TCC first: on macOS 26, CGEventTapCreate can succeed yet
        // deliver no events when permission is missing (a "deaf" tap), so a
        // null check alone would run silently deaf. If missing, request it and
        // wait: the request only presents the system prompt while the
        // requester is alive, and preflight flips as soon as the user allows
        // the prompt or toggles the entry in Settings.
        if !listen_access_granted() {
            unsafe { CGRequestListenEventAccess() };
            eprintln!(
                "aural: Input Monitoring permission is missing — waiting up to 5 minutes.\n  \
                 → Grant \"aural\" in System Settings → Privacy & Security → Input\n    \
                 Monitoring; a system prompt may also appear (it names aural). aural\n    \
                 continues automatically once granted."
            );
            let mut granted = false;
            for _ in 0..600 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if listen_access_granted() {
                    granted = true;
                    break;
                }
            }
            if !granted {
                ready_tx
                    .send(Err(anyhow::anyhow!(
                        "timed out waiting for Input Monitoring permission"
                    )))
                    .ok();
                return;
            }
            eprintln!("aural: Input Monitoring granted");
        }
        unsafe {
            let mask = (1u64 << CG_EVENT_KEY_DOWN)
                | (1u64 << CG_EVENT_KEY_UP)
                | (1u64 << CG_EVENT_FLAGS_CHANGED);
            let port = CGEventTapCreate(
                CG_HID_EVENT_TAP,
                CG_HEAD_INSERT_EVENT_TAP,
                CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                mask,
                tap_callback,
                std::ptr::null_mut(),
            );
            if port.is_null() {
                // One-shot TCC prompt (async; no-op once granted), then fail
                // with actionable instructions.
                let _ = CGRequestListenEventAccess();
                ready_tx
                    .send(Err(anyhow::anyhow!(
                        "could not create the keyboard event tap (CGEventTapCreate failed)\n\
                         → Input Monitoring permission is missing for aural. Enable it in\n\
                         System Settings → Privacy & Security → Input Monitoring (the prompt\n\
                         names aural), then run `aural run` again."
                    )))
                    .ok();
                return;
            }
            TAP_PORT.store(port, Ordering::Relaxed);
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
            if source.is_null() {
                CFMachPortInvalidate(port);
                CFRelease(port);
                ready_tx
                    .send(Err(anyhow::anyhow!("CFMachPortCreateRunLoopSource failed")))
                    .ok();
                return;
            }
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            CGEventTapEnable(port, true);
            if !CGEventTapIsEnabled(port) {
                eprintln!("aural: warning — event tap not enabled after CGEventTapEnable");
            }
            let run_loop = CFRetain(run_loop) as usize; // hand a Send-able copy to the handle
            ready_tx.send(Ok(run_loop)).ok();
            CFRunLoopRun(); // until CFRunLoopStop from `stop`
            CFMachPortInvalidate(port);
            TAP_PORT.store(std::ptr::null_mut(), Ordering::Relaxed);
            CFRelease(source);
            CFRelease(port);
            CFRelease(run_loop as CFTypeRef);
        }
    });
    let run_loop = ready_rx
        .recv()
        .context("hook thread exited before signaling readiness")??;
    Ok(HookHandle {
        run_loop,
        thread: Some(thread),
    })
}

pub fn stop(mut handle: HookHandle) {
    unsafe {
        let run_loop = handle.run_loop as CFRunLoopRef;
        CFRunLoopStop(run_loop);
        CFRunLoopWakeUp(run_loop);
    }
    if let Some(thread) = handle.thread.take() {
        let _ = thread.join();
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        // Safety net: never leak a running tap thread (the engine stops first).
        if self.thread.is_some() {
            unsafe {
                let run_loop = self.run_loop as CFRunLoopRef;
                CFRunLoopStop(run_loop);
                CFRunLoopWakeUp(run_loop);
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
