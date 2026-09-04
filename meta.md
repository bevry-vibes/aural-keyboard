# meta.md — local amendments to bevry-vibes/skills

Project-local amendments to the skills referenced in [AGENTS.md](AGENTS.md).
The remote skills are authoritative where these do not conflict.

## amendments to commits.md — release flow

Release cuts follow this flow, which extends the commit-hygiene skill's scope
to GitHub releases:

1. **Version bump** — `chore: release X.Y.Z — <headline>`; bump `version` in
   `Cargo.toml` and refresh `Cargo.lock` (`cargo check -q`).
2. **Tag** — annotated `vX.Y.Z`; the tag message is a one-paragraph summary.
3. **Push** — `main` + the tag. `release.yml` builds all three platforms and
   attaches artifacts to the GitHub release (Windows also publishes to
   crates.io — crates.io versions are immutable, so never re-cut a version
   that has been pushed; cut a patch bump instead).
4. **Release notes** — immediately after pushing, win the race against the
   workflow: `gh release create vX.Y.Z --title "aural X.Y.Z — <headline>"
   --notes-file <file>` (use `gh release edit vX.Y.Z --notes-file <file>` if
   the workflow created it first). Notes rules learned in v0.4.0/v0.4.1:
   - **No H1** in the body — the release title already renders as the header.
   - **Only the release's own changes** — never duplicate prior releases'
     notes.
   - Exactly **one** `**Full Changelog**: compare/A...B` footer.
   - `generate_release_notes: true` lives on the **Windows release job only**
     in `release.yml` — every job that runs it appends another generated
     block to the body (duplicated footers in v0.4.0/v0.4.1).
5. **Verification** — `gh run list` until the release and CI runs complete;
   `gh release view vX.Y.Z --json assets` for the artifact set.

> Signing note: commits and pushes sign through the 1Password SSH agent —
> unlock 1Password first; the agent intermittently returns errors otherwise.

## amendments to commits.md — co-author trailer

`agent-detect` is unavailable in this harness; per user instruction, the
trailer for this project's agent commits is:

```
Co-authored-by: Cline - GLM 5.3 Flash <cline-clinepass-glm53flash@local>
```
