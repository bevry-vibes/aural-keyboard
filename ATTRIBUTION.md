# Attribution

## Sound samples

The bundled instrument samples (`assets/soundfonts/piano/*.ogg`,
`assets/soundfonts/drums/*.ogg`) are per-note renderings from the **FluidR3_GM**
General MIDI soundfont:

- Original Fluid R3 soundfont: Copyright © 2000–2002, 2008 Frank Wen
- Pre-rendered per-note samples: [gleitz/midi-js-soundfonts](https://github.com/gleitz/midi-js-soundfonts),
  released under [Creative Commons Attribution 3.0 Unported (CC BY 3.0)](https://creativecommons.org/licenses/by/3.0/)
- Instruments used: `acoustic_grand_piano` (letters), `synth_drum` (all other keys)

These are the same soundfont lineages used by the original
[aural-coding](https://github.com/probablycorey/aural-coding) (Atom) and
[aural-coding-vscode](https://github.com/jengjeng/aural-coding-vscode) projects,
whose musical mapping this program faithfully ports.

No mechanical-keyboard samples are used anywhere in this project, by design
(see `DESIGN.md`).

## Extraction

The per-note OGGs are extracted from the upstream base64 `.js` bundles by
`scripts/extract-soundfonts.ps1` (kept for provenance/reproducibility; the
bundles themselves are not committed).
