param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [string]$StorageProbePath = (Join-Path $PSScriptRoot "..\target\debug\examples\storage_probe.exe"),
    [int]$WaitSeconds = 60,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase26-app-ingest")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This Phase 2.6 smoke requires an elevated Administrator PowerShell. It does not self-elevate.")
    exit 2
}

$app = @(Get-Process sentinel-lite -ErrorAction SilentlyContinue)
if ($app.Count -eq 0) {
    [Console]::Error.WriteLine("sentinel-lite.exe must already be running as the non-elevated application consumer.")
    exit 2
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
foreach ($path in @($ExePath, $StorageProbePath)) {
    if (-not (Test-Path -LiteralPath $path)) {
        [Console]::Error.WriteLine("Required binary not found: $path")
        exit 2
    }
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path
$StorageProbePath = (Resolve-Path -LiteralPath $StorageProbePath).Path

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$sessionName = "SentinelLite-Sensor-AppIngest-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$dbProbePath = Join-Path $LogDir "database-$stamp.json"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"
$tempDir = Join-Path $env:TEMP "sentinelite-phase26-ingest-$stamp"
$dbPath = Join-Path $env:LOCALAPPDATA "app.sentinellite.desktop\SentinelLite\events.db"

$commit = "unknown"
$gitStatus = ""
try {
    $commit = (& git -C $repoRoot rev-parse HEAD 2>$null | Select-Object -First 1)
    $gitStatus = ((& git -C $repoRoot status --porcelain 2>$null) -join "`n")
} catch { $commit = "unknown" }
$dirty = -not [string]::IsNullOrWhiteSpace($gitStatus)
$hash = (Get-FileHash -Path $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$cmd = Join-Path $env:SystemRoot "System32\cmd.exe"
$stimulusProcesses = @()
$sensor = $null
$helperExit = $null

$before = $null
if (Test-Path -LiteralPath $dbPath) {
    try {
        & $StorageProbePath --database $dbPath --sessions --limit 100 --output $dbProbePath 2>$null | Out-Null
        $before = Get-Content -LiteralPath $dbProbePath -Raw -Encoding UTF8 | ConvertFrom-Json
    } catch { $before = $null }
}
$beforeSessions = if ($null -eq $before -or $null -eq $before.status) { 0 } else { [int64]$before.status.session_count }

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
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

    if ($active) {
        # Give the non-elevated application ingestion worker time to connect
        # before generating stimulus events.
        Start-Sleep -Seconds 2
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
        [Console]::Error.WriteLine("Sensor did not exit within $WaitSeconds seconds.")
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
    }

    # Let the application ingestion worker consume the final SessionEnd and
    # commit it before reading the database.
    Start-Sleep -Seconds 2

    $processExitCode = $null
    if ($sensor.HasExited) {
        try { $sensor.WaitForExit(); $sensor.Refresh(); $processExitCode = $sensor.ExitCode } catch { }
    }

    $summary = $null
    if (Test-Path -LiteralPath $summaryPath) {
        try { $summary = Get-Content -LiteralPath $summaryPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }

    $db = $null
    if (Test-Path -LiteralPath $dbPath) {
        & $StorageProbePath --database $dbPath --sessions --limit 100 --output $dbProbePath 2>$null | Out-Null
        try { $db = Get-Content -LiteralPath $dbProbePath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }

    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0
    $latestSession = $null
    if ($null -ne $db -and $null -ne $db.sessions -and $db.sessions.Count -gt 0) {
        $latestSession = $db.sessions | Sort-Object last_seen_at -Descending | Select-Object -First 1
    }
    $delivery = if ($null -eq $summary) { $null } else { $summary.delivery }
    $sessionEnd = if ($null -eq $summary -or $null -eq $summary.pipeline) { $null } else { $summary.pipeline.session_end }

    $checks = [ordered]@{
        "app_running_before"             = ($app.Count -gt 0)
        "session_observed"               = $active
        "helper_exit_0"                  = ($helperExit -eq 0)
        "sensor_exited"                  = ($sensor.HasExited -eq $true)
        "exit_code_degraded_3"           = ($processExitCode -eq 3)
        "summary_parsed"                 = ($null -ne $summary)
        "event_ipc_started"              = ($null -ne $summary -and $summary.event_ipc_started -eq $true)
        "delivery_session_end_confirmed" = ($null -ne $delivery -and $delivery.session_end_delivered -eq $true)
        "delivery_batches_served"        = ($null -ne $delivery -and [int64]$delivery.batches_served -gt 0)
        "delivery_no_evictions"          = ($null -ne $delivery -and [int64]$delivery.evictions -eq 0)
        "session_end_interrupted"        = ($null -ne $sessionEnd -and $sessionEnd.reason -eq "interrupted")
        "etw_events_lost_zero"           = ($null -ne $summary -and [int64]$summary.events_lost -eq 0)
        "db_probe_parsed"                = ($null -ne $db -and $null -eq $db.statusError)
        "db_schema_v2"                   = ($null -ne $db -and $null -ne $db.status -and [int64]$db.status.schema_version -eq 2)
        "db_session_added"               = ($null -ne $db -and $null -ne $db.status -and [int64]$db.status.session_count -gt $beforeSessions)
        "db_latest_session_found"        = ($null -ne $latestSession)
        "db_session_closed"              = ($null -ne $latestSession -and $latestSession.closed -eq $true)
        "db_session_end_saved"           = ($null -ne $latestSession -and $latestSession.has_session_end -eq $true)
        "db_session_end_interrupted"     = ($null -ne $latestSession -and $latestSession.end_reason -eq "interrupted")
        "db_session_has_events"          = ($null -ne $latestSession -and [int64]$latestSession.event_count -gt 0)
        "db_no_ingestion_gap"            = ($null -ne $db -and $null -ne $db.status -and [int64]$db.status.gap_count -eq 0)
        "db_no_invalid_records"          = ($null -ne $db -and $null -ne $db.status -and [int64]$db.status.invalid_count -eq 0)
        "no_residual_session"            = (-not $residual)
        "no_overall_error"               = ($null -ne $summary -and $null -eq $summary.error)
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
        "app_processes=$($app.Count)",
        "session_name=$sessionName",
        "database=$dbPath",
        "summary_path=$summaryPath",
        "db_probe_path=$dbProbePath",
        "residual_session=$residual"
    )
    foreach ($entry in $checks.GetEnumerator()) {
        $lines += "check.$($entry.Key)=$($entry.Value)"
    }
    if ($null -ne $summary) {
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_processed=$($summary.events_processed)"
        $lines += "pipeline.normalized=$($summary.pipeline.normalized)"
        $lines += "delivery.batches_served=$($delivery.batches_served)"
        $lines += "session_end.reason=$($sessionEnd.reason)"
    }
    if ($null -ne $latestSession) {
        $lines += "db.latest_session_id=$($latestSession.session_id)"
        $lines += "db.latest_session_events=$($latestSession.event_count)"
        $lines += "db.latest_session_end_reason=$($latestSession.end_reason)"
    }
    if ($null -ne $db -and $null -ne $db.status) {
        $lines += "db.event_count=$($db.status.event_count)"
        $lines += "db.session_count=$($db.status.session_count)"
        $lines += "db.revision_count=$($db.status.revision_count)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.6 APP INGEST SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.6 APP INGEST SMOKE: FAIL"
    Write-Host "Evidence: $runInfoPath"
    exit 1
}
finally {
    foreach ($process in $stimulusProcesses) {
        try { if (-not $process.HasExited) { $process.Kill() } } catch { }
    }
    if ($null -ne $sensor -and -not $sensor.HasExited) {
        try { if (-not $sensor.WaitForExit(15000)) { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } } catch { }
    }
}
