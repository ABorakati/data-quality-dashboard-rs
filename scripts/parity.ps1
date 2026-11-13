param(
  [string]$Upstream = 'C:/Users/abora/tmp/dqd-upstream',
  [string]$OracleOut = 'C:/Users/abora/tmp/data-quality-dashboard-rs/tests/oracle/generated',
  [string]$Rscript = 'C:/Program Files/R/R-4.5.2/bin/Rscript.exe',
  [string]$RLibrary = 'C:/Users/abora/tmp/dqd-r-oracle-library',
  # Only R needs Java (rJava/DatabaseConnector), and only for -Generate/-Refresh. The Rust side needs none.
  [string]$JavaHome = 'C:/Users/abora/tmp/dqd-java/jdk-21.0.12.1+1',
  # Start the private PostgreSQL oracle cluster (port 54329) and run the PostgreSQL parity tests against it.
  [switch]$Postgres,
  [Alias('Generate')][switch]$GenerateOnly,
  [switch]$Refresh
)
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..')
$pin = 'b9d2a9b4a4736f7ea81415d974c72119b6f0bd11'
$env:DQD_ORACLE_DIR = $OracleOut
if ($GenerateOnly -and $Refresh) { throw 'Use only one of -Generate or -Refresh.' }
if ($GenerateOnly -or $Refresh) {
  $actual = (& git -C $Upstream rev-parse HEAD).Trim()
  if ($actual -ne $pin) { throw "Expected upstream $pin, found $actual" }
  if (-not (Test-Path (Join-Path $RLibrary 'DataQualityDashboard'))) {
    throw "Install pinned DQD into $RLibrary before generation: R CMD INSTALL --library=$RLibrary $Upstream"
  }
  $env:JAVA_HOME = $JavaHome
  $env:PATH = "$JavaHome/bin;$env:PATH"
  $env:DQD_R_LIBRARY = $RLibrary
  New-Item -ItemType Directory -Force -Path $OracleOut | Out-Null
  & $Rscript --vanilla (Join-Path $repo 'tests/oracle/generate.R') $Upstream $OracleOut
  if ($LASTEXITCODE -ne 0) { throw "Upstream oracle generation failed: Rscript exit $LASTEXITCODE" }
} elseif (-not (Test-Path (Join-Path $OracleOut 'manifest.json'))) {
  throw "Oracle fixtures are missing from $OracleOut. Run with -Generate first."
}
if (-not $GenerateOnly) {
  if ($Postgres) {
    & (Join-Path $PSScriptRoot 'postgres-oracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw "PostgreSQL oracle cluster failed to start: exit $LASTEXITCODE" }
    $env:DQD_PG_PORT = '54329'
  } else {
    Remove-Item Env:DQD_PG_PORT -ErrorAction SilentlyContinue
  }
  Push-Location $repo
  try { cargo test --test parity -- --nocapture --test-threads=1; if ($LASTEXITCODE -ne 0) { throw "Rust differential parity failed: cargo exit $LASTEXITCODE" } }
  finally { Pop-Location }
}
