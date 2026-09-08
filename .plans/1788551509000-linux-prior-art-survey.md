# Linux prior-art survey — input capture & tray approaches of the reference projects

> **Provenance:** retroactive plan, reconstructed 2026-09-09 from the DESIGN.md devlog
> entry "Linux prior-art survey (2026-09-03)" (then §13) before DESIGN.md was reduced
> to evergreen content. The original prompts/plan for this session were not preserved
> — no `.prompts.md` companion exists. The epoch is the author timestamp of the
> landing commit `0be6c5f` (2026-09-05 +0800). The evergreen survey tables live in
> DESIGN.md "Prior-art surveys".

## Task

Requested checkpoint before resuming dedicated-user debugging: evaluate the §3
reference projects' actual Linux approaches — security (input capture) and tray —
and compare against our port (then §11). Sources: each project's README + Linux
docs + (where it mattered) hook source.

## Survey

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

## Findings

1. **The field has exactly two capture routes: X11 and evdev+`input` group.** X11
   needs zero permissions but is dead on Wayland; evdev needs the group but works
   everywhere. The actively developed Linux efforts converged on evdev; KeyEcho's
   `MissingDisplay` failure mode is the counterfactual. Our evdev choice is the
   only route that works on modern Fedora GNOME.
2. **Nobody goes beyond "join the `input` group".** No udev rules, no
   capabilities, no logind TakeDevice, no portals, no dedicated user. Our
   dedicated-user mode is beyond the state of the art in this niche — worth
   documenting publicly / upstreaming as a reference pattern.
3. **Tray: total convergence on StatusNotifierItem via libappindicator** — the
   GNOME AppIndicator extension requirement is industry-wide, not an aural
   defect. `ksni` (our recorded fallback) implements the same protocol without
   GTK. No change warranted.
4. **Repeat suppression at source is universal best practice** — KeyEcho filters
   `XIKeyRepeat`; we filter evdev `value=2` plus the pressed-table. Parity
   confirmed.
5. **Audio-stack convergence validates D1/D6** — KeyEcho independently arrived at
   symphonia + rodio + pre-decoded caching (ours is fully pre-decoded and
   resampled at load — stronger), and Mechvibes' own successor is an
   Electron→Rust rewrite.
6. **Login persistence is undocumented across the set** (Mechvibes uses
   Electron's auto-launch, i.e. XDG autostart); our XDG-autostart default and
   dedicated-user systemd service remain the most explicit approaches surveyed.

## Re-hash triggers added (folded into the DESIGN decision register)

- If any project demonstrates a **portal-based capture** route (the freedesktop
  GlobalShortcuts portal covers hotkeys only; no global *capture* portal
  exists), re-evaluate evdev for a permission-free future path.
- If PipeWire or WirePlumber tightens cross-UID client access by default, the
  dedicated-user audio bridge needs revisiting.
