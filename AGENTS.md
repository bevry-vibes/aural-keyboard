# AGENTS.md

This project conforms to Bevry's skills.
Reference their remote URLs only — do not pull their contents into this file.
When a referenced skill applies with this project's tweaks, the local `<name>.md` file at this repo root references the remote URL and lists the tweaks underneath;
this process is documented in the upstream repo's [local tweaks pattern](https://github.com/bevry-vibes/skills#local-tweaks-pattern).

- https://github.com/bevry-vibes/skills/blob/main/policy.md — **applies.** Bevry's AI policy, mandating which AIs are permitted
- https://github.com/bevry-vibes/skills/blob/main/commits.md — **applies.** Commit hygiene and the release flow: version bump, annotated `<version>` tag, wait for the workflow run, then `gh release edit` sets the body to the full notes
- https://github.com/bevry-vibes/skills/blob/main/conventions.md — **applies.** The bevry/base config files, the wrapping rule (break for meaning, never for width), and splat naming
- https://github.com/bevry-vibes/skills/blob/main/plans.md — **applies.** Cross-harness plan recording in `.plans/`
- https://github.com/bevry-vibes/skills/blob/main/build.md — **applies.** Menu-bar/tray build, packaging, and login-autostart; this repo is the native-Rust worked example (`src/system.rs`)
- https://github.com/bevry-vibes/skills/blob/main/minimax.md — **applies** when the running agent is a MiniMax M3 model (its rules gate themselves on model and harness)
