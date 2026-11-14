# Differential data fuzzing: R DataQualityDashboard vs the Rust port on seeded, mutated OMOP CDM
# SQLite databases. Every mismatching case is copied to fuzz/reproducers/<case> with its database,
# options, mutations, both results and diff.txt. Exit code 1 means at least one mismatch.
#
#   pwsh -NoProfile -File scripts/fuzz-differential.ps1 -Seed 1 -Count 10
#   pwsh -NoProfile -File scripts/fuzz-differential.ps1 -Seed 7 -Count 40 -Checks cdmField,isRequired,measureValueCompleteness
#
# An R run takes about 2 minutes per case for all checks on GiBleed; -Checks narrows it.
# Re-running with the same -Seed resumes: finished R cases are skipped.
param(
  [Parameter(Mandatory)][uint64]$Seed,
  [int]$Count = 10,
  [string[]]$Checks = @(),
  [string]$Base = '',
  [string]$CdmVersion = '5.3',
  [int]$MaxMutations = 3,
  # Defaults come from the environment variables listed in CONTRIBUTING.md.
  [string]$Rscript = $(if ($env:DQD_RSCRIPT) { $env:DQD_RSCRIPT } else { 'Rscript' }),
  [string]$RLibrary = $(if ($env:DQD_R_LIBRARY) { $env:DQD_R_LIBRARY } else { Join-Path ([IO.Path]::GetTempPath()) 'dqd-r-oracle-library' }),
  [string]$JavaHome = $env:DQD_JAVA_HOME
)
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..')
if (-not $Base) { $Base = Join-Path $repo 'tests/oracle/generated/gibleed/sqlite/gibleed-copy.sqlite' }
$work = Join-Path $repo "fuzz/work/seed-$Seed"
$reproducers = Join-Path $repo 'fuzz/reproducers'
$example = @('run', '--quiet', '--release', '--example', 'fuzz_differential', '--')
Push-Location $repo
try {
  if (-not (Test-Path (Join-Path $work 'case-*'))) {
    $generate = @('generate', '--base', $Base, '--out', $work, '--seed', $Seed, '--count', $Count, '--cdm-version', $CdmVersion, '--max-mutations', $MaxMutations)
    if ($Checks.Count) { $generate += @('--checks', ($Checks -join ',')) }
    cargo @example @generate
    if ($LASTEXITCODE -ne 0) { throw "generate failed: $LASTEXITCODE" }
  }
  if ($JavaHome) { $env:JAVA_HOME = $JavaHome; $env:PATH = "$JavaHome/bin;$env:PATH" }
  $env:DQD_R_LIBRARY = $RLibrary
  & $Rscript --vanilla (Join-Path $repo 'fuzz/run_r_batch.R') $work
  if ($LASTEXITCODE -ne 0) { throw "R batch failed: $LASTEXITCODE" }
  cargo @example rust $work
  if ($LASTEXITCODE -ne 0) { throw "Rust batch failed: $LASTEXITCODE" }
  cargo @example compare $work --reproducers $reproducers
  exit $LASTEXITCODE
} finally { Pop-Location }
