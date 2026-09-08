# macOS bring-up resolution — secure input root cause + self-disclaim attribution

> **Provenance:** retroactive plan, reconstructed 2026-09-09 from the DESIGN.md devlog
> entry "Bring-up complete (2026-08-29)" (then §11) before DESIGN.md was reduced to
> evergreen content. The original prompts/plan for this session were not preserved — no
> `.prompts.md` companion exists. The epoch is the author timestamp of the landing commit
> `5338012` (2026-08-29). The evergreen distillation of these learnings lives in
> DESIGN.md "macOS hard-won lessons".

## Context

The macOS port (see `1786440659865-macos-bringup.md`) was left blocked twice over:
the CGEventTap was deaf (only `flagsChanged` callbacks arrived), and TCC attribution
required launchd/LaunchServices because the private disown API appeared gone. An
instrumented build and a post-restart test plan were in place.

## Work

1. **Root-cause the deaf tap.** `ioreg -n Root -d1 -a` exposes
   `kCGSSessionSecureInputPID` — the PID holding Secure Event Input. Reading it
   identified **Terminal.app**, launched at boot by launchd with
   `SecureKeyboardEntry = true`. While any app holds secure input, macOS withholds
   `keyDown`/`keyUp` from all event taps system-wide; only `flagsChanged` leaks
   through — exactly the observed "Caps Lock sounds, letters don't". Fix: uncheck
   Terminal.app → Shell → Secure Keyboard Entry; verify
   `IsSecureEventInputEnabled = False`.
2. **Codify detection, not just the fix.** Restart does **not** clear the flag
   (disproved the earlier post-restart assumption). thock#127 and ghostty#11883
   document the same blackout, so `aural doctor` now reports secure-input state and
   names the holder (`kCGSSessionSecureInputPID` → `ps`) with workaround
   instructions.
3. **Solve TCC attribution via self-disclaim.** The earlier probe tested the wrong
   symbol: `responsibility_spawnattrs_setdisown` is gone, but
   **`responsibility_spawnattrs_setdisclaim`** is present in libSystem on macOS 26
   (verified via dlsym) — the documented mechanism Terminal.app/iTerm2 use
   (orca#12971; Qt "Curious Case"; Chromium/LLDB/Firefox; `disclaim` crate;
   `selfauth`).

## Implementation

`src/macos.rs`: before any side effects, commands that install the keyboard hook
(`run`, live `bench`) or report on it (`doctor`) re-exec a copy of self (same
argv/env, plus `AURAL_DISCLAIMED=1` guard; no file actions, stdio inherited) with
the disclaim posix_spawn attribute — the proven **child-spawn** pattern
(Terminal.app/iTerm2/selfauth/disclaim all spawn *without* `POSIX_SPAWN_SETEXEC`,
because the disclaim flag is applied in the spawn child and `SETEXEC` bypasses
it). The parent forwards SIGINT/SIGTERM/SIGHUP and exits with the child's status,
preserving foreground Ctrl+C and exit codes. `daemon::spawn_detached` strips the
guard env so the daemon re-evaluates disclaim for itself. `--stdin`/`--synthetic`
skip disclaim (no hook, no TCC). `doctor` also disclaims so its Input Monitoring
line reflects aural's own grant in every launch mode.

## Outcome

- Every launch mode prompts/grants under **aural/Aural**: plain `aural run`
  (disclaims), `open Aural.app --args …` (LaunchServices), `aural install`
  (launchd). The README permission table and the old §9 bundle-attribution claim
  were superseded.
- Live bench (real typing, post-grant, M1, CoreAudio, 128-frame buffer):
  **n=142, p50 1.42 ms / p95 2.49 ms / p99 2.56 ms / max 2.67 ms** — under even
  the synthetic pre-blocker figures; the tap path verified end-to-end.
- Ghostty (the host terminal) was exonerated — it never held the flag (it can
  re-hold it later via `macos-auto-secure-input` on password prompts,
  ghostty#11883).
