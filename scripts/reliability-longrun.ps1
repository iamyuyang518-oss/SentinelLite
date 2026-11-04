param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [string]$ProbePath = (Join-Path $PSScriptRoot "..\target\release\examples\probe.exe"),
    [string]$StorageProbePath = (Join-Path $PSScriptRoot "..\target\release\examples\storage_probe.exe"),
    [string]$AnalyzeProbePath = (Join-Path $PSScriptRoot "..\target\release\examples\analyze_probe.exe"),
    [int]$DurationMinutes = 30,
    [int]$AppProcessId = 0,
    [string]$DatabasePath = "",
    [string]$PythonPath = "",
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\reliability-longrun")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This long-run test requires an elevated Administrator PowerShell. It does not self-elevate.")
    exit 2
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
foreach ($path in @($ExePath, $ProbePath, $StorageProbePath, $AnalyzeProbePath)) {
    if (-not (Test-Path -LiteralPath $path)) {
        [Console]::Error.WriteLine("Required binary not found: $path")
        exit 2
    }
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path
$ProbePath = (Resolve-Path -LiteralPath $ProbePath).Path
$StorageProbePath = (Resolve-Path -LiteralPath $StorageProbePath).Path
$AnalyzeProbePath = (Resolve-Path -LiteralPath $AnalyzeProbePath).Path

if ($AppProcessId -le 0) {
    [Console]::Error.WriteLine("-AppProcessId is required; start the non-elevated sentinel-lite.exe first.")
    exit 2
}
$app = Get-Process -Id $AppProcessId -ErrorAction SilentlyContinue
if ($null -eq $app -or $app.ProcessName -ne "sentinel-lite") {
    [Console]::Error.WriteLine("App process $AppProcessId is not a running sentinel-lite.exe.")
    exit 2
}

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$sessionName = "SentinelLite-Sensor-LongRun-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$csvPath = Join-Path $LogDir "samples-$stamp.csv"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"
$statusPath = Join-Path $LogDir "status-$stamp.json"
$storageProbeOutputPath = Join-Path $LogDir "storage-$stamp.json"
$analysisPath = Join-Path $LogDir "analysis-$stamp.json"
$tempDir = Join-Path $env:TEMP "sentinelite-phase29-longrun-$stamp"
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
$dbMetricsScript = Join-Path $tempDir "dbmetrics.py"

$commit = "unknown"
$gitStatus = ""
try {
    $commit = (& git -C $repoRoot rev-parse HEAD 2>$null | Select-Object -First 1)
    $gitStatus = ((& git -C $repoRoot status --porcelain 2>$null) -join "`n")
} catch { $commit = "unknown" }
$dirty = -not [string]::IsNullOrWhiteSpace($gitStatus)
$sensorHash = (Get-FileHash -Path $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$probeHash = (Get-FileHash -Path $ProbePath -Algorithm SHA256).Hash.ToLowerInvariant()
$appHash = (Get-FileHash -Path $app.Path -Algorithm SHA256).Hash.ToLowerInvariant()
$python = $null
if (-not [string]::IsNullOrWhiteSpace($PythonPath) -and (Test-Path -LiteralPath $PythonPath)) {
    $python = $PythonPath
} else {
    $python = (Get-Command python -ErrorAction SilentlyContinue | Select-Object -First 1).Source
}
if (-not $python) {
    foreach ($candidate in @(
        (Join-Path $env:LOCALAPPDATA "Programs\Python\Python312\python.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\Python\Python311\python.exe")
    )) {
        if (Test-Path -LiteralPath $candidate) { $python = $candidate; break }
    }
}
$cmd = Join-Path $env:SystemRoot "System32\cmd.exe"
$logicalProcessors = [Environment]::ProcessorCount

function Get-TreeProcessIds([int]$RootId) {
    $processes = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    $ids = New-Object "System.Collections.Generic.HashSet[int]"
    [void]$ids.Add($RootId)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($process in $processes) {
            $parent = [int]$process.ParentProcessId
            $child = [int]$process.ProcessId
            if ($ids.Contains($parent) -and -not $ids.Contains($child)) {
                [void]$ids.Add($child)
                $changed = $true
            }
        }
    }
    return @($ids)
}

function Get-TreeMetrics([int]$RootId) {
    $ids = Get-TreeProcessIds $RootId
    $cpu = [TimeSpan]::Zero
    $working = 0L
    $private = 0L
    $count = 0
    foreach ($id in $ids) {
        $process = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($null -eq $process) { continue }
        try {
            $cpu += $process.TotalProcessorTime
            $working += $process.WorkingSet64
            $private += $process.PrivateMemorySize64
            $count += 1
        } catch { }
    }
    return [pscustomobject]@{
        Cpu = $cpu
        WorkingSetBytes = $working
        PrivateBytes = $private
        ProcessCount = $count
    }
}

function Convert-ToPercent([TimeSpan]$Delta, [TimeSpan]$Elapsed) {
    if ($Elapsed.TotalSeconds -le 0) { return 0.0 }
    return [Math]::Round(($Delta.TotalSeconds / ($Elapsed.TotalSeconds * $logicalProcessors)) * 100.0, 3)
}

function Get-DbMetrics([string]$Database) {
    if (-not $python -or -not (Test-Path -LiteralPath $Database)) { return $null }
    $code = @'
import json, sqlite3, sys
path = sys.argv[1]
# The database is an isolated test copy. A read-write connection can see the
# application's WAL while the app is running; the real user database is never
# used by this harness.
con = sqlite3.connect(path, timeout=2.0)
cur = con.cursor()
def scalar(sql, default=None):
    row = cur.execute(sql).fetchone()
    return row[0] if row else default
latest = cur.execute(
    "SELECT closed, end_reason, (session_end_json IS NOT NULL), "
    "(json_extract(session_end_json, '$.final_stats') IS NOT NULL) "
    "FROM sessions ORDER BY last_seen_at DESC LIMIT 1"
).fetchone()
payload = {
    "event_count": int(scalar("SELECT COUNT(*) FROM process_events", 0) or 0),
    "invalid_count": int(scalar("SELECT COUNT(*) FROM invalid_events", 0) or 0),
    "session_count": int(scalar("SELECT COUNT(*) FROM sessions", 0) or 0),
    "gap_count": int(scalar("SELECT COUNT(*) FROM ingestion_gaps", 0) or 0),
    "final_stats_sessions": int(scalar(
        "SELECT COUNT(*) FROM sessions WHERE json_extract(session_end_json, '$.final_stats') IS NOT NULL",
        0) or 0),
    "latest_closed": bool(latest[0]) if latest else None,
    "latest_end_reason": latest[1] if latest else None,
    "latest_has_session_end": bool(latest[2]) if latest else None,
    "latest_has_final_stats": bool(latest[3]) if latest else None,
}
print(json.dumps(payload))
'@
    try {
        if (-not (Test-Path -LiteralPath $dbMetricsScript)) {
            Set-Content -LiteralPath $dbMetricsScript -Value $code -Encoding UTF8
        }
        $output = & $python -I $dbMetricsScript $Database 2>$null
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($output)) {
            return $output | ConvertFrom-Json
        }
    } catch { }
    return $null
}

function Stop-SensorGracefully {
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
    $helper = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-File", $helperPath, "$($sensor.Id)") `
        -PassThru -Wait -WindowStyle Hidden
    return $helper.ExitCode
}

$sensor = $null
$scriptFailed = $false
$samples = @()
$sensorCpuLast = $null
$appCpuLast = $null
$sampleIndex = 0
$probeFailures = 0
$stimulusFailures = 0

try {
    $sensor = Start-Process -FilePath $ExePath `
        -ArgumentList @("--serve", "--session-name", $sessionName, "--queue-capacity", "4096", "--event-buffer", "4096", "--output", $summaryPath) `
        -WindowStyle Hidden -PassThru

    $active = $false
    for ($attempt = 0; $attempt -lt 80; $attempt++) {
        $match = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
        if ($match.Count -gt 0) { $active = $true; break }
        if ($sensor.HasExited) { break }
        Start-Sleep -Milliseconds 250
    }
    if (-not $active) {
        [Console]::Error.WriteLine("Sensor ETW session was not observed.")
    }

    Start-Sleep -Seconds 2
    $initialDb = Get-DbMetrics $DatabasePath
    $start = Get-Date
    $lastTick = $start
    $sensorCpuLast = (Get-Process -Id $sensor.Id -ErrorAction SilentlyContinue).TotalProcessorTime
    $appCpuLast = (Get-TreeMetrics $AppProcessId).Cpu
    $deadline = $start.AddMinutes($DurationMinutes)

    while ((Get-Date) -lt $deadline) {
        $tick = Get-Date
        Start-Sleep -Seconds 15
        if ($sensor.HasExited) { break }

        # Two short-lived processes per sample, about 4 events per sample.
        for ($i = 0; $i -lt 2; $i++) {
            try {
                Start-Process -FilePath $cmd -ArgumentList "/c exit" -WindowStyle Hidden | Out-Null
            } catch { $stimulusFailures += 1 }
        }

        $sensorProcess = Get-Process -Id $sensor.Id -ErrorAction SilentlyContinue
        if ($null -eq $sensorProcess) { break }
        $interval = $tick - $lastTick
        $sensorCpuNow = $sensorProcess.TotalProcessorTime
        $sensorCpuPct = Convert-ToPercent ($sensorCpuNow - $sensorCpuLast) $interval
        $sensorCpuLast = $sensorCpuNow
        $sensorWorking = [Math]::Round($sensorProcess.WorkingSet64 / 1MB, 2)
        $sensorPrivate = [Math]::Round($sensorProcess.PrivateMemorySize64 / 1MB, 2)

        $appTree = Get-TreeMetrics $AppProcessId
        $appCpuPct = Convert-ToPercent ($appTree.Cpu - $appCpuLast) $interval
        $appCpuLast = $appTree.Cpu
        $lastTick = $tick
        $appWorking = [Math]::Round($appTree.WorkingSetBytes / 1MB, 2)
        $appPrivate = [Math]::Round($appTree.PrivateBytes / 1MB, 2)

        $status = $null
        & $ProbePath --output $statusPath 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $statusPath)) {
            try { $status = (Get-Content -LiteralPath $statusPath -Raw -Encoding UTF8 | ConvertFrom-Json).status } catch { $status = $null }
        }
        if ($null -eq $status) { $probeFailures += 1 }

        $dbMetrics = $null
        if (($sampleIndex % 4) -eq 0) { $dbMetrics = Get-DbMetrics $DatabasePath }
        $dbBytes = 0
        $dbWalBytes = 0
        if (Test-Path -LiteralPath $DatabasePath) {
            $dbBytes = (Get-Item -LiteralPath $DatabasePath).Length
            $wal = "$DatabasePath-wal"
            if (Test-Path -LiteralPath $wal) { $dbWalBytes = (Get-Item -LiteralPath $wal).Length }
        }

        $received = if ($null -eq $status) { $null } else { [int64]$status.events_received }
        $processed = if ($null -eq $status) { $null } else { [int64]$status.events_processed }
        $dropped = if ($null -eq $status) { $null } else { [int64]$status.events_dropped }
        $inflight = if ($null -eq $received -or $null -eq $processed -or $null -eq $dropped) { $null } else { $received - $processed - $dropped }
        $dbEventCount = if ($null -eq $dbMetrics) { $null } else { [int64]$dbMetrics.event_count }
        $ingestLag = if ($null -eq $processed -or $null -eq $dbEventCount) { $null } else { $processed - $dbEventCount }

        $samples += [pscustomobject]@{
            timestamp = $tick.ToString("o")
            elapsed_seconds = [int]($tick - $start).TotalSeconds
            sensor_cpu_pct = $sensorCpuPct
            sensor_working_set_mb = $sensorWorking
            sensor_private_mb = $sensorPrivate
            sensor_events_received = $received
            sensor_events_processed = $processed
            sensor_events_dropped = $dropped
            sensor_events_lost = if ($null -eq $status) { $null } else { $status.events_lost }
            sensor_realtime_buffers_lost = if ($null -eq $status) { $null } else { $status.realtime_buffers_lost }
            sensor_health_ok = if ($null -eq $status) { $null } else { $status.health_ok }
            sensor_inflight = $inflight
            app_cpu_pct = $appCpuPct
            app_working_set_mb = $appWorking
            app_private_mb = $appPrivate
            app_process_count = $appTree.ProcessCount
            db_bytes = $dbBytes
            db_wal_bytes = $dbWalBytes
            db_event_count = $dbEventCount
            db_session_count = if ($null -eq $dbMetrics) { $null } else { $dbMetrics.session_count }
            db_gap_count = if ($null -eq $dbMetrics) { $null } else { $dbMetrics.gap_count }
            db_latest_closed = if ($null -eq $dbMetrics) { $null } else { $dbMetrics.latest_closed }
            db_latest_has_session_end = if ($null -eq $dbMetrics) { $null } else { $dbMetrics.latest_has_session_end }
            db_final_stats_sessions = if ($null -eq $dbMetrics) { $null } else { $dbMetrics.final_stats_sessions }
            ingest_lag = $ingestLag
        }
        $samples | Export-Csv -LiteralPath $csvPath -NoTypeInformation -Encoding UTF8
        $sampleIndex += 1
    }

    $helperExit = $null
    if (-not $sensor.HasExited) {
        $helperExit = Stop-SensorGracefully
    }
    $forced = $false
    if (-not $sensor.WaitForExit(60000)) {
        $forced = $true
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
    }
    # Give the non-elevated ingestion worker time to commit the final
    # SessionEnd, including the new final_stats block.
    $persistDeadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Seconds 2
        $persistCheck = Get-DbMetrics $DatabasePath
    } while (($null -eq $persistCheck -or [int64]$persistCheck.final_stats_sessions -lt 1) -and (Get-Date) -lt $persistDeadline)
    Start-Sleep -Seconds 1

    $processExitCode = $null
    if ($sensor.HasExited) {
        try { $sensor.WaitForExit(); $sensor.Refresh(); $processExitCode = $sensor.ExitCode } catch { }
    }
    $elapsed = ((Get-Date) - $start).TotalSeconds

    $summary = $null
    if (Test-Path -LiteralPath $summaryPath) {
        try { $summary = Get-Content -LiteralPath $summaryPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }
    $storage = $null
    if (Test-Path -LiteralPath $DatabasePath) {
        & $StorageProbePath --database $DatabasePath --sessions --limit 100 --output $storageProbeOutputPath 2>$null | Out-Null
        try { $storage = Get-Content -LiteralPath $storageProbeOutputPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }
    $analysis = $null
    if (Test-Path -LiteralPath $DatabasePath) {
        & $AnalyzeProbePath --database $DatabasePath --output $analysisPath 2>$null | Out-Null
        try { $analysis = Get-Content -LiteralPath $analysisPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }
    $finalDb = Get-DbMetrics $DatabasePath
    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0

    $finalStats = $null
    if ($null -ne $summary -and $null -ne $summary.pipeline -and $null -ne $summary.pipeline.session_end) {
        $finalStats = $summary.pipeline.session_end.final_stats
    }
    $latestSession = $null
    if ($null -ne $storage -and $null -ne $storage.sessions -and $storage.sessions.Count -gt 0) {
        $latestSession = $storage.sessions | Sort-Object last_seen_at -Descending | Select-Object -First 1
    }
    $analysisInput = if ($null -eq $analysis) { $null } else { $analysis.analysis.input }
    $analysisRules = if ($null -eq $analysis) { @() } else { @($analysis.analysis.results | ForEach-Object { $_.rule_id }) }

    $sensorCpuValues = @($samples | ForEach-Object { [double]$_.sensor_cpu_pct })
    $appCpuValues = @($samples | ForEach-Object { [double]$_.app_cpu_pct })
    $sensorWorkingValues = @($samples | ForEach-Object { [double]$_.sensor_working_set_mb })
    $appWorkingValues = @($samples | ForEach-Object { [double]$_.app_working_set_mb })
    $healthySamples = @($samples | Where-Object { $_.sensor_health_ok -eq $true }).Count
    $lossSamples = @($samples | Where-Object { ($null -ne $_.sensor_events_lost -and [int64]$_.sensor_events_lost -gt 0) -or ($null -ne $_.sensor_realtime_buffers_lost -and [int64]$_.sensor_realtime_buffers_lost -gt 0) -or ($null -ne $_.sensor_events_dropped -and [int64]$_.sensor_events_dropped -gt 0) }).Count
    $dbCounts = @($samples | Where-Object { $null -ne $_.db_event_count } | ForEach-Object { [int64]$_.db_event_count })
    $lagValues = @($samples | Where-Object { $null -ne $_.ingest_lag } | ForEach-Object { [int64]$_.ingest_lag })
    if ($sensorCpuValues.Count -eq 0) { $sensorCpuValues = @(0) }
    if ($appCpuValues.Count -eq 0) { $appCpuValues = @(0) }
    if ($sensorWorkingValues.Count -eq 0) { $sensorWorkingValues = @(0) }
    if ($appWorkingValues.Count -eq 0) { $appWorkingValues = @(0) }
    if ($dbCounts.Count -eq 0) { $dbCounts = @(0) }
    if ($lagValues.Count -eq 0) { $lagValues = @(0) }

    $checks = [ordered]@{
        "duration_at_least_30_minutes" = ($DurationMinutes -ge 30)
        "session_observed" = $active
        "sensor_exited" = ($sensor.HasExited -eq $true)
        "graceful_ctrl_c" = ($helperExit -eq 0 -and -not $forced)
        "exit_code_is_3" = ($processExitCode -eq 3)
        "summary_parsed" = ($null -ne $summary)
        "session_end_present" = ($null -ne $summary -and $null -ne $summary.pipeline.session_end)
        "final_stats_present" = ($null -ne $finalStats)
        "final_etw_events_lost_0" = ($null -ne $finalStats -and $finalStats.events_lost -eq 0)
        "final_etw_realtime_lost_0" = ($null -ne $finalStats -and $finalStats.realtime_buffers_lost -eq 0)
        "final_pipeline_backlog_0" = ($null -ne $finalStats -and [int64]$finalStats.pipeline_backlog -eq 0)
        "final_pipeline_drops_0" = ($null -ne $finalStats -and [int64]$finalStats.pipeline_dropped_queue_full -eq 0 -and [int64]$finalStats.pipeline_dropped_consumer_gone -eq 0)
        "final_pipeline_shutdown_complete" = ($null -ne $finalStats -and $finalStats.pipeline_shutdown_timed_out -eq $false)
        "events_processed_positive" = ($null -ne $summary -and [int64]$summary.events_processed -gt 0)
        "no_residual_session" = (-not $residual)
        "db_probe_parsed" = ($null -ne $storage -and $null -eq $storage.statusError)
        "db_final_stats_persisted" = ($null -ne $finalDb -and [int64]$finalDb.final_stats_sessions -ge 1)
        "db_latest_session_closed" = ($null -ne $finalDb -and $finalDb.latest_closed -eq $true)
        "db_latest_session_end" = ($null -ne $finalDb -and $finalDb.latest_has_session_end -eq $true)
        "db_latest_final_stats" = ($null -ne $finalDb -and $finalDb.latest_has_final_stats -eq $true)
        "db_event_count_matches_processed" = ($null -ne $finalDb -and $null -ne $summary -and [int64]$finalDb.event_count -eq [int64]$summary.events_processed)
        "db_no_gaps" = ($null -ne $finalDb -and [int64]$finalDb.gap_count -eq 0)
        "analysis_etw_known_zero" = ($null -ne $analysisInput -and $analysisInput.etw_events_lost -eq 0)
        "analysis_pipeline_dropped_known_zero" = ($null -ne $analysisInput -and $analysisInput.pipeline_dropped -eq 0)
        "analysis_pipeline_backlog_known_zero" = ($null -ne $analysisInput -and $analysisInput.pipeline_backlog -eq 0)
        "analysis_no_etw_unknown_rule" = (-not ($analysisRules -contains "DATA-ETW-UNKNOWN-001"))
        "analysis_no_loss_rule" = (-not ($analysisRules -contains "DATA-LOSS-001"))
        "probe_failures_0" = ($probeFailures -eq 0)
        "stimulus_failures_0" = ($stimulusFailures -eq 0)
        "samples_recorded" = ($samples.Count -ge [Math]::Max(4, $DurationMinutes * 2))
        "sensor_healthy_samples" = ($healthySamples -ge [Math]::Max(1, $samples.Count - 2))
        "no_loss_samples" = ($lossSamples -eq 0)
        "app_alive_at_end" = ($null -ne (Get-Process -Id $AppProcessId -ErrorAction SilentlyContinue))
    }

    $failed = @($checks.GetEnumerator() | Where-Object { -not $_.Value })
    $status = if ($failed.Count -eq 0) { "PASS" } else { "FAIL" }

    $lines = @(
        "status=$status",
        "timestamp=$stamp",
        "source_commit=$commit",
        "source_worktree_dirty=$dirty",
        "sensor_binary=$ExePath",
        "sensor_sha256=$sensorHash",
        "probe_binary=$ProbePath",
        "probe_sha256=$probeHash",
        "app_binary=$($app.Path)",
        "app_sha256=$appHash",
        "app_pid=$AppProcessId",
        "database=$DatabasePath",
        "session_name=$sessionName",
        "duration_minutes=$DurationMinutes",
        "elapsed_seconds=$([Math]::Round($elapsed, 1))",
        "process_exit_code=$processExitCode",
        "helper_exit=$helperExit",
        "forced_kill=$forced",
        "residual_session=$residual",
        "samples=$($samples.Count)",
        "python_path=$python",
        "probe_failures=$probeFailures",
        "stimulus_failures=$stimulusFailures",
        "summary_path=$summaryPath",
        "samples_csv=$csvPath",
        "storage_probe_path=$storageProbeOutputPath",
        "analysis_path=$analysisPath"
    )
    foreach ($entry in $checks.GetEnumerator()) {
        $lines += "check.$($entry.Key)=$($entry.Value)"
    }
    if ($null -ne $summary) {
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_processed=$($summary.events_processed)"
        $lines += "events_dropped=$($summary.events_dropped)"
        $lines += "events_lost=$($summary.events_lost)"
        $lines += "realtime_buffers_lost=$($summary.realtime_buffers_lost)"
        $lines += "pipeline_peak_backlog=$($summary.pipeline.peak_backlog)"
        $lines += "pipeline_normalized=$($summary.pipeline.normalized)"
        $lines += "session_end_reason=$($summary.pipeline.session_end.reason)"
    }
    if ($null -ne $finalStats) {
        $lines += "final_stats.events_lost=$($finalStats.events_lost)"
        $lines += "final_stats.realtime_buffers_lost=$($finalStats.realtime_buffers_lost)"
        $lines += "final_stats.pipeline_received=$($finalStats.pipeline_received)"
        $lines += "final_stats.pipeline_consumed=$($finalStats.pipeline_consumed)"
        $lines += "final_stats.pipeline_normalized=$($finalStats.pipeline_normalized)"
        $lines += "final_stats.pipeline_invalid=$($finalStats.pipeline_invalid)"
        $lines += "final_stats.pipeline_filtered_ignored=$($finalStats.pipeline_filtered_ignored)"
        $lines += "final_stats.pipeline_backlog=$($finalStats.pipeline_backlog)"
        $lines += "final_stats.pipeline_peak_backlog=$($finalStats.pipeline_peak_backlog)"
        $lines += "final_stats.pipeline_shutdown_timed_out=$($finalStats.pipeline_shutdown_timed_out)"
    }
    if ($null -ne $finalDb) {
        $lines += "db.event_count=$($finalDb.event_count)"
        $lines += "db.invalid_count=$($finalDb.invalid_count)"
        $lines += "db.session_count=$($finalDb.session_count)"
        $lines += "db.gap_count=$($finalDb.gap_count)"
        $lines += "db.final_stats_sessions=$($finalDb.final_stats_sessions)"
        $lines += "db.latest_end_reason=$($finalDb.latest_end_reason)"
    }
    if ($null -ne $analysisInput) {
        $lines += "analysis.etw_events_lost=$($analysisInput.etw_events_lost)"
        $lines += "analysis.realtime_buffers_lost=$($analysisInput.realtime_buffers_lost)"
        $lines += "analysis.pipeline_dropped=$($analysisInput.pipeline_dropped)"
        $lines += "analysis.pipeline_backlog=$($analysisInput.pipeline_backlog)"
        $lines += "analysis.pipeline_shutdown_timed_out=$($analysisInput.pipeline_shutdown_timed_out)"
        $lines += "analysis.overall=$($analysis.analysis.overall)"
    }
    $lines += "sensor_cpu_pct_min=$(($sensorCpuValues | Measure-Object -Minimum).Minimum)"
    $lines += "sensor_cpu_pct_max=$(($sensorCpuValues | Measure-Object -Maximum).Maximum)"
    $lines += "sensor_cpu_pct_avg=$([Math]::Round((($sensorCpuValues | Measure-Object -Average).Average), 3))"
    $lines += "sensor_working_set_mb_min=$(($sensorWorkingValues | Measure-Object -Minimum).Minimum)"
    $lines += "sensor_working_set_mb_max=$(($sensorWorkingValues | Measure-Object -Maximum).Maximum)"
    $lines += "sensor_working_set_mb_last=$($sensorWorkingValues[-1])"
    $lines += "app_cpu_pct_min=$(($appCpuValues | Measure-Object -Minimum).Minimum)"
    $lines += "app_cpu_pct_max=$(($appCpuValues | Measure-Object -Maximum).Maximum)"
    $lines += "app_cpu_pct_avg=$([Math]::Round((($appCpuValues | Measure-Object -Average).Average), 3))"
    $lines += "app_working_set_mb_min=$(($appWorkingValues | Measure-Object -Minimum).Minimum)"
    $lines += "app_working_set_mb_max=$(($appWorkingValues | Measure-Object -Maximum).Maximum)"
    $lines += "app_working_set_mb_last=$($appWorkingValues[-1])"
    $lines += "db_event_count_first=$($dbCounts[0])"
    $lines += "db_event_count_last=$($dbCounts[-1])"
    $lines += "ingest_lag_max=$(($lagValues | Measure-Object -Maximum).Maximum)"
    $lines += "healthy_samples=$healthySamples/of $($samples.Count)"
    $lines += "loss_samples=$lossSamples"
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "RELIABILITY LONG RUN: PASS"
        exit 0
    }
    Write-Host "RELIABILITY LONG RUN: FAIL"
    Write-Host "Evidence: $runInfoPath"
    exit 1
}
catch {
    $scriptFailed = $true
    $errorText = $_ | Out-String
    Set-Content -LiteralPath (Join-Path $LogDir "error-$stamp.txt") -Value $errorText -Encoding UTF8
    Write-Error $errorText
}
finally {
    if ($null -ne $sensor -and -not $sensor.HasExited) {
        try {
            if (-not $sensor.WaitForExit(15000)) {
                & taskkill.exe /PID $sensor.Id /T /F | Out-Null
            }
        } catch { }
    }
}
if ($scriptFailed) { exit 1 }
