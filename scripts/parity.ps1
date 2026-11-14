param(
  # An LF checkout (core.autocrlf=false) of the pin: the bytes of upstream's git blobs, which a
  # CRAN or GitHub install ships. A Windows checkout with autocrlf turns every template into CRLF.
  # Defaults come from the environment variables in CONTRIBUTING.md (DQD_UPSTREAM, ...).
  [string]$Upstream = $(if ($env:DQD_UPSTREAM) { $env:DQD_UPSTREAM } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-upstream-lf' }),
  [string]$OracleOut = (Join-Path (Split-Path $PSScriptRoot -Parent) 'tests/oracle/generated'),
  [string]$Rscript = $(if ($env:DQD_RSCRIPT) { $env:DQD_RSCRIPT } else { 'Rscript' }),
  [string]$RLibrary = $(if ($env:DQD_R_LIBRARY) { $env:DQD_R_LIBRARY } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-r-oracle-library' }),
  # Only R needs Java (rJava/DatabaseConnector), and only for -Generate/-Refresh. The Rust side needs none.
  # Unset: R uses whatever JAVA_HOME the shell already has.
  [string]$JavaHome = $env:DQD_JAVA_HOME,
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
  if ($JavaHome) { $env:JAVA_HOME = $JavaHome; $env:PATH = "$JavaHome/bin;$env:PATH" }
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
