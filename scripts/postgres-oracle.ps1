# Private PostgreSQL 18 cluster used only as the DQD parity oracle backend.
#   scripts/postgres-oracle.ps1          create (if missing) and start the cluster, idempotent
#   scripts/postgres-oracle.ps1 -Stop    stop the cluster, idempotent
# Every pg_ctl call is scoped with -D to the private data directory, so no other PostgreSQL
# instance, service or data directory is ever touched.
param (
    [switch]$Stop
)

$ErrorActionPreference = 'Stop'
# DQD_PG_ROOT: where the private cluster lives (default: a folder in the temporary directory).
# DQD_PG_BIN: the PostgreSQL 18 bin directory (default: the standard Windows install location).
$root = if ($env:DQD_PG_ROOT) { $env:DQD_PG_ROOT } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-pg-oracle' }
$dbDir = "$root/data"
$logFile = "$root/pg.log"
$binDir = if ($env:DQD_PG_BIN) { $env:DQD_PG_BIN } else { 'C:/Program Files/PostgreSQL/18/bin' }
$port = 54329

function Test-Running {
    & "$binDir/pg_ctl.exe" status -D $dbDir *> $null
    return ($LASTEXITCODE -eq 0)
}

if ($Stop) {
    if ((Test-Path "$dbDir/PG_VERSION") -and (Test-Running)) {
        Write-Host 'Stopping private oracle cluster...'
        & "$binDir/pg_ctl.exe" stop -D $dbDir -m fast -w
        if ($LASTEXITCODE -ne 0) { Write-Error "pg_ctl stop failed: exit $LASTEXITCODE"; exit 1 }
    } else {
        Write-Host 'Private oracle cluster is not running.'
    }
    exit 0
}

New-Item -ItemType Directory -Force -Path $root | Out-Null

if (-not (Test-Path "$dbDir/PG_VERSION")) {
    Write-Host 'Initializing private oracle cluster...'
    & "$binDir/initdb.exe" -A trust -U postgres -E UTF8 -D $dbDir
    if ($LASTEXITCODE -ne 0) { Write-Error "initdb failed: exit $LASTEXITCODE"; exit 1 }
}

if (Test-Running) {
    Write-Host "Private oracle cluster is already running (port $port)."
} else {
    Write-Host "Starting private oracle cluster on port $port..."
    & "$binDir/pg_ctl.exe" start -D $dbDir -o "-p $port" -l $logFile -w
    if ($LASTEXITCODE -ne 0) { Write-Error "pg_ctl start failed: exit $LASTEXITCODE"; exit 1 }
}

& "$binDir/psql.exe" -h localhost -p $port -U postgres -d postgres -tAc 'SELECT 1' *> $null
if ($LASTEXITCODE -ne 0) { Write-Error "Cluster on port $port does not accept connections"; exit 1 }
exit 0
