# Build the fork and install it over the running Skim (PLAN.md 0.5).
#
#   powershell -ExecutionPolicy Bypass -File scripts\fork\build-install.ps1 [-SkipGates] [-NoInstall]
#
# 1. Refuses a dirty tree (tracked files).
# 2. Backs up %APPDATA%\com.skim.app\skim.db, -wal, -shm to
#    %USERPROFILE%\.skim-fork\backups\<timestamp>\ (app is closed first so the
#    three files are consistent).
# 3. Loads %USERPROFILE%\.skim-fork\build.env (signing key + optional
#    SKIM_GOOGLE_CLIENT_ID/SECRET) and the Rebound defaults from
#    C:\Users\Patrick\OS\.env into the build environment only.
# 4. npm ci, gates, `npm run tauri build -- --bundles nsis`.
# 5. Runs the NSIS installer silently, relaunches Skim.
# 6. Prints the installed version.
param(
    [switch]$SkipGates,
    [switch]$NoInstall
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root

function Step($m) { Write-Host "`n== $m" -ForegroundColor Cyan }

# 1. Clean tree.
Step "git status"
$dirty = git status --porcelain --untracked-files=no
if ($dirty) { Write-Host $dirty; throw "Tracked files are modified. Commit or stash first." }

# 3. Build environment (never printed, never committed).
Step "build environment"
$envFile = Join-Path $env:USERPROFILE ".skim-fork\build.env"
if (-not (Test-Path $envFile)) { throw "Missing $envFile (see PLAN.md 0.4)" }
Get-Content $envFile | ForEach-Object {
    if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)=(.*)$') {
        [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2].Trim(), "Process")
    }
}
if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and -not $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    throw "build.env must set TAURI_SIGNING_PRIVATE_KEY_PATH (or TAURI_SIGNING_PRIVATE_KEY)"
}
# The Tauri CLI wants the key's CONTENT in TAURI_SIGNING_PRIVATE_KEY; a path
# alone is not honoured by the bundler's updater signing step (seen 2026-09-23).
if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $env:TAURI_SIGNING_PRIVATE_KEY_PATH -Raw)
}
# Rebound defaults (Phase 9): read from the OS .env, baked in via option_env!.
$osEnv = "C:\Users\Patrick\OS\.env"
if (Test-Path $osEnv) {
    $map = @{
        "REBOUND_API_BASE_URL"      = "SKIM_REBOUND_BASE_URL"
        "REBOUND_SUPABASE_URL"      = "SKIM_REBOUND_SUPABASE_URL"
        "REBOUND_SUPABASE_ANON_KEY" = "SKIM_REBOUND_SUPABASE_ANON_KEY"
    }
    Get-Content $osEnv | ForEach-Object {
        if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)=(.*)$' -and $map.ContainsKey($Matches[1])) {
            $v = $Matches[2].Trim().Trim('"').Trim("'")
            [Environment]::SetEnvironmentVariable($map[$Matches[1]], $v, "Process")
        }
    }
    foreach ($k in $map.Values) {
        $set = if ([Environment]::GetEnvironmentVariable($k, "Process")) { "set" } else { "MISSING" }
        Write-Host "  $k $set"
    }
} else {
    Write-Host "  (no OS .env; Rebound defaults will be empty)"
}

# 4. Build.
Step "npm ci"
npm ci --silent
if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
if (-not $SkipGates) {
    Step "gates"
    bash scripts/fork/gates.sh
    if ($LASTEXITCODE -ne 0) { throw "gates failed" }
}
Step "tauri build (nsis)"
npm run tauri build -- --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
$version = (Get-Content src-tauri\tauri.conf.json | ConvertFrom-Json).version
$installer = Join-Path $root "src-tauri\target\release\bundle\nsis\Skim_${version}_x64-setup.exe"
if (-not (Test-Path $installer)) { throw "Installer not found at $installer" }
$sig = "$installer.sig"
if (-not (Test-Path $sig)) { throw "Installer signature missing at $sig (updater signing failed)" }
Write-Host ("  installer {0:N1} MB, signature present" -f ((Get-Item $installer).Length / 1MB))
if ($NoInstall) { Write-Host "`n-NoInstall: stopping before install."; exit 0 }

# 5a. Close Skim, then back up (a closed app leaves db/-wal/-shm consistent).
Step "closing Skim"
Get-Process -Name skim -ErrorAction SilentlyContinue | ForEach-Object {
    & taskkill /PID $_.Id /F | Out-Null
}
$deadline = (Get-Date).AddSeconds(15)
while ((Get-Process -Name skim -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
if (Get-Process -Name skim -ErrorAction SilentlyContinue) { throw "Skim is still running" }

Step "backup"
$data = Join-Path $env:APPDATA "com.skim.app"
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$backup = Join-Path $env:USERPROFILE ".skim-fork\backups\$stamp"
New-Item -ItemType Directory -Force -Path $backup | Out-Null
$copied = 0
foreach ($f in @("skim.db", "skim.db-wal", "skim.db-shm")) {
    $src = Join-Path $data $f
    if (Test-Path $src) { Copy-Item $src (Join-Path $backup $f); $copied++ }
}
if (-not (Test-Path (Join-Path $backup "skim.db"))) { throw "Backup of skim.db failed" }
Write-Host "  $copied file(s) -> $backup"

# 5b. Install silently (per-user NSIS), relaunch.
Step "install"
$p = Start-Process -FilePath $installer -ArgumentList "/S" -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "installer exited $($p.ExitCode)" }
$exe = Join-Path $env:LOCALAPPDATA "Skim\skim.exe"
if (-not (Test-Path $exe)) { throw "skim.exe not found after install at $exe" }
Start-Process -FilePath $exe

# 6. Installed version.
Step "installed"
$fv = (Get-Item $exe).VersionInfo
Write-Host ("  {0}  version {1}  ({2})" -f $exe, $fv.ProductVersion, $fv.FileVersion)
Write-Host "  backup: $backup"
