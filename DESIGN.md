# DESIGN — aural-keyboard (`aural`)

**Status:** Foundational design, agreed pre-implementation · **Date:** 2026-08-04 · **Repo:** <https://github.com/bevry-vibes/aural-keyboard> · **Platform order:** Windows 10 → macOS → Linux

> This document preserves the original intent, research, and reasoning so the
> foundational decisions can be re-hashed if reality disagrees. §7 lists the
> explicit triggers for revisiting each decision.

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

- **No mechanical-keyboard sounds/soundpacks may be copied** — system projects contribute
  integration *techniques only*; audio assets come solely from the aural lineage.
- Language evaluation must consider **all 23 starred projects** and include **Zig and Swift**
  (Swift's Windows/Linux toolchains are now official).
- Conventions per <https://github.com/bevry-labs/skills> (RPL-1.5 license, conventional
  commits with agent co-author trailer, bevry base .editorconfig/.gitattributes/.gitignore).

## 2. Goal

A CLI-only, system-wide, minimal-latency program that turns typing into **melodic music**
(grand piano for letters, synth-drum percussion for other keys — with velocity and release
envelopes), faithfully porting the aural-coding musical logic out of the editor sandbox and
onto the OS. Windows 10 first. Success = typing anywhere feels like the Atom extension
(< ~15 ms p95 press→sound), with zero UI.

## 3. Design research ([23 starred projects](https://github.com/stars/balupton/lists/keyboard-sounds), all evaluated)

**The two aural sources (musical logic — ported verbatim):**

- `probablycorey/aural-coding` (Atom, CoffeeScript): Web Audio graph; piano/drum MIDI.js
  soundfonts; letters→scale notes (vel 1.0, shift = +12 scale steps); specials→drum notes
  (GM-percussion numbering: backspace 50, delete 49, space 41 @ vel 0.025, tab 41, . 56,
  " 57, ' 58, + 61, [ 36, ] 37, ( 38, ) 39, ! 54 @ vel 2.0, else 45 @ vel 0.2);
  per-key retrigger dedup; noteOff = 0.5 s linear ramp; modifiers silent.
  The `majorScaleNotes` filter ((i+4)%12 ∈ {0,2,4,5,7,9,11} over MIDI 21..108) is labeled
  "C Major (I think?)" by its author — by pitch class it is actually **C Mixolydian**
  (C D E F G A Bb). We port the formula, not the intent.
- `jengjeng/aural-coding-vscode`: same logic on text-change events (lossy); per-note MP3s;
  known issue "slow audio player on Windows" — the per-event-process anti-pattern.

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

**Novel gap confirmed:** every system app fires one-shot clicks; none do polyphonic pitched
voices with velocity + release envelopes. Our design = aural-coding's Web Audio semantics
rebuilt as a game-audio RT mixer.

## 4. Analysis — language selection

Compared **Rust, Swift, Go, Crystal, Zig** on: Windows-first support, RT-safety in the
audio callback, hooks+audio ecosystem per OS, single-binary/cross-compile, prior art.

- **Rust — chosen.** Tier-1 on all three OSes; no GC/runtime (RT-safe by design); complete
  pure-language stack (`windows` crate hooks, cpal/rodio audio, symphonia decode); 4 prior
  art projects in the surveyed set de-risking every hard choice.
- **Zig — strong #2, designated escape hatch.** Equal RT control, smallest binaries, best
  cross-compilation; viable stack exists (miniaudio via zaudio + hand-rolled hooks). Loses on
  pre-1.0 churn (0.16.0), zero prior art, everything hand-rolled. Revisit if binary size or
  dependency minimalism becomes the priority.
- **Swift — macOS-native alternate universe only.** Best-in-class on Darwin (all 6 macOS
  apps in the list are Swift; TypeTock's AUHAL design is the latency champion); off-Darwin
  the toolchain is official but the systems ecosystem is empty and ARC is an RT liability.
- **Go — #3.** Excellent CLI/daemon + oto audio; but hooks require cgo/libuiohook (breaks
  single-binary + cross-compile), GC pauses tension with ~2.6 ms callbacks, oto adds a
  buffering hop. Defensible only with a relaxed (~30 ms) latency target.
- **Crystal — eliminated.** Windows support officially Tier 2/incomplete; Boehm GC worst for
  RT; zero domain ecosystem.

## 5. Specification (summary)

- **Binary:** `aural` — single static exe, ~3 MB + ~1.5 MB embedded OGGs; ~40 MB RAM.
- **Pipeline:** WH_KEYBOARD_LL thread → translate/dedup → crossbeam SPSC ring → custom
  `rodio::Source` voice-pool mixer (32 voices, velocity gains, 0.5 s release ramps, stereo
  pan option, zero alloc/locks/I/O in render) → WASAPI shared (128–256-frame request,
  fallback to default).
- **Assets:** 37 notes (24 piano + 13 drum) FluidR3_GM OGGs (CC-BY 3.0, README → Attribution),
  extracted from the base64 `.js` bundles (`scripts/extract-soundfonts.ps1`), symphonia-decoded,
  rubato-resampled at load. **No mechanical samples.**
  Piano notes: D4,E4,F4,G4,A4,Bb4,C5,D5,E5,F5,G5,A5,Bb5,C6,D6,E6,F6,G6,A6,Bb6,C7,D7,E7,F7
  (majorScaleNotes[24..47]). Drum notes: C2,Db2,D2,Eb2,F2,A2,Db3,D3,Gb3,Ab3,A3,Bb3,Db4
  (MIDI 36,37,38,39,41,45,49,50,54,56,57,58,61).
- **Mapping:** verbatim port (§3); v1 VK-based (letters+shift layout-safe; symbols
  US-assumed); v1.1 `ToUnicodeEx` for layout-true characters.
- **CLI:** `run`, `start/stop/status` (PID file), `install/uninstall` (Run key),
  `mute/unmute/toggle` (+ configurable global hotkey, default Ctrl+Shift+F12),
  `volume`, `bench` (p50/p95/p99 hook→mix), `doctor`, `about`.
- **Quality:** mapping unit tests vs. derived test vectors; mixer tests (envelope ±1
  sample; no-alloc proof); CI fmt/clippy/test/build; release = zip + sha256.
- **Targets:** p95 < 15 ms press→sound (`bench`-verified 2026-08-04: p50 5.5, p95 9.4, p99 9.6, max 9.8 ms; n=212, 128-frame buffer @ 48 kHz, Windows 10); CPU ~0% idle; clean device-loss
  recovery (KeyEcho-documented rodio/cpal pitfall).

## 6. Considerations & known limitations

- WASAPI shared mode floors at ~3 ms period; exclusive mode possible only via custom
  windows-rs code (cpal doesn't expose it) — escape hatch, not planned.
- LL hooks don't fire into secure/elevated contexts (lock screen, elevated apps unless we
  run elevated, some games) — OS behavior.
- Wayland: no global key capture without compositor portals; Linux target = X11 (x11rb) or
  rdev; evdev fallback needs `input` group. Documented, not solved.
- macOS requires Input Monitoring permission; ad-hoc rebuilds re-prompt (TickeysRedux note);
  Developer ID signature eliminates.
- Privacy optics (a key listener): keys map to notes in-memory only; nothing stored or sent;
  stated in README (same stance as TypeTock/keesound).
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

## 8. Open items pending user feedback

> **Sequencing (ruled 2026-08-04; macOS landed 2026-08-30):** platform parity
> outranks feature work — the macOS port then the Linux port ship before any of
> the feature items below (mute-hotkey changes, musical variants, extras) are
> entertained. The ports themselves change no behavior: existing defaults are
> carried over as-is. macOS shipped 2026-08-30 (bring-up, §10); **Linux is the
> remaining port before feature work.**

1. ~~**Commit co-author identity**~~ — resolved: `Co-authored-by: Cline - Kimi K3
   <cline-kimik3@local>` (per bevry `commits.md` known identities); used since the root
   commit.
2. ~~Binary name `aural`~~ — confirmed by the user (2026-08-04); unchanged.
3. Default global mute hotkey Ctrl+Shift+F12 — confirm or change. **Frozen by the
   sequencing note above:** the default stands unchanged through the macOS and Linux
   ports (including on macOS, where F12 sits on the Fn layer by default); revisit after.
4. Optional musical variants (off by default): "corrected" C-major scale option; stereo pan
   by key column; velocity humanization (±small %). **Frozen by the sequencing note above.**
   Same for layout-aware letters (macOS `UCKeyTranslate`, Windows `ToUnicodeEx`).
5. Menubar/status-bar/tray icon app. D8 keeps the engine shell-agnostic, so this is a
   UI shell only. **macOS `aural menubar` shipped 2026-08-31** (this branch) — a
   `tray-icon`-wrapped `NSStatusItem` hosting the engine with native checkboxes:
   **Mute**, **Enable at Login**, **Open Doctor**, and **Quit**; run the engine on a
   worker thread and pump the AppKit main loop for the menu (no winit/tao). Linux
   tray still pending (needs gtk/libappindicator); Windows tray optional.

## 9. macOS implementation notes (2026-08-04; macOS 26.6, M1 arm64)

- **Hook** — `src/hook/macos.rs`: listen-only `CGEventTap` (`kCGHIDEventTap`,
  head-insert, keyDown|keyUp|flagsChanged) on a dedicated `CFRunLoop` thread,
  hand-rolled CoreGraphics/CoreFoundation FFI per D3. Tap is re-enabled on
  `kCGEventTapDisabledByTimeout`; events always pass through (a listen-only tap
  cannot block input, parity with the Windows LL hook). `HookHandle` owns the
  run loop + thread; `stop` = `CFRunLoopStop`+`CFRunLoopWakeUp`+join (both
  documented thread-safe).
- **Key identity** — `src/keycodes.rs` translates positional CGKeyCodes to the
  Windows VK identity `mapping.rs` consumes (unit-tested table; unknown codes
  → `VK_UNKNOWN` 0xFF → default drum). Caveat: letters are US-assumed on macOS
  too — layout-true characters (`UCKeyTranslate`) deferred with the Windows
  `ToUnicodeEx` refinement (§8 item 4, frozen by sequencing).
- **Shift/modifiers** arrive via `flagsChanged`; pressed state derived from
  `CGEventFlags`. Caps lock is reconciled against the shared pressed table
  (macOS only reports the "off" transition as a release). Held-key autorepeat
  dedup happens in the shared pressed table, as on Windows.
- **Mute hotkey** — detected in-tap (keycode + `CGEventFlags` modifier match);
  no Carbon hotkey API. Default unchanged: Ctrl+Shift+F12 (on default MacBook
  keyboards F12 sits on the Fn layer; configurable via `hotkey` in config.json).
- **Permission model (TCC "responsible process")** — grants attach to the
  responsible process, inherited from the spawner: terminal-launched runs are
  attributed to the terminal; launchd-spawned and in-bundle executables get
  their own identity. The private disown API is gone (probed 2026-08-04:
  `responsibility_spawnattrs_setdisown` is absent from macOS 26 — only the
  query call survives), so bundle/launchd are the only per-app routes.
  **Superseded 2026-08-29** by the self-disclaim mechanism (§10, below): the
  still-present private symbol `responsibility_spawnattrs_setdisclaim` re-execs
  aural as its own responsible process, so plain `aural run` names aural too.
  Manually adding the binary in System Settings is inert: the responsible
  process, not the caller, is evaluated. `scripts/package-app.sh` wraps the
  same binary as `Aural.app` (ad-hoc signed; `AURAL_SIGN_IDENTITY` for a
  self-signed cert) — running its inner binary from a terminal attributes the
  prompt to "Aural". Ad-hoc identity is per-build, so rebuilds re-prompt.
  SDK rename: the macOS 26 SDK renamed `CGPreflightListenEventTapAccess` →
  `CGPreflightListenEventAccess` (same for Request…); we use the new names.
  Note: with a permission dialog pending, `CGEventTapCreate` blocks until the
  user answers — expected TCC behavior, not a hang.
- **Daemon** — `src/daemon/unix.rs`: `pre_exec(setsid)` detach, stdio →
  `aural.log` at spawn, `kill(pid, 0/SIGTERM)` for status/stop, `flock`
  single-instance (`aural.lock`, auto-released on exit), LaunchAgent plist
  (`~/Library/LaunchAgents/com.bevry.aural.plist`, RunAtLoad) for
  install/uninstall — parity with the Windows Run key (starts at login, not
  immediately).
- **Audio** — cpal 0.18 CoreAudio backend, engine code unchanged. The
  128-frame buffer request succeeds on CoreAudio ("buffer 128 frames
  (requested)"). Synthetic bench on M1 @ 48 kHz: **p50 1.36 ms / p95 2.64 ms**
  (Windows comparison from 2026-08-04: p50 5.5 / p95 9.4 on WASAPI shared).
- **Toolchain** — the lockfile (symphonia 0.6, edition-2024) requires
  rustc/cargo ≥ 1.85; CI uses current stable. New direct dep: `libc`
  (unix-only; was already in the tree transitively).

## 10. macOS bring-up state (2026-08-04 evening; rate-limit checkpoint)

### Working
- Full port compiles/gates green: fmt, clippy `-D warnings`, 22 tests,
  release build, `doctor`, synthetic bench (p50 1.2 ms).
- **`aural run --stdin`** (new, this session): chars from stdin become key
  presses (`mapping::vk_for_char`; uppercase/shifted punct sent as shift
  chords; Enter = Return; EOF stops engine). No hook, **no permissions** —
  proven end-to-end: `printf 'Hello World! (123)\n' | AURAL_LOG=1 aural run
  --stdin` maps every char correctly and exits 0. Paths:
  `/Users/balupton/.cargo/target/release/aural` (naked) and
  `.../Aural.app/Contents/MacOS/aural` (bundled, same binary).
- Daemon lifecycle, single-instance (flock), LaunchAgent install, bundle
  packaging (`scripts/package-app.sh`, ad-hoc; `AURAL_SIGN_IDENTITY` override).

### BLOCKER: event tap receives zero events
Symptom: preflight = **granted**, tap created, run loop running, process
launched via LaunchServices **after** the grant — yet live bench reports
`no samples collected` and no keys log. Engine/audio proven fine via stdin
mode. Root cause unknown; diagnostics queued below.

### TCC/macOS learnings (hard-won, all verified this session)
1. Responsible-process inheritance decides attribution; **bundle context alone
   does NOT** — tccd `AUTHREQ_ATTRIBUTION` showed `responsible=com.mitchellh.
   ghostty` for the bundled binary exec'd from a terminal. Only **launchd**
   (LaunchAgent) or **LaunchServices** (`open Aural.app --args …`) break
   inheritance. (README table row for the bundle MUST be corrected — see
   pending docs.)
2. macOS 26: `CGEventTapCreate` **succeeds-but-deaf** without permission → we
   gate on `CGPreflightListenEventAccess()` before creating the tap.
3. `CGRequestListenEventAccess()` only presents its prompt **while the
   requester is alive** → spawn waits (500 ms poll, 5 min cap) instead of
   fail-fast; continues automatically on grant.
4. A pending prompt for the same responsible process **suppresses** further
   prompts (why nothing re-appeared for Ghostty).
5. Grant dialog has only "Open System Settings"/Deny → user toggles →
   "**Quit & Reopen**" required for the grant to take effect on a running app.
6. **Ad-hoc cdhash changes every rebuild → re-grant every rebuild.** Fix:
   self-signed cert (Keychain Access → Certificate Assistant → Create a
   Certificate → Self-Signed Root / Code Signing, e.g. `aural-dev`), then
   `AURAL_SIGN_IDENTITY="aural-dev" ./scripts/package-app.sh`. Offered to
   user; pending their choice.
7. `doctor`'s permission line reflects the *responsible* process (terminal
   when run from a terminal) — not aural's bundle identity. Needs a doc
   caveat (or future responsible-process display).
8. Private `responsibility_spawnattrs_setdisown` is **removed** in macOS 26
   (probed). Manual Settings add of a naked binary is inert (responsible
   process is evaluated, not the caller).
9. Multiple stale "Aural" TCC entries can accumulate across rebuilds;
   Settings toggle must match the *current* build's identity.

### Next steps (in order)
1. User verifies `run --stdin` interactively (audible engine proof).
2. Optional: create `aural-dev` self-signed cert; repackage with it.
3. Re-grant Aural (Settings → Input Monitoring) for the CURRENT build.
4. Tap debug: `launchctl setenv AURAL_LOG 1`, then
   `open --stdout /tmp/bench.out --stderr /tmp/bench.err …/Aural.app --args bench`,
   user types, `kill -INT <pid>`, read files (`launchctl unsetenv AURAL_LOG`
   after). Callback now logs `tap event type N` (AURAL_LOG) at entry:
   - **No tap lines** → delivery never fires: try `kCGSessionEventTap`(1)
     instead of `kCGHIDEventTap`(0); inspect SkyLight logs
     (`log show --last 5m | grep -i skylight`); diff against known-good
     KeyEcho/TickeysRedux recipes.
   - **Tap lines, no samples** → bug in our callback/handle_key path.
5. Pending doc fixes: README permission table (bundle row = `open Aural.app
   --args run --daemon`, quit&reopen step, doctor caveat), README usage line
   for `--stdin`, DESIGN §9 corrections matching learning #1.
6. Live bench numbers into README/DESIGN; then **commit** everything
   (co-author trailer `Co-authored-by: Cline - Kimi K3 <cline-kimik3@local>`
   per §8.1). Uncommitted work tree currently holds the whole macOS port.

### Late-evening update: signing solved; deaf-tap root cause found (Secure Event Input)

**Codesigning — permanently fixed (dev machine):**
- Created self-signed `aural-dev` code-signing cert via openssl: `-addext` silently drops
  keyUsage/EKU with `req -x509` (only basicConstraints applied) — use a `-config` file with
  `x509_extensions` (CA:TRUE, digitalSignature, codeSigning EKU, SKID/AKID).
- p12 import needs `openssl pkcs12 -export -legacy` (macOS SecKeychainItemImport rejects the
  default AES MAC: "MAC verification failed").
- `security find-identity -v -p codesigning` does NOT list the self-signed identity, but
  `codesign -s aural-dev` resolves it fine — trust codesign, not find-identity.
- First `codesign` per imported key item prompts "codesign wants to access key aural-dev" →
  **Always Allow** (key was imported twice → two prompts, one each). Silent thereafter.
  This is dev-machine-only UX; end users never sign anything.
- `AURAL_SIGN_IDENTITY=aural-dev ./scripts/package-app.sh` → designated requirement
  `identifier "com.bevry.aural" and certificate leaf = H"61de87…"` — **stable across rebuilds,
  so the TCC grant now survives rebuilds**. Iteration is free again.

**TCC grant mechanics (verified against tccd logs):**
- A stale ad-hoc-era grant blocks a new cert requirement:
  `log show --predicate 'subsystem == "com.apple.TCC"'` shows
  "Failed to match existing code requirement … certificate leaf = H"…"".
- Fix: `tccutil reset ListenEvent com.bevry.aural`, relaunch → fresh modal prompt → enable
  toggle → **quit & reopen is mandatory**: preflight does NOT flip live for an already-running
  process (our 5-min wait-for-grant poll only helps when the grant is made before/between
  launches; after a Settings toggle the process must be relaunched). Adjust UX expectations
  accordingly; the wait loop is still useful for the first-run prompt race.

**Tap itself verified working:** callbacks fire; keycode table correct
(60→0xa1 RShift, 55→0x5b LCmd). TEMP-DIAG unconditional first-10 callback prints + a
secure-input startup check were added in `hook/macos.rs` (remove/gate before commit).

**CURRENT BLOCKER — Secure Event Input:**
`IsSecureEventInputEnabled()` returns **True globally**. While any app holds secure input,
macOS withholds keyDown/keyUp from ALL event taps system-wide; only FlagsChanged (modifier)
events leak through — exactly matches observations (10/10 callbacks were type=12 despite
typing letters in many apps; zero type=10/11).
- 1Password fully killed (`pkill -f 1Password`, incl. login-item helpers) → still True.
- Suspects still running: Ghostty (our host terminal — v1.1+ auto-enables secure input on
  password prompts and it can stick if something died mid-prompt; menu Ghostty → Secure Input
  toggles it), Terminal.app (Secure Keyboard Entry menu item), browsers with a stuck WebKit
  password field (Vivaldi/Orion), wox/Alfred-style launchers.
- Probe (no build needed):
  `python3 -c "import ctypes; lib=ctypes.CDLL('/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox'); f=lib.IsSecureEventInputEnabled; f.restype=ctypes.c_bool; print(f())"`
- **macOS 26 path change:** HIToolbox now lives at
  `/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/`; the old
  ApplicationServices subframework path is gone (not even in the dyld cache). The in-app diag
  used the old path and printed nothing — fix to the Carbon path when touching macos.rs.
- Bisect plan: toggle Ghostty's Secure Input (don't quit Ghostty — the agent runs inside it),
  uncheck Terminal.app's Secure Keyboard Entry, then quit browsers one at a time, probing
  after each. Once False, letters should play immediately (tap+grant already proven).

## 11. macOS project survey (2026-08-04; github.com/stars/balupton/lists/keyboard-sounds)

How the working macOS keyboard-sound apps capture keys, versus our approach:

| Project | Lang | Capture | Tap point | Options | Run loop | Autorepeat |
|---|---|---|---|---|---|---|
| thock (860★) | Swift | CGEventTap | HID, **tail-append** | **defaultTap** (swallows events in "cleaning mode") | current + commonModes | dedup via pressedKeys |
| keesound | Swift | CGEventTap | **session**, head-insert | listenOnly | **main** + commonModes | **not filtered** (tracks on-screen chars) |
| TickeysRedux | Rust | CGEventTap | HID, head-insert | listenOnly, keyDown-only mask | current + commonModes | n/a (keyDown only) |
| KeyEcho (850★) | Rust | rdev → CGEventTap | (rdev default) | listen | rdev thread | filtered |
| Mechvibes et al | JS | iohook/global-listener | — | — | — | — |

**Every one of them uses CGEventTap-style capture; none detect or handle Secure Event
Input.** Our implementation (HID, head-insert, listen-only, dedicated thread) is equivalent.

Corroborating thock issues (proof the blackout is environmental, not our bug):
- **#127 "ONLY function keys work on mac"** (open, 2026-06): "works perfectly when I start…
  but after that only command shift capslock option control and fn work, others have no
  sounds" — **identical to our symptom** (flagsChanged flow, keyDown/Up withheld). Same root
  cause as ours: secure input enabled mid-session by some app.
- #114 "app not working on Tahoe 26.4.1" (open): 1.23.0 silent for several users, "downgrade
  to 1.22.0 works" — possibly a thock-1.23 regression, possibly the same class of issue.

Adopted from the survey (queued, post-bring-up): `kCFRunLoopCommonModes` instead of
DefaultMode (all three); `CGEventTapIsEnabled` post-enable assertion (TickeysRedux). We keep:
disabled→re-enable (thock merely stops), autorepeat filter (aural parity; keesound's
unfiltered choice noted as a possible future config), keycode logging gated behind AURAL_LOG
(keesound's "classify in-callback, keycode never escapes" pattern noted; the TEMP-DIAG prints
must be removed before commit). TickeysRedux's README independently documents the ad-hoc
regrant caveat and the Developer-ID fix — validates our cert approach.

**Karabiner-Elements** (installed+enabled on the dev machine, do-not-modify): grabs physical
keyboards and re-injects via its DriverKit virtual HID device; taps then observe the
re-injected stream. This coexists fine with all surveyed apps and with ours (the modifier
events we receive arrive through that path). Karabiner does **not** set the secure-input flag,
so it is not the deafness cause; per Apple docs, when secure input IS active even Karabiner
itself can't see keystrokes. No Karabiner changes made or needed.

Caps Lock datum (user-observed): Caps Lock plays a sound, letters don't. Caps Lock arrives as
flagsChanged (type 12) — which macOS still delivers to taps during secure input — while
letter keyDown/Up (type 10/11) are withheld. Direct live confirmation of the diagnosis.


**Research task handed off:** compare capture approaches of the macOS projects in
https://github.com/stars/balupton/lists/keyboard-sounds (thock, TickeysRedux, KeyEcho,
TypeTock, keesound, kutuk, MKSTE…) — tap location, permission model, and whether/how they
detect or document the secure-input blackout.


### End-of-session state (2026-08-04 night; machine restart planned)

- All aural processes stopped. Secure input was **still True** at session end; the holding
  app was not yet identified. Eliminated: 1Password (killed, incl. helpers — flag stayed True).
  Not yet tested: Terminal.app (Secure Keyboard Entry toggle), Ghostty (host terminal; check
  its app menu for a Secure Input item — do NOT quit it while the agent runs), Keychain
  Access, Vivaldi/Orion (stuck WebKit password field), wox/Alfred/Adguard, mail/chat apps.
- Instrumented build in place: diag prints first 50 tap callbacks (type/keycode/vk) +
  startup `IsSecureEventInputEnabled` print (correct Carbon path). Grant persists across the
  restart (cert-signed, requirement = certificate leaf hash) — no re-grant needed.

**Post-restart test plan (ordered):**
1. Restart clears the secure-input flag. Open **only Ghostty** (needed for the agent), then:
   `open --stdout /tmp/bench.out --stderr /tmp/bench.err ~/.cargo/target/release/Aural.app --args bench`
   The log prints `IsSecureEventInputEnabled = false` if clear. Type letters → expect sound.
   If letters work with Ghostty running, Ghostty is exonerated; if not, Ghostty is prime suspect.
2. If clear: open remaining apps **one at a time**, typing a few letters after each
   (Terminal.app, Keychain Access, Vivaldi, Orion, 1Password, Signal/WhatsApp/Discord,
   thunderbird, Mail…). When letters go silent (`IsSecureEventInputEnabled` flips true), the
   last-opened app is the securer → disable its secure-entry feature or avoid it.
3. Once letters play: remove the TEMP-DIAG block from `hook/macos.rs`, adopt
   `kCFRunLoopCommonModes` + `CGEventTapIsEnabled` assertion (§11), capture live-bench
   latency numbers for the docs, then commit (tree intentionally uncommitted all session).

### Bring-up complete (2026-08-29) — secure input root-caused, self-disclaim resolves attribution

**Secure Event Input — root cause found and fixed.** `ioreg -n Root -d1 -a`
showed `kCGSSessionSecureInputPID = 944` = **Terminal.app**, launched at boot by
launchd with its `SecureKeyboardEntry = true`. While any app holds secure input,
macOS 26 withholds `keyDown`/`keyUp` from **all** event taps system-wide; only
`flagsChanged` (modifiers/Caps Lock) leaks through — exactly the "Caps Lock
sounds, letters don't" symptom. Ghostty (the host terminal) was exonerated; its
toggle had no effect because it never held the flag (it can re-hold later via
`macos-auto-secure-input` on password prompts, ghostty#11883). Fix: uncheck
Terminal.app → Shell → Secure Keyboard Entry. Verified
`IsSecureEventInputEnabled = False`. **Restart does NOT clear the flag**
(disproving the post-restart assumption above). Not a freak thing — thock#127
and ghostty#11883 document the same blackout, so we codified detection in
`aural doctor` (names the holder via `kCGSSessionSecureInputPID` → `ps`) and
left the workaround instructions there.

**TCC attribution solved via self-disclaim.** The correct private API is
**`responsibility_spawnattrs_setdisclaim`** — present in libSystem on macOS 26
(verified via dlsym), the documented mechanism Terminal.app/iTerm2 use, still
honored by macOS 26 (orca#12971; Qt "Curious Case"; Chromium/LLDB/Firefox;
`disclaim` crate; `selfauth`). Earlier probes tested the wrong name
(`setdisown`, removed). DESIGN learning #1 stands: Ghostty does **not**
disclaim, so aural disclaims **itself**.

Implementation (`src/macos.rs`): before any side effects, commands that install
the keyboard hook (`run`, live `bench`) or report on it (`doctor`) re-exec a
copy of self (same argv/env, plus `AURAL_DISCLAIMED=1` guard; no file actions,
so stdio is inherited) with the disclaim posix_spawn attribute. We use the
proven **child-spawn** pattern — Terminal.app/iTerm2/selfauth/disclaim all spawn
without `POSIX_SPAWN_SETEXEC`, because the disclaim flag is applied in the
spawn child and `SETEXEC` bypasses it. The parent forwards
SIGINT/SIGTERM/SIGHUP and exits with the child's status, preserving foreground
Ctrl+C and exit-code behavior. `daemon::spawn_detached` strips the guard env so
the daemon always re-evaluates disclaim for itself. `--stdin` and `--synthetic`
skip disclaim (no hook, no TCC needed).

Net effect: **every launch mode prompts/grants under aural/Aural** — plain
`aural run` (disclaims), `open Aural.app --args …` (LaunchServices), and
`aural install` (launchd). The README permission table and the §9
bundle-attribution claim are superseded accordingly.

`aural doctor` gained `secure input: not active` / names-the-holder reporting,
and doctor also disclaims itself — so its Input Monitoring line now reflects
aural's own grant in every launch mode (the learning #7 caveat predates
doctor's self-disclaim and no longer applies).

**Live bench (real typing, post-grant, M1, CoreAudio, 128-frame buffer):**
**n=142, p50 1.42 ms / p95 2.49 ms / p99 2.56 ms / max 2.67 ms**. The TCC
prompt named **aural**; typing produced sound; the tap path is verified
end-to-end and lands under even the synthetic pre-blocker figures.

## 12. macOS menu-bar app learnings (2026-08-31; `aural menubar`)

The menu-bar app shipped this session. The permission/audio debugging took far
too long; these are the hard-won lessons so it never recurs.

### TCC / Input Monitoring (the recurring "no sound" trap)
1. **A TCC grant is keyed to the code signature (cdhash), not the name.** The
   `com.bevry.aural` row can sit in the DB as granted (`auth_value=2`) while
   the *current* binary still reports "NOT granted" — because every re-sign
   changed the cdhash and the old grant no longer applies. **Do not trust the
   DB row; trust `CGPreflightListenEventAccess()` / `aural doctor`.**
2. **Ad-hoc signing re-signs on every build → re-grant every build.** This was
   the single biggest time sink. Fix: a **stable self-signed identity**
   (`AURAL_SIGN_IDENTITY="Aural Code Signing"`), which keeps the cdhash
   constant so the grant persists. `scripts/package-app.sh` now auto-detects
   the identity and defaults to it. (Learning #6 in §10 was the same root
   cause; the menubar hit it again because the bundle was ad-hoc.)
3. **The self-disclaim breaks the bundle's grant.** `aural menubar` re-exec'd
   itself disclaimed, creating a *new* TCC identity (the raw binary path) that
   wasn't granted — so the hook saw "NOT granted" even though `com.bevry.aural`
   was granted. Fix: **skip disclaim when running inside the `.app` bundle**
   (`in_app_bundle()` in `main.rs`), where LaunchServices already attributes
   the grant to "Aural".
4. **A stale grant + a changed signature = the "granted but still no sound"
   paradox.** The DB says granted, the app says not. Reset with
   `tccutil reset ListenEvent com.bevry.aural`, then re-grant once with the
   current signature. The 3 s poll (was 500 ms) picks it up without a restart.
5. **Diagnose the hook vs. audio separately.** `aural run --stdin` proves the
   audio path (engine/mixer/output) with no hook and no TCC. If that works but
   the hook doesn't, the problem is the hook/permission — not the sound system.
   This split saved the session.

### Menu-bar / tray-icon runtime
6. **tray-icon's menu needs the AppKit run loop in event-tracking mode.**
   `NSRunLoop runUntilDate:` (default mode) renders the icon but drops menu
   clicks. Use `NSApplication.run()` (services all modes) + `MenuEvent::
   set_event_handler`, not a manual poll.
7. **muda menu items default to `enabled: false`.** Every `MenuItemBuilder`/
   `CheckMenuItemBuilder` must call `.enabled(true)` or the whole menu renders
   greyed-out and unclickable.
8. **A colored status icon renders small vs. template icons.** Template icons
   (black+alpha) auto-tint and auto-size to fill the status bar; a colored
   image renders at a fixed 18 pt. To keep the colored icon, crop to content
   and scale up to fill the canvas; generate at 44 px (2× of ~22 pt on retina)
   and **trim before downscaling** (downscale is sharp; upscaling a cropped
   glyph blurs it).
9. **`Icon::from_rgba` needs true RGBA.** `png`'s `EXPAND` does not expand
   GrayscaleAlpha→RGBA; force `png:color-type=6` when generating the icon or
   the decode fails with a pixel-count mismatch.
10. **`open -a Terminal <script>` avoids the Automation prompt.** AppleScript
    (`osascript`) to control Terminal triggers a "control Terminal" permission
    prompt; plain LaunchServices `open` does not. The script needs the exec bit
    (`0o755`) or `open` refuses to run it.


