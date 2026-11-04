param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [string]$ProbePath = (Join-Path $PSScriptRoot "..\target\debug\examples\probe.exe"),
    [int]$WaitSeconds = 60,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase25-event-delivery")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This Phase 2.5 smoke test requires an elevated Administrator PowerShell. It does not self-elevate.")
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
$sessionName = "SentinelLite-Sensor-Events-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$watchPath = Join-Path $LogDir "events-$stamp.json"
$startProbePath = Join-Path $LogDir "events-start-$stamp.json"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"
$tempDir = Join-Path $env:TEMP "sentinelite-phase25-events-$stamp"

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

$cmd = Join-Path $env:SystemRoot "System32\cmd.exe"
$stimulusProcesses = @()
$sensor = $null
$watcher = $null
$helperExit = $null

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    $sensor = Start-Process -FilePath $ExePath `
        -ArgumentList @("--serve", "--session-name", $sessionName, "--queue-capacity", "4096", "--event-buffer", "4096", "--output", $summaryPath) `
        -WindowStyle Hidden -PassThru

    $active = $false
    for ($attempt = 0; $attempt -lt 80; $attempt++) {
        $match = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
        if ($match.Count -gt 0) {
            $active = $true
            break
        }
        if ($sensor.HasExited) { break }
        Start-Sleep -Milliseconds 250
    }

    $firstReadExit = 1
    for ($attempt = 0; $attempt -lt 40 -and $active; $attempt++) {
        & $ProbePath --events --limit 64 --output $startProbePath 2>$null | Out-Null
        $firstReadExit = $LASTEXITCODE
        if ($firstReadExit -eq 0) { break }
        Start-Sleep -Milliseconds 250
    }

    if ($active) {
        $watcher = Start-Process -FilePath $ProbePath `
            -ArgumentList @("--watch-events", "25", "--limit", "128", "--output", $watchPath) `
            -WindowStyle Hidden -PassThru
        Start-Sleep -Seconds 1

        for ($index = 0; $index -lt 3; $index++) {
            try {
                $stimulusProcesses += Start-Process -FilePath $cmd `
                    -ArgumentList "/c timeout /t 2 /nobreak >nul" `
                    -WindowStyle Hidden -PassThru
            } catch { }
        }
        Start-Sleep -Seconds 3

        $helperScript = @'
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class ConsoleCtrl {
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool FreeConsole();
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool AttachConsole(uint processId);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetConsoleCtrlHandler(IntPtr handler, bool add);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GenerateConsoleCtrlEvent(uint ctrlEvent, uint processGroup);
}
"@
[ConsoleCtrl]::FreeConsole() | Out-Null
if (-not [ConsoleCtrl]::AttachConsole([uint32]$args[0])) { exit 2 }
[ConsoleCtrl]::SetConsoleCtrlHandler([IntPtr]::Zero, $true) | Out-Null
$sent = [ConsoleCtrl]::GenerateConsoleCtrlEvent(0, 0)
Start-Sleep -Milliseconds 250
[ConsoleCtrl]::FreeConsole() | Out-Null
if ($sent) { exit 0 } else { exit 3 }
'@
        $helperPath = Join-Path $tempDir "send-ctrlc.ps1"
        Set-Content -LiteralPath $helperPath -Value $helperScript -Encoding UTF8

        if (-not $sensor.HasExited) {
            $helper = Start-Process -FilePath "powershell.exe" `
                -ArgumentList @("-NoProfile", "-File", $helperPath, "$($sensor.Id)") `
                -PassThru -Wait -WindowStyle Hidden
            $helperExit = $helper.ExitCode
        }
    }

    if (-not $sensor.WaitForExit($WaitSeconds * 1000)) {
        [Console]::Error.WriteLine("Sensor did not exit within $WaitSeconds seconds.")
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
    }

    if ($null -ne $watcher) {
        if (-not $watcher.WaitForExit(30000)) {
            try { $watcher.Kill() } catch { }
        }
    }

    $processExitCode = $null
    if ($sensor.HasExited) {
        try {
            $sensor.WaitForExit()
            $sensor.Refresh()
            $processExitCode = $sensor.ExitCode
        } catch {
            $processExitCode = $null
        }
    }

    $summary = $null
    if (Test-Path -LiteralPath $summaryPath) {
        try { $summary = Get-Content -LiteralPath $summaryPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }
    $watch = $null
    if (Test-Path -LiteralPath $watchPath) {
        try { $watch = Get-Content -LiteralPath $watchPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }
    $events = $null
    if ($null -ne $watch) {
        $events = $watch.events
    }

    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0
    $delivery = $null
    $sessionEnd = $null
    if ($null -ne $summary) {
        $delivery = $summary.delivery
        if ($null -ne $summary.pipeline) {
            $sessionEnd = $summary.pipeline.session_end
        }
    }

    $checks = [ordered]@{
        "session_observed"                = $active
        "first_event_read_exit_0"         = ($firstReadExit -eq 0)
        "ctrl_c_event_sent"               = ($helperExit -eq 0)
        "sensor_exited"                   = ($sensor.HasExited -eq $true)
        "exit_code_is_degraded_3"         = ($processExitCode -eq 3)
        "summary_parsed"                  = ($null -ne $summary)
        "event_ipc_started"               = ($null -ne $summary -and $summary.event_ipc_started -eq $true)
        "event_ipc_no_error"              = ($null -ne $summary -and $null -eq $summary.event_ipc_error)
        "delivery_present"                = ($null -ne $delivery)
        "delivery_batches_served"         = ($null -ne $delivery -and [int64]$delivery.batches_served -gt 0)
        "delivery_session_closed"         = ($null -ne $delivery -and $delivery.session_closed -eq $true)
        "delivery_session_end_confirmed"  = ($null -ne $delivery -and $delivery.session_end_delivered -eq $true)
        "delivery_no_evictions"           = ($null -ne $delivery -and [int64]$delivery.evictions -eq 0)
        "watch_parsed"                    = ($null -ne $events)
        "watch_process_events_positive"   = ($null -ne $events -and [int64]$events.processEvents -gt 0)
        "watch_records_positive"          = ($null -ne $events -and [int64]$events.records -gt 0)
        "watch_session_end_once"          = ($null -ne $events -and [int64]$events.sessionEndRecords -eq 1)
        "watch_session_end_interrupted"   = ($null -ne $events -and $events.sessionEndReason -eq "interrupted")
        "watch_session_closed"            = ($null -ne $events -and $events.sessionClosed -eq $true)
        "watch_no_cursor_expiry"          = ($null -ne $events -and [int64]$events.cursorExpired -eq 0)
        "watch_no_missed_records"         = ($null -ne $events -and [int64]$events.missedRecords -eq 0)
        "watch_no_session_restart"        = ($null -ne $events -and [int64]$events.sessionRestarts -eq 0)
        "watch_no_transient_errors"       = ($null -ne $events -and [int64]$events.transientErrors -eq 0)
        "watch_paired_stops_positive"     = ($null -ne $events -and [int64]$events.pairedStopped -gt 0)
        "pipeline_consumer_stopped"       = ($null -ne $summary -and $summary.pipeline.consumer_state -eq "stopped")
        "pipeline_backlog_zero"           = ($null -ne $summary -and [int64]$summary.pipeline.backlog -eq 0)
        "pipeline_no_drops"               = ($null -ne $summary -and [int64]$summary.pipeline.dropped_queue_full -eq 0 -and [int64]$summary.pipeline.dropped_consumer_gone -eq 0)
        "session_end_interrupted"         = ($null -ne $sessionEnd -and $sessionEnd.reason -eq "interrupted")
        "events_processed_positive"       = ($null -ne $summary -and [int64]$summary.events_processed -gt 0)
        "etw_events_lost_zero"            = ($null -ne $summary -and [int64]$summary.events_lost -eq 0)
        "no_residual_session_json"        = ($null -ne $summary -and $summary.residual_session -eq $false)
        "no_residual_session_logman"      = (-not $residual)
        "no_overall_error"                = ($null -ne $summary -and $null -eq $summary.error)
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
        "probe=$ProbePath",
        "session_name=$sessionName",
        "session_observed=$active",
        "helper_exit=$helperExit",
        "process_exit_code=$processExitCode",
        "residual_session=$residual",
        "summary_path=$summaryPath",
        "watch_path=$watchPath"
    )
    foreach ($entry in $checks.GetEnumerator()) {
        $lines += "check.$($entry.Key)=$($entry.Value)"
    }
    if ($null -ne $summary) {
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_processed=$($summary.events_processed)"
        $lines += "state=$($summary.state)"
        $lines += "delivery.latest_cursor=$($delivery.latest_cursor)"
        $lines += "delivery.batches_served=$($delivery.batches_served)"
    }
    if ($null -ne $events) {
        $lines += "watch.process_events=$($events.processEvents)"
        $lines += "watch.pairing_revisions=$($events.pairingRevisions)"
        $lines += "watch.session_end_reason=$($events.sessionEndReason)"
        $lines += "watch.records=$($events.records)"
        $lines += "watch.paired_stopped=$($events.pairedStopped)"
        $lines += "watch.unpaired_stopped=$($events.unpairedStopped)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.5 EVENT DELIVERY SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.5 EVENT DELIVERY SMOKE: FAIL"
    Write-Host "Evidence: $runInfoPath"
    exit 1
}
finally {
    foreach ($process in $stimulusProcesses) {
        try {
            if (-not $process.HasExited) { $process.Kill() }
        } catch { }
    }
    if ($null -ne $watcher -and -not $watcher.HasExited) {
        try { $watcher.Kill() } catch { }
    }
    if ($null -ne $sensor -and -not $sensor.HasExited) {
        try {
            if (-not $sensor.WaitForExit(15000)) {
                & taskkill.exe /PID $sensor.Id /T /F | Out-Null
            }
        } catch { }
    }
}
