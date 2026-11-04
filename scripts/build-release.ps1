# Build a release with the build user's profile path remapped out of panic metadata.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1 [tauri build args]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $TauriArgs
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot
$profilePath = [Environment]::GetFolderPath("UserProfile")
$remap = "--remap-path-prefix=$profilePath=C:\Users\redacted"

if ($env:CARGO_ENCODED_RUSTFLAGS) {
    $env:CARGO_ENCODED_RUSTFLAGS = "$env:CARGO_ENCODED_RUSTFLAGS$([char]31)$remap"
} else {
    $env:CARGO_ENCODED_RUSTFLAGS = $remap
}

Push-Location $repo
try {
    & npx tauri build @TauriArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}
