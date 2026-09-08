# DESIGN — aural-keyboard (`aural`)

**Status:** Evergreen design reference — research, decisions, and how the shipped system works · **Originally agreed:** 2026-08-04 · **Repo:** <https://github.com/bevry-vibes/aural-keyboard> · **Platform order:** Windows 10 → macOS → Linux

> This document preserves the original intent, research, and reasoning so the foundational decisions can be re-hashed if reality disagrees.
> §7 lists the explicit triggers for revisiting each decision.
> It records *what is true now*, not how we got here: dated session devlogs live in [`.plans/`](.plans/) (one plan file per work arc), and per-release changes live in the [GitHub Releases](https://github.com/bevry-vibes/aural-keyboard/releases) notes.

## 1. Original prompt (verbatim)

> "We are wanting to make a macos, windows, and linux program that has minimal/zero latency
> to plays aural sound effects on keyboard presses. Currently, the login for these aural
> sound effects are only wihin vscode/atom extensions https://github.com/probablycorey/aural-coding
> and https://github.com/jengjeng/aural-coding-vscode - but we want such aural sound effects
> for system wide. We are currently on windows 10, and will do macos and linux later.
> Research all relevant keyboard sound projects https://github.com/stars/balupton/lists/keyboard-sounds
> to identify novel approaches, technology discernmens, elegant solutions towards latency and
> system integration, and propose a plan. Unfortunatelly, all system keyboard sounds care about
> mimicking mechanical keyboard sounds, which is against our aural musicality goal as per those
> two original project implementations - as such, we want the aural sound logic (happily
> reimplement for whatever language is best for the platform or for all platforms) combined
> with the system implementation approach of hte other proejcts (discarding their keyboard
> sound effects). It is completely fine for this to be a CLI only project with no interface.
> Do not use scripting languages, as they will be too slow. It must be a compilable language
> (rust, swift, c#, go, crystal, zig)."

Amendments made during discussion:

- **No mechanical-keyboard sounds/soundpacks may be copied** — system projects contribute integration *techniques only*; audio assets come solely from the aural lineage.
- Language evaluation must consider **all 23 starred projects** and include **Zig and Swift** (Swift's Windows/Linux toolchains are now official).
- Conventions per <https://github.com/bevry-labs/skills> (RPL-1.5 license, conventional commits with agent co-author trailer, bevry base .editorconfig/.gitattributes/.gitignore).

## 2. Goal

A CLI-only, system-wide, minimal-latency program that turns typing into **melodic music** (grand piano for letters, synth-drum percussion for other keys — with velocity and release envelopes), faithfully porting the aural-coding musical logic out of the editor sandbox and onto the OS.
Windows 10 first.
Success = typing anywhere feels like the Atom extension (< ~15 ms p95 press→sound), with zero UI.

## 3. Design research ([23 starred projects](https://github.com/stars/balupton/lists/keyboard-sounds), all evaluated)

**The two aural sources (musical logic — ported verbatim):**

- `probablycorey/aural-coding` (Atom, CoffeeScript): Web Audio graph; piano/drum MIDI.js soundfonts; letters→scale notes (vel 1.0, shift = +12 scale steps); specials→drum notes (GM-percussion numbering: backspace 50, delete 49, space 41 @ vel 0.025, tab 41, . 56, " 57, ' 58, + 61, [ 36, ] 37, ( 38, ) 39, ! 54 @ vel 2.0, else 45 @ vel 0.2); per-key retrigger dedup; noteOff = 0.5 s linear ramp; modifiers silent.
  The `majorScaleNotes` filter ((i+4)%12 ∈ {0,2,4,5,7,9,11} over MIDI 21..108) is labeled "C Major (I think?)" by its author — by pitch class it is actually **C Mixolydian** (C D E F G A Bb).
  We port the formula, not the intent.
- `jengjeng/aural-coding-vscode`: same logic on text-change events (lossy); per-note MP3s; known issue "slow audio player on Windows" — the per-event-process anti-pattern.

**System-wide implementations (techniques only — no sounds taken):**

| Finding | Projects |
|---|---|
| Rust cross-platform stack proven: rodio + symphonia + rubato + crossbeam, **custom native hooks** (winapi / core-graphics / x11), not rdev; device-change rerouting pitfall documented (pinned rodio git rev) | KeyEcho (850★) |
| Rust on macOS: rodio 0.20 + CGEventTap FFI + crossbeam worker channel; Input Monitoring (no Accessibility); re-grant after ad-hoc rebuilds | TickeysRedux |
| CLI/daemon UX: `start`/`stop`/PID file, pre-loaded WAVs, key-repeat dedup (rdev + rodio `play_raw` — no envelopes, hence our custom mixer) | Thockify-CLI |
| Latency gold standard: dedicated hook thread + **custom render callback = lock-free polyphonic mixer**, 64–128-frame buffers, PCM pre-decoded, **SPSC lock-free ring hook→audio**, RT-safe callback (no alloc/locks/ARC). Target <3 ms | TypeTock (Swift) |
| Voice-pool on AVAudioEngine; ±5% pitch / ±25% gain jitter; 30 s idle auto-suspend | keesound (Swift) |
| Win32 discipline: WH_KEYBOARD_LL (no admin), embedded WAVs, single-instance mutex, global mute hotkey, soundpack folders, ~2.1 MB RAM | clavis (C), keyboard-sounds-cpp (C++/BASS) |
| Feature ideas banked: spatial pan by physical key column; pitch/volume humanization; app rules; music-aware auto-mute; press/release distinction | mechvibes-x, TypeTock, keyboardsounds-pro (Go backend), SoundType (C#), thock |
| Anti-patterns: Electron/Python/JS stacks = 30–100 ms+ latency, 100 MB+ RAM | Mechvibes lineage, keyboardsounds (Python), keyBeats |
| Non-starters: closed-source (key-clicker), Discord plugin (Vencord), web typing test (keythm), docs repo (GK-Keyboard) | — |

**Novel gap confirmed:** every system app fires one-shot clicks; none do polyphonic pitched voices with velocity + release envelopes.
Our design = aural-coding's Web Audio semantics rebuilt as a game-audio RT mixer.

## 4. Analysis — language selection

Compared **Rust, Swift, Go, Crystal, Zig** on: Windows-first support, RT-safety in the audio callback, hooks+audio ecosystem per OS, single-binary/cross-compile, prior art.

- **Rust — chosen.**
  Tier-1 on all three OSes; no GC/runtime (RT-safe by design); complete pure-language stack (`windows` crate hooks, cpal/rodio audio, symphonia decode); 4 prior art projects in the surveyed set de-risking every hard choice.
- **Zig — strong #2, designated escape hatch.**
  Equal RT control, smallest binaries, best cross-compilation; viable stack exists (miniaudio via zaudio + hand-rolled hooks).
  Loses on pre-1.0 churn (0.16.0), zero prior art, everything hand-rolled.
  Revisit if binary size or dependency minimalism becomes the priority.
- **Swift — macOS-native alternate universe only.**
  Best-in-class on Darwin (all 6 macOS apps in the list are Swift; TypeTock's AUHAL design is the latency champion); off-Darwin the toolchain is official but the systems ecosystem is empty and ARC is an RT liability.
- **Go — #3.**
  Excellent CLI/daemon + oto audio; but hooks require cgo/libuiohook (breaks single-binary + cross-compile), GC pauses tension with ~2.6 ms callbacks, oto adds a buffering hop.
  Defensible only with a relaxed (~30 ms) latency target.
- **Crystal — eliminated.** Windows support officially Tier 2/incomplete; Boehm GC worst for RT; zero domain ecosystem.

## 5. Specification (summary)

- **Binary:** `aural` — single static exe, ~3 MB + ~1.5 MB embedded OGGs; ~40 MB RAM.
- **Pipeline:** WH_KEYBOARD_LL thread → translate/dedup → crossbeam SPSC ring → custom `rodio::Source` voice-pool mixer (32 voices, velocity gains, 0.5 s release ramps, stereo pan option, zero alloc/locks/I/O in render) → WASAPI shared (128–256-frame request, fallback to default).
- **Assets:** 37 notes (24 piano + 13 drum) FluidR3_GM OGGs (CC-BY 3.0, README → Attribution), extracted from the base64 `.js` bundles (`scripts/extract-soundfonts.ps1`), symphonia-decoded, rubato-resampled at load.
  **No mechanical samples.**
  Piano notes: D4,E4,F4,G4,A4,Bb4,C5,D5,E5,F5,G5,A5,Bb5,C6,D6,E6,F6,G6,A6,Bb6,C7,D7,E7,F7 (majorScaleNotes[24..47]).
  Drum notes: C2,Db2,D2,Eb2,F2,A2,Db3,D3,Gb3,Ab3,A3,Bb3,Db4 (MIDI 36,37,38,39,41,45,49,50,54,56,57,58,61).
- **Mapping:** verbatim port (§3); VK-based (letters+shift layout-safe; symbols US-assumed); layout-true characters (`ToUnicodeEx` / `UCKeyTranslate`) deferred (§8).
  Caps-lock awareness (a deliberate deviation from the original): caps lock swaps the letter registers — with caps on, plain letters take the shifted (higher) register and shift+letters the plain one, so the sound reflects the capital actually written; non-letters keep their shift sounds (`mapping::effective_shift`; state seeded from macOS session flags / Linux `EVIOCGLED` / Windows `GetKeyState`, kept synced by macOS `flagsChanged` and Linux `EV_LED`, with a toggle on caps key-down as the fallback).
- **CLI:** `aural stdin` (stdin test mode — no hook, no permissions) and `aural system <action>` — `install` / `uninstall` / `enable` / `disable` / `doctor` / `mute` / `unmute` / `toggle` / `volume` / `bench` / `about`. Bare `aural` prints help (except inside the Aural.app bundle, where a bare launch runs the tray — §9). The old v1 surface (`run`/`start`/`stop`/`install`/`menubar`/…) was replaced **outright, not deprecated** — old names get clap's unrecognized-subcommand error (`c972d3a`). Full surface and install behavior: §9.
- **Quality:** mapping unit tests vs. derived test vectors; mixer tests (envelope ±1 sample; no-alloc proof); CI fmt/clippy/test/build; release = zip + sha256.
- **Targets:** p95 < 15 ms press→sound. Verified — Windows 10 (2026-08-04, n=212, WASAPI shared, 128-frame @ 48 kHz): **p50 5.5 / p95 9.4 / p99 9.6 / max 9.8 ms**; macOS live typing (2026-08-29, n=142, M1, CoreAudio, 128-frame): **p50 1.42 / p95 2.49 / p99 2.56 / max 2.67 ms**. CPU ~0% idle; clean device-loss recovery (KeyEcho-documented rodio/cpal pitfall).

## 6. Considerations & known limitations

- WASAPI shared mode floors at ~3 ms period; exclusive mode possible only via custom windows-rs code (cpal doesn't expose it) — escape hatch, not planned.
- LL hooks don't fire into secure/elevated contexts (lock screen, elevated apps unless we run elevated, some games) — OS behavior. macOS has the parallel Secure Event Input blackout (§12); Linux evdev notably *does* see the lock screen (§13) — documented behavior differences, not bugs.
- macOS requires Input Monitoring permission, keyed to the code signature — ad-hoc rebuilds re-prompt; the stable self-signed identity and the installed app avoid it (§12, §15).
- Linux input capture needs read access to `/dev/input/event*`: either the `input` group or the dedicated-user isolation installed by `aural system install` (§9, §13).
- Privacy optics (a key listener): keys map to notes in-memory only; nothing stored or sent; stated in README (same stance as TypeTock/keesound).
- Soundfont RAM: stereo f32 ≈ 40 MB; mono/downsample/FLAC-streaming are future flags.

## 7. Decision register & re-hash triggers

| # | Decision | Revisit IF |
|---|---|---|
| D1 | Language = Rust | Compile velocity hurts; binary size becomes the priority (→ Zig+miniaudio); product pivots to macOS-native GUI (→ Swift shell) |
| D2 | rodio hosts output | Device-change bugs persist (→ raw cpal + own stream mgmt, kira, or miniaudio-sys) |
| D3 | Native `windows`-crate hooks (not rdev) | Maintaining 3 native backends exceeds rdev's quirk cost (→ consolidate on rdev) |
| D4 | Verbatim mapping incl. Mixolydian filter | Only with user-facing musical reason; the formula is the spec, "C major" comment is not |
| D5 | WASAPI shared mode | `bench` shows p95 > 15 ms AND users perceive lag (→ exclusive mode via windows-rs); tested 2026-08-04: p95 9.4 ms, stands |
| D6 | SPSC ring + voice-pool mixer | Load-bearing, TypeTock-validated; revisit only with bench evidence |
| D7 | FluidR3_GM OGG assets (CC-BY) | Licensing concern (→ synth mode / own recordings); RAM pressure (→ mono/downsample) |
| D8 | CLI-only | A GUI is requested — engine is shell-agnostic by design; add tray/GUI without touching it |
| D9 | Linux capture = evdev (below the display server) | A **portal-based global capture** route appears in the field (the freedesktop GlobalShortcuts portal covers hotkeys only; no global capture portal exists today) — re-evaluate for a permission-free path |
| D10 | Linux isolation = dedicated user + udev ACL + pipewire socket ACL bridge | PipeWire or WirePlumber tightens cross-UID client access by default (the bridge breaks; §13) |

## 8. Open items

> Platform sequencing (ruled 2026-08-04) is complete and feature work is unblocked: macOS landed 2026-08-30 (`5338012`), the menu-bar app 2026-08-31 (`0e1faf1`, 0.3.0), Linux 2026-09-05 (`76ead17`, 0.4.0), caps-lock registers 2026-09-06 (`af5cb26`, 0.5.0), the self-contained install 2026-09-09 (`c972d3a`, 0.6.0).
> The ports changed no behavior; defaults carried over as-is.

1. Default global mute hotkey Ctrl+Shift+F12 — confirm or change. It was carried unchanged through the ports (including macOS, where F12 sits on the Fn layer by default); configurable via `hotkey` in config.json.
2. Optional musical variants (off by default): "corrected" C-major scale option; stereo pan by key column; velocity humanization (±small %). Layout-aware letters (macOS `UCKeyTranslate`, Windows `ToUnicodeEx`) remain deferred with these.
3. First real-world Linux `aural system install` run — the privileged path is compile-gated by the Linux CI job and byte-equivalent to the proven setup script it absorbed, but has not yet been executed on a real Linux box.
4. Windows tray (optional) — the menubar/tray shipped on macOS and Linux; D8 keeps the engine shell-agnostic so this is a UI shell only.

## 9. System & CLI architecture

The surface is two user commands; everything else was removed outright (typing an old name gets clap's unrecognized-subcommand error, not a deprecation shim):

| Command | Effect |
|---|---|
| `aural` | same as `aural --help` |
| `aural stdin` | stdin mode — no hook, no permissions |
| `aural system install` | install the app (permissions included), start now + at login |
| `aural system uninstall` | stop the app, remove everything the install created |
| `aural system enable` / `disable` | toggle start-at-login |
| `aural system doctor` | diagnostics incl. engine state and the install chain |
| `aural system mute` / `unmute` / `toggle` / `volume` / `bench` / `about` | control & diagnostics |

Three **hidden internal** subcommands remain because the machinery invokes the binary across a process boundary (login entries, detached spawners, systemd units) — a command line is the only interface that crosses:

- `aural system daemon` — the engine in daemon mode; the child that `daemon::spawn_detached` launches, the Windows Run key runs at login, and the Linux systemd unit's `ExecStart`.
- `aural system tray` — the menu-bar/tray app; the macOS LaunchAgent (explicit `system tray` args), the Linux XDG autostart entry, and the dedicated-user `aural-tray.desktop`. **No `--no-engine`**: the tray auto-detects by probing the single-instance lock — held by another instance (the system-user daemon) → control surface (Mute/Open Doctor/Quit; lifecycle items hidden); free → hosts the engine. The probe releases before the engine thread takes the real lock; a race there just degrades the tray to control surface.
- `aural system acl-wait <user> <uid> <timeout>` — the pipewire audio bridge: wait for the session socket, then `setfacl` traverse on the runtime dir + rw on the socket.

**Bare `aural` = help**, with one exception: inside the Aural.app bundle a no-argument launch runs the tray — double-clicking the app must behave like an app (Finder/`open` pass no args).

### What `aural system install` does per platform

- **macOS** — wraps this binary as `~/Applications/Aural.app` (copy + embedded `AppIcon.icns` + `LSUIElement` plist + code-sign: prefer the stable "Aural Code Signing" self-signed identity so the TCC grant survives reinstall, else `AURAL_SIGN_IDENTITY`, else ad-hoc). LaunchAgent `ProgramArguments` = `[bundle binary, system, tray]`; launches via `open` right after install (LaunchServices attributes the TCC prompt to "Aural").
- **Linux** — `src/system/linux.rs`, the dedicated-user mode (§13).
  Two phases: the user phase explains the plan and re-execs `sudo <exe> system install`; the root phase (euid 0 + `SUDO_USER`) writes everything — `aural` system user + group, group-writable `/var/lib/aural` state, binary copied to `/usr/local/bin/aural` (immune to `cargo clean`), the keyboard-only udev ACL rule, the hardened `aural.service` (`ExecStart=aural system daemon`, `ExecStartPre=+aural system acl-wait`, full sandbox block), `AURAL_CONFIG_DIR` via environment.d + profile.d, the per-login `aural-pipewire-acl.service`, and the tray autostart.
  Without systemd: fallback to the simple mode (input group + autostarted engine-hosting tray).
  `enable`/`disable` = `systemctl enable/disable` (sudo re-exec); `uninstall` stops/disables units, revokes the ACL, removes files, drops group/user/state; `doctor` verifies the whole chain.
- **Windows** — copies the exe to `%LOCALAPPDATA%\Programs\aural\aural.exe`; the Run key runs `"<copy>" system daemon` at login.
  The daemon `FreeConsole()`s itself at startup — the Run key launches it directly (no `start` intermediary to apply CREATE_NO_WINDOW), so without that its console would linger for the whole session.

### The tray menu

Mute (check), Enable at Login (check), Install Aural, Uninstall Aural, Open Doctor, Quit — the install items greyed to match reality (Install when `system::installed()` — the artifacts exist — and Uninstall when not), and all three lifecycle items hidden in control-surface mode (Mute/Doctor/Quit only).
Invoking install/uninstall from within the running app is self-safe: `install()` short-circuits to the login registration when the recorded instance pid is this process; `uninstall()` removes files *before* the stop that kills this process (on macOS the bundle is removed while still executing from it — unlink only).

### Hardened behaviors (load-bearing)

- The engine writes its PID file in every mode (foreground, daemon, tray-hosted) and writes it *before* installing the hook — a grant-blocked instance must stay visible to `aural system doctor`/`uninstall` (the macOS grant wait can block for minutes).
- Single-instance `flock` actually excludes on unix — `Ok(None)` bails instead of falling through to run a second engine alongside the first (a latent double-sound bug); Windows already bailed.
  `stop()` waits for the instance to exit (≤3 s) so install's stop-then-start doesn't race the lock.
- Stale TCC rows are auto-cleared before prompting: every ad-hoc rebuild changes the cdhash and leaves the prior row behind, and every unanswered request parks one — all worthless on the "we don't have access" path.
  Both `CGRequestListenEventAccess` sites first run `tccutil reset ListenEvent <code-signing identifier>` ("aural" for a naked binary, "com.bevry.aural" in the bundle), so only *this* identity's rows clear; a grant under the other identity is untouched.

### Scripts

`package-app.sh` is the CI/dev packager for the release `Aural.app` zip (`release.yml`) — not part of user install.
`extract-soundfonts.ps1` and the icon scripts are provenance/dev-only.
(`setup-dedicated-user.sh` was deleted — absorbed into `src/system/linux.rs`.)

## 10. Platform notes — Windows

- **Hook** — `src/hook/windows.rs` (implied by D3): WH_KEYBOARD_LL on a dedicated thread, no admin required; listen-only pass-through.
- **Audio** — WASAPI shared via rodio/cpal; the 128–256-frame request with fallback.
- **Bench** (2026-08-04, Windows 10, n=212, 128-frame @ 48 kHz): p50 5.5 / p95 9.4 / p99 9.6 / max 9.8 ms.
- **Install** — exe copy to `%LOCALAPPDATA%\Programs\aural\aural.exe` + Run key `system daemon` (§9); the daemon `FreeConsole()`s at startup.
- **Toolchain** — both MSVC and GNU host toolchains work; with GNU, binutils (`dlltool`) must be on PATH for linking (§15).

## 11. Platform notes — macOS

- **Hook** — `src/hook/macos.rs`: listen-only `CGEventTap` (`kCGHIDEventTap`, head-insert, keyDown|keyUp|flagsChanged) on a dedicated `CFRunLoop` thread, hand-rolled CoreGraphics/CoreFoundation FFI per D3.
  Tap is re-enabled on `kCGEventTapDisabledByTimeout`; events always pass through (a listen-only tap cannot block input, parity with the Windows LL hook).
  `HookHandle` owns the run loop + thread; `stop` = `CFRunLoopStop`+`CFRunLoopWakeUp`+join (both documented thread-safe).
  Run loop serviced in `kCFRunLoopCommonModes`; `CGEventTapIsEnabled` asserted post-enable.
- **Key identity** — `src/keycodes.rs` translates positional CGKeyCodes to the Windows VK identity `mapping.rs` consumes (unit-tested table; unknown codes → `VK_UNKNOWN` 0xFF → default drum).
  Caveat: letters are US-assumed on macOS too — layout-true characters (`UCKeyTranslate`) deferred with the Windows `ToUnicodeEx` refinement (§8 item 2).
- **Shift/modifiers** arrive via `flagsChanged`; pressed state derived from `CGEventFlags`.
  Caps lock is reconciled against the shared pressed table (macOS only reports the "off" transition as a release).
  Held-key autorepeat dedup happens in the shared pressed table, as on Windows.
- **Mute hotkey** — detected in-tap (keycode + `CGEventFlags` modifier match); no Carbon hotkey API.
  Default: Ctrl+Shift+F12 (on default MacBook keyboards F12 sits on the Fn layer; configurable via `hotkey` in config.json).
- **Permissions (TCC)** — macOS gates keyboard capture behind System Settings → Privacy & Security → Input Monitoring; the prompt names the **responsible process**, and the grant covers everything it runs.
  `aural` re-execs itself as its own responsible process on launch (**self-disclaim** — `responsibility_spawnattrs_setdisclaim`, the documented Terminal.app/iTerm2 mechanism; child-spawn without `POSIX_SPAWN_SETEXEC`, `AURAL_DISCLAIMED=1` guard, parent forwards signals and exits with the child's status; `daemon::spawn_detached` strips the guard so the daemon re-evaluates; `--stdin`/`--synthetic` skip it; running inside the `.app` bundle skips it — LaunchServices already attributes "Aural"), so the prompt and grant key to aural itself in every mode: `open Aural.app`/double-click/login → **Aural**; a terminal-launched engine (the hidden daemon command) → **aural**.
After granting or toggling the entry, **relaunch** — the grant only takes effect on a fresh launch (`aural system install` relaunches the app).
`aural system doctor` disclaims too, so its Input Monitoring line reports the *CLI binary's* grant — the app (stable bundle identity) can be granted while a freshly built ad-hoc CLI is not; the message names both cases.
Pitfalls and diagnostics: §12.
- **SDK notes** — the macOS 26 SDK renamed `CGPreflightListenEventTapAccess` → `CGPreflightListenEventAccess` (same for Request…); we use the new names.
  With a permission dialog pending, `CGEventTapCreate` blocks until the user answers — expected TCC behavior, not a hang.
  HIToolbox lives at `/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/` (the old ApplicationServices subframework path is gone).
- **Daemon** — `src/daemon/unix.rs`: `pre_exec(setsid)` detach, stdio → `aural.log` at spawn, `kill(pid, 0/SIGTERM)` for status/stop, `flock` single-instance (`aural.lock`, auto-released on exit), LaunchAgent plist (`~/Library/LaunchAgents/com.bevry.aural.plist`, RunAtLoad) for enable/disable — parity with the Windows Run key (starts at login, not immediately).
- **Audio** — cpal 0.18 CoreAudio backend, engine code unchanged. The 128-frame buffer request succeeds on CoreAudio ("buffer 128 frames (requested)").
- **Toolchain** — the lockfile (symphonia 0.6, edition-2024) requires rustc/cargo ≥ 1.85; CI uses current stable.
  Direct dep: `libc` (unix-only; was already in the tree transitively).

## 12. macOS hard-won lessons

Reference distillation of the bring-up, resolution, and menubar sessions (full session narratives: `.plans/1786440659865-macos-bringup.md`, `.plans/1787984401000-macos-bringup-resolution.md`, `.plans/1788152861000-macos-menubar.md`).

### TCC / Input Monitoring (the recurring "no sound" trap)

1. **A TCC grant is keyed to the code signature (cdhash), not the name.**
   The `com.bevry.aural` row can sit in the DB as granted (`auth_value=2`) while the *current* binary still reports "NOT granted" — every re-sign changed the cdhash and the old grant no longer applies.
   Do not trust the DB row; trust `CGPreflightListenEventAccess()` / `aural doctor`.
2. **Ad-hoc signing re-signs every build → re-grant every build.**
   Fix: a stable self-signed identity ("Aural Code Signing"; §15 for creation) keeps the cdhash constant so the grant survives rebuilds — `aural system install` and `scripts/package-app.sh` auto-detect and prefer it.
3. **Self-disclaim creates a new TCC identity; the bundle must not disclaim.**
   A disclaimed re-exec of the bundled binary attributes to the raw binary path, which has no grant.
   `in_app_bundle()` in `main.rs` skips disclaim there — LaunchServices already attributes "Aural".
4. **A stale grant + a changed signature = "granted but still no sound."**
   Reset with `tccutil reset ListenEvent com.bevry.aural`, re-grant once with the current signature; the 3 s poll picks it up without a restart.
   The install/prompt path now auto-clears this identity's stale rows first (§9).
5. **A pending prompt for the same responsible process suppresses further prompts**, and the grant dialog only offers "Open System Settings"/Deny → user toggles → **relaunch required** — preflight does not flip live for an already-running process.
6. **Diagnose the hook vs. the audio separately.**
   `aural stdin` proves the audio path (engine/mixer/output) with no hook and no TCC.
   If that works but the hook doesn't, the problem is the hook/permission — not the sound system.

### Secure Event Input (the "modifiers sound, letters don't" blackout)

7. While any app holds secure input, macOS withholds `keyDown`/`keyUp` from **all** event taps system-wide; only `flagsChanged` (modifiers/Caps Lock) leak through — exactly the "Caps Lock plays, letters don't" symptom.
   Corroborated publicly: thock#127, ghostty#11883.
8. **Restart does NOT clear the flag** — quitting (or configuring) the holder does.
   Known holders: Terminal.app (Shell → Secure Keyboard Entry, and launchd can start it at boot with it on), Ghostty (auto-enables on password prompts; can stick).
9. Diagnostics: `IsSecureEventInputEnabled()` (HIToolbox via the Carbon path, §11); the holder's PID = `kCGSSessionSecureInputPID` from `ioreg -n Root -d1 -a` → `ps`.
   `aural doctor` reports both the state and the holder, with workaround instructions.

### Menu-bar / tray-icon runtime

10. **tray-icon's menu needs the AppKit run loop in event-tracking mode.**
    `NSRunLoop runUntilDate:` (default mode) renders the icon but drops menu clicks.
    Use `NSApplication.run()` (services all modes) + `MenuEvent::set_event_handler`, not a manual poll.
11. **muda menu items default to `enabled: false`.** Every `MenuItemBuilder`/ `CheckMenuItemBuilder` must call `.enabled(true)` or the whole menu renders greyed-out and unclickable.
12. **A colored status icon renders small vs. template icons.**
    Template icons (black+alpha) auto-tint and auto-size to fill the status bar; a colored image renders at a fixed 18 pt.
    Keep the colored icon by cropping to content and scaling up; generate at 44 px (2× of ~22 pt on retina) and **trim before downscaling** (downscale is sharp; upscaling a cropped glyph blurs it).
13. **`Icon::from_rgba` needs true RGBA.**
    `png`'s `EXPAND` does not expand GrayscaleAlpha→RGBA; force `png:color-type=6` when generating the icon or the decode fails with a pixel-count mismatch.
14. **`open -a Terminal <script>` avoids the Automation prompt.**
    AppleScript to control Terminal triggers a "control Terminal" permission prompt; plain LaunchServices `open` does not.
    The script needs the exec bit (`0o755`) or `open` refuses to run it.

## 13. Platform notes — Linux

- **Hook** — `src/hook/linux.rs`: listen-only evdev readers over `/dev/input/event*`, one per keyboard-class device (a device is a keyboard when it can produce both `KEY_A` and `KEY_SPACE` — excludes power buttons/headphone jacks), driven by one `poll(2)` loop on a dedicated thread.
  Devices are never grabbed (pass-through parity with the LL hook / listen-only tap).
  Hotplug: the device set is rescanned every 5 s (poll timeout 250 ms bounds shutdown latency).
  `Btn_*`/misc codes (0x100+) are filtered out before translation so mouse clicks stay silent; unmapped keyboard codes fall to `VK_UNKNOWN` (default drum).
  Evdev repeat (`value=2`) plus the shared pressed-table provide autorepeat dedup.
- **Why evdev (D9)** — GNOME 49+ on Wayland has no X11 fallback and X11 capture (XRecord/XGrabKey) cannot see native Wayland windows.
  evdev sits below the display server and works identically under X11 and Wayland.
  Price: reading `/dev/input/event*` requires the `input` group (`sudo usermod -aG input $USER` + re-login; `aural doctor` reports it) — or the dedicated-user isolation below.
  Unlike Win/mac hooks, evdev sees the lock screen (no secure-context silencing) — documented behavior difference.
- **Key identity** — `src/keycodes.rs` has an evdev `KEY_*` → VK table (set-1 scancodes are positional; letters US-assumed like macOS; layout-true characters stay deferred, §8 item 2).
  Unit-tested table (uniqueness, spot checks, hotkey reverse lookup).
- **Mute hotkey** — detected in the poll loop from the shared pressed-table (evdev reports modifier presses directly, like macOS's `flagsChanged`); same packed `(mods << 32) | vk` config as macOS.
  Default unchanged: Ctrl+Shift+F12.
- **Daemon** — `daemon/unix.rs` is shared POSIX code (setsid detach, PID file, flock single-instance, log redirection); autostart via XDG autostart or the systemd service below — parity with the Windows Run key / macOS LaunchAgent.
- **Tray** — shared menu logic + per-platform shells.
  Linux: `tray-icon` with its `gtk` feature (muda/gtk + libappindicator; `libxdo` disabled — only the separator is used), `gtk::init()` + `gtk::main()` on the main thread, menu events polled from muda's `MenuEvent::receiver()` crossbeam channel via a 50 ms glib timeout.
  GNOME displays the tray only with the **AppIndicator and KStatusNotifierItem Support** extension (`gnome-shell-extension-appindicator`); `aural doctor` reports when missing.
  Engine runs on a worker thread exactly as on macOS; the flock single-instance keeps the tray and daemon mutually exclusive.
- **Audio** — cpal 0.18 ALSA backend, engine code unchanged; build dep `alsa-lib-devel`.
- **CI/release** — `linux` job (ubuntu-22.04: `libappindicator3-dev` is still packaged there; `libasound2-dev`, `libgtk-3-dev`) with the same fmt/clippy/test/build gates; release artifact `aural-linux-x64.tar.gz` + sha256.
- **Dedicated-user hardening (D10)** — delivered by `aural system install` (`src/system/linux.rs`, §9; it absorbed the original setup script, contents byte-equivalent).
  The `input`-group route grants every process of the invoking user read access to all input devices; the dedicated mode instead runs the engine as an `aural` system user with a udev rule ACLing keyboard-only event nodes (`ID_INPUT_KEYBOARD`) to that user (no groups anywhere), a hardened `aural.service`, and audio bridged by ACL grants on the session runtime dir + the world-rw `pipewire-0` socket (exposure is audio-only; verified `srw-rw-rw-`).
  Shared control flows through `AURAL_CONFIG_DIR` (the daemon, the CLI, and the tray point at the group-writable `/var/lib/aural`); `alive()` counts EPERM as alive and `stop()` reports across the uid boundary; the tray auto-detects control-surface mode (§9).
  Two hard-won lessons for any system-service-to-user-session bridge: `ProtectHome=yes` makes `/run/user` **inaccessible (empty)** inside the service namespace — use `ProtectHome=read-only` when the service must reach session sockets (symptom: the daemon got ENOENT on the socket, reported as pipewire's `EHOSTDOWN` / "Host is down"); and SELinux was exonerated for that failure empirically (permissive mode changed nothing; zero AVC denials; the `unconfined_service_exec_t` relabel approach is invalid — the type does not exist in Fedora 44 policy, `chcon` returns EINVAL).
  Escape hatches recorded for re-hash: the broad alternative (file capability `cap_dac_read_search=ep` on the binary — narrower deployment, broader file-read exposure if aural is exploited) and the future narrowing (Landlock self-restriction of the hook thread's reads).
- **Packaging: Flatpak considered and deferred.**
  A sandboxed Flatpak runs as the invoking user with *granted* permissions — input capture would need `--device=input`, i.e. the app itself holds raw `/dev/input` access as the user.
  That cannot express the dedicated-user isolation model, which is the point of this mode; the daemon therefore stays a native system service.
  If Flathub distribution is ever wanted, the plausible shape is a Flatpak'd tray/config UI talking to the native daemon via `AURAL_CONFIG_DIR`.
- **Known follow-up:** `libappindicator` is legacy (GNOME-adjacent stacks are migrating to ayatana); if it breaks on a future desktop stack, swap the Linux shell to `ksni` (pure-Rust StatusNotifierItem over DBus) — the shared menu logic is the only part that would change.

## 14. Prior-art surveys

### macOS (working keyboard-sound apps vs. ours)

| Project | Lang | Capture | Tap point | Options | Run loop | Autorepeat |
|---|---|---|---|---|---|---|
| thock (860★) | Swift | CGEventTap | HID, **tail-append** | **defaultTap** (swallows events in "cleaning mode") | current + commonModes | dedup via pressedKeys |
| keesound | Swift | CGEventTap | **session**, head-insert | listenOnly | **main** + commonModes | **not filtered** (tracks on-screen chars) |
| TickeysRedux | Rust | CGEventTap | HID, head-insert | listenOnly, keyDown-only mask | current + commonModes | n/a (keyDown only) |
| KeyEcho (850★) | Rust | rdev → CGEventTap | (rdev default) | listen | rdev thread | filtered |
| Mechvibes et al | JS | iohook/global-listener | — | — | — | — |

**Every one of them uses CGEventTap-style capture; none detect or handle Secure Event Input.**
Our implementation (HID, head-insert, listen-only, dedicated thread) is equivalent.
Adopted from the survey: `kCFRunLoopCommonModes` (all three) and a `CGEventTapIsEnabled` post-enable assertion (TickeysRedux).
We keep: re-enable on `kCGEventTapDisabledByTimeout` (thock merely stops), the autorepeat filter (aural parity; keesound's unfiltered choice noted as a possible future config), and keycode logging gated behind `AURAL_LOG`.
TickeysRedux's README independently documents the ad-hoc regrant caveat and the Developer-ID fix.

**Karabiner-Elements coexistence:** Karabiner grabs physical keyboards and re-injects via its DriverKit virtual HID device; taps then observe the re-injected stream.
This coexists fine with all surveyed apps and with ours.
Karabiner does **not** set the secure-input flag; when secure input IS active even Karabiner itself can't see keystrokes.
No Karabiner changes needed.

### Linux (input capture & tray)

| Project | Stack | Linux status | Input capture | Permission model | Wayland | Tray |
|---|---|---|---|---|---|---|
| KeyEcho (860★) | Rust + Tauri | ships (x64/ARM64) | **X11 XInput2 raw events**: `XOpenDisplay` + `XISelectEvents(XI_RawKeyPress/Release, XIAllMasterDevices)` + blocking `XNextEvent` loop; hand-rolled (rdev-inspired), no rdev dep; `XIKeyRepeat` filtered at source | **none** — X11 client (any client can record on X11) | ✗ (fails `MissingDisplay` on pure Wayland; XWayland raw events only see X11-app input) | Tauri tray → **tray-icon crate (ours)**; audio = symphonia + rodio + LRU-decoded WAV cache |
| keyboardsounds (Python, →Pro) | Python daemon + desktop app | desktop beta | **session-switched: pynput/X11 on X11, libevdev on Wayland** | **`input` group via usermod required; explicitly "not supported when running as root"** | ✓ via libevdev | pystray → libappindicator (SNI); tkinter daemon window for OBS |
| keyboardsounds-pro (Go backend, active successor) | Go + GUI | ships | evdev (implied by the group requirement) | **`input` group via usermod required** | ✓ (evdev) | GUI app (SNI ecosystem) |
| keyboardsounds-prime | fork of keyboardsounds | same lineage | same | same | same | same |
| Mechvibes (Electron, legacy) / mechvibes-x fork | Electron + iohook | Linux builds existed; **mechvibes-x is Windows-only** | **iohook/libuiohook → X11 XRecord** | none (X11 client) | ✗ | Electron Tray → libappindicator (SNI) |
| mechvibes-dx (successor, in dev) | **Rust + webview rewrite** | in progress | n/a yet | n/a | n/a | n/a |
| Thockify-CLI (Rust, rdev + rodio) | Rust CLI | **Windows-only today** (Linux "planned"; config path already documented) | rdev (Linux backend = X11) | none (X11) | ✗ | none (CLI daemon; start/stop/PID file mirrors ours) |
| keyboard-sounds-cpp (C++/BASS) | C++ | **Windows-only** (Linux/macOS unchecked roadmap) | iohook (would be X11) | none | ✗ | traypp (Soundux) — same SNI family; BASS = proprietary, a licensing anti-pattern we avoided (D7) |
| macOS-only set (thock, keesound, TypeTock, kutuk, MKSTE, TickeysRedux, key-clicker, keyBeats) | — | no Linux | — | — | — | — |

Findings (the re-hash triggers they produced are D9/D10 in §7):

1. **The field has exactly two capture routes: X11 and evdev+`input` group.**
   X11 (XInput2 raw / XRecord / libuiohook) needs zero permissions but is dead on Wayland; evdev needs the group but works everywhere.
   The actively developed Linux efforts (keyboardsounds → Pro, aural) converged on evdev; KeyEcho's `MissingDisplay` failure mode is the counterfactual.
2. **Nobody goes beyond "join the `input` group".**
   No udev rules, no capabilities, no logind TakeDevice, no portals, no dedicated user; keyboardsounds explicitly refuses root.
   Our dedicated-user mode (keyboard-only udev ACL + hardened systemd service + `AURAL_CONFIG_DIR` state sharing) is beyond the state of the art in this niche — a differentiator worth documenting publicly and potentially upstreaming as a reference pattern.
3. **Tray: total convergence on StatusNotifierItem via libappindicator** — Electron Tray, pystray, traypp, and Tauri's tray-icon (our crate, via KeyEcho) all land on the same protocol.
   The GNOME AppIndicator extension requirement is industry-wide, not an aural defect.
   `ksni` (our recorded fallback) implements the same protocol without GTK; nothing in the field ships XEmbed or a custom menubar.
4. **Repeat suppression at source is universal best practice**: KeyEcho filters `XIKeyRepeat` in the event converter; we filter evdev `value=2` plus the pressed-table — parity confirmed.
5. **Audio-stack convergence validates D1/D6**: KeyEcho independently arrived at symphonia + rodio + pre-decoded caching (our bank is fully pre-decoded and resampled at load — stronger), and Mechvibes' own successor (mechvibes-dx) is an Electron→Rust rewrite.
   Both reinforce the §4 language decision and the anti-Electron stance.
6. **Login persistence** is undocumented across the set (Mechvibes uses Electron's auto-launch, i.e. XDG autostart under the hood); our XDG-autostart default and dedicated-user systemd service remain the most explicit approaches surveyed.

## 15. Signing, packaging, building

### macOS TCC attribution (user-facing summary)

macOS gates keyboard capture behind System Settings → Privacy & Security → Input Monitoring.
The prompt names the **responsible process** — the app macOS holds accountable — and the grant covers everything it runs.
`aural` self-disclaims on launch (§11), so the prompt and grant key to aural itself in every mode:

| How you run aural | Prompt names | Grant covers |
|---|---|---|
| the app (`open Aural.app` / double-click / login) | **Aural** | only aural |
| a terminal-launched engine (the hidden daemon command) | **aural** | only aural |

After granting or toggling the entry, **relaunch** aural — the grant only takes effect on a fresh launch (`aural system install` relaunches the app).
`aural system doctor` disclaims too, so its Input Monitoring line reports the *CLI binary's* grant — the app (stable bundle identity) can be granted while a freshly built ad-hoc CLI is not; the message names both cases.
Doctor also reports Secure Event Input state and names any app holding it (§12).

### App signing

The grant is keyed to the code signature, so ad-hoc rebuilds re-prompt.
For a stable identity, create a self-signed code-signing certificate (Keychain Access → Certificate Assistant → Create a Certificate → Self-Signed Root / Code Signing).
`aural system install` uses `AURAL_SIGN_IDENTITY` if set, else the "Aural Code Signing" identity if present in the login keychain, else ad-hoc — same rules as `scripts/package-app.sh`.

Dev-machine openssl route (CLI instead of Keychain GUI — end users never sign anything):

- `openssl req -x509` with `-addext` silently drops keyUsage/EKU (only basicConstraints applied) — use a `-config` file with `x509_extensions` (CA:TRUE, digitalSignature, codeSigning EKU, SKID/AKID).
- p12 import needs `openssl pkcs12 -export -legacy` (macOS SecKeychainItemImport rejects the default AES MAC: "MAC verification failed").
- `security find-identity -v -p codesigning` does NOT list the self-signed identity, but `codesign -s <name>` resolves it fine — trust codesign, not find-identity.
- The first `codesign` per imported key item prompts "codesign wants to access key" → **Always Allow** (once per imported key item). Silent thereafter.

### Building from source

`git clone … && cargo install --path .` (or `cargo build --release`) — the release binary lands in `target/release/aural`, or `$CARGO_TARGET_DIR/release/aural` when that is overridden.
On Windows both MSVC and GNU host toolchains work; with the GNU toolchain, binutils (`dlltool`) must be on PATH for linking.
Linux needs the ALSA + tray dev packages (Fedora: `alsa-lib-devel gtk3-devel libappindicator-gtk3-devel`; Debian/Ubuntu: `libasound2-dev libgtk-3-dev libappindicator3-dev`).
Quality gates (enforced by CI): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
