# Build Rust AudioHelper (Windows) and copy into src-tauri\resources\
# so Tauri bundler includes it in the .msi.
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$SrcTauriDir = Split-Path -Parent $ScriptDir
$ProjectRoot = Split-Path -Parent $SrcTauriDir
$AudioHelperDir = Join-Path $ProjectRoot "audio-helper-win"
$ResourcesDir = Join-Path $SrcTauriDir "resources"

Write-Host "[bundle-audio-helper] building Rust AudioHelper (release)..."
Push-Location $AudioHelperDir
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed (exit $LASTEXITCODE)"
    }
} finally {
    Pop-Location
}

$BuiltBin = Join-Path $AudioHelperDir "target\release\AudioHelper.exe"
if (-not (Test-Path $BuiltBin)) {
    throw "[bundle-audio-helper] ERROR: built binary not found at $BuiltBin"
}

New-Item -ItemType Directory -Force -Path $ResourcesDir | Out-Null
Copy-Item -Force $BuiltBin (Join-Path $ResourcesDir "AudioHelper.exe")
Write-Host "[bundle-audio-helper] copied to $ResourcesDir\AudioHelper.exe"
