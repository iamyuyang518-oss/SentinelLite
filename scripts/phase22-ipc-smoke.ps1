param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [string]$ProbePath = (Join-Path $PSScriptRoot "..\target\debug\examples\probe.exe"),
    [int]$WaitSeconds = 180,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase22-ipc-smoke")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This IPC smoke test requires an elevated Administrator PowerShell. It does not self-elevate.")
    exit 2
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not (Test-Path -LiteralPath $ExePath)) {
    [Console]::Error.WriteLine("Sensor executable not found: $ExePath. Run: cargo build -p sentinel-sensor --release")
    exit 2
}
if (-not (Test-Path -LiteralPath $ProbePath)) {
    [Console]::Error.WriteLine("Probe not found: $ProbePath. Run: cargo build -p sentinel-ipc --example probe")
    exit 2
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path
$ProbePath = (Resolve-Path -LiteralPath $ProbePath).Path

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$sessionName = "SentinelLite-Sensor-IPC-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$probeJsonPath = Join-Path $LogDir "probe-$stamp.json"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"

$commit = "unknown"
$gitStatus = ""
try {
    $commit = (& git -C $repoRoot rev-parse HEAD 2>$null | Select-Object -First 1)
    $gitStatus = ((& git -C $repoRoot status --porcelain 2>$null) -join "`n")
} catch {
    $commit = "unknown"
}
$dirty = -not [string]::IsNullOrWhiteSpace($gitStatus)
$hash = (Get-FileHash -Path $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$probeHash = (Get-FileHash -Path $ProbePath -Algorithm SHA256).Hash.ToLowerInvariant()

$sensor = $null
$probeOutput = $null
$probeExit = 1
$previousErrorAction = $ErrorActionPreference
try {
    $sensor = Start-Process -FilePath $ExePath `
        -ArgumentList @("--serve", "--session-name", $sessionName, "--output", $summaryPath) `
        -PassThru

    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        if ($sensor.HasExited) { break }
        Remove-Item -LiteralPath $probeJsonPath -ErrorAction SilentlyContinue
        try {
            # A missing pipe makes probe write to stderr; do not let
            # ErrorActionPreference turn that expected retry into a terminating error.
            $ErrorActionPreference = "Continue"
            $probeOutput = (& $ProbePath --output $probeJsonPath 2>&1 | Out-String)
            $probeExit = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorAction
        }
        if ($probeExit -eq 0) { break }
        Start-Sleep -Milliseconds 500
    }

    $status = $null
    if (Test-Path -LiteralPath $probeJsonPath) {
        $probeOutput = Get-Content -LiteralPath $probeJsonPath -Raw -Encoding UTF8
        try { $status = $probeOutput | ConvertFrom-Json } catch { $status = $null }
    }

    $checks = [ordered]@{
        sensor_started      = ($null -ne $sensor -and -not $sensor.HasExited)
        probe_exit_0        = ($probeExit -eq 0)
        probe_json_parsed   = ($null -ne $status)
        sensor_state_valid  = ($null -ne $status -and $status.sensor_state -in @("active", "degraded"))
        uptime_present      = ($null -ne $status -and $null -ne $status.uptime_ms)
        counts_present      = ($null -ne $status -and $null -ne $status.events_received -and $null -ne $status.events_processed)
        health_ok_boolean   = ($null -ne $status -and $status.health_ok -is [bool])
    }

    if ($probeExit -ne 0) {
        Write-Host "Probe failed before Ctrl-C. Active sensor console is left running for inspection."
        Write-Host "Probe output: $probeOutput"
        $checks["probe_error_visible"] = $true
    }

    Write-Host ""
    Write-Host "Sensor status received: $($status.sensor_state); health_ok=$($status.health_ok); uptime_ms=$($status.uptime_ms)"
    Write-Host "Switch to the SentinelLiteSensor console and press Ctrl-C there."
    Write-Host "Do not close the window. Waiting up to $WaitSeconds seconds..."

    if (-not $sensor.WaitForExit($WaitSeconds * 1000)) {
        [Console]::Error.WriteLine("Sensor did not exit after Ctrl-C within $WaitSeconds seconds.")
        $checks["sensor_exited"] = $false
    } else {
        $sensor.WaitForExit()
        $sensor.Refresh()
        $checks["sensor_exited"] = $true
        $checks["sensor_exit_3"] = ($sensor.ExitCode -eq 3)

        $summary = $null
        if (Test-Path -LiteralPath $summaryPath) {
            $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
        }
        $checks["summary_exists"] = ($null -ne $summary)
        $checks["ipc_started"] = ($null -ne $summary -and $summary.ipc_started -eq $true)
        $checks["no_ipc_error"] = ($null -ne $summary -and $null -eq $summary.ipc_error)
        $checks["session_stopped"] = ($null -ne $summary -and $summary.session_stopped -eq $true)
        $checks["no_residual_json"] = ($null -ne $summary -and $summary.residual_session -eq $false)
    }

    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $checks["no_residual_logman"] = ($residualOutput.Count -eq 0)

    $failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value })
    $statusText = if ($failed.Count -eq 0) { "PASS" } else { "FAIL" }
    $lines = @(
        "status=$statusText",
        "timestamp=$stamp",
        "source_commit=$commit",
        "source_worktree_dirty=$dirty",
        "binary=$ExePath",
        "binary_sha256=$hash",
        "probe=$ProbePath",
        "probe_sha256=$probeHash",
        "session_name=$sessionName",
        "summary_path=$summaryPath",
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
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($statusText -eq "PASS") {
        Write-Host "PHASE 2.2 IPC SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.2 IPC SMOKE: FAIL"
    Write-Host "Evidence: $runInfoPath"
    exit 1
}
finally {
    if ($null -ne $sensor -and -not $sensor.HasExited) {
        Write-Host "Sensor is still running for manual inspection. Press Ctrl-C in its console; it is not forced-killed by this script."
    }
}
