[CmdletBinding()]
param(
    [string]$VideoPath,
    [string]$ApiBaseUrl = 'http://localhost:8080',
    [ValidateRange(1, 20)][int]$Runs = 3,
    [string]$Configurations = '1:1,4:1,4:2,8:1',
    [string]$OutputRoot = 'artifacts/performance',
    [switch]$SkipWorkerRestart,
    [switch]$DefineOnly
)

function Get-Percentile {
    param([double[]]$Values, [ValidateRange(0, 100)][double]$Percentile)
    if (-not $Values -or $Values.Count -eq 0) { return $null }
    $sorted = @($Values | Sort-Object)
    $rank = [math]::Ceiling(($Percentile / 100) * $sorted.Count)
    $index = [math]::Max(0, [math]::Min($sorted.Count - 1, [int]$rank - 1))
    return [double]$sorted[$index]
}

function Get-BenchmarkRows {
    param([object[]]$Configurations, [object[]]$Samples)
    $rows = [System.Collections.Generic.List[object]]::new()
    foreach ($configuration in $Configurations) {
        $values = @($Samples | ForEach-Object { [double]$_.TotalMs })
        $p50 = Get-Percentile -Values $values -Percentile 50
        $p95 = Get-Percentile -Values $values -Percentile 95
        foreach ($sample in $Samples) {
            $rows.Add([pscustomobject]@{
                Batch = [int]$configuration.Batch
                Concurrency = [int]$configuration.Concurrency
                TotalMs = [double]$sample.TotalMs
                P50Ms = $p50
                P95Ms = $p95
            })
        }
    }
    return @($rows)
}

function ConvertTo-BenchmarkConfigurations {
    param([string]$Value)
    return @($Value.Split(',') | ForEach-Object {
        $parts = $_.Trim().Split(':')
        if ($parts.Count -ne 2) { throw "Invalid configuration '$($_)'; expected batch:concurrency" }
        [pscustomobject]@{ Batch = [int]$parts[0]; Concurrency = [int]$parts[1] }
    })
}

function Get-VideoHash {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Wait-VideoJob {
    param([string]$BaseUrl, [string]$JobId)
    do {
        $job = Invoke-RestMethod -Method Get -Uri "$BaseUrl/api/v1/jobs/$JobId"
        if ($job.status -in @('completed', 'failed', 'cancelled')) { return $job }
        Start-Sleep -Seconds 1
    } while ($true)
}

function Get-WorkerPerformanceSummary {
    param([string]$JobId)
    try {
        $lines = @(docker compose logs --no-color --since 2m worker 2>$null)
        foreach ($line in $lines) {
            if ($line -match 'performance_summary\s+(\{.*\})') {
                $summary = $Matches[1] | ConvertFrom-Json
                if ([string]$summary.job_id -eq $JobId) { return $summary }
            }
        }
    } catch {
        return $null
    }
    return $null
}

function Invoke-VideoBenchmarkSample {
    param([string]$BaseUrl, [string]$Path)
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $response = Invoke-WebRequest -Method Post -Uri "$BaseUrl/api/v1/videos/upload" -Form @{ file = Get-Item -LiteralPath $Path }
    $job = $response.Content | ConvertFrom-Json
    if ($job.status -notin @('processing', 'completed')) {
        $processResponse = Invoke-WebRequest -Method Post -Uri "$BaseUrl/api/v1/videos/$($job.id)/process"
        $job = $processResponse.Content | ConvertFrom-Json
    }
    $job = Wait-VideoJob -BaseUrl $BaseUrl -JobId $job.id
    $summary = Get-WorkerPerformanceSummary -JobId ([string]$job.id)
    $watch.Stop()
    [pscustomobject]@{
        JobId = [string]$job.id
        Status = [string]$job.status
        TotalMs = [long]$watch.Elapsed.TotalMilliseconds
        Frames = if ($summary) { $summary.frames } else { $null }
        Batches = if ($summary) { $summary.batches } else { $null }
        Detections = if ($summary) { $summary.detections } else { $null }
        Events = if ($summary) { $summary.events } else { $null }
        StageMs = if ($summary) { $summary.stages_ms } else { $null }
    }
}

function Set-BenchmarkWorkerConfiguration {
    param([object]$Configuration, [switch]$SkipRestart)
    $env:YOLO_BATCH_SIZE = [string]$Configuration.Batch
    $env:YOLO_CONCURRENCY = [string]$Configuration.Concurrency
    if (-not $SkipRestart) {
        docker compose up -d --force-recreate --no-deps worker | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Unable to recreate the worker for the benchmark configuration.' }
    }
}

if ($DefineOnly) { return }
if (-not $VideoPath) { throw 'VideoPath is required.' }
$resolvedVideo = (Resolve-Path -LiteralPath $VideoPath).Path
$configurations = ConvertTo-BenchmarkConfigurations $Configurations
$runDirectory = Join-Path $OutputRoot (Get-Date -Format 'yyyyMMdd-HHmmss')
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
$videoHash = Get-VideoHash $resolvedVideo
$allRows = [System.Collections.Generic.List[object]]::new()

foreach ($configuration in $configurations) {
    Set-BenchmarkWorkerConfiguration -Configuration $configuration -SkipRestart:$SkipWorkerRestart
    $samples = @()
    Invoke-VideoBenchmarkSample -BaseUrl $ApiBaseUrl -Path $resolvedVideo | Out-Null
    1..$Runs | ForEach-Object {
        $sample = Invoke-VideoBenchmarkSample -BaseUrl $ApiBaseUrl -Path $resolvedVideo
        $samples += $sample
        $allRows.Add([pscustomobject]@{
            Timestamp = (Get-Date).ToUniversalTime().ToString('o')
            VideoHash = $videoHash
            VideoPath = $resolvedVideo
            Batch = [int]$configuration.Batch
            Concurrency = [int]$configuration.Concurrency
            Run = [int]$_
            JobId = $sample.JobId
            Status = $sample.Status
            TotalMs = $sample.TotalMs
            Frames = $sample.Frames
            Batches = $sample.Batches
            Detections = $sample.Detections
            Events = $sample.Events
            ExtractMs = if ($sample.StageMs) { $sample.StageMs.frame_extract } else { $null }
            YoloMs = if ($sample.StageMs) { $sample.StageMs.yolo_inference } else { $null }
            RuleMs = if ($sample.StageMs) { $sample.StageMs.rule_evaluation } else { $null }
            EvidenceMs = if ($sample.StageMs) { $sample.StageMs.evidence_save } else { $null }
            EncodeMs = if ($sample.StageMs) { $sample.StageMs.annotated_video_encode } else { $null }
            P50Ms = $null
            P95Ms = $null
        })
    }
    $p50 = Get-Percentile -Values @($samples.TotalMs) -Percentile 50
    $p95 = Get-Percentile -Values @($samples.TotalMs) -Percentile 95
    foreach ($row in $allRows | Where-Object { $_.Batch -eq $configuration.Batch -and $_.Concurrency -eq $configuration.Concurrency }) {
        $row.P50Ms = $p50
        $row.P95Ms = $p95
    }
}

$allRows | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $runDirectory 'results.json') -Encoding utf8
$allRows | Export-Csv -LiteralPath (Join-Path $runDirectory 'results.csv') -NoTypeInformation -Encoding utf8
Write-Output "Benchmark results written to $runDirectory"
