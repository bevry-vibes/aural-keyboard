# aural-keyboard (`aural`)

System-wide **melodic** keyboard sounds for Windows, macOS, and Linux — a faithful port of [aural-coding](https://github.com/probablycorey/aural-coding) out of the editor and onto the OS, with a real-time, game-audio-grade engine.
Letters play grand-piano notes in a scale (shift = higher register); every other key plays synth-drum percussion (space thumps quietly, backspace cracks, `!` crashes).
No mechanical-keyboard sounds, by design.

**How it sounds:**

| Key | Sound |
|---|---|
| `a`–`z` | grand piano, walking up the scale (shift = one register higher; caps lock swaps the registers so the sound matches the capital written) |
| space | deep drum, whisper-quiet (velocity 0.025) |
| backspace / delete | punchy toms (velocity 1.0) |
| `[` `]` `(` `)` | kick/snare family (GM percussion mapping) |
| `!` | crash (velocity 2.0) |
| everything else | low tom (velocity 0.2) |
| shift/ctrl/alt/win alone | silence |

Key release applies the original 0.5 s fade-out, so melodies ring naturally as you type.

CLI-first, with an optional menu-bar (macOS) / system-tray (Linux) app.
No telemetry, no network.
Keystrokes are mapped to notes in memory and immediately discarded — nothing is ever stored or sent anywhere.

## Setup

### Get the binary

Pick one:

- **Prebuilt binaries** — download from [Releases](https://github.com/bevry-vibes/aural-keyboard/releases): `aural-windows-x64.zip`, `aural-linux-x64.tar.gz`, `aural-macos-arm64.zip`, or `aural-macos-arm64-Aural.app.zip` (each with a sha256 sidecar).
  macOS downloads carry the quarantine attribute — after unzipping, run `xattr -d com.apple.quarantine ./aural` (or on `Aural.app`) once.
- **cargo install** — requires a [Rust toolchain](https://rustup.rs) (stable); puts `aural` on your PATH:

  ```sh
  cargo install aural                                            # from crates.io
  cargo install --git https://github.com/bevry-vibes/aural-keyboard   # from the repo
  ```

- **Build locally**:

  ```sh
  git clone https://github.com/bevry-vibes/aural-keyboard
  cd aural-keyboard
  cargo build --release
  ```

  The binary lands at `target/release/aural` under the repo — or `$CARGO_TARGET_DIR/release/aural` when that variable is set.
  Run it by that path, or use `cargo install --path .` from the clone to put `aural` on your PATH instead.

Compiling on Linux (cargo install or local build) needs the ALSA + tray dev packages (Fedora: `sudo dnf install alsa-lib-devel gtk3-devel libappindicator-gtk3-devel`; Debian/Ubuntu: `sudo apt install libasound2-dev libgtk-3-dev libappindicator3-dev`).

With the binary in hand, install the app once for your platform below — `aural system install` handles the permissions, starts now, and starts at every login.
(The `Aural.app` zip needs no CLI at all: move it to `/Applications` or `~/Applications`, open it, and use its menu-bar **Enable at Login**.)

### macOS

```sh
aural system install
```

This installs `Aural.app` in `~/Applications`, starts it (a menu-bar entry appears), and starts it at every login.
macOS asks once for **Input Monitoring** — the prompt names "Aural"; approve it in System Settings → Privacy & Security → Input Monitoring and the sounds start.
If they stay silent after granting, run `aural system install` once more (it relaunches the app), or run `aural system doctor` for a diagnosis.

### Windows

```powershell
aural system install
```

Starts the engine now and at every login. No permissions needed.

### Linux

```sh
aural system install
```

Asks for sudo once and sets up the hardened **dedicated-user mode**: the engine runs as a dedicated `aural` system user — the only account able to read keyboard devices — under a locked-down systemd service, with your CLI and tray sharing its state.
Sound works immediately; your session-level conveniences (CLI mute/volume, the tray control surface) need one log out & back in (`aural system doctor` verifies the chain).
On GNOME the tray icon needs the "AppIndicator and KStatusNotifierItem Support" extension:

```sh
sudo dnf install gnome-shell-extension-appindicator
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com   # then restart the shell
```

Without systemd the install falls back to the simple mode: your user joins the `input` group and the tray hosts the engine.
Details and security notes: [DESIGN.md](DESIGN.md) (§13).

## Usage

```text
aural                        print this help
aural stdin                  read keys from stdin instead of the OS hook (testing; no permissions)
aural system install         install the app — permissions included — and start it now + at login
aural system uninstall       stop the app and remove everything the install created
aural system enable          start the app automatically at login
aural system disable         don't start the app automatically at login
aural system doctor          diagnostics: engine, device, hook, permissions, install state
aural system mute | unmute | toggle
aural system volume 60       set volume (0-100)
aural system bench           measure press→sound latency (p50/p95/p99)
aural system about           version + sound attribution
```

The menu-bar/tray app (macOS, Linux) covers the same ground visually: **Mute**, **Enable at Login**, **Install Aural** (greyed once installed), **Uninstall Aural** (greyed once removed), **Open Doctor**, and **Quit**.

Global mute hotkey: **Ctrl+Shift+F12** (configurable via `config.json`, shown by `aural system doctor`).

## Design

Windows 10+, **macOS (Apple Silicon)**, and **Linux (X11 or Wayland)** are supported from one codebase.
Live `bench` on Windows 10: **p50 5.5 ms / p95 9.4 ms** press→sound, under the 15 ms target.
On macOS (M1, CoreAudio, 128-frame buffer): live bench **p50 1.42 ms / p95 2.49 ms**.
[`DESIGN.md`](DESIGN.md) holds the full research, analysis, and decision register — the evaluation of all [23 reference projects](https://github.com/stars/balupton/lists/keyboard-sounds), the language analysis, the latency architecture (lock-free SPSC ring → voice-pool mixer in the audio callback), and every platform's implementation notes.
It also holds the technical detail behind the setup above: the macOS TCC/permission model, app signing and packaging, building from source, and the Linux dedicated-user isolation mode.

## Attribution

### aural-coding (the original)

This project exists because of [aural-coding](https://github.com/probablycorey/aural-coding), created by [Corey Johnson](https://github.com/probablycorey) with [Kevin Sawicki](https://github.com/kevinsawicki) back in 2013, for the Atom editor.
Its musical mapping — letters walking up a scale on grand piano (shift = higher register), synth-drum percussion for every other key, the velocity choices, the 0.5 s release fade — is the foundation this engine faithfully ports to the OS level.
Without that original innovation, this project would never have happened.

### Sound samples

The bundled instrument samples (`assets/soundfonts/piano/*.ogg`, `assets/soundfonts/drums/*.ogg`) are per-note renderings from the **FluidR3_GM** General MIDI soundfont:

- Original Fluid R3 soundfont: Copyright &copy; 2000&ndash;2002, 2008 Frank Wen
- Pre-rendered per-note samples: [gleitz/midi-js-soundfonts](https://github.com/gleitz/midi-js-soundfonts),
  released under [Creative Commons Attribution 3.0 Unported (CC BY 3.0)](https://creativecommons.org/licenses/by/3.0/)
- Instruments used: `acoustic_grand_piano` (letters), `synth_drum` (all other keys)

These are the same soundfont lineages used by the original [aural-coding](https://github.com/probablycorey/aural-coding) project, whose musical mapping this program faithfully ports.

No mechanical-keyboard samples are used anywhere in this project, by design (see [`DESIGN.md`](DESIGN.md)).

### Extraction

The per-note OGGs are extracted from the upstream base64 `.js` bundles by [`scripts/extract-soundfonts.ps1`](scripts/extract-soundfonts.ps1) (kept for provenance/reproducibility; the bundles themselves are not committed).

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
