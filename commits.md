# commits.md

Local application of the bevry-vibes skills [commits.md](https://github.com/bevry-vibes/skills/blob/main/commits.md) — see the upstream [local tweaks pattern](https://github.com/bevry-vibes/skills#local-tweaks-pattern).

## this project's tweaks

- Releases follow the upstream release flow (`chore: release X.Y.Z — <headline>` bump, annotated `vX.Y.Z`, wait for the workflow stub, then `gh release edit` with the full notes); the title's headline is the one in the release-bump commit.
- The Windows release job publishes to crates.io before packaging (publish requires a clean tree). Registries are immutable — never re-cut a pushed version; cut a patch bump instead.
- `generate_release_notes: true` lives on the Windows release job only, per the upstream rule.
- Commits and pushes sign through the 1Password SSH agent — unlock 1Password first; the agent intermittently returns errors otherwise.
