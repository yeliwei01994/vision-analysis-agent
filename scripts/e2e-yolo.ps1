$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$sample = Join-Path $root 'backend\media\clip.mp4'
$base = 'http://localhost:8080'

function Wait-Healthy($url) {
  for ($attempt = 0; $attempt -lt 60; $attempt++) {
    try {
      $response = Invoke-RestMethod -Uri $url -TimeoutSec 3
      if ($response.status -eq 'ok') { return }
    } catch { }
    Start-Sleep -Seconds 2
  }
  throw "service did not become healthy: $url"
}

docker compose -f (Join-Path $root 'docker-compose.yml') up -d --build api worker yolo mysql redis
Wait-Healthy "$base/health"

$upload = Invoke-RestMethod -Method Post -Uri "$base/api/v1/videos/upload" -Form @{ file = Get-Item $sample }
if (-not $upload.id) { throw 'upload did not return a job id' }

$finished = $null
for ($attempt = 0; $attempt -lt 120; $attempt++) {
  $finished = Invoke-RestMethod -Uri "$base/api/v1/jobs/$($upload.id)"
  if ($finished.status -in @('completed', 'failed', 'cancelled')) { break }
  Start-Sleep -Seconds 2
}
if ($finished.status -ne 'completed') { throw "job did not complete successfully: $($finished | ConvertTo-Json -Compress)" }

$events = Invoke-RestMethod -Uri "$base/api/v1/events"
$event = @($events | Where-Object { $_.job_id -eq $upload.id }) | Select-Object -First 1
if (-not $event) { throw 'completed upload did not create an event' }
if (-not $event.objects -or $event.objects.Count -eq 0) { throw 'event has no real detections' }
if ($event.detector_version -like 'mock*' -or $event.detector_version -eq 'yolo-pending') { throw "event has invalid detector version: $($event.detector_version)" }

Write-Output "E2E passed: job=$($upload.id), detector=$($event.detector_version), objects=$($event.objects.Count)"
