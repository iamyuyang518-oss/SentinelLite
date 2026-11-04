param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [int]$DurationSeconds = 20,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase24-pipeline-smoke")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This pipeline smoke test requires an elevated Administrator PowerShell. It does not self-elevate.")
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
$sessionName = "SentinelLite-Sensor-Pipeline-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$stdoutPath = Join-Path $LogDir "stdout-$stamp.json"
$stderrPath = Join-Path $LogDir "stderr-$stamp.log"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"
$tempDir = Join-Path $env:TEMP "sentinelite-phase24-$stamp"

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
$stimulusErrors = @()
$sensor = $null
$timedOut = $false

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    $wrapperPath = Join-Path $tempDir "run-sensor.cmd"
    $wrapperLines = @(
        "@echo off",
        "`"$ExePath`" --session-name `"$sessionName`" --duration $DurationSeconds --queue-capacity 4096 --output `"$summaryPath`" > `"$stdoutPath`" 2> `"$stderrPath`"",
        "exit /b %ERRORLEVEL%"
    )
    Set-Content -LiteralPath $wrapperPath -Value ($wrapperLines -join "`r`n") -Encoding ASCII
    $sensor = Start-Process -FilePath $wrapperPath -PassThru -WindowStyle Hidden

    Start-Sleep -Seconds 2
    if (-not $sensor.HasExited) {
        for ($index = 0; $index -lt 3; $index++) {
            try {
                $stimulusProcesses += Start-Process -FilePath $cmd `
                    -ArgumentList "/c timeout /t 2 /nobreak >nul" `
                    -WindowStyle Hidden -PassThru
            } catch {
                $stimulusErrors += "cmd[$index]: $($_.Exception.Message)"
            }
        }
    }

    if (-not $sensor.WaitForExit(($DurationSeconds + 30) * 1000)) {
        $timedOut = $true
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
    }

    $processExitCode = $null
    if (-not $timedOut -and $sensor.HasExited) {
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
    $stderrExitCode = $null
    if (Test-Path -LiteralPath $stderrPath) {
        $stderrText = Get-Content -LiteralPath $stderrPath -Raw
        $stderrMatch = [regex]::Match($stderrText, '\[sensor\] state=[^\r\n]*\sexit=(-?\d+)')
        if ($stderrMatch.Success) {
            $stderrExitCode = [int]$stderrMatch.Groups[1].Value
        }
    }
    $exitCode = if ($timedOut) {
        -1
    } elseif ($null -ne $processExitCode) {
        [int]$processExitCode
    } elseif ($null -ne $jsonExitCode) {
        [int]$jsonExitCode
    } elseif ($null -ne $stderrExitCode) {
        [int]$stderrExitCode
    } else {
        -1
    }
    $exitCodeSourcesAgree = (
        ($null -eq $jsonExitCode -or $null -eq $processExitCode -or $jsonExitCode -eq $processExitCode) -and
        ($null -eq $stderrExitCode -or $null -eq $processExitCode -or $stderrExitCode -eq $processExitCode) -and
        ($null -eq $jsonExitCode -or $null -eq $stderrExitCode -or $jsonExitCode -eq $stderrExitCode)
    )

    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0

    $pipeline = $null
    $sessionEnd = $null
    $firstPaired = $null
    $recentEvents = @()
    if ($null -ne $summary) {
        $pipeline = $summary.pipeline
        if ($null -ne $pipeline) {
            $sessionEnd = $pipeline.session_end
            $firstPaired = $pipeline.first_paired_stop
            if ($null -ne $pipeline.recent_events) {
                $recentEvents = @($pipeline.recent_events)
            }
        }
    }
    $observedImages = @()
    if ($null -ne $summary -and $null -ne $summary.pairing.observed_start_image_names) {
        $observedImages = @($summary.pairing.observed_start_image_names)
    }
    $cmdObserved = @($observedImages | Where-Object {
        $_ -and $_.Equals("cmd.exe", [StringComparison]::OrdinalIgnoreCase)
    }).Count -gt 0

    $pipelineReceivedMatches = (
        $null -ne $summary -and $null -ne $pipeline -and
        [int64]$pipeline.received -eq ([int64]$summary.events_processed + [int64]$summary.events_dropped)
    )
    $pipelineProcessedMatches = (
        $null -ne $summary -and $null -ne $pipeline -and
        [int64]$pipeline.normalized -eq [int64]$summary.events_processed
    )
    $pipelineDroppedMatches = (
        $null -ne $summary -and $null -ne $pipeline -and
        ([int64]$pipeline.invalid + [int64]$pipeline.dropped_queue_full + [int64]$pipeline.dropped_consumer_gone) -eq [int64]$summary.events_dropped
    )
    $firstPairedValid = (
        $null -ne $firstPaired -and
        $null -ne $firstPaired.event_id -and
        $null -ne $firstPaired.pairing -and
        $firstPaired.pairing.state -eq "paired" -and
        $null -ne $firstPaired.pairing.start_event_id -and
        $null -ne $firstPaired.pairing.stop_event_id
    )
    $recentEventValid = $false
    if ($recentEvents.Count -gt 0) {
        $sample = $recentEvents[0]
        $recentEventValid = (
            $null -ne $sample.event_id -and
            $null -ne $sample.session_id -and
            $null -ne $sample.etw -and
            $null -ne $sample.etw.occurred_at -and
            $null -ne $sample.received_at -and
            $null -ne $sample.normalized_at -and
            $null -ne $sample.completeness
        )
    }

    $checks = [ordered]@{
        "not_timed_out"                  = (-not $timedOut)
        "exit_code_is_0"                 = ($exitCode -eq 0)
        "exit_code_sources_agree"        = $exitCodeSourcesAgree
        "summary_parsed"                 = ($null -ne $summary)
        "session_created"                = ($null -ne $summary -and $summary.session_created -eq $true)
        "session_stopped"                = ($null -ne $summary -and $summary.session_stopped -eq $true)
        "trace_opened"                   = ($null -ne $summary -and $summary.trace_opened -eq $true)
        "trace_closed"                   = ($null -ne $summary -and $summary.trace_closed -eq $true)
        "not_interrupted"                = ($null -ne $summary -and $summary.interrupted -eq $false)
        "window_complete"                = ($null -ne $summary -and $summary.window_complete -eq $true)
        "health_ok"                      = ($null -ne $summary -and $summary.ok -eq $true)
        "no_health_last_error"           = ($null -ne $summary -and $null -eq $summary.last_error)
        "events_processed_positive"      = ($null -ne $summary -and [int64]$summary.events_processed -gt 0)
        "events_dropped_zero"            = ($null -ne $summary -and [int64]$summary.events_dropped -eq 0)
        "etw_events_lost_zero"           = ($null -ne $summary -and [int64]$summary.events_lost -eq 0)
        "realtime_buffers_lost_zero"     = ($null -ne $summary -and [int64]$summary.realtime_buffers_lost -eq 0)
        "decode_errors_zero"             = ($null -ne $summary -and [int64]$summary.decode_errors -eq 0)
        "session_id_present"             = ($null -ne $summary -and -not [string]::IsNullOrWhiteSpace($summary.session_id))
        "pipeline_present"               = ($null -ne $pipeline)
        "pipeline_session_matches"       = ($null -ne $summary -and $null -ne $pipeline -and $summary.session_id -eq $pipeline.session_id)
        "pipeline_capacity_4096"         = ($null -ne $pipeline -and [int64]$pipeline.capacity -eq 4096)
        "pipeline_consumer_stopped"      = ($null -ne $pipeline -and $pipeline.consumer_state -eq "stopped")
        "pipeline_consumer_no_error"     = ($null -ne $pipeline -and $null -eq $pipeline.consumer_error)
        "pipeline_no_shutdown_timeout"   = ($null -ne $pipeline -and $pipeline.shutdown_timed_out -eq $false)
        "pipeline_no_queue_full_drops"   = ($null -ne $pipeline -and [int64]$pipeline.dropped_queue_full -eq 0)
        "pipeline_no_consumer_gone_drops" = ($null -ne $pipeline -and [int64]$pipeline.dropped_consumer_gone -eq 0)
        "pipeline_backlog_zero"          = ($null -ne $pipeline -and [int64]$pipeline.backlog -eq 0)
        "pipeline_sink_errors_zero"      = ($null -ne $pipeline -and [int64]$pipeline.sink_errors -eq 0)
        "pipeline_received_accounted"    = $pipelineReceivedMatches
        "pipeline_processed_accounted"   = $pipelineProcessedMatches
        "pipeline_dropped_accounted"     = $pipelineDroppedMatches
        "pipeline_normalized_started"    = ($null -ne $pipeline -and [int64]$pipeline.normalized_started -gt 0)
        "pipeline_normalized_stopped"    = ($null -ne $pipeline -and [int64]$pipeline.normalized_stopped -gt 0)
        "pairing_paired_positive"        = ($null -ne $summary -and [int64]$summary.pairing.paired -gt 0)
        "pairing_no_duplicates"          = ($null -ne $summary -and [int64]$summary.pairing.duplicate_starts -eq 0 -and [int64]$summary.pairing.duplicate_stops -eq 0)
        "pairing_no_integrity_errors"    = ($null -ne $summary -and [int64]$summary.pairing.pid_reuse_detected -eq 0 -and [int64]$summary.pairing.sequence_mismatches -eq 0 -and [int64]$summary.pairing.start_overflow -eq 0 -and [int64]$summary.pairing.out_of_order_pairs -eq 0 -and [int64]$summary.pairing.indeterminate_identities -eq 0)
        "first_paired_stop_valid"        = $firstPairedValid
        "recent_event_valid"             = $recentEventValid
        "session_end_completed"          = ($null -ne $sessionEnd -and $sessionEnd.reason -eq "completed")
        "session_end_open_starts_array"  = ($null -ne $sessionEnd -and $null -ne $sessionEnd.open_start_events)
        "cmd_stimulus_observed"          = $cmdObserved
        "stimulus_launches_ok"           = ($stimulusErrors.Count -eq 0)
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
        "duration_seconds=$DurationSeconds",
        "process_exit_code=$processExitCode",
        "json_exit_code=$jsonExitCode",
        "stderr_exit_code=$stderrExitCode",
        "resolved_exit_code=$exitCode",
        "residual_session=$residual",
        "cmd_stimulus_observed=$cmdObserved",
        "stimulus_errors=$($stimulusErrors -join ' | ')",
        "summary_path=$summaryPath",
        "stdout_path=$stdoutPath",
        "stderr_path=$stderrPath"
    )
    foreach ($entry in $checks.GetEnumerator()) {
        $lines += "check.$($entry.Key)=$($entry.Value)"
    }
    if ($null -ne $summary) {
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_processed=$($summary.events_processed)"
        $lines += "events_dropped=$($summary.events_dropped)"
        $lines += "events_lost=$($summary.events_lost)"
        $lines += "ignored_events=$($summary.ignored_events)"
        $lines += "state=$($summary.state)"
        $lines += "pipeline.received=$($pipeline.received)"
        $lines += "pipeline.normalized=$($pipeline.normalized)"
        $lines += "pipeline.invalid=$($pipeline.invalid)"
        $lines += "pipeline.peak_backlog=$($pipeline.peak_backlog)"
        $lines += "pipeline.normalized_started=$($pipeline.normalized_started)"
        $lines += "pipeline.normalized_stopped=$($pipeline.normalized_stopped)"
        $lines += "pairing.paired=$($summary.pairing.paired)"
        $lines += "pairing.starts_seen=$($summary.pairing.starts_seen)"
        $lines += "pairing.stops_seen=$($summary.pairing.stops_seen)"
        $lines += "pairing.unpaired_starts=$($summary.pairing.unpaired_starts)"
        $lines += "pairing.orphan_stops=$($summary.pairing.orphan_stops)"
        $lines += "session_end.reason=$($sessionEnd.reason)"
        $lines += "session_end.open_start_events=$($sessionEnd.open_start_events.Count)"
        $lines += "first_paired_event_id=$($firstPaired.event_id)"
        $lines += "first_paired_image=$($firstPaired.image_name)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.4 PIPELINE SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.4 PIPELINE SMOKE: FAIL"
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
            if (-not $sensor.WaitForExit(($DurationSeconds + 30) * 1000)) {
                & taskkill.exe /PID $sensor.Id /T /F | Out-Null
            }
        } catch { }
    }
}
