# aural-system-keyboard (`aural`)

System-wide **melodic** keyboard sounds for Windows, macOS, and Linux — a faithful port of
[aural-coding](https://github.com/probablycorey/aural-coding) (Atom) /
[aural-coding-vscode](https://github.com/jengjeng/aural-coding-vscode) out of the editor and
onto the OS, with a real-time, game-audio-grade engine. Letters play grand-piano notes in a
scale (shift = higher register); every other key plays synth-drum percussion (space thumps
quietly, backspace cracks, `!` crashes). No mechanical-keyboard sounds, by design.

CLI only. No UI, no telemetry, no network. Keystrokes are mapped to notes in memory and
immediately discarded — nothing is ever stored or sent anywhere.

## Status

Windows 10 first (active development). macOS and Linux follow from the same codebase —
see [`DESIGN.md`](DESIGN.md) for the full research, analysis, and decision register.

## Usage

```text
aural run                  run the engine in the foreground (Ctrl+C to quit)
aural start                start as a background daemon
aural stop                 stop the daemon
aural status               is it running?
aural mute | unmute | toggle
aural volume 60            set volume (0-100)
aural install              start automatically at login (Windows Run key)
aural uninstall
aural bench                measure press→sound latency (p50/p95/p99)
aural doctor               diagnostics: device, buffer, hook, assets
aural about                version + sound attribution
```

Global mute hotkey: **Ctrl+Shift+F12** (configurable).

## How it sounds

| Key | Sound |
|---|---|
| `a`–`z` | grand piano, walking up the scale (shifted = one register higher) |
| space | deep drum, whisper-quiet (velocity 0.025) |
| backspace / delete | punchy toms (velocity 1.0) |
| `[` `]` `(` `)` | kick/snare family (GM percussion mapping) |
| `!` | crash (velocity 2.0) |
| everything else | low tom (velocity 0.2) |
| shift/ctrl/alt/win alone | silence |

Key release applies the original 0.5 s fade-out, so melodies ring naturally as you type.

## Building from source

Requires a [Rust toolchain](https://rustup.rs) (stable). Sound samples are committed
(see [`ATTRIBUTION.md`](ATTRIBUTION.md), CC BY 3.0 FluidR3_GM soundfont renderings).

```powershell
cargo build --release
target\release\aural.exe run
```

## Design & research

[`DESIGN.md`](DESIGN.md) documents the original goal, the evaluation of all 23 reference
projects, the Rust/Swift/Go/Crystal/Zig language analysis, the latency architecture
(lock-free SPSC ring → voice-pool mixer in the audio callback), and the decision register
with explicit re-hash triggers.

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
