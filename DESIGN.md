# DESIGN — aural-system-keyboard (`aural`)

**Status:** Foundational design, agreed pre-implementation · **Date:** 2026-08-04 · **Repo:** <https://github.com/bevry-labs/aural-system-keyboard> · **Platform order:** Windows 10 → macOS → Linux

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

## 3. Design research (23 starred projects, all evaluated)

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

1. ~~**Commit co-author identity**~~ — resolved: `Co-authored-by: Cline - Kimi K3
   <cline-kimik3@local>` (per bevry `commits.md` known identities); used since the root
   commit.
2. ~~Binary name `aural`~~ — confirmed by the user (2026-08-04); unchanged.
3. Default global mute hotkey Ctrl+Shift+F12 — confirm or change.
4. Optional musical variants (off by default): "corrected" C-major scale option; stereo pan
   by key column; velocity humanization (±small %).
