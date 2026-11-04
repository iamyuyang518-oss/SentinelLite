param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase21-ctrlc-smoke"),
    [int]$DurationSeconds = 120,
    [int]$WaitSeconds = 180
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This Ctrl-C smoke test requires an elevated Administrator PowerShell. It does not self-elevate.")
    exit 2
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not (Test-Path -LiteralPath $ExePath)) {
    [Console]::Error.WriteLine("Sensor executable not found: $ExePath. Run: cargo build -p sentinel-sensor --release")
    exit 2
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$sessionName = "SentinelLite-Sensor-CtrlC-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
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

$sensor = Start-Process -FilePath $ExePath `
    -ArgumentList @("--session-name", $sessionName, "--duration", "$DurationSeconds", "--output", $summaryPath) `
    -PassThru

try {
    $active = $false
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        $match = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
        if ($match.Count -gt 0) {
            $active = $true
            break
        }
        if ($sensor.HasExited) { break }
        Start-Sleep -Milliseconds 250
    }

    if (-not $active) {
        [Console]::Error.WriteLine("ETW session was not observed as active: $sessionName")
        if (-not $sensor.HasExited) {
            try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
        }
        exit 1
    }

    Write-Host ""
    Write-Host "Session is active: $sessionName"
    Write-Host "Switch to the SentinelLiteSensor console window and press Ctrl-C there."
    Write-Host "Do not close the window. Waiting up to $WaitSeconds seconds..."
    Write-Host ""

    if (-not $sensor.WaitForExit($WaitSeconds * 1000)) {
        [Console]::Error.WriteLine("Sensor did not exit after Ctrl-C within $WaitSeconds seconds.")
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
        exit 1
    }

    $processExitCode = $null
    try {
        $sensor.WaitForExit()
        $sensor.Refresh()
        $processExitCode = $sensor.ExitCode
    } catch {
        $processExitCode = $null
    }

    $summary = $null
    if (Test-Path -LiteralPath $summaryPath) {
        $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
    }
    $jsonExitCode = $null
    if ($null -ne $summary -and $null -ne $summary.exit_code) {
        $jsonExitCode = [int]$summary.exit_code
    }
    $exitCode = if ($null -ne $jsonExitCode) { $jsonExitCode } elseif ($null -ne $processExitCode) { [int]$processExitCode } else { -1 }
    $sourcesAgree = ($null -eq $jsonExitCode -or $null -eq $processExitCode -or $jsonExitCode -eq $processExitCode)
    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0

    $checks = [ordered]@{
        "sensor_exited"             = ($sensor.HasExited -eq $true)
        "exit_code_is_degraded_3"   = ($exitCode -eq 3)
        "exit_code_sources_agree"   = $sourcesAgree
        "interrupted_true"          = ($null -ne $summary -and $summary.interrupted -eq $true)
        "window_incomplete"         = ($null -ne $summary -and $summary.window_complete -eq $false)
        "session_created"           = ($null -ne $summary -and $summary.session_created -eq $true)
        "session_stopped"           = ($null -ne $summary -and $summary.session_stopped -eq $true)
        "trace_opened"              = ($null -ne $summary -and $summary.trace_opened -eq $true)
        "trace_closed"              = ($null -ne $summary -and $summary.trace_closed -eq $true)
        "state_degraded"            = ($null -ne $summary -and $summary.state -eq "degraded")
        "no_session_stop_error"     = ($null -ne $summary -and $null -eq $summary.session_stop_error)
        "no_session_close_error"    = ($null -ne $summary -and $null -eq $summary.session_close_error)
        "no_process_trace_error"    = ($null -ne $summary -and $null -eq $summary.process_trace_error)
        "no_overall_error"          = ($null -ne $summary -and $null -eq $summary.error)
        "no_residual_session_json"  = ($null -ne $summary -and $summary.residual_session -eq $false)
        "no_residual_session_logman" = (-not $residual)
    }

    $failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value })
    $status = if ($failed.Count -eq 0) { "PASS" } else { "FAIL" }
    $lines = @(
        "status=$status",
        "timestamp=$stamp",
        "source_commit=$commit",
        "source_worktree_dirty=$dirty",
        "binary=$ExePath",
        "binary_sha256=$hash",
        "session_name=$sessionName",
        "duration_seconds=$DurationSeconds",
        "process_exit_code=$processExitCode",
        "json_exit_code=$jsonExitCode",
        "resolved_exit_code=$exitCode",
        "residual_session=$residual",
        "summary_path=$summaryPath"
    )
    foreach ($entry in $checks.GetEnumerator()) {
        $lines += "check.$($entry.Key)=$($entry.Value)"
    }
    if ($null -ne $summary) {
        $lines += "state=$($summary.state)"
        $lines += "interrupted=$($summary.interrupted)"
        $lines += "window_complete=$($summary.window_complete)"
        $lines += "elapsed_ms=$($summary.elapsed_ms)"
        $lines += "expected_ms=$($summary.expected_ms)"
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_lost=$($summary.events_lost)"
        $lines += "session_stop_error=$($summary.session_stop_error)"
        $lines += "session_close_error=$($summary.session_close_error)"
        $lines += "process_trace_error=$($summary.process_trace_error)"
        $lines += "overall_error=$($summary.error)"
        $lines += "residual_session_json=$($summary.residual_session)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.1 CTRL-C SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.1 CTRL-C SMOKE: FAIL"
    Write-Host "Evidence: $runInfoPath"
    exit 1
}
finally {
    if ($null -ne $sensor -and -not $sensor.HasExited) {
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
    }
}
