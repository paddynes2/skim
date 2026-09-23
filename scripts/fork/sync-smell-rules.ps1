# Fork (6.5.1): vendor the AI-tell rule set from the OS repo into Skim.
#
#   powershell -File scripts/fork/sync-smell-rules.ps1 [-OsRoot C:\Users\Patrick\OS]
#
# OS is the canonical home of the rules (tools/outreach/slop_scrub.py and
# tools/no-smell/nosmell/structural.py); Skim holds a copy so the composer works
# offline and the app never shells out. Re-runnable: the JSON carries the OS commit
# it came from (`os_commit`, written by the exporter from `git rev-parse HEAD`),
# so a diff of rules.json says exactly which OS change it picked up. The exporter
# refuses to run outside a checkout, and this script refuses a dirty exporter so
# the recorded commit is the code that produced the rules.
param(
  [string]$OsRoot = "C:\Users\Patrick\OS"
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$out = Join-Path (Resolve-Path (Join-Path $here "..\..")) "src\fork\smell\rules.json"
$scrub = Join-Path $OsRoot "tools\outreach\slop_scrub.py"
if (-not (Test-Path $scrub)) { throw "slop_scrub.py not found at $scrub" }

$dirty = & git -C $OsRoot status --porcelain -- tools/outreach/slop_scrub.py tools/no-smell/nosmell/structural.py
if ($dirty) { throw "OS exporter has uncommitted changes; commit them first so os_commit is truthful:`n$dirty" }

$env:PYTHONUTF8 = "1"
$env:PYTHONIOENCODING = "utf-8"
# python writes UTF-8; without this PowerShell decodes it as the console code page
# and the Greek / Cyrillic homoglyph ranges arrive as box-drawing characters.
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$json = & python $scrub --export-rules
if ($LASTEXITCODE -ne 0) { throw "slop_scrub.py --export-rules failed ($LASTEXITCODE)" }
$text = ($json -join "`n") + "`n"
if (-not $text.Contains('"os_commit": "')) { throw "export carries no os_commit; run from an OS checkout" }
[System.IO.File]::WriteAllText($out, $text, (New-Object System.Text.UTF8Encoding($false)))
$n = ([regex]::Matches($text, '"category":')).Count
Write-Host "wrote $out ($n rules)"
