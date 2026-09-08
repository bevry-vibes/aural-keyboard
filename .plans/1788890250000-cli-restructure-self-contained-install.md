# Self-contained install: `cargo install` → `aural system install`, nothing else

> **Provenance:** rescued 2026-09-09 from harness-private `.zcode/plans/plan-sess_e4dc50e8-8d98-41fd-b975-cfa531940d4c.md` (gitignored) during the `.plans/` adoption, renamed to the `<epoch-ms>-<slug>` convention — the epoch is the author timestamp of the landing commit `c972d3a` (2026-09-09). This is the authentic plan for the CLI restructure recorded as DESIGN.md §14; its original prompts were not preserved, so no `.prompts.md` companion exists. Superseded plan rounds it references predate this file.

Everything below supersedes the previous plan round (CLI surface is unchanged from it — bare `aural` = help, only `aural stdin` + `aural system …`, no `status`, flagless `tray`). The new work: `aural system install` absorbs the helper scripts — permissions, users, services — because a `cargo install` user never has the repo's `scripts/` directory at all.

## Final user surface

```
aural                      same as `aural --help`
aural stdin                stdin test mode (no hook, no permissions)
aural system install       app + permissions + start now + start at login
aural system uninstall     stop the app, remove everything it installed
aural system enable | disable     toggle start-at-login
aural system doctor        diagnostics (engine state + install chain verification)
aural system mute | unmute | toggle
aural system volume 60
aural system bench [--synthetic N]
aural system about
```

Hidden internal subcommands (needed only because spawners/login entries cross a process boundary): `aural system daemon` (engine in daemon mode — spawn child, Windows Run key, Linux systemd unit) and `aural system tray` (menu-bar/tray app — macOS LaunchAgent, Linux autostart; auto-detects control-surface mode when the single-instance lock is held, replacing `--no-engine`) plus `aural system acl-wait <user> <uid> <timeout>` (the pipewire audio-bridge helper, replacing the generated shell script). Inside the Aural.app bundle only, a bare no-argument launch still runs the tray (double-click must work); the LaunchAgent passes explicit `system tray` args anyway.

## `aural system install` per platform

**macOS** — already self-contained; unchanged: builds `~/Applications/Aural.app` (signed, embedded icon), LaunchAgent runs `[bundle_exe, "system", "tray"]`, launches via `open`, TCC prompt appears automatically.

**Linux** — absorbs `scripts/setup-dedicated-user.sh` (deleted afterwards). New `src/system/linux.rs` privileged module; `install()` detects non-root → prints the plan and re-execs `sudo <exe> system install`; as root (with `SUDO_USER`):
- create the `aural` system user + group (no shell, home `/var/lib/aural`, state dir group-writable `2775`), add your user to the group
- install the binary to `/usr/local/bin/aural` (copy — the whole install is immune to `cargo clean`)
- udev rule `70-aural-input.rules`: keyboard event nodes ACL'd to `aural` only; `udevadm control --reload` + trigger
- hardened systemd unit `aural.service`: `User=aural`, `AURAL_CONFIG_DIR=/var/lib/aural`, `XDG_RUNTIME_DIR=/run/user/<uid>`, `ExecStartPre=+<bin> system acl-wait aural <uid> 90`, **`ExecStart=<bin> system daemon`** (the current script's bare `aural` would now show help), the full hardening block (ProtectSystem=strict, ReadWritePaths, ProtectHome=read-only with its load-bearing comment, PrivateTmp, etc.), `Restart=on-failure`; enable + restart
- session env: `/etc/environment.d/50-aural.conf` + `/etc/profile.d/aural.sh` (shared config dir) so your CLI/tray control the daemon
- per-login user unit `aural-pipewire-acl.service` re-applying the audio ACL at each login (runs `aural system acl-wait` — Rust port of the wait-then-`setfacl` loop)
- tray autostart `~/.config/autostart/aural-tray.desktop` → `aural system tray`
- idempotent reruns (getent/install-over checks); final summary separating "works now" from "needs log out & back in" (group/env), same as the script
- no systemd/udev present → fallback: sudo `usermod -aG input $SUDO_USER` + XDG autostart tray + start it (the simple mode, automated)
- `enable`/`disable` = `systemctl enable/disable aural` (sudo re-exec) in dedicated mode; XDG entry toggle in fallback mode. `uninstall` = the script's `--uninstall` ported (disable+stop units, revoke ACL, remove rule/unit/env/binary/tray entry, drop group + user + state). `doctor` absorbs the `--verify` checks (service active, user, ACL, groups, env, shared-dir writable)
- all file contents are byte-equivalent to the proven script (§11's hard-won lessons — ProtectHome=read-only, EPERM-alive, world-rw pipewire socket — carry over verbatim)

**Windows** — install now copies the exe to `%LOCALAPPDATA%\Programs\aural\aural.exe`; the Run key (`"<copy>" system daemon`) and daemon spawns target the copy (immune to `cargo clean`; replaces the build-dir warning); `uninstall` stops + deregisters + removes the copy. The daemon calls `FreeConsole()` at startup since the Run key launches it directly.

**Scripts** — `setup-dedicated-user.sh` deleted. `package-app.sh` stays as the CI/dev artifact packager only (release.yml builds `aural-macos-arm64-Aural.app.zip` with it; not part of user install). `extract-soundfonts.ps1` and the icon scripts are provenance/dev-only and stay.

## Code changes (carried from the settled rounds)

- `src/main.rs`: `Command` = `Stdin` + `System{action}` only; `SystemAction` as listed + hidden `Daemon`/`Tray`/`AclWait{user, uid, timeout}`; bare `aural` → in-bundle tray or `print_help()`; `arg_required_else_help` on `System`; `disclaim_needed` matches live Bench/Doctor/System{Doctor,Daemon,Tray}; shared `set_muted`/`toggle_muted`/`doctor()`.
- `src/menubar.rs`: `run()` flagless — probes the single-instance lock (held → control surface: Mute/Doctor/Quit; free → hosts engine, full menu with Install/Uninstall/Enable); strings say `aural system tray`.
- `src/daemon/`: spawn `system daemon`; Windows `FreeConsole`; doc comments.
- `src/system.rs`: macOS plist args + Linux XDG `system tray` + Windows Run key `system daemon`; Linux `start_app` = systemctl restart (dedicated) or detached tray (fallback).

## Docs

- README: setup is `cargo install aural` (Linux build-deps line stays) → `aural system install` on all three OSes; drop the `usermod` step and the dedicated-user section (one line pointing at DESIGN).
- DESIGN.md: §14 updated for the final surface, script absorption, `acl-wait`, Windows copy, bare-`aural`=help + in-bundle exception; §11 marked superseded-by-binary (history retained); §5 CLI bullet updated.

## Verification

- Gates: `cargo fmt --check`, `clippy --all-targets -D warnings`, `cargo test`, release build.
- One-time cleanup: `tccutil reset ListenEvent aural` for your three stale rows (auto-clean on future prompts already implemented).
- macOS smoke: `aural` == `--help`; old names → unrecognized; `system mute/unmute/toggle/about/doctor`; `system daemon` starts muted, visible in `doctor` while TCC-blocked, stopped by `system uninstall`; full `system install` → `uninstall` round-trip.
- The Linux privileged path cannot be runtime-tested from macOS: it is compile-gated by the Linux CI job and byte-equivalent to the script, but flagged here as needing your first real Linux `aural system install` run to confirm.
