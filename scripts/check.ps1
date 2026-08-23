$ErrorActionPreference = 'Stop'
$required = @(
  'backend/Cargo.toml',
  'frontend/package.json',
  'db/migrations/001_initial.sql',
  'deploy/nginx.conf',
  'docker-compose.yml',
  '.env.example'
)
$forbidden = @('apps', 'platform', 'runtime', 'quality', 'migrations')
foreach ($path in $required) {
  if (-not (Test-Path -LiteralPath $path)) { throw "Missing required path: $path" }
}
foreach ($path in $forbidden) {
  if (Test-Path -LiteralPath $path) { throw "Unexpected legacy layout path exists: $path" }
}
Write-Output 'layout ok'
