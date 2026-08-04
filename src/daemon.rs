//! Daemon lifecycle (Windows): detached background process, PID file,
//! single-instance mutex, autostart via the Registry Run key.

use anyhow::{Context, Result};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Held for the process lifetime; a second `run`/`start` fails fast.
pub fn acquire_single_instance() -> Result<HANDLE> {
    unsafe {
        let handle = CreateMutexW(None, true, w!("Global\\AuralKeyboardMutex"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            anyhow::bail!("another aural instance is already running");
        }
        Ok(handle)
    }
}

pub fn pid() -> Option<u32> {
    std::fs::read_to_string(crate::config::pid_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

pub fn status() -> Option<u32> {
    let pid = pid()?;
    if alive(pid) {
        Some(pid)
    } else {
        let _ = std::fs::remove_file(crate::config::pid_path()); // stale
        None
    }
}

pub fn start() -> Result<()> {
    if let Some(pid) = status() {
        println!("aural: already running (pid {pid})");
        return Ok(());
    }
    let exe = std::env::current_exe().context("current exe")?;
    // CreateProcessW with bInheritHandles=FALSE: the daemon must not inherit our
    // stdio pipes, or the caller's shell (PowerShell) waits on them forever.
    // The daemon redirects its own std handles to aural.log (see engine::run).
    let app: Vec<u16> = format!("{}\0", exe.display()).encode_utf16().collect();
    let mut cmd: Vec<u16> = format!("\"{}\" run --daemon\0", exe.display())
        .encode_utf16()
        .collect();
    unsafe {
        let si = windows::Win32::System::Threading::STARTUPINFOW {
            cb: std::mem::size_of::<windows::Win32::System::Threading::STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut pi = windows::Win32::System::Threading::PROCESS_INFORMATION::default();
        windows::Win32::System::Threading::CreateProcessW(
            windows::core::PCWSTR::from_raw(app.as_ptr()),
            Some(windows::core::PWSTR::from_raw(cmd.as_mut_ptr())),
            None,
            None,
            false,
            windows::Win32::System::Threading::PROCESS_CREATION_FLAGS(
                CREATE_NO_WINDOW | DETACHED_PROCESS,
            ),
            None,
            None,
            &si,
            &mut pi,
        )
        .context("spawning daemon")?;
        let _ = CloseHandle(pi.hProcess);
        let _ = CloseHandle(pi.hThread);
    }
    // The daemon writes its PID after engine init (asset decode), so allow time.
    for _ in 0..40 {
        if let Some(pid) = status() {
            println!("aural: started (pid {pid})");
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    anyhow::bail!("daemon did not report a PID in time")
}

/// Called by the daemon child (`run --daemon`) at startup: point our std handles
/// at aural.log so engine logs land somewhere useful despite having no console.
pub fn redirect_stdio_to_log() {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
    let dir = crate::config::dir();
    std::fs::create_dir_all(&dir).ok();
    let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("aural.log"))
    else {
        return;
    };
    use std::os::windows::io::IntoRawHandle;
    let raw = f.into_raw_handle();
    unsafe {
        let _ = SetStdHandle(STD_OUTPUT_HANDLE, HANDLE(raw as _));
        let _ = SetStdHandle(STD_ERROR_HANDLE, HANDLE(raw as _));
    }
}

pub fn stop() -> Result<()> {
    let Some(pid) = status() else {
        println!("aural: not running");
        return Ok(());
    };
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, false, pid).context("opening daemon process")?;
        TerminateProcess(h, 0).context("terminating daemon")?;
        let _ = CloseHandle(h);
    }
    let _ = std::fs::remove_file(crate::config::pid_path());
    println!("aural: stopped (pid {pid})");
    Ok(())
}

fn open_run_key() -> Result<HKEY> {
    unsafe {
        let mut key = HKEY::default();
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
        .ok()
        .context("opening Run key")?;
        Ok(key)
    }
}

pub fn install() -> Result<()> {
    let exe = std::env::current_exe().context("current exe")?;
    let value = format!("\"{}\" start", exe.display());
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let key = open_run_key()?;
        let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
        let r = RegSetValueExW(key, w!("AuralKeyboard"), None, REG_SZ, Some(bytes));
        let _ = RegCloseKey(key);
        r.ok().context("writing Run value")?;
    }
    println!("aural: will start at login");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    unsafe {
        let key = open_run_key()?;
        let r = RegDeleteValueW(key, w!("AuralKeyboard"));
        let _ = RegCloseKey(key);
        r.ok().context("deleting Run value")?;
    }
    println!("aural: removed from login autostart");
    Ok(())
}
