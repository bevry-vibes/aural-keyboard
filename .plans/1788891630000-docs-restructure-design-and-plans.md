# Adopt `.plans/` convention + reduce DESIGN.md to evergreen reference + retroactive devlog plans

> Prompts: [1788891630000-docs-restructure-design-and-plans.prompts.md](1788891630000-docs-restructure-design-and-plans.prompts.md)

## Context

- Upstream `bevry-vibes/skills/plans.md`: every plan goes to repo-root `.plans/<epoch-ms>-<slug>.md` (committed, never harness-private dirs). This repo has 2 tracked plans in `.kilo/plans/` (already epoch-named), 1 untracked plan in `.zcode/plans/` (user confirmed: move it too), and no `.plans/` yet. `.zcode/` is gitignored; `.plans/` is not ignored.
- DESIGN.md (859 lines) mixes evergreen reference with dated session devlogs; numbering is broken (two "§11" headings, §11–14 appear after §9–10). `Cargo.toml:28` references "DESIGN.md D8" → decision IDs D1–D8 must keep their numbering.
- Retroactive plan epochs = git author timestamp ×1000 of the landing commit (observable, never fabricated); each file carries a provenance note stating it was reconstructed from the DESIGN.md devlog and that no prompts survived (no `.prompts.md` companions — fabricating prompts is forbidden).

## Step 1 — `.plans/` adoption (moves)

1. `git mv .kilo/plans/1786439075642-create-agents-md.md .plans/`
2. `git mv .kilo/plans/1786440659865-macos-bringup.md .plans/`
3. Move `.zcode/plans/plan-sess_e4dc50e8-8d98-41fd-b975-cfa531940d4c.md` → `.plans/1788890250000-cli-restructure-self-contained-install.md` (plain move + `git add` — currently untracked), prepending a short provenance note: rescued from harness-private `.zcode/plans/` during `.plans/` adoption.
4. AGENTS.md: add the `plans.md` skill URL to the referenced upstream skills list.

## Step 2 — retroactive plan files (reconstructed from DESIGN.md devlog entries)

One retro file per devlog arc (epoch = landing commit author-time ×1000; body = plan-shaped record of what was planned/attempted/learned; provenance blockquote citing DESIGN §, its stated date, and the commit):

1. `1787984401000-macos-bringup-resolution.md` — from §11 "Bring-up complete" (2026-08-29): `ioreg kCGSSessionSecureInputPID` root-cause (Terminal.app; restart does NOT clear the flag; flagsChanged-only symptom; thock#127 / ghostty#11883 corroboration; doctor detection), self-disclaim mechanism (`responsibility_spawnattrs_setdisclaim`, child-spawn without `SETEXEC`, `AURAL_DISCLAIMED` guard, signal forwarding, daemon re-evaluation), live bench n=142 p50 1.42 / p95 2.49 ms.
2. `1788152861000-macos-menubar.md` — from §12 (2026-08-31): the ten hard-won TCC + tray lessons as the session record.
3. `1788541494000-linux-port.md` — from §11-Linux base notes: evdev hook design, keycode table, mute hotkey, POSIX daemon, GTK/AppIndicator tray, ALSA/cpal, CI + 0.4.0.
4. `1788543323000-linux-dedicated-user.md` — from §11-Linux dedicated-user sub-entry: udev keyboard-only ACL, hardened `aural.service`, `AURAL_CONFIG_DIR` sharing, ProtectHome=read-only / SELinux-exonerated lessons, verify chain, escape hatches (file capability, Landlock).
5. `1788551509000-linux-prior-art-survey.md` — from §13: survey checkpoint, table, findings, two re-hash triggers.
6. §14 → covered by the moved `.zcode` plan (no duplicate).

## Step 3 — rewrite DESIGN.md (evergreen only; original prompt kept verbatim)

New structure (sequential numbering; drop all dated session narrative — it lives in `.plans/` + git history):

1. **Original prompt (verbatim)** — unchanged.
2. **Goal** — unchanged.
3. **Design research** (23-project evaluation) — unchanged.
4. **Analysis — language selection** — unchanged.
5. **Specification (current)** — CLI bullet rewritten to the current `aural stdin` + `aural system …` surface (pointing at §9); caps-lock register behavior kept; bench targets + verified numbers kept.
6. **Considerations & known limitations** — drop the dead "Wayland → X11 target" line (evdev landed); keep WASAPI floor, LL-hook secure contexts, privacy stance, RAM note.
7. **Decision register & re-hash triggers** — D1–D8 verbatim (Cargo.toml D8 ref stays valid) + fold §13's triggers in as D9 (evdev ↔ portal-based capture appearing) and D10 (PipeWire/WirePlumber tightening cross-UID access).
8. **Open items** — remove resolved/strikethrough items (cite commits); keep genuinely open: mute-hotkey confirmation, optional musical variants / layout-true letters; menubar item collapses to "shipped on all platforms".
9. **System & CLI architecture** — §14's evergreen core: command table, three hidden subcommands and why, per-platform `aural system install` behavior, bare-`aural`-in-bundle rule, tray menu + control-surface mode, scripts inventory, hardened behaviors (PID-before-hook visibility, single-instance bail on unix, stale-TCC pre-clear).
10. **Platform notes — Windows** — LL hook, WASAPI shared, bench p50 5.5 / p95 9.4, Run key + `FreeConsole`, GNU-toolchain dlltool note.
11. **Platform notes — macOS** — §9 cleaned: hook/keycodes/modifiers/hotkey; TCC = self-disclaim as the mechanism (drop superseded bundle/launchd-only paragraph); SDK rename; pending-prompt blocks `CGEventTapCreate`; daemon; audio + bench; toolchain ≥1.85.
12. **macOS hard-won lessons** — distilled §10 + bring-up-complete + §12: grant keyed to cdhash not name; stable self-signed identity; skip-disclaim-in-bundle; `tccutil reset`; quit-&-reopen requirement; `stdin` vs hook diagnostic split; secure-input symptom + doctor detection; tray-icon run-loop, muda `.enabled(true)`, template icons, `Icon::from_rgba` RGBA, `open -a Terminal` + exec bit.
13. **Platform notes — Linux** — §11-Linux cleaned, dedicated-user mode now delivered by `aural system install` (script deleted): evdev hook, keycodes, hotkey, daemon, tray + AppIndicator extension, audio, CI, ProtectHome/SELinux lessons, Flatpak deferral, `ksni` fallback.
14. **Prior-art surveys** — macOS survey table + Karabiner coexistence note; Linux survey + findings (drop dated framing and the "research task handed off" note).
15. **Signing, packaging, building** — the moved-from-README content + dev-machine cert essentials (openssl `-config`/`pkcs12 -legacy`, trust `codesign` over `find-identity`).

Preamble: status becomes "evergreen design reference"; add pointer — dated devlogs → `.plans/`, per-release changes → GitHub Releases. §8's sequencing note is absorbed into item text/commits.

## Step 4 — cross-reference fixes

- `README.md:108` `(§11)` → new Linux-notes section number; re-check L138/L173 wording.
- Spot-check `src/**` + `scripts/` + `tests/` comments for `DESIGN §N`-style refs (grep) and fix stale ones; Cargo.toml D8 ref stays valid.

## Step 5 — commits (per commits.md: one logical change each, agent-detect trailer, verify post-commit)

1. `docs(plans): adopt .plans and record retroactive devlog plans` — moves + retro files + session plan + AGENTS.md line.
2. `docs: reduce DESIGN.md to evergreen reference` — DESIGN.md rewrite + README cross-ref fixes.

Docs-only change — no build required. Verify: grep for stale `§` refs repo-wide, `git status` clean, `.kilo/plans/` gone, `.plans/` populated and tracked.
