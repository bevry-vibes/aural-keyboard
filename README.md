# aural-keyboard (`aural`)

System-wide **melodic** keyboard sounds for Windows, macOS, and Linux — a faithful port of
[aural-coding](https://github.com/probablycorey/aural-coding) out of the editor and
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

Windows 10+, **macOS (Apple Silicon)**, and **Linux (X11 or Wayland)** are supported
from one codebase — see
[`DESIGN.md`](DESIGN.md) for the full research, analysis, and decision register.
macOS support has landed (2026-08-30) including a **menu-bar app**; Linux support has
landed (2026-09-02) including a **system-tray app**. Live `bench` on Windows 10:
**p50 5.5 ms / p95 9.4 ms** press→sound, under the 15 ms target. On macOS 26 (M1,
CoreAudio, 128-frame buffer): live bench **p50 1.42 ms / p95 2.49 ms** (n=142).

### Research

[`DESIGN.md`](DESIGN.md) documents the original goal, the evaluation of all [23 reference
projects](https://github.com/stars/balupton/lists/keyboard-sounds), the
Rust/Swift/Go/Crystal/Zig language analysis, the latency architecture
(lock-free SPSC ring → voice-pool mixer in the audio callback), and the decision register
with explicit re-hash triggers.

## Setup

### Install

The binary is named `aural`. Requires a [Rust toolchain](https://rustup.rs) (stable) —
`cargo install` places `aural.exe` in `%USERPROFILE%\.cargo\bin`, which rustup puts on
your PATH.

**From crates.io:**

```powershell
cargo install aural
```

**Straight from the repo (no clone needed):**

```powershell
cargo install --git https://github.com/bevry-vibes/aural-keyboard
```

**From a local clone:**

```powershell
git clone https://github.com/bevry-vibes/aural-keyboard
cd aural-keyboard
cargo install --path .
```

**Prebuilt binary:** download `aural-windows-x64.zip` or `aural-macos-arm64.zip` /
`aural-macos-arm64-Aural.app.zip` (each with a sha256 sidecar) from
[Releases](https://github.com/bevry-vibes/aural-keyboard/releases); CI also uploads
binaries as artifacts on every green build (see the Actions tab).
macOS downloads carry the quarantine attribute — after unzipping, run
`xattr -d com.apple.quarantine ./aural` (or on `Aural.app`) once.

Note: the `aural install` *subcommand* is a different thing — it registers an
already-installed `aural` to start at login (Windows Run key / macOS LaunchAgent). See
[Usage](#usage).

### Building from source

Requires a [Rust toolchain](https://rustup.rs) (stable). Sound samples are committed
(see [Attribution](#attribution), CC BY 3.0 FluidR3_GM soundfont renderings), so a
plain build is all you need:

```powershell
git clone https://github.com/bevry-vibes/aural-keyboard
cd aural-keyboard
cargo build --release
target\release\aural.exe run
```

On Windows both the MSVC and GNU host toolchains work; with the GNU toolchain, binutils
(`dlltool`) must be on PATH for linking. On macOS (Apple Silicon), any recent stable
toolchain works; see the next section for the one permission the OS requires. On Linux,
install the ALSA + tray build deps first (Fedora):

```sh
sudo dnf install alsa-lib-devel gtk3-devel libappindicator-gtk3-devel
```

(Debian/Ubuntu: `sudo apt install libasound2-dev libgtk-3-dev libappindicator3-dev`.)

Quality gates (enforced by CI): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

### macOS: Input Monitoring permission

macOS gates keyboard capture behind System Settings → Privacy & Security → Input
Monitoring. The prompt names the **responsible process** — the app macOS holds
accountable — and the grant covers everything it runs. `aural` re-execs itself
as its own responsible process on launch (self-disclaim), so the prompt and
grant key to aural itself in every launch mode:

| How you run `aural` | Prompt names | Grant covers |
|---|---|---|
| `aural run` from a terminal | **aural** | only aural |
| `open Aural.app --args run` (LaunchServices) | **Aural** | only aural |
| `aural install` (LaunchAgent, at login) | **aural** | only the daemon binary |

After granting or toggling the entry, **quit & reopen** aural — the grant only
takes effect on a fresh launch. `aural doctor` disclaims too, so its Input
Monitoring line reports aural's own grant (e.g. `granted` once aural is in the
list); `aural doctor` also reports Secure Event Input state and names any app
holding it.

The self-disclaim uses the same mechanism Terminal.app/iTerm2 use; for a stable
per-build identity, sign the app bundle with a self-signed certificate
(Keychain Access → Certificate Assistant → Create a Certificate → Code Signing)
and package with `AURAL_SIGN_IDENTITY="YourCert" ./scripts/package-app.sh`:

```sh
cargo build --release
./scripts/package-app.sh        # ad-hoc signs Aural.app (free, no Apple account)
open target/release/Aural.app --args run
```

### macOS menu-bar app

`aural menubar` hosts the engine (daemon) in-process and adds a status-bar item
with native checkboxes for **Mute** and **Enable at Login**, plus **Open Doctor**
and **Quit** (`LSUIElement`, so no Dock icon). Requires the same Input Monitoring
permission as `aural run`; grant it via System Settings → Privacy & Security →
Input Monitoring after first launch.

**Open Doctor** opens a new Terminal window running `aural doctor` (kept open so
you can read the diagnostics). The menubar only runs from within the packaged
`Aural.app` bundle — `aural menubar` from a bare binary refuses.

For the Input Monitoring grant to survive rebuilds, sign the bundle with a stable
self-signed identity (see `scripts/package-app.sh`); `package-app.sh` uses the
"Aural Code Signing" identity automatically if it exists in the login keychain,
falling back to ad-hoc otherwise.

The menubar icon (`assets/aural-menubar.png`) was created by Microsoft Copilot.

### Linux: input device access (`input` group)

The Linux hook reads the kernel's evdev devices (`/dev/input/event*`) — the only
global-capture route that works under both X11 and Wayland. Read access requires
membership in the `input` group:

```sh
sudo usermod -aG input $USER   # then log out and back in
```

`aural doctor` reports whether access is granted; `aural run` fails with these
instructions if it is not. Keys are translated to notes in memory and immediately
discarded — nothing is logged or stored (see [Usage](#usage)).

Note: evdev sits below the display server, so sounds also play on the lock screen
(unlike the Windows/macOS hooks, which the OS silences in secure contexts).

### Linux system-tray app

`aural menubar` hosts the engine (daemon) in-process and adds a system-tray icon
with checkboxes for **Mute** and **Enable at Login**, plus **Open Doctor** and
**Quit**. It registers a StatusNotifierItem via libappindicator; **GNOME shows it
only with the "AppIndicator and KStatusNotifierItem Support" extension enabled**:

```sh
sudo dnf install gnome-shell-extension-appindicator
# then enable and restart the shell (or log out/in):
gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
```

`aural doctor` reports when no StatusNotifier host is present. `aural install`
registers an XDG autostart entry (`~/.config/autostart/com.bevry.aural.desktop`)
so the daemon starts at login.

### Linux: dedicated-user mode (input isolation)

Adding yourself to the `input` group (the quick route above) gives **everything
running as your account** read access to all input devices. For stricter
isolation, run the engine as a dedicated `aural` system user instead — then
nothing running as you can read `/dev/input` at all:

```sh
sudo ./scripts/setup-dedicated-user.sh [path-to-aural-binary]
# undo:
sudo ./scripts/setup-dedicated-user.sh --uninstall
```

What it does:

- creates a `aural` system user (no shell, no groups) and a udev rule granting
  **that user** read access to keyboard event nodes only — mice/touchpads stay
  out of reach even for aural;
- runs the engine via a hardened `aural.service` (systemd) at login; control it
  with `systemctl status|start|stop aural` instead of `aural start/stop`;
- bridges audio by granting the `aural` user traverse on your runtime dir at
  each login (the pipewire sockets are already world-rw, so this exposes
  audio only);
- shares daemon state with your CLI/tray via `AURAL_CONFIG_DIR=/var/lib/aural`
  (you join the `aural` group), so `aural mute|unmute|toggle|volume|status`,
  the mute hotkey, and the tray all keep working;
- the tray becomes a control surface: `aural menubar --no-engine`.

If you previously added yourself to the `input` group, undo it:

```sh
sudo gpasswd -d $USER input    # then log out & back in
```

## Usage

```text
aural run                  run the engine in the foreground (Ctrl+C to quit)
aural run --stdin          read keys from stdin, not the OS hook (testing; no permissions)
aural start                start as a background daemon
aural stop                 stop the daemon
aural status               is it running?
aural mute | unmute | toggle
aural volume 60            set volume (0-100)
aural install              start automatically at login (Windows Run key / macOS LaunchAgent / Linux XDG autostart)
aural uninstall
aural bench                measure press→sound latency (p50/p95/p99)
aural doctor               diagnostics: device, buffer, hook, input access, assets
aural menubar              (macOS, Linux) run as a tray/menubar agent: tray icon with Mute,
                           Enable at Login, Open Doctor, and Quit
aural menubar --no-engine  (Linux, dedicated-user mode) tray control surface only —
                           the engine runs as the `aural` system user via systemd
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
[aural-coding](https://github.com/probablycorey/aural-coding) project,
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
