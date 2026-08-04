# Extracts the aural-coding mapped notes from gleitz/midi-js-soundfonts FluidR3_GM
# base64 .js bundles into per-note .ogg files under assets/soundfonts/.
# Provenance/reproducibility script — the .js bundles in assets/source/ are git-ignored.
#
# Usage:
#   Invoke-WebRequest https://raw.githubusercontent.com/gleitz/midi-js-soundfonts/gh-pages/FluidR3_GM/acoustic_grand_piano-ogg.js -OutFile assets/source/acoustic_grand_piano-ogg.js
#   Invoke-WebRequest https://raw.githubusercontent.com/gleitz/midi-js-soundfonts/gh-pages/FluidR3_GM/synth_drum-ogg.js -OutFile assets/source/synth_drum-ogg.js
#   powershell -ExecutionPolicy Bypass -File scripts/extract-soundfonts.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

# Piano: majorScaleNotes[24..47] from the aural-coding mapping (see DESIGN.md §5)
$pianoNotes = @(
    'D4','E4','F4','G4','A4','Bb4','C5','D5','E5','F5','G5','A5','Bb5',
    'C6','D6','E6','F6','G6','A6','Bb6','C7','D7','E7','F7'
)
# Drums: GM-percussion MIDI 36,37,38,39,41,45,49,50,54,56,57,58,61
$drumNotes = @(
    'C2','Db2','D2','Eb2','F2','A2','Db3','D3','Gb3','Ab3','A3','Bb3','Db4'
)

$jobs = @(
    @{ Src = 'assets\source\acoustic_grand_piano-ogg.js'; Dest = 'assets\soundfonts\piano'; Notes = $pianoNotes },
    @{ Src = 'assets\source\synth_drum-ogg.js';            Dest = 'assets\soundfonts\drums'; Notes = $drumNotes }
)

$missing = 0
foreach ($job in $jobs) {
    $raw = [IO.File]::ReadAllText((Join-Path (Get-Location) $job.Src))
    foreach ($note in $job.Notes) {
        $m = [regex]::Match($raw, '"' + [regex]::Escape($note) + '"\s*:\s*"data:audio/ogg;base64,([^"]+)"')
        if (-not $m.Success) { Write-Host "MISSING: $note in $($job.Src)"; $missing++; continue }
        $out = Join-Path $job.Dest "$note.ogg"
        [IO.File]::WriteAllBytes($out, [Convert]::FromBase64String($m.Groups[1].Value))
        $head = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($out)[0..3])
        if ($head -ne 'OggS') { Write-Host "BAD HEADER: $out"; $missing++ }
    }
}
Get-ChildItem 'assets\soundfonts\*\*.ogg' | Measure-Object -Property Length -Sum |
    ForEach-Object { "files=$($_.Count) totalKB=$([math]::Round($_.Sum/1KB))" }
if ($missing) { Write-Host "FAILURES: $missing"; exit 1 } else { Write-Host 'ALL OK' }
