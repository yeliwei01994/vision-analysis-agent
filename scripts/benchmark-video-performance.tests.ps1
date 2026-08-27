$scriptPath = Join-Path $PSScriptRoot 'benchmark-video-performance.ps1'

Describe 'video performance benchmark helpers' {
    BeforeAll {
        . $scriptPath -DefineOnly
    }

    It 'expands benchmark matrix and calculates percentile fields' {
        $rows = Get-BenchmarkRows -Configurations @(
            @{ Batch = 1; Concurrency = 1 },
            @{ Batch = 4; Concurrency = 2 },
            @{ Batch = 8; Concurrency = 1 }
        ) -Samples @(
            @{ TotalMs = 100 }, @{ TotalMs = 200 }
        )
        $rows.Count | Should Be 6
        ($rows | Select-Object -First 1).P50Ms | Should Not BeNullOrEmpty
        ($rows | Select-Object -First 1).P95Ms | Should Not BeNullOrEmpty
    }
}
