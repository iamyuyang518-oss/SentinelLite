param(
    [string]$ProbePath = (Join-Path $PSScriptRoot "..\target\debug\examples\probe.exe"),
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase22-medium-probe")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This probe must run from a NON-elevated PowerShell to exercise the Medium-integrity client path.")
    exit 2
}
if (-not (Test-Path -LiteralPath $ProbePath)) {
    [Console]::Error.WriteLine("Probe not found: $ProbePath. Run: cargo build -p sentinel-ipc --example probe")
    exit 2
}
$ProbePath = (Resolve-Path -LiteralPath $ProbePath).Path

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$probeJsonPath = Join-Path $LogDir "probe-medium-$stamp.json"
$runInfoPath = Join-Path $LogDir "run-medium-$stamp.txt"

$groups = (& whoami.exe /groups 2>&1) -join "`n"
$probeText = ""
$probeExit = 1
try {
    $probeText = (& $ProbePath --output $probeJsonPath 2>&1 | Out-String)
    $probeExit = $LASTEXITCODE
}
catch {
    $probeText = $_.Exception.Message
}

$status = $null
if (Test-Path -LiteralPath $probeJsonPath) {
    try {
        $status = Get-Content -LiteralPath $probeJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        $status = $null
    }
}

$checks = [ordered]@{
    non_admin           = (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))
    probe_exit_0        = ($probeExit -eq 0)
    probe_json_parsed   = ($null -ne $status)
    sensor_state_valid  = ($null -ne $status -and $status.sensor_state -in @("active", "degraded"))
    uptime_present      = ($null -ne $status -and $null -ne $status.uptime_ms)
    counts_present      = ($null -ne $status -and $null -ne $status.events_received -and $null -ne $status.events_processed)
    health_ok_boolean   = ($null -ne $status -and $status.health_ok -is [bool])
}
$failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value })
$statusText = if ($failed.Count -eq 0) { "PASS" } else { "FAIL" }

$lines = @(
    "status=$statusText",
    "timestamp=$stamp",
    "identity=$($identity.Name)",
    "is_admin=False",
    "probe=$ProbePath",
    "probe_json_path=$probeJsonPath",
    "probe_exit=$probeExit"
)
foreach ($entry in $checks.GetEnumerator()) {
    $lines += "check.$($entry.Key)=$($entry.Value)"
}
if ($null -ne $status) {
    $lines += "sensor_state=$($status.sensor_state)"
    $lines += "health_ok=$($status.health_ok)"
    $lines += "events_received=$($status.events_received)"
    $lines += "events_processed=$($status.events_processed)"
    $lines += "events_lost=$($status.events_lost)"
    $lines += "uptime_ms=$($status.uptime_ms)"
}
if (-not [string]::IsNullOrWhiteSpace($probeText)) {
    $lines += "probe_text=$($probeText.Trim())"
}
$lines += "whoami_groups_begin"
$lines += $groups
$lines += "whoami_groups_end"
$lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8

Get-Content -LiteralPath $runInfoPath
if ($statusText -eq "PASS") {
    Write-Host "PHASE 2.2 MEDIUM IPC PROBE: PASS"
    exit 0
}
Write-Host "PHASE 2.2 MEDIUM IPC PROBE: FAIL"
Write-Host "Evidence: $runInfoPath"
exit 1
