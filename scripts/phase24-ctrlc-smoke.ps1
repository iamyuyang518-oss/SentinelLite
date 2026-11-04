param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [int]$WaitSeconds = 60,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase24-ctrlc-smoke")
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
$tempDir = Join-Path $env:TEMP "sentinelite-phase24-ctrlc-$stamp"

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
$helperExit = $null

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    $sensor = Start-Process -FilePath $ExePath `
        -ArgumentList @("--serve", "--session-name", $sessionName, "--queue-capacity", "4096", "--output", $summaryPath) `
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

    if ($active) {
        for ($index = 0; $index -lt 3; $index++) {
            try {
                $stimulusProcesses += Start-Process -FilePath $cmd `
                    -ArgumentList "/c timeout /t 2 /nobreak >nul" `
                    -WindowStyle Hidden -PassThru
            } catch { }
        }
        Start-Sleep -Seconds 3
    }

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

    if ($active -and -not $sensor.HasExited) {
        $helper = Start-Process -FilePath "powershell.exe" `
            -ArgumentList @("-NoProfile", "-File", $helperPath, "$($sensor.Id)") `
            -PassThru -Wait -WindowStyle Hidden
        $helperExit = $helper.ExitCode
    }

    if (-not $sensor.WaitForExit($WaitSeconds * 1000)) {
        [Console]::Error.WriteLine("Sensor did not exit after Ctrl-C within $WaitSeconds seconds.")
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
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
        try {
            $summary = Get-Content -LiteralPath $summaryPath -Raw -Encoding UTF8 | ConvertFrom-Json
        } catch {
            $summary = $null
        }
    }

    $jsonExitCode = $null
    if ($null -ne $summary -and $null -ne $summary.exit_code) {
        $jsonExitCode = [int]$summary.exit_code
    }
    $exitCode = if ($null -ne $processExitCode) {
        [int]$processExitCode
    } elseif ($null -ne $jsonExitCode) {
        [int]$jsonExitCode
    } else {
        -1
    }
    $exitCodeSourcesAgree = (
        $null -eq $jsonExitCode -or $null -eq $processExitCode -or $jsonExitCode -eq $processExitCode
    )

    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0

    $pipeline = $null
    $sessionEnd = $null
    if ($null -ne $summary) {
        $pipeline = $summary.pipeline
        if ($null -ne $pipeline) {
            $sessionEnd = $pipeline.session_end
        }
    }
    $pipelineReceivedMatches = (
        $null -ne $summary -and $null -ne $pipeline -and
        [int64]$pipeline.received -eq ([int64]$summary.events_processed + [int64]$summary.events_dropped)
    )

    $checks = [ordered]@{
        "session_observed"               = $active
        "ctrl_c_event_sent"              = ($helperExit -eq 0)
        "sensor_exited"                  = ($sensor.HasExited -eq $true)
        "exit_code_is_degraded_3"        = ($exitCode -eq 3)
        "exit_code_sources_agree"        = $exitCodeSourcesAgree
        "summary_parsed"                 = ($null -ne $summary)
        "interrupted_true"               = ($null -ne $summary -and $summary.interrupted -eq $true)
        "window_incomplete"              = ($null -ne $summary -and $summary.window_complete -eq $false)
        "session_created"                = ($null -ne $summary -and $summary.session_created -eq $true)
        "session_stopped"                = ($null -ne $summary -and $summary.session_stopped -eq $true)
        "trace_opened"                   = ($null -ne $summary -and $summary.trace_opened -eq $true)
        "trace_closed"                   = ($null -ne $summary -and $summary.trace_closed -eq $true)
        "pipeline_present"               = ($null -ne $pipeline)
        "pipeline_consumer_stopped"      = ($null -ne $pipeline -and $pipeline.consumer_state -eq "stopped")
        "pipeline_no_consumer_error"     = ($null -ne $pipeline -and $null -eq $pipeline.consumer_error)
        "pipeline_no_shutdown_timeout"   = ($null -ne $pipeline -and $pipeline.shutdown_timed_out -eq $false)
        "pipeline_backlog_zero"          = ($null -ne $pipeline -and [int64]$pipeline.backlog -eq 0)
        "pipeline_no_queue_full_drops"   = ($null -ne $pipeline -and [int64]$pipeline.dropped_queue_full -eq 0)
        "pipeline_no_consumer_gone_drops" = ($null -ne $pipeline -and [int64]$pipeline.dropped_consumer_gone -eq 0)
        "pipeline_received_accounted"    = $pipelineReceivedMatches
        "pipeline_normalized_positive"   = ($null -ne $pipeline -and [int64]$pipeline.normalized -gt 0)
        "session_end_interrupted"        = ($null -ne $sessionEnd -and $sessionEnd.reason -eq "interrupted")
        "events_processed_positive"      = ($null -ne $summary -and [int64]$summary.events_processed -gt 0)
        "events_dropped_zero"            = ($null -ne $summary -and [int64]$summary.events_dropped -eq 0)
        "etw_events_lost_zero"           = ($null -ne $summary -and [int64]$summary.events_lost -eq 0)
        "decode_errors_zero"             = ($null -ne $summary -and [int64]$summary.decode_errors -eq 0)
        "no_residual_session_json"       = ($null -ne $summary -and $summary.residual_session -eq $false)
        "no_residual_session_logman"     = (-not $residual)
        "no_session_stop_error"          = ($null -ne $summary -and $null -eq $summary.session_stop_error)
        "no_session_close_error"         = ($null -ne $summary -and $null -eq $summary.session_close_error)
        "no_process_trace_error"         = ($null -ne $summary -and $null -eq $summary.process_trace_error)
        "no_overall_error"               = ($null -ne $summary -and $null -eq $summary.error)
        "protection_claim_none"          = ($null -ne $summary -and $summary.protection_claim -eq "none")
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
        "session_observed=$active",
        "helper_exit=$helperExit",
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
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_processed=$($summary.events_processed)"
        $lines += "events_dropped=$($summary.events_dropped)"
        $lines += "ignored_events=$($summary.ignored_events)"
        $lines += "state=$($summary.state)"
        $lines += "interrupted=$($summary.interrupted)"
        $lines += "pipeline.received=$($pipeline.received)"
        $lines += "pipeline.normalized=$($pipeline.normalized)"
        $lines += "pipeline.peak_backlog=$($pipeline.peak_backlog)"
        $lines += "session_end.reason=$($sessionEnd.reason)"
        $lines += "session_end.open_start_events=$($sessionEnd.open_start_events.Count)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.4 CTRL-C SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.4 CTRL-C SMOKE: FAIL"
    Write-Host "Evidence: $runInfoPath"
    exit 1
}
finally {
    foreach ($process in $stimulusProcesses) {
        try {
            if (-not $process.HasExited) { $process.Kill() }
        } catch { }
    }
    if ($null -ne $sensor -and -not $sensor.HasExited) {
        try {
            if (-not $sensor.WaitForExit(15000)) {
                & taskkill.exe /PID $sensor.Id /T /F | Out-Null
            }
        } catch { }
    }
}
