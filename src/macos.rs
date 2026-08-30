//! macOS-only process self-disclaim for TCC attribution, plus the
//! Secure Event Input probe used by `aural doctor`.
//!
//! macOS grants Input Monitoring to the *responsible process* — normally the
//! app that launched us (DESIGN.md §10 learning #1), so a terminal-launched
//! `aural run` is attributed to the terminal. Terminal.app/iTerm2 break that
//! inheritance with the private `responsibility_spawnattrs_setdisclaim`
//! posix_spawn attribute, making the spawned process its own responsible app.
//! We do the same by re-exec'ing ourselves: the re-exec'd process is its own
//! responsible process, so the TCC prompt and grant key to aural itself
//! (DESIGN.md §10, "Blocker 2").

use anyhow::Result;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicI32, Ordering};

/// Guard env var: set on the re-exec'd child so it doesn't re-disclaim.
const DISCLAIM_ENV: &str = "AURAL_DISCLAIMED";

/// PID of the disclaimed child, for the parent's signal-forwarding handler.
static CHILD_PID: AtomicI32 = AtomicI32::new(0);
/// The Apple `spawn_private.h` signature: `int
/// responsibility_spawnattrs_setdisclaim(posix_spawnattr_t *attrs, int
/// disclaim)`. The `disclaim` argument is load-bearing — a one-arg call leaves
/// it as caller garbage, which can silently skip the flag.
type SetDisclaim = unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int;

/// Re-exec `self` disclaimed so TCC treats this process as its own app. Called
/// from `main` before any side effects, only for commands that install the
/// keyboard hook (`run`, live `bench`) or report on it (`doctor`).
///
/// Uses the proven child-spawn pattern (Terminal.app/iTerm2/selfauth/disclaim
/// all spawn without `POSIX_SPAWN_SETEXEC`, since the disclaim flag is applied
/// in the spawn child — `SETEXEC` bypasses it). The parent forwards
/// SIGINT/SIGTERM/SIGHUP to the child and exits with its status, so a
/// foreground `aural run` keeps normal Ctrl+C and exit-code behavior. The
/// child inherits stdin/stdout/stderr (no file actions), which keeps
/// `--stdin`, live `bench` typing, and foreground Ctrl+C working.
pub fn disclaim() -> Result<()> {
    if std::env::var_os(DISCLAIM_ENV).is_some() {
        return Ok(()); // already disclaimed (the re-exec'd child): run normally
    }
    let set_disclaim = match disclaim_sym() {
        Some(f) => f,
        None => {
            eprintln!(
                "aural: self-disclaim unavailable (responsibility_spawnattrs_setdisclaim not \
                 found); TCC will name the app that launched aural"
            );
            return Ok(());
        }
    };
    let child = match spawn_disclaimed(set_disclaim) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("aural: self-disclaim failed ({e:#}); TCC will name the launching app");
            return Ok(());
        }
    };
    CHILD_PID.store(child as i32, Ordering::Relaxed);
    forward_signals();
    std::process::exit(wait_for(child));
}

/// dlsym `responsibility_spawnattrs_setdisclaim` from libSystem (absent from
/// the SDK headers, so no static link).
fn disclaim_sym() -> Option<SetDisclaim> {
    let path = b"/usr/lib/libSystem.B.dylib\0";
    unsafe {
        let h = libc::dlopen(path.as_ptr() as *const _, libc::RTLD_LAZY);
        if h.is_null() {
            return None;
        }
        let sym = libc::dlsym(h, c"responsibility_spawnattrs_setdisclaim".as_ptr());
        if sym.is_null() {
            return None;
        }
        Some(std::mem::transmute::<*mut libc::c_void, SetDisclaim>(sym))
    }
}

/// posix_spawn a copy of self (same argv/env) with the disclaim attribute.
fn spawn_disclaimed(set_disclaim: SetDisclaim) -> Result<u32> {
    let exe = std::env::current_exe()?;
    let exe_c = CString::new(exe.as_os_str().as_bytes())?;
    let args: Vec<CString> = std::env::args_os()
        .map(|a| CString::new(a.as_os_str().as_bytes()).expect("argv has no NUL"))
        .collect();
    let mut argv: Vec<*mut libc::c_char> = args.iter().map(|c| c.as_ptr() as *mut _).collect();
    argv.push(std::ptr::null_mut());

    // Current env + the guard; drop any stale guard first.
    let mut env_pairs = std::env::vars_os()
        .filter(|(k, _)| k != DISCLAIM_ENV)
        .map(|(k, v)| {
            let mut s = k.as_os_str().as_bytes().to_vec();
            s.push(b'=');
            s.extend_from_slice(v.as_os_str().as_bytes());
            CString::new(s).expect("env has no NUL")
        })
        .collect::<Vec<_>>();
    env_pairs.push(CString::new(format!("{DISCLAIM_ENV}=1")).expect("const has no NUL"));
    let mut envp: Vec<*mut libc::c_char> = env_pairs.iter().map(|c| c.as_ptr() as *mut _).collect();
    envp.push(std::ptr::null_mut());

    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
        if libc::posix_spawnattr_init(&mut attr) != 0 {
            anyhow::bail!("posix_spawnattr_init failed");
        }
        let rc = set_disclaim(&mut attr, 1);
        if rc != 0 {
            libc::posix_spawnattr_destroy(&mut attr);
            anyhow::bail!("setdisclaim returned {rc}");
        }
        let mut pid: libc::pid_t = 0;
        let rc = libc::posix_spawn(
            &mut pid,
            exe_c.as_ptr(),
            std::ptr::null(), // no file actions: inherit stdio
            &attr,
            argv.as_ptr(),
            envp.as_ptr(),
        );
        libc::posix_spawnattr_destroy(&mut attr);
        if rc != 0 {
            anyhow::bail!("posix_spawn failed: {}", std::io::Error::last_os_error());
        }
        Ok(pid as u32)
    }
}

fn forward_signals() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = forward_signal as *const () as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
}

/// Async-signal-safe: relay the signal to the disclaimed child (the child is
/// in the same process group, so the terminal's Ctrl+C reaches it directly
/// too; this just makes explicit forwards idempotent-safe).
unsafe extern "C" fn forward_signal(sig: libc::c_int) {
    let pid = CHILD_PID.load(Ordering::Relaxed);
    if pid > 0 {
        libc::kill(pid, sig);
    }
}

fn wait_for(child: u32) -> i32 {
    unsafe {
        let mut status: libc::c_int = 0;
        loop {
            if libc::waitpid(child as libc::pid_t, &mut status, 0) >= 0 {
                break;
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                break;
            }
        }
        if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else if libc::WIFSIGNALED(status) {
            128 + libc::WTERMSIG(status)
        } else {
            1
        }
    }
}

// --- Secure Event Input (aural doctor) ---

/// True while any app holds "Secure Event Input": macOS withholds keyDown/keyUp
/// from ALL event taps system-wide (only flagsChanged leaks) until it is
/// released (DESIGN.md §10). Loaded from HIToolbox — the Carbon subframework
/// path; the old ApplicationServices path is gone on macOS 26.
pub fn secure_input_enabled() -> bool {
    type SeiFn = unsafe extern "C" fn() -> bool;
    let path =
        b"/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox\0";
    unsafe {
        let h = libc::dlopen(path.as_ptr() as *const _, libc::RTLD_LAZY);
        if h.is_null() {
            return false;
        }
        let sym = libc::dlsym(h, c"IsSecureEventInputEnabled".as_ptr());
        if sym.is_null() {
            return false;
        }
        let f: SeiFn = std::mem::transmute(sym);
        f()
    }
}

/// `aural doctor` line: `secure input: not active` normally; when active,
/// names the holding app via IORegistry's `kCGSSessionSecureInputPID`.
pub fn secure_input_check() -> String {
    if !secure_input_enabled() {
        return "secure input: not active".to_string();
    }
    match secure_input_holder_pid() {
        Some(pid) => match secure_input_holder_name(pid) {
            Some(name) => format!(
                "secure input: ACTIVE — \"{name}\" (pid {pid}) is holding Secure Event Input.\n  \
                 → terminal: uncheck Shell → Secure Keyboard Entry (or the app's Secure Input\n    \
                 menu); password manager or stuck browser password field: quit it. aural hears\n    \
                 nothing until it is released."
            ),
            None => format!(
                "secure input: ACTIVE — pid {pid} is holding it (name unavailable). Quit that\n  \
                 app or disable its secure-entry feature."
            ),
        },
        None => {
            "secure input: ACTIVE — holder unknown (ioreg/ps probe failed). Quit any password\n  \
             managers or terminals with secure entry enabled."
                .to_string()
        }
    }
}

/// PID holding secure input from `ioreg -n Root -d1 -a` (works for both the
/// human and XML-plist output; no sudo needed).
fn secure_input_holder_pid() -> Option<u32> {
    let out = std::process::Command::new("ioreg")
        .args(["-n", "Root", "-d1", "-a"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let key = "kCGSSessionSecureInputPID";
    let pos = text.find(key)?;
    let rest = &text[pos + key.len()..];
    let start = rest.find(|c: char| c.is_ascii_digit())?;
    let digits: String = rest[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn secure_input_holder_name(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("comm=")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    // Prefer the .app component (Terminal.app, …) over the executable basename.
    let app = path.rsplit('/').find(|c| c.ends_with(".app"));
    Some(
        app.map(str::to_string)
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(&path).to_string()),
    )
}
