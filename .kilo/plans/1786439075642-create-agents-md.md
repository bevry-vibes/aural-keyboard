# Create repo-root AGENTS.md referencing bevry-vibes/skills

## Context

- Project `bevry-labs/aural-system-keyboard` was bootstrapped with
  https://github.com/bevry-vibes/skills (license + conventions applied:
  `LICENSE.md`, `.editorconfig`, `.gitattributes` exist), but the resulting
  `AGENTS.md` is missing.
- The skills README prescribes the AGENTS.md shape: reference the remote skill
  URLs only — do not copy their contents in. Four files to reference:
  `policy.md`, `commits.md`, `minimax.md`, `kilo.md`.
- User chose the **minimal** style: bare remote links + one-line purpose each,
  no project-specific tweaks (no cargo gates, no agent-detect trailer command,
  no kilo plans rule, no `## Project` section).
- Skills repo default branch is `main`.

## Task

Create a single new file: `/Users/balupton/Projects/vibes/aural-system-keyboard/AGENTS.md`

## Content

```markdown
# AGENTS.md

This project conforms to [Bevry's skills](https://github.com/bevry-vibes/skills).
Reference their remote URLs only — do not pull their contents into this file.

- https://github.com/bevry-vibes/skills/blob/main/policy.md — Bevry's AI policy, mandating which AIs are permitted
- https://github.com/bevry-vibes/skills/blob/main/commits.md — commit hygiene
- https://github.com/bevry-vibes/skills/blob/main/minimax.md — MiniMax model tweaks
- https://github.com/bevry-vibes/skills/blob/main/kilo.md — Kilo harness tweaks
```

## Constraints / notes

- Do not modify any other file.
- Use `blob/main` (skills default branch; the repo's master→main migration is
  complete).
- File is plain Markdown; no license header required (it is a pointer, not
  policy; the repo LICENSE.md already covers the project).
- No commit unless the user asks.

## Validation

- File exists at repo root and renders as Markdown.
- The four URLs resolve (https://github.com/bevry-vibes/skills/blob/main/{policy,commits,minimax,kilo}.md).
- No other files changed (`git status` shows only the new AGENTS.md).
