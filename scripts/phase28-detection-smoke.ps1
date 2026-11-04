param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [string]$AnalyzeProbePath = (Join-Path $PSScriptRoot "..\target\debug\examples\analyze_probe.exe"),
    [int]$WaitSeconds = 60,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase28-detection")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This Phase 2.8 smoke requires an elevated Administrator PowerShell. It does not self-elevate.")
    exit 2
}

$app = @(Get-Process sentinel-lite -ErrorAction SilentlyContinue)
if ($app.Count -eq 0) {
    [Console]::Error.WriteLine("sentinel-lite.exe must already be running as the non-elevated application consumer.")
    exit 2
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
foreach ($path in @($ExePath, $AnalyzeProbePath)) {
    if (-not (Test-Path -LiteralPath $path)) {
        [Console]::Error.WriteLine("Required binary not found: $path")
        exit 2
    }
}
$ExePath = (Resolve-Path -LiteralPath $ExePath).Path
$AnalyzeProbePath = (Resolve-Path -LiteralPath $AnalyzeProbePath).Path

New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$sessionName = "SentinelLite-Sensor-Detection-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$analysisPath = Join-Path $LogDir "analysis-$stamp.json"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"
$tempDir = Join-Path $env:TEMP "sentinelite-phase28-detection-$stamp"
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
        Start-Sleep -Seconds 2
        # A burst of short-lived processes. This is normal administrative
        # activity, used to verify that the rule reports a suspicious pattern,
        # not a malware verdict.
        for ($index = 0; $index -lt 40; $index++) {
            try {
                $stimulusProcesses += Start-Process -FilePath $cmd `
                    -ArgumentList "/c exit 0" -WindowStyle Hidden -PassThru
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
        try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
    }
    Start-Sleep -Seconds 2

    $processExitCode = $null
    if ($sensor.HasExited) {
        try { $sensor.WaitForExit(); $sensor.Refresh(); $processExitCode = $sensor.ExitCode } catch { }
    }

    $summary = $null
    if (Test-Path -LiteralPath $summaryPath) {
        try { $summary = Get-Content -LiteralPath $summaryPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    }

    & $AnalyzeProbePath --database $dbPath --output $analysisPath 2>$null | Out-Null
    $analysisFile = $null
    try { $analysisFile = Get-Content -LiteralPath $analysisPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
    $analysis = if ($null -eq $analysisFile) { $null } else { $analysisFile.analysis }
    $rules = if ($null -eq $analysis) { @() } else { @($analysis.results) }
    $burst = $rules | Where-Object { $_.rule_id -in @("PROC-BURST-001", "PROC-EXIT-002") }
    $evidenceIds = @()
    foreach ($rule in $rules) {
        foreach ($item in @($rule.evidence)) {
            $evidenceIds += @($item.event_ids)
        }
    }
    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0
    $delivery = if ($null -eq $summary) { $null } else { $summary.delivery }

    $checks = [ordered]@{
        "app_running"                  = ($app.Count -gt 0)
        "session_observed"             = $active
        "helper_exit_0"                = ($helperExit -eq 0)
        "exit_code_degraded_3"         = ($processExitCode -eq 3)
        "summary_parsed"               = ($null -ne $summary)
        "delivery_session_end"         = ($null -ne $delivery -and $delivery.session_end_delivered -eq $true)
        "analysis_parsed"              = ($null -ne $analysis)
        "analysis_session_closed"      = ($null -ne $analysis -and $analysis.input.session_closed -eq $true)
        "analysis_events_positive"     = ($null -ne $analysis -and [int64]$analysis.input.total_events -gt 30)
        "burst_rule_triggered"         = (@($burst).Count -gt 0)
        "burst_status_suspicious"      = (@($burst | Where-Object { $_.status -eq "suspicious" }).Count -gt 0)
        "burst_has_evidence"           = ($evidenceIds.Count -gt 0)
        "burst_has_false_positive_note" = (@($burst | Where-Object { @($_.false_positive_notes).Count -gt 0 }).Count -gt 0)
        "burst_does_not_claim_malware" = (@($burst | Where-Object { $_.explanation -notmatch "恶意软件" }).Count -gt 0)
        "rule_version_present"         = (@($rules | Where-Object { $_.rule_version -eq "1" }).Count -gt 0)
        "no_residual_session"          = (-not $residual)
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
        "analysis_path=$analysisPath",
        "residual_session=$residual"
    )
    foreach ($entry in $checks.GetEnumerator()) { $lines += "check.$($entry.Key)=$($entry.Value)" }
    if ($null -ne $analysis) {
        $lines += "analysis.session_id=$($analysis.session_id)"
        $lines += "analysis.overall=$($analysis.overall)"
        $lines += "analysis.event_count=$($analysis.input.total_events)"
        $lines += "analysis.etw_known=$($analysis.input.sensor_health_known)"
        $lines += "analysis.rule_ids=$((@($rules | ForEach-Object { $_.rule_id })) -join ',')"
        $lines += "analysis.evidence_ids=$($evidenceIds.Count)"
    }
    if ($null -ne $summary) {
        $lines += "events_received=$($summary.events_received)"
        $lines += "events_processed=$($summary.events_processed)"
        $lines += "delivery.batches_served=$($delivery.batches_served)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.8 DETECTION SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.8 DETECTION SMOKE: FAIL"
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
