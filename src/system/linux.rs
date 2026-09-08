//! Privileged Linux install — absorbed from the deleted
//! `scripts/setup-dedicated-user.sh`, so `cargo install aural && aural system
//! install` is the whole setup: no repo checkout, no helper scripts, just one
//! sudo prompt. File contents below are equivalent to the proven script;
//! DESIGN.md §11 records the lessons baked into them (`ProtectHome=read-only`
//! or the pipewire socket disappears, EPERM counts as alive, the pipewire
//! sockets are already world-rw so the audio bridge exposes audio only).
//!
//! Two modes:
//!
//! - **dedicated-user** (default where systemd exists): the engine runs as a
//!   hardened `aural` system service under a dedicated `aural` user that alone
//!   can read keyboard event nodes (udev ACL); your CLI and tray share its
//!   state via `AURAL_CONFIG_DIR`; the tray is an autostarted control surface.
//! - **fallback** (no systemd): your user joins the `input` group and the
//!   autostarted tray hosts the engine.
//!
//! Two phases: the user phase explains the plan and re-execs under sudo; the
//! root phase (euid 0, `SUDO_USER` set) writes everything.

use anyhow::{bail, Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Shared state dir — also the `aural` user's home.
pub const STATE: &str = "/var/lib/aural";
/// Where the install copies the binary (stable across `cargo clean`).
pub const INSTALL_BIN: &str = "/usr/local/bin/aural";
const SERVICE: &str = "/etc/systemd/system/aural.service";
const UDEV_RULE: &str = "/etc/udev/rules.d/70-aural-input.rules";
const ENV_D: &str = "/etc/environment.d/50-aural.conf";
const PROFILE_D: &str = "/etc/profile.d/aural.sh";
const USER_UNIT: &str = ".config/systemd/user/aural-pipewire-acl.service";
const TRAY_DESKTOP: &str = ".config/autostart/aural-tray.desktop";
const XDG_DESKTOP: &str = ".config/autostart/com.bevry.aural.desktop";

/// The dedicated-user install is present (the systemd unit exists).
pub fn dedicated_installed() -> bool {
    Path::new(SERVICE).exists()
}

/// Whether the engine service is registered to start (systemd `is-enabled`).
pub fn service_enabled() -> bool {
    run_ok("systemctl", &["is-enabled", "--quiet", "aural.service"])
}

fn systemd_present() -> bool {
    Path::new("/run/systemd/system").exists()
}

/// Whether this process runs as root (euid — under sudo the real uid stays
/// the invoking user's).
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// The invoking user (SUDO_USER, set by sudo) and their uid.
fn sudo_user() -> Result<(String, u32)> {
    let name = std::env::var("SUDO_USER")
        .context("must run via sudo from your own account (SUDO_USER is not set)")?;
    let uid = id_of(&name, "-u").context(format!("resolving the uid of {name}"))?;
    Ok((name, uid))
}

/// Re-exec this command under sudo and exit with its status (user phase).
fn sudo_reexec(args: &[&str]) -> ! {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("aural"));
    println!(
        "aural: administrator access is needed — running: sudo {} system {}",
        exe.display(),
        args.join(" ")
    );
    let status = Command::new("sudo")
        .arg(&exe)
        .arg("system")
        .args(args)
        .status();
    match status {
        Ok(s) if s.success() => std::process::exit(0),
        Ok(s) => {
            eprintln!("aural: sudo exited with {}", s.code().unwrap_or(1));
            std::process::exit(s.code().unwrap_or(1));
        }
        Err(e) => {
            eprintln!("aural: failed to run sudo: {e}");
            std::process::exit(1);
        }
    }
}

fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("running {cmd}"))?;
    if !status.success() {
        bail!("`{} {}` failed: {status}", cmd, args.join(" "));
    }
    Ok(())
}

fn run_ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn id_of(name: &str, flag: &str) -> Result<u32> {
    let out = Command::new("id")
        .arg(flag)
        .arg(name)
        .output()
        .with_context(|| format!("running id {flag} {name}"))?;
    if !out.status.success() {
        bail!("id {flag} {name} failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .context("parsing id output")?)
}

fn user_exists(name: &str) -> bool {
    run_ok("getent", &["passwd", name])
}

fn group_exists(name: &str) -> bool {
    run_ok("getent", &["group", name])
}

fn chown(path: &Path, user: &str, group: &str) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes())
        .with_context(|| format!("chown {}", path.display()))?;
    let rc = unsafe { libc::chown(cpath.as_ptr(), id_of(user, "-u")?, id_of(group, "-g")?) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("chown {} to {user}:{group}", path.display()));
    }
    Ok(())
}

/// Write a root-owned 0644 file at an absolute path.
fn write_root(path: &str, contents: &str) -> Result<()> {
    let p = Path::new(path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, contents).with_context(|| format!("writing {path}"))?;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644))?;
    Ok(())
}

/// Write a file under the invoking user's home, owned by them.
fn write_user(user: &str, rel: &str, contents: &str) -> Result<()> {
    let path = PathBuf::from("/home").join(user).join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
    chown(&path, user, user)?;
    Ok(())
}

/// Copy this binary to [`INSTALL_BIN`] (root-owned 0755). The root phase runs
/// the sudo-invoked path, so the source is wherever the user launched it
/// (cargo bin, build dir) — the copy decouples the install from that. Running
/// as the install target itself (rerun) is a no-op.
fn install_binary() -> Result<()> {
    let src = std::fs::canonicalize(std::env::current_exe().context("current_exe")?)
        .context("resolving current_exe")?;
    let same = match std::fs::canonicalize(INSTALL_BIN) {
        Ok(dest) => dest == src,
        Err(_) => false,
    };
    if same {
        return Ok(());
    }
    if let Some(dir) = Path::new(INSTALL_BIN).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(INSTALL_BIN);
    std::fs::copy(&src, INSTALL_BIN)
        .with_context(|| format!("copying {} to {INSTALL_BIN}", src.display()))?;
    std::fs::set_permissions(INSTALL_BIN, std::fs::Permissions::from_mode(0o755))?;
    chown(Path::new(INSTALL_BIN), "root", "root")?;
    Ok(())
}

// --- install ---

/// `aural system install` (Linux): user phase explains + elevates, root phase
/// writes everything.
pub fn install() -> Result<()> {
    if !is_root() {
        println!("aural: installing the app needs administrator access (sudo):");
        println!("aural:   - a dedicated `aural` system user that alone can read keyboard devices");
        println!("aural:   - the engine as a hardened systemd service (aural.service)");
        println!("aural:   - the binary copied to {INSTALL_BIN}");
        println!("aural:   - session wiring: shared config dir, tray autostart, audio bridge");
        sudo_reexec(&["install"]);
    }
    let (user, uid) = sudo_user()?;
    if systemd_present() {
        dedicated_install(&user, uid)
    } else {
        fallback_install(&user)
    }
}

fn dedicated_install(user: &str, uid: u32) -> Result<()> {
    println!("aural: creating the system user `aural` (no shell, no groups)");
    if !user_exists("aural") {
        run(
            "useradd",
            &[
                "--system",
                "--shell",
                "/usr/sbin/nologin",
                "--home-dir",
                STATE,
                "--create-home",
                "aural",
            ],
        )?;
    }
    if !group_exists("aural") {
        run("groupadd", &["--system", "aural"])?;
    }
    println!("aural: state dir {STATE} (aural:aural, group-writable for your CLI)");
    std::fs::create_dir_all(STATE)?;
    chown(Path::new(STATE), "aural", "aural")?;
    std::fs::set_permissions(STATE, std::fs::Permissions::from_mode(0o2775))?;
    run("usermod", &["-aG", "aural", user])?;

    println!("aural: installing the binary to {INSTALL_BIN}");
    install_binary()?;

    println!("aural: udev rule — keyboard event nodes readable by `aural` only");
    write_root(UDEV_RULE, UDEV_RULE_CONTENT)?;

    println!("aural: systemd unit {SERVICE} (the engine as the `aural` user)");
    write_root(SERVICE, &service_content(uid))?;
    run("udevadm", &["control", "--reload"])?;
    let _ = run_ok(
        "udevadm",
        &["trigger", "--subsystem-match=input", "--action=change"],
    );
    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", "aural.service"])?;
    // restart (not just start): applies the freshly installed binary and unit
    // deterministically, whether or not a previous instance is running
    run("systemctl", &["restart", "aural.service"])?;
    std::thread::sleep(Duration::from_secs(2));
    if run_ok("systemctl", &["is-active", "--quiet", "aural.service"]) {
        println!("aural: aural.service is running");
    } else {
        println!("aural: NOTE aural.service is not active yet (it retries every 15 s)");
        println!("aural:      inspect with: journalctl -u aural -n 30 --no-pager");
    }

    println!("aural: session env — your CLI and tray share the daemon's state dir");
    write_root(
        ENV_D,
        &format!(
            "# aural dedicated-user mode: CLI/tray share the daemon's state dir\nAURAL_CONFIG_DIR={STATE}\n"
        ),
    )?;
    write_root(
        PROFILE_D,
        &format!(
            "# aural dedicated-user mode: CLI/tray share the daemon's state dir\nexport AURAL_CONFIG_DIR={STATE}\n"
        ),
    )?;

    println!("aural: per-login audio bridge + tray autostart for {user}");
    write_user(
        user,
        USER_UNIT,
        &format!(
            concat!(
                "[Unit]\n",
                "Description=aural dedicated-user mode: re-grant user 'aural' audio-socket access\n",
                "\n",
                "[Service]\n",
                "Type=oneshot\n",
                "RemainAfterExit=yes\n",
                "ExecStart={bin} system acl-wait aural {uid} 30\n",
                "\n",
                "[Install]\n",
                "WantedBy=default.target\n"
            ),
            bin = INSTALL_BIN,
            uid = uid
        ),
    )?;
    write_user(
        user,
        TRAY_DESKTOP,
        &format!(
            concat!(
                "[Desktop Entry]\n",
                "Type=Application\n",
                "Name=aural tray\n",
                "Comment=System-wide melodic keyboard sounds — tray control surface\n",
                "Exec={bin} system tray\n",
                "Terminal=false\n",
                "X-GNOME-Autostart-enabled=true\n",
                "Categories=Utility;Audio;\n"
            ),
            bin = INSTALL_BIN
        ),
    )?;
    // Under sudo there is no user bus via local transport; --machine connects
    // to the invoking user's systemd --user instance through its bus.
    let machine = format!("--machine={user}@.host");
    let machine = machine.as_str();
    if !run_ok("systemctl", &["--user", machine, "daemon-reload"])
        || !run_ok(
            "systemctl",
            &[
                "--user",
                machine,
                "enable",
                "--now",
                "aural-pipewire-acl.service",
            ],
        )
    {
        println!("aural: NOTE could not enable the per-login unit from here — after your next login run:");
        println!("aural:      systemctl --user enable --now aural-pipewire-acl.service");
    }

    println!();
    println!("aural: installed. what works RIGHT NOW (no logout needed):");
    println!("aural:   - sound: the engine runs as the `aural` user (systemctl status aural)");
    println!("aural:   - the mute hotkey Ctrl+Shift+F12 (the daemon toggles the shared config)");
    println!("aural: needs a LOG OUT & BACK IN (fresh session picks up groups + env):");
    println!("aural:   - your CLI control: aural system mute/volume/doctor (aural group + AURAL_CONFIG_DIR)");
    println!("aural:   - the tray control surface (autostarts every login)");
    println!(
        "aural:   - input isolation for your account: undo any old input-group grant: \
         sudo gpasswd -d {user} input"
    );
    println!("aural: then verify the whole chain with: aural system doctor");
    Ok(())
}

/// No systemd: your user joins the `input` group and the autostarted tray
/// hosts the engine. Needs one log out & back in before sound works.
fn fallback_install(user: &str) -> Result<()> {
    println!("aural: no systemd detected — installing the simple mode instead:");
    println!("aural:   your user joins the `input` group; the tray hosts the engine");
    run("usermod", &["-aG", "input", user])?;
    install_binary()?;
    write_user(
        user,
        XDG_DESKTOP,
        &crate::system::desktop_entry(INSTALL_BIN),
    )?;
    println!("aural: log out & back in (the input group needs a fresh session) —");
    println!("aural: the tray then starts at every login (aural system tray)");
    Ok(())
}

// --- enable / disable / uninstall ---

/// `aural system enable`/`disable` in dedicated mode — systemctl (needs root;
/// re-execs under sudo from the user phase).
pub fn set_enabled(on: bool) -> Result<()> {
    if !is_root() {
        sudo_reexec(&[if on { "enable" } else { "disable" }]);
    }
    run(
        "systemctl",
        &[if on { "enable" } else { "disable" }, "aural.service"],
    )?;
    println!(
        "aural: {} at login ({SERVICE})",
        if on { "starts" } else { "no longer starts" }
    );
    Ok(())
}

/// `aural system uninstall` in dedicated mode — full teardown. Needs root;
/// re-execs under sudo from the user phase.
pub fn uninstall() -> Result<()> {
    if !is_root() {
        println!("aural: removing the dedicated-user install needs administrator access");
        sudo_reexec(&["uninstall"]);
    }
    // Direct root (no sudo context) skips the user-session files.
    let (user, uid) = match sudo_user() {
        Ok(pair) => pair,
        Err(_) => (String::new(), 0),
    };
    println!("aural: stopping and disabling aural.service");
    let _ = run_ok("systemctl", &["disable", "--now", "aural.service"]);
    if !user.is_empty() {
        println!("aural: disabling the per-login audio-bridge unit for {user}");
        let runuser = |args: &[&str]| run_ok("runuser", args);
        runuser(&[
            "-u",
            &user,
            "--",
            "systemctl",
            "--user",
            "disable",
            "--now",
            "aural-pipewire-acl.service",
        ]);
        let home = PathBuf::from("/home").join(&user);
        let _ = std::fs::remove_file(home.join(USER_UNIT));
        let _ = std::fs::remove_file(home.join(TRAY_DESKTOP));
        let _ = std::fs::remove_file(home.join(XDG_DESKTOP));
        runuser(&["-u", &user, "--", "systemctl", "--user", "daemon-reload"]);
        println!("aural: revoking the runtime-dir ACL (best effort)");
        let _ = run_ok("setfacl", &["-x", "u:aural", &format!("/run/user/{uid}")]);
        println!("aural: removing your `aural` group membership");
        let _ = run_ok("gpasswd", &["-d", &user, "aural"]);
    }
    println!("aural: removing the unit, udev rule, env files, binary, state");
    let _ = std::fs::remove_file(SERVICE);
    let _ = std::fs::remove_file(UDEV_RULE);
    let _ = std::fs::remove_file(ENV_D);
    let _ = std::fs::remove_file(PROFILE_D);
    let _ = std::fs::remove_file(INSTALL_BIN);
    let _ = std::fs::remove_dir_all("/usr/local/lib/aural"); // legacy script helper dir
    let _ = run_ok("udevadm", &["control", "--reload"]);
    println!("aural: removing the `aural` user and state");
    let _ = run_ok("userdel", &["aural"]);
    let _ = std::fs::remove_dir_all(STATE);
    if !user.is_empty() {
        println!("aural: done — log out & back in to drop the `aural` group from your session");
    }
    Ok(())
}

// --- the audio bridge (replaces the script's pipewire-acl.sh helper) ---

/// Wait for the session's pipewire socket, then grant `user` traverse (x) on
/// the runtime dir and rw on the socket. The sockets are created world-rw, so
/// this grants audio access only. Ok even when the socket never appears (no
/// session yet) — the systemd unit restarts and retries.
pub fn acl_wait(user: &str, uid: u32, timeout: u64) -> Result<()> {
    use std::os::unix::fs::FileTypeExt;
    let rundir = format!("/run/user/{uid}");
    let socket = format!("{rundir}/pipewire-0");
    let socket_ready = || {
        std::fs::metadata(&socket)
            .map(|m| m.file_type().is_socket())
            .unwrap_or(false)
    };
    let deadline = Instant::now() + Duration::from_secs(timeout);
    while Instant::now() < deadline && !socket_ready() {
        std::thread::sleep(Duration::from_secs(1));
    }
    if !socket_ready() {
        return Ok(());
    }
    let _ = run_ok("setfacl", &["-m", &format!("u:{user}:x"), &rundir]);
    let _ = run_ok("setfacl", &["-m", &format!("u:{user}:rw"), &socket]);
    Ok(())
}

// --- diagnostics (`aural system doctor`) ---

/// One-line dedicated-mode chain summary for `aural system doctor` (absorbs
/// the setup script's `--verify`). Group/env items only pass in a session
/// started after the install — that is the documented relogin boundary.
pub fn verify_chain() -> String {
    let items: Vec<(bool, &str)> = vec![
        (
            run_ok("systemctl", &["is-active", "--quiet", "aural.service"]),
            "engine service active",
        ),
        (user_in_group("aural"), "your session in the aural group"),
        (
            std::env::var_os("AURAL_CONFIG_DIR").is_some(),
            "AURAL_CONFIG_DIR set in this session",
        ),
        (Path::new(STATE).exists(), "shared state dir present"),
    ];
    let passed = items.iter().filter(|(p, _)| *p).count();
    let failed: Vec<&str> = items
        .iter()
        .filter(|(p, _)| !p)
        .map(|(_, name)| *name)
        .collect();
    if failed.is_empty() {
        format!("{passed}/{} checks pass", items.len())
    } else {
        format!(
            "{passed}/{} checks pass — missing: {} \
             (log out & back in to pick up group and environment changes)",
            items.len(),
            failed.join(", ")
        )
    }
}

fn user_in_group(group: &str) -> bool {
    // `id -nG` lists the *effective session's* groups — the relogin boundary
    Command::new("id")
        .arg("-nG")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .any(|g| g == group)
        })
        .unwrap_or(false)
}

/// The udev rule, verbatim from the setup script: ACL keyboard event nodes to
/// the dedicated user — mice/touchpads stay out of reach even for aural, and
/// everything running as your account stays locked out of /dev/input.
const UDEV_RULE_CONTENT: &str = concat!(
    "# aural: let the dedicated `aural` user read keyboard event nodes only.\n",
    "# Everything running as your own account stays locked out of /dev/input.\n",
    "ACTION==\"add|change\", SUBSYSTEM==\"input\", ENV{ID_INPUT_KEYBOARD}==\"1\", \\\n",
    "  RUN+=\"/usr/bin/setfacl -m u:aural:rw $devnode\"\n"
);

/// The hardened engine unit — `ProtectHome=read-only` (not "yes": that hides
/// /run/user entirely, including the pipewire socket; see DESIGN.md §11).
fn service_content(uid: u32) -> String {
    format!(
        concat!(
            "[Unit]\n",
            "Description=aural — system-wide melodic keyboard sounds (dedicated user)\n",
            "Documentation=https://github.com/bevry-vibes/aural-keyboard\n",
            "After=sound.target\n",
            "\n",
            "[Service]\n",
            "Type=simple\n",
            "User=aural\n",
            "Group=aural\n",
            "UMask=0002\n",
            "Environment=AURAL_CONFIG_DIR={state}\n",
            "Environment=XDG_RUNTIME_DIR=/run/user/{uid}\n",
            "# Grant the audio ACL as root, waiting for the session's pipewire socket\n",
            "# (it appears at first login; + keeps ExecStartPre privileged).\n",
            "ExecStartPre=+{bin} system acl-wait aural {uid} 90\n",
            "ExecStart={bin} system daemon\n",
            "Restart=on-failure\n",
            "RestartSec=15\n",
            "TimeoutStartSec=120\n",
            "\n",
            "# hardening: the engine needs only its state dir, input devices, and audio\n",
            "NoNewPrivileges=yes\n",
            "ProtectSystem=strict\n",
            "ReadWritePaths={state}\n",
            "# read-only (not \"yes\"): \"yes\" would hide /run/user entirely — including the\n",
            "# pipewire socket the daemon must connect to (XDG_RUNTIME_DIR above).\n",
            "ProtectHome=read-only\n",
            "PrivateTmp=yes\n",
            "ProtectKernelTunables=yes\n",
            "ProtectKernelModules=yes\n",
            "ProtectControlGroups=yes\n",
            "ProtectClock=yes\n",
            "RestrictSUIDSGID=yes\n",
            "LockPersonality=yes\n",
            "\n",
            "[Install]\n",
            "WantedBy=multi-user.target\n"
        ),
        state = STATE,
        uid = uid,
        bin = INSTALL_BIN
    )
}
