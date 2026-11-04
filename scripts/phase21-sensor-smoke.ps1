param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\release\SentinelLiteSensor.exe"),
    [int]$DurationSeconds = 20,
    [string]$LogDir = (Join-Path $PSScriptRoot "..\target\phase21-smoke")
)

$ErrorActionPreference = "Stop"

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    [Console]::Error.WriteLine("This smoke test requires an elevated Administrator PowerShell. It does not self-elevate.")
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
$sessionName = "SentinelLite-Sensor-Smoke-$PID-$stamp"
$summaryPath = Join-Path $LogDir "summary-$stamp.json"
$stdoutPath = Join-Path $LogDir "stdout-$stamp.json"
$stderrPath = Join-Path $LogDir "stderr-$stamp.log"
$runInfoPath = Join-Path $LogDir "run-$stamp.txt"
$tempDir = Join-Path $env:TEMP "sentinelite-phase21-$stamp"

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
$longNames = @()
$stimulusProcesses = @()
$stimulusErrors = @()
$sensor = $null
$timedOut = $false

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    for ($index = 1; $index -le 3; $index++) {
        $longName = "sentinelite-phase21-longname-$('x' * 48)-$index.exe"
        $destination = Join-Path $tempDir $longName
        Copy-Item -LiteralPath $cmd -Destination $destination
        $longNames += $longName
    }

    # Start the sensor through a tiny cmd wrapper. Start-Process with
    # PowerShell-level redirection does not reliably expose ExitCode after a
    # timed WaitForExit on this host; the wrapper performs the redirection and
    # propagates the sensor's exit code with `exit /b %ERRORLEVEL%`.
    $wrapperPath = Join-Path $tempDir "run-sensor.cmd"
    $wrapperLines = @(
        "@echo off",
        "`"$ExePath`" --session-name `"$sessionName`" --duration $DurationSeconds --output `"$summaryPath`" > `"$stdoutPath`" 2> `"$stderrPath`"",
        "exit /b %ERRORLEVEL%"
    )
    Set-Content -LiteralPath $wrapperPath -Value ($wrapperLines -join "`r`n") -Encoding ASCII
    $sensor = Start-Process -FilePath $wrapperPath -PassThru -WindowStyle Hidden

    Start-Sleep -Seconds 2
    if ($sensor.HasExited) {
        Write-Warning "Sensor wrapper exited before stimuli were launched. See $stderrPath"
    } else {
        for ($index = 0; $index -lt 3; $index++) {
            try {
                $stimulusProcesses += Start-Process -FilePath $cmd `
                    -ArgumentList "/c timeout /t 3 /nobreak >nul" `
                    -WindowStyle Hidden -PassThru
            } catch {
                $stimulusErrors += "cmd[$index]: $($_.Exception.Message)"
            }
        }
        foreach ($longName in $longNames) {
            try {
                $stimulusProcesses += Start-Process -FilePath (Join-Path $tempDir $longName) `
                    -ArgumentList "/c timeout /t 3 /nobreak >nul" `
                    -WindowStyle Hidden -PassThru
            } catch {
                $stimulusErrors += "${longName}: $($_.Exception.Message)"
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
        $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
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
    $exitCodeSource = if ($timedOut) {
        "timeout"
    } elseif ($null -ne $processExitCode) {
        "process"
    } elseif ($null -ne $jsonExitCode) {
        "json"
    } elseif ($null -ne $stderrExitCode) {
        "stderr"
    } else {
        "missing"
    }
    $exitCodeSourcesAgree = (
        ($null -eq $jsonExitCode -or $null -eq $processExitCode -or $jsonExitCode -eq $processExitCode) -and
        ($null -eq $stderrExitCode -or $null -eq $processExitCode -or $stderrExitCode -eq $processExitCode) -and
        ($null -eq $jsonExitCode -or $null -eq $stderrExitCode -or $jsonExitCode -eq $stderrExitCode)
    )

    $residualOutput = @(& logman query -ets 2>$null | Select-String -SimpleMatch $sessionName)
    $residual = $residualOutput.Count -gt 0

    $observedImages = @()
    if ($null -ne $summary -and $null -ne $summary.pairing.observed_start_image_names) {
        $observedImages = @($summary.pairing.observed_start_image_names)
    }
    $longMatches = 0
    $longDetails = @()
    foreach ($longName in $longNames) {
        $matched = @($observedImages | Where-Object {
            $_ -and $_.EndsWith($longName, [StringComparison]::OrdinalIgnoreCase)
        }).Count -gt 0
        if ($matched) { $longMatches++ }
        $longDetails += "$longName=$matched"
    }

    $checks = [ordered]@{
        "exit_code_is_0"               = ($exitCode -eq 0)
        "exit_code_sources_agree"      = $exitCodeSourcesAgree
        "sensor_session_created"       = ($null -ne $summary -and $summary.session_created -eq $true)
        "sensor_session_stopped"       = ($null -ne $summary -and $summary.session_stopped -eq $true)
        "trace_opened"                 = ($null -ne $summary -and $summary.trace_opened -eq $true)
        "trace_closed"                 = ($null -ne $summary -and $summary.trace_closed -eq $true)
        "not_interrupted"              = ($null -ne $summary -and $summary.interrupted -eq $false)
        "window_complete"              = ($null -ne $summary -and $summary.window_complete -eq $true)
        "realtime_mode"                = ($null -ne $summary -and $summary.mode -eq "realtime")
        "etl_output_false"             = ($null -ne $summary -and $summary.etl_output -eq $false)
        "no_residual_session_json"     = ($null -ne $summary -and $summary.residual_session -eq $false)
        "no_residual_session_logman"   = (-not $residual)
        "events_received_at_least_20"  = ($null -ne $summary -and $summary.events_received -ge 20)
        "events_lost_known_zero"       = ($null -ne $summary -and $summary.events_lost -eq 0)
        "realtime_buffers_lost_zero"   = ($null -ne $summary -and $summary.realtime_buffers_lost -eq 0)
        "decode_errors_zero"           = ($null -ne $summary -and $summary.decode_errors -eq 0)
        "health_ok"                    = ($null -ne $summary -and $summary.ok -eq $true)
        "no_health_last_error"         = ($null -ne $summary -and $null -eq $summary.last_error)
        "protection_claim_none"        = ($null -ne $summary -and $summary.protection_claim -eq "none")
        "long_names_3_of_3"            = ($longMatches -eq 3)
        "starts_seen_at_least_6"       = ($null -ne $summary -and $summary.pairing.starts_seen -ge 6)
        "paired_at_least_6"            = ($null -ne $summary -and $summary.pairing.paired -ge 6)
        "no_session_stop_error"        = ($null -ne $summary -and $null -eq $summary.session_stop_error)
        "no_session_close_error"       = ($null -ne $summary -and $null -eq $summary.session_close_error)
        "no_process_trace_error"       = ($null -ne $summary -and $null -eq $summary.process_trace_error)
        "no_overall_error"             = ($null -ne $summary -and $null -eq $summary.error)
        "stimulus_launches_ok"         = ($stimulusErrors.Count -eq 0)
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
        "exit_code=$exitCode",
        "exit_code_source=$exitCodeSource",
        "process_exit_code=$processExitCode",
        "json_exit_code=$jsonExitCode",
        "stderr_exit_code=$stderrExitCode",
        "exit_code_sources_agree=$exitCodeSourcesAgree",
        "timed_out=$timedOut",
        "residual_session=$residual",
        "long_name_matches=$longMatches/3",
        "long_name_details=$($longDetails -join ',')",
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
        $lines += "realtime_buffers_lost=$($summary.realtime_buffers_lost)"
        $lines += "decode_errors=$($summary.decode_errors)"
        $lines += "state=$($summary.state)"
        $lines += "permission_state=$($summary.permission_state)"
        $lines += "interrupted=$($summary.interrupted)"
        $lines += "elapsed_ms=$($summary.elapsed_ms)"
        $lines += "expected_ms=$($summary.expected_ms)"
        $lines += "window_complete=$($summary.window_complete)"
        $lines += "paired=$($summary.pairing.paired)"
        $lines += "starts_seen=$($summary.pairing.starts_seen)"
        $lines += "stops_seen=$($summary.pairing.stops_seen)"
        $lines += "unpaired_starts=$($summary.pairing.unpaired_starts)"
        $lines += "orphan_stops=$($summary.pairing.orphan_stops)"
        $lines += "pid_reuse_detected=$($summary.pairing.pid_reuse_detected)"
        $lines += "sequence_mismatches=$($summary.pairing.sequence_mismatches)"
        $lines += "observed_start_image_names_truncated=$($summary.pairing.observed_start_image_names_truncated)"
    }
    $lines | Set-Content -LiteralPath $runInfoPath -Encoding UTF8
    Get-Content -LiteralPath $runInfoPath

    if ($status -eq "PASS") {
        Write-Host "PHASE 2.1 SENSOR SMOKE: PASS"
        exit 0
    }
    Write-Host "PHASE 2.1 SENSOR SMOKE: FAIL"
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
            # Do not kill a live sensor: let it reach its duration and clean up
            # its ETW session. Only kill the wrapper's process tree as a last
            # resort after the grace period.
            if (-not $sensor.WaitForExit(($DurationSeconds + 30) * 1000)) {
                & taskkill.exe /PID $sensor.Id /T /F | Out-Null
            }
        } catch {
            try { & taskkill.exe /PID $sensor.Id /T /F | Out-Null } catch { }
        }
    }
    if (Test-Path -LiteralPath $tempDir) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
