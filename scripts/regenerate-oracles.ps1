# Regenerates every R oracle fixture under tests/oracle/generated, one generator after another
# (parallel R runs on these datasets have exhausted the disk before). About 25 minutes.
#
#   pwsh -NoProfile -File scripts/regenerate-oracles.ps1
#   pwsh -NoProfile -File scripts/regenerate-oracles.ps1 -Only upstream,eunomia
#
# Steps: core (generate.R: catalog, enumeration, render, evaluation, exports, database cases),
# gibleed (full GiBleed SQLite run), upstream (testthat scenarios), eunomia (MIMIC, Synthea27Nj),
# postgres (needs the private cluster from scripts/postgres-oracle.ps1, started here).
param(
  [string[]]$Only = @('core', 'gibleed', 'upstream', 'eunomia', 'postgres'),
  # Defaults come from the environment variables listed in CONTRIBUTING.md.
  [string]$Upstream = $(if ($env:DQD_UPSTREAM) { $env:DQD_UPSTREAM } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-upstream-lf' }),
  [string]$Rscript = $(if ($env:DQD_RSCRIPT) { $env:DQD_RSCRIPT } else { 'Rscript' }),
  [string]$RLibrary = $(if ($env:DQD_R_LIBRARY) { $env:DQD_R_LIBRARY } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-r-oracle-library' }),
  # Unset: R uses whatever JAVA_HOME the shell already has.
  [string]$JavaHome = $env:DQD_JAVA_HOME,
  [string]$EunomiaDir = $(if ($env:DQD_EUNOMIA_DIR) { $env:DQD_EUNOMIA_DIR } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-bench/eunomia' })
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$out = Join-Path $repo 'tests/oracle/generated'
$oracle = Join-Path $repo 'tests/oracle'
if ($JavaHome) { $env:JAVA_HOME = $JavaHome; $env:PATH = "$JavaHome/bin;$env:PATH" }
$env:DQD_R_LIBRARY = $RLibrary
function Invoke-R([string]$Label, [string[]]$Arguments) {
  Write-Host "== $Label"
  & $Rscript --vanilla @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Label failed: Rscript exit $LASTEXITCODE" }
}
Push-Location $repo
try {
  if ($Only -contains 'core') {
    Invoke-R 'core (generate.R)' @("$oracle/generate.R", $Upstream, $out)
  }
  if ($Only -contains 'gibleed') {
    # Repo-relative, because gibleed-cases.R records the output folder in options.json.
    $source = Join-Path $EunomiaDir 'GiBleed_5.3.sqlite'
    Invoke-R 'gibleed' @("$oracle/gibleed-cases.R", $Upstream, $source, 'tests/oracle/generated/gibleed/sqlite', '1')
  }
  if ($Only -contains 'upstream') {
    Invoke-R 'upstream testthat scenarios' @("$oracle/upstream-testthat-cases.R", $Upstream, "$out/upstream", "$out/gibleed/sqlite/gibleed-copy.sqlite")
  }
  if ($Only -contains 'eunomia') {
    Invoke-R 'eunomia synthea27nj' @("$oracle/eunomia-cases.R", $Upstream, "$EunomiaDir/Synthea27Nj_5.4.sqlite", "$out/eunomia/synthea27nj", '1', 'Synthea27Nj', '5.4')
    Invoke-R 'eunomia mimic' @("$oracle/eunomia-cases.R", $Upstream, "$EunomiaDir/MIMIC_5.3.sqlite", "$out/eunomia/mimic", '1', 'MIMIC', '5.3')
  }
  if ($Only -contains 'postgres') {
    & (Join-Path $PSScriptRoot 'postgres-oracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw "PostgreSQL oracle cluster failed to start: exit $LASTEXITCODE" }
    # postgres-cases.R writes below OUTPUT/postgres itself.
    Invoke-R 'postgres' @("$oracle/postgres-cases.R", $Upstream, $out)
  }
} finally { Pop-Location }
