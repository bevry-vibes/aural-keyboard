# aural-system-keyboard (`aural`)

System-wide **melodic** keyboard sounds for Windows, macOS, and Linux — a faithful port of
[aural-coding](https://github.com/probablycorey/aural-coding) (Atom) /
[aural-coding-vscode](https://github.com/jengjeng/aural-coding-vscode) out of the editor and
onto the OS, with a real-time, game-audio-grade engine. Letters play grand-piano notes in a
scale (shift = higher register); every other key plays synth-drum percussion (space thumps
quietly, backspace cracks, `!` crashes). No mechanical-keyboard sounds, by design.

**How it sounds:**

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

CLI only. No UI, no telemetry, no network. Keystrokes are mapped to notes in memory and
immediately discarded — nothing is ever stored or sent anywhere.

## Design

### Status

Windows 10 first (active development). macOS and Linux follow from the same codebase —
see [`DESIGN.md`](DESIGN.md) for the full research, analysis, and decision register.
Live `bench` on Windows 10: **p50 5.5 ms / p95 9.4 ms** press→sound, under the 15 ms target.

### Research

[`DESIGN.md`](DESIGN.md) documents the original goal, the evaluation of all 23 reference
projects, the Rust/Swift/Go/Crystal/Zig language analysis, the latency architecture
(lock-free SPSC ring → voice-pool mixer in the audio callback), and the decision register
with explicit re-hash triggers.

## Setup

### Install

The binary is named `aural`. Requires a [Rust toolchain](https://rustup.rs) (stable) —
`cargo install` places `aural.exe` in `%USERPROFILE%\.cargo\bin`, which rustup puts on
your PATH.

**Straight from the repo (no clone needed):**

```powershell
cargo install --git https://github.com/bevry-labs/aural-system-keyboard
```

**From a local clone:**

```powershell
git clone https://github.com/bevry-labs/aural-system-keyboard
cd aural-system-keyboard
cargo install --path .
```

**From crates.io:** planned for the first tagged release (`cargo install aural`).

**Prebuilt binary:** CI uploads `aural.exe` as the `aural-windows-x64` artifact on every
green build (see the Actions tab); versioned GitHub Releases (zip + sha256) start at v0.1.0.

Note: the `aural install` *subcommand* is a different thing — it registers an
already-installed `aural` to start at login via the Windows Run key. See
[Usage](#usage).

### Building from source

Requires a [Rust toolchain](https://rustup.rs) (stable). Sound samples are committed
(see [Attribution](#attribution), CC BY 3.0 FluidR3_GM soundfont renderings), so a
plain build is all you need:

```powershell
git clone https://github.com/bevry-labs/aural-system-keyboard
cd aural-system-keyboard
cargo build --release
target\release\aural.exe run
```

On Windows both the MSVC and GNU host toolchains work; with the GNU toolchain, binutils
(`dlltool`) must be on PATH for linking. macOS and Linux builds are not wired up yet
(see [`DESIGN.md`](DESIGN.md)).

Quality gates (enforced by CI): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

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

## Attribution

### aural-coding (the original)

This project exists because of [aural-coding](https://github.com/probablycorey/aural-coding),
created by [Corey Johnson](https://github.com/probablycorey) with
[Kevin Sawicki](https://github.com/kevinsawicki) back in 2013, for the Atom editor. Its
musical mapping — letters walking up a scale on grand piano (shift = higher register),
synth-drum percussion for every other key, the velocity choices, the 0.5 s release fade —
is the foundation this engine faithfully ports to the OS level. Without that original
innovation, this project would never have happened.

### Sound samples

The bundled instrument samples (`assets/soundfonts/piano/*.ogg`,
`assets/soundfonts/drums/*.ogg`) are per-note renderings from the **FluidR3_GM**
General MIDI soundfont:

- Original Fluid R3 soundfont: Copyright &copy; 2000&ndash;2002, 2008 Frank Wen
- Pre-rendered per-note samples: [gleitz/midi-js-soundfonts](https://github.com/gleitz/midi-js-soundfonts),
  released under [Creative Commons Attribution 3.0 Unported (CC BY 3.0)](https://creativecommons.org/licenses/by/3.0/)
- Instruments used: `acoustic_grand_piano` (letters), `synth_drum` (all other keys)

These are the same soundfont lineages used by the original
[aural-coding](https://github.com/probablycorey/aural-coding) (Atom) and
[aural-coding-vscode](https://github.com/jengjeng/aural-coding-vscode) projects,
whose musical mapping this program faithfully ports.

No mechanical-keyboard samples are used anywhere in this project, by design
(see [`DESIGN.md`](DESIGN.md)).

### Extraction

The per-note OGGs are extracted from the upstream base64 `.js` bundles by
[`scripts/extract-soundfonts.ps1`](scripts/extract-soundfonts.ps1) (kept for
provenance/reproducibility; the bundles themselves are not committed).

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
