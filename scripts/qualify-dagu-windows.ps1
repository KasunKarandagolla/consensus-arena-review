$ErrorActionPreference = 'Stop'

$version = '2.16.6'
$asset = "dagu_${version}_windows_amd64.tar.gz"
$release = "https://github.com/dagucloud/dagu/releases/download/v$version"
$work = Join-Path $env:RUNNER_TEMP 'dagu windows qualification'
$archive = Join-Path $work $asset
$checksumFile = Join-Path $work 'checksums.txt'
$extract = Join-Path $work 'bin'
$daguHome = Join-Path $work 'home'
$dags = Join-Path $work 'fixture with spaces'
$beforePath = Join-Path $work 'before.txt'
$afterPath = Join-Path $work 'after.txt'
$beforeLiteral = "'" + $beforePath.Replace("'", "''") + "'"
$afterLiteral = "'" + $afterPath.Replace("'", "''") + "'"
$serverOut = Join-Path $work 'server-first.stdout.log'
$serverErr = Join-Path $work 'server-first.stderr.log'
$restartOut = Join-Path $work 'server-restart.stdout.log'
$restartErr = Join-Path $work 'server-restart.stderr.log'
$baseUrl = 'http://127.0.0.1:18216'
$pinnedSha256 = '65193670d974ece9e14b2fd9c61a06dd3073f0b03a553445623ca03f162be7b8'
$dagName = 'windows-dagu-smoke'
$runId = 'windows-human-restart-1'
$server = $null

function Get-RunStatusText {
    param(
        [string]$DaguPath,
        [string]$DaguHome,
        [string]$RunId,
        [string]$DagName
    )
    $text = (& $DaguPath status --dagu-home $DaguHome --run-id $RunId $DagName 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "Dagu status failed for run '$RunId'."
    }
    return $text
}

function Wait-ForDaguServer {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$StdoutPath,
        [string]$Phase
    )
    for ($i = 0; $i -lt 120; $i++) {
        if ($Process.HasExited) {
            throw "Dagu server exited during $Phase with code $($Process.ExitCode)."
        }
        try {
            # The HTTP listener is the durable readiness contract. Log wording
            # has changed across Dagu releases, so do not gate qualification on
            # an internal message such as "Scheduler started".
            $response = Invoke-WebRequest -Uri $baseUrl -TimeoutSec 2
            if ($response.StatusCode -ge 200 -and $response.StatusCode -lt 500) {
                return
            }
        } catch {
            Start-Sleep -Seconds 1
            continue
        }
        Start-Sleep -Seconds 1
    }
    throw "Dagu server did not become scheduler-ready during $Phase within 120 seconds."
}

New-Item -ItemType Directory -Force -Path $work, $extract, $daguHome, $dags | Out-Null

try {
    Invoke-WebRequest -Uri "$release/checksums.txt" -OutFile $checksumFile -MaximumRedirection 5
    Invoke-WebRequest -Uri "$release/$asset" -OutFile $archive -MaximumRedirection 5

    $checksumMatch = Select-String -Path $checksumFile -Pattern "^([0-9a-fA-F]{64})\s+$([regex]::Escape($asset))$"
    if (-not $checksumMatch) {
        throw "Published checksum for $asset was not found."
    }
    $expectedHash = $checksumMatch.Matches[0].Groups[1].Value.ToLowerInvariant()
    $actualHash = (Get-FileHash -Path $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "Dagu archive checksum mismatch: expected $expectedHash, got $actualHash."
    }
    if ($actualHash -ne $pinnedSha256) {
        throw "Dagu archive does not match the qualification pin: expected $pinnedSha256, got $actualHash."
    }

    & tar.exe -xzf $archive -C $extract
    if ($LASTEXITCODE -ne 0) {
        throw "tar.exe failed to extract the verified Dagu archive (exit $LASTEXITCODE)."
    }
    $dagu = Join-Path $extract 'dagu.exe'
    if (-not (Test-Path -LiteralPath $dagu)) {
        throw 'The verified archive did not contain dagu.exe.'
    }

    $reportedVersion = (& $dagu version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $reportedVersion -ne $version) {
        throw "Expected Dagu $version, got '$reportedVersion'."
    }

    $env:DAGU_AUTH_MODE = 'none'
    $workflow = Join-Path $dags "$dagName.yaml"
    $workflowText = @'
steps:
  - id: record_before_wait
    run: >-
      powershell.exe -NoProfile -Command
      "Set-Content -LiteralPath __BEFORE_PATH__ -Value 'ran'"

  - id: approve
    depends: [record_before_wait]
    action: human.task
    with:
      prompt: Approve the standalone Windows runtime check
      form:
        type: object
        properties:
          answer:
            type: string
            enum: [approved]
        required: [answer]

  - id: record_after_wait
    depends: [approve]
    run: >-
      powershell.exe -NoProfile -Command
      "Set-Content -LiteralPath __AFTER_PATH__ -Value 'resumed'"
'@
    $workflowText = $workflowText.Replace('__BEFORE_PATH__', $beforeLiteral).Replace('__AFTER_PATH__', $afterLiteral)
    $workflowText | Set-Content -LiteralPath $workflow -Encoding utf8NoBOM

    & $dagu validate --dagu-home $daguHome $workflow
    if ($LASTEXITCODE -ne 0) {
        throw 'Dagu rejected the Windows qualification workflow.'
    }

    $serverArguments = "start-all --dagu-home `"$daguHome`" --dags `"$dags`" --host 127.0.0.1 --port 18216 --coordinator.port 50166"
    $server = Start-Process -FilePath $dagu -ArgumentList $serverArguments -PassThru -WindowStyle Hidden -RedirectStandardOutput $serverOut -RedirectStandardError $serverErr
    Wait-ForDaguServer -Process $server -StdoutPath $serverOut -Phase 'startup'

    & $dagu start --dagu-home $daguHome --run-id $runId --quiet $workflow
    if ($LASTEXITCODE -ne 0) {
        throw 'Dagu CLI could not start the Windows qualification workflow.'
    }

    $statusText = Get-RunStatusText -DaguPath $dagu -DaguHome $daguHome -RunId $runId -DagName $dagName
    for ($i = 0; $i -lt 60; $i++) {
        if ($statusText -match '(?im)^Waiting\b') { break }
        if ($statusText -match '(?im)^(Failed|Aborted|Succeeded)\b') {
            throw 'Dagu run ended before the human task opened.'
        }
        Start-Sleep -Seconds 1
        $statusText = Get-RunStatusText -DaguPath $dagu -DaguHome $daguHome -RunId $runId -DagName $dagName
    }
    if ($statusText -notmatch '(?im)^Waiting\b') {
        throw 'Dagu run did not enter Waiting.'
    }
    if (-not (Test-Path -LiteralPath $beforePath)) {
        throw 'The basic workflow step did not complete before its human task.'
    }

    Stop-Process -Id $server.Id -Force
    Wait-Process -Id $server.Id -Timeout 20 -ErrorAction SilentlyContinue
    $server = Start-Process -FilePath $dagu -ArgumentList $serverArguments -PassThru -WindowStyle Hidden -RedirectStandardOutput $restartOut -RedirectStandardError $restartErr
    Wait-ForDaguServer -Process $server -StdoutPath $restartOut -Phase 'restart'

    $statusText = Get-RunStatusText -DaguPath $dagu -DaguHome $daguHome -RunId $runId -DagName $dagName
    if ($statusText -notmatch '(?im)^Waiting\b') {
        throw 'The pending human task did not survive controller restart.'
    }

    $completeOutput = (& $dagu human-task complete --dagu-home $daguHome --run-id $runId --step approve --input answer=approved $dagName 2>&1 | Out-String)
    $completeExitCode = $LASTEXITCODE
    if ($completeExitCode -ne 0 -and $completeOutput -notmatch 'dag-run is not queued: waiting') {
        throw 'Dagu CLI could not complete the persisted human task.'
    }

    $statusText = ''
    $resumeMode = 'automatic'
    for ($i = 0; $i -lt 60; $i++) {
        $statusText = Get-RunStatusText -DaguPath $dagu -DaguHome $daguHome -RunId $runId -DagName $dagName
        if ($statusText -match '(?im)^Succeeded\b') { break }
        if ($statusText -match '(?im)^(Failed|Aborted)\b') { break }
        Start-Sleep -Seconds 1
    }
    if ($statusText -match '(?im)^Failed\b') {
        # A controller restart may leave the run in a failed terminal state
        # even though the human answer was durably recorded. Exercise the
        # documented retry path, and require the continuation to succeed.
        $resumeMode = 'explicit_retry_after_restart_failure'
        & $dagu retry --dagu-home $daguHome --run-id $runId $workflow
        if ($LASTEXITCODE -ne 0) {
            throw 'Dagu CLI could not explicitly retry the persisted human-task run.'
        }
        for ($i = 0; $i -lt 60; $i++) {
            $statusText = Get-RunStatusText -DaguPath $dagu -DaguHome $daguHome -RunId $runId -DagName $dagName
            if ($statusText -match '(?im)^Succeeded\b') { break }
            if ($statusText -match '(?im)^(Failed|Aborted)\b') {
                throw 'The explicit retry of the human-task run failed.'
            }
            Start-Sleep -Seconds 1
        }
    }
    if ($statusText -notmatch '(?im)^Succeeded\b') {
        throw 'The human-task run did not succeed after the available resume path.'
    }
    if ((Get-Content -LiteralPath $afterPath -Raw).Trim() -ne 'resumed') {
        throw 'The post-human-task continuation did not produce its expected output.'
    }

    $historyText = (& $dagu history --dagu-home $daguHome --run-id $runId --format json $dagName | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw 'Dagu history command failed.'
    }
    $history = $historyText | ConvertFrom-Json
    $historyRun = @($history | Where-Object { $_.dagRunId -eq $runId }) | Select-Object -First 1
    if (-not $historyRun -or $historyRun.status -ne 'succeeded') {
        throw "Dagu CLI history did not retain successful run '$runId'."
    }

    Write-Output "Dagu Windows standalone qualification PASS: version=$reportedVersion sha256=$actualHash"
    Write-Output "Run=$runId status=succeeded restart_resume=$resumeMode history=$($historyRun.status) fixture=$dags"
} finally {
    if ($server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
        Wait-Process -Id $server.Id -Timeout 20 -ErrorAction SilentlyContinue
    }
}
