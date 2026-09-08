# Linux dedicated-user mode — input isolation beyond the `input` group

> **Provenance:** retroactive plan, reconstructed 2026-09-09 from the DESIGN.md
> dedicated-user sub-entry inside "Linux implementation notes" (dated 2026-09-03,
> then §11) before DESIGN.md was reduced to evergreen content. The original
> prompts/plan for this session were not preserved — no `.prompts.md` companion
> exists. The epoch is the author timestamp of the landing commit `e03f522`
> (2026-09-05 +0800). The evergreen version (now delivered by `aural system
> install`, see `1788890250000-cli-restructure-self-contained-install.md`) lives
> in DESIGN.md "Platform notes — Linux".

## Context

The `input`-group requirement grants every process of the invoking user read
access to all input devices. For stricter isolation, move the engine to a
dedicated `aural` system user — beyond the state of the art of the surveyed
field (nobody goes beyond "join the `input` group").

## Work

`scripts/setup-dedicated-user.sh` (idempotent; rerun to update the binary;
`--uninstall`; `--verify` as yourself after relogin):

- udev rule ACLing keyboard-only event nodes (`ID_INPUT_KEYBOARD`) to the
  `aural` user — no groups anywhere.
- hardened `aural.service` runs the engine as that user.
- audio bridged by granting the user traverse on the session runtime dir at each
  login — sufficient because Fedora's `pipewire-0` socket is world-rw (verified
  `srw-rw-rw-`), so the exposure is audio-only.
- shared control via a new `AURAL_CONFIG_DIR` override on `config::dir()` (the
  daemon, the CLI, and the tray point at the group-writable `/var/lib/aural`);
  `alive()` counts EPERM as alive; `stop()` reports across the uid boundary;
  `aural menubar --no-engine` turns the tray into a pure control surface
  (login persistence belongs to the systemd service there).
- `--verify` checks the whole chain; final setup messaging separates
  daemon-side effects (immediate) from session-level conveniences (relogin:
  group membership, environment.d variables only apply to fresh sessions).

## Hard-won lessons (any future system-service-to-user-session bridge)

- **`ProtectHome=yes` makes `/run/user` inaccessible (empty) inside the service
  namespace** — use `ProtectHome=read-only` when the service must reach session
  sockets. Symptom: the daemon got ENOENT on the pipewire socket, reported as
  pipewire's `EHOSTDOWN` / "Host is down".
- **SELinux was exonerated empirically** (permissive mode changed nothing; zero
  AVC denials) after an invalid `unconfined_service_exec_t` relabel approach —
  the type does not exist in Fedora 44 policy (`chcon` returns EINVAL).

## Escape hatches recorded for re-hash

- The broad alternative: file capability `cap_dac_read_search=ep` on the binary
  — narrower deployment, broader file-read exposure if aural is exploited.
- The future narrowing: Landlock self-restriction of the hook thread's reads.

## Outcome

Landed with follow-up fixes (`aca601e` ProtectHome, `835b42b` deterministic
restart + verify cleanup, `4652464` never install the binary onto itself,
`a78c997`/`94d04e0` binary auto-find/robustness, `e708adb` tray autostart,
`c176d6a` verify-flow docs). Superseded 2026-09-09: the script was deleted and
absorbed into `aural system install` itself (`src/system/linux.rs`), file
contents byte-equivalent.
