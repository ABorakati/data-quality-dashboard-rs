# Contributing

Thank you for looking at this port. It aims to produce check results identical to OHDSI DataQualityDashboard 2.9.0, so most
of the rules below exist to protect that. Read [README.md](README.md) first, in particular "Status and compatibility".

By contributing you agree that your contribution is licensed under the Apache License 2.0, the licence of this
project (see [LICENSE](LICENSE)). Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Build

You need a Rust toolchain that supports the 2024 edition (see the "Requirements" section of the README).

```sh
cargo build                          # default features: DuckDB and PostgreSQL TLS
cargo build --no-default-features    # SQLite and PostgreSQL only; no DuckDB library
```

The build downloads DuckDB's prebuilt 2.0.0 shared library from GitHub, so it needs network access the first time.
`.cargo/config.toml` sets `DUCKDB_DOWNLOAD_LIB=1` for this. Do not switch to a bundled C++ build: it needs several
gigabytes of disk.

## Test

```sh
cargo test --workspace --release
```

The Rust tests need no R and no Java. They compare the port with the committed parity fixtures in
`tests/oracle/generated`. Tests that need a PostgreSQL server skip themselves unless `DQD_PG_PORT` is set.

To run the PostgreSQL scenarios, start a PostgreSQL server with trust authentication, user `postgres` and database
`postgres`, then set the port and run the tests one at a time:

```sh
DQD_PG_PORT=5432 cargo test --release --test parity -- --test-threads=1 postgresql
```

On Windows, `pwsh -NoProfile -File scripts/parity.ps1 -Postgres` starts a private PostgreSQL 18 cluster on port 54329
(`scripts/postgres-oracle.ps1`) and runs them. Continuous integration runs the scenarios except the full GiBleed
run, which needs data that the repository does not contain (see the comment in `.github/workflows/ci.yml`).

Before you open a pull request, also run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Parity rules

1. **Output changes need evidence.** A change to check SQL, check results, threshold evaluation, result files or
   SQL-only output must match the pinned R package. If it cannot match, it must be a documented intentional
   difference in `README.md` and `CHANGELOG.md`. The pull request template asks you to say which applies.
2. **Never edit `tests/oracle/generated` by hand.** These fixtures are produced by the R oracle, and a hand edit
   makes the test pass for the wrong reason. Regenerate them (below) and review the diff.
3. **Add a test.** A new behaviour needs a Rust test. Where R can show the expected result, extend the oracle
   generator in `tests/oracle/` so the fixture records it.
4. **Keep upstream bytes.** Files listed in `resources/provenance.json` keep the exact bytes of the upstream git
   blobs: the CSV catalogues and SQL templates, and the viewer assets. Do not reformat or re-encode them. Port
   additions (for example `resources/viewer/css/bridge.css`) are not upstream files and follow the project licence.

## Regenerating the parity fixtures

[`docs/test-data.md`](docs/test-data.md) lists what each fixture is built from, where the source data comes from and
what its licence allows. The steps below are the regeneration procedure.

Regeneration needs a Windows or Linux machine with:

- R 4.5.2, and a library that holds the pinned DataQualityDashboard (2.9.0). Install it from an LF checkout of the
  pinned commit `b9d2a9b4a4736f7ea81415d974c72119b6f0bd11`, so the templates keep upstream's LF line endings:

  ```sh
  git -c core.autocrlf=false clone https://github.com/OHDSI/DataQualityDashboard "$DQD_UPSTREAM"
  git -C "$DQD_UPSTREAM" checkout b9d2a9b4a4736f7ea81415d974c72119b6f0bd11
  R CMD INSTALL --library="$DQD_R_LIBRARY" "$DQD_UPSTREAM"
  ```

- Java, for the R packages `DatabaseConnector` and `rJava`. Only R needs Java; the Rust port does not.
- PowerShell 7 (`pwsh`), to run the scripts in `scripts/`.
- For the Eunomia datasets, the GiBleed, MIMIC and Synthea27Nj SQLite files in a directory named by `DQD_EUNOMIA_DIR`.

The scripts read these variables. Each has a default in the script's parameters if you leave it unset:

| Variable | Used for | Default |
|---|---|---|
| `DQD_UPSTREAM` | LF checkout of the pinned upstream commit | `<temp>/dqd-upstream-lf` |
| `DQD_R_LIBRARY` | R library with the pinned package | `<temp>/dqd-r-oracle-library` |
| `DQD_RSCRIPT` | Path to `Rscript` | `Rscript` on `PATH` |
| `DQD_JAVA_HOME` | JDK for R | unset: uses the existing `JAVA_HOME` |
| `DQD_EUNOMIA_DIR` | Eunomia SQLite files | `<temp>/dqd-bench/eunomia` |
| `DQD_PG_ROOT`, `DQD_PG_BIN` | Private PostgreSQL cluster and its `bin` directory | `<temp>/dqd-pg-oracle`, a PostgreSQL 18 install |
| `DQD_JDBC_DIR` | Folder for the PostgreSQL JDBC driver, read by `scripts/download-jdbc.R` (required there) | no default |
| `DQD_GIBLEED_SQLITE` | GiBleed CDM 5.3 SQLite file, for the one-off helpers `scripts/check-duplicates.R` and `scripts/debug-gibleed.R` (required there) | no default |

`<temp>` is the operating system's temporary directory. Then run:

```powershell
pwsh -NoProfile -File scripts/regenerate-oracles.ps1                 # every oracle, about 25 minutes
pwsh -NoProfile -File scripts/regenerate-oracles.ps1 -Only upstream  # one part
pwsh -NoProfile -File scripts/parity.ps1 -Refresh                    # regenerate, then run the parity tests
```

Known limitations in `tests/oracle/`: the R generators still fall back to a maintainer's Windows path when
`DQD_R_LIBRARY` is unset (the scripts always set it). `postgres-cases.R` also hard-codes the JDBC driver directory and
the GiBleed source database. Edit those two lines to match your machine before running the `postgres` step. The
fixture diff must be reviewed like any other change.

## Line endings

`.gitattributes` contains `* -text`. Git must store and check out the exact bytes, because the parity fixtures
compare bytes. On Windows, set Git to leave line endings alone before cloning:

```powershell
git config --global core.autocrlf false
```

Do not run a formatter or editor that converts line endings on files under `tests/`, `resources/` or `docs/`. The
port does write CRLF where R does on Windows, in result CSV and SQL-only files, because that is R's behaviour. That
is intended.

## Releases and the Zenodo DOI

A DOI comes from [Zenodo](https://zenodo.org), which archives a GitHub release and mints a DOI for it. It can only be
requested by the repository owner: it needs your Zenodo login and a public repository. Zenodo records are meant to be
permanent, so settle the points under "Before the first release" first.

**Before the first release**

1. The version is `0.1.0`, deliberately not upstream's `2.9.0`, so this port is not mistaken for the official release. The
   README says it tracks DQD 2.9.0, and results files still carry `dqdVersion` 2.9.0 (the upstream version they match, a
   constant in `src/runner.rs`). The three crates share the number; change them together, with the `version` on the
   path dependencies.
2. Your ORCID and affiliation are already in `CITATION.cff`.
3. Make the repository public. Zenodo cannot archive a private repository. Before that, read the "Open items" in
   `THIRD_PARTY_NOTICES.md` and decide about the large fixtures in `tests/oracle`: the archive is a copy of the tagged tree,
   including the MIMIC demo database (ODbL, with its `LICENSE.txt` and `NOTICE.txt`) and about 240 MB of fixtures.
4. Optionally rehearse on <https://sandbox.zenodo.org>, which is separate from the real service and mints test DOIs.

**Releasing**

1. Log in to Zenodo with your GitHub account, open the GitHub page under your account settings, and switch the repository on.
2. Set the version in the root `Cargo.toml` and in `CITATION.cff`, and update `date-released` in `CITATION.cff` to the release date.
   Do not add a `.zenodo.json`: Zenodo prefers it over `CITATION.cff`, so the two would drift apart.
3. Tag `v<version>` and push the tag. `.github/workflows/release.yml` checks that the tag matches `Cargo.toml`, builds the
   archives and creates the GitHub release. Zenodo archives the release and mints two DOIs: one for this version and a
   "concept" DOI that always resolves to the latest. If no record appears on Zenodo within a few minutes, publish the release
   once by hand from the GitHub releases page.
4. Copy the concept DOI into the DOI badge in `README.md` (the commented block under the badges) and into `CITATION.cff`
   (`doi:`), and commit. In the Zenodo record, add a line to the description that the archive includes MIMIC-IV demo data
   under the ODbL (see `THIRD_PARTY_NOTICES.md`).

## Writing

- Use British English in prose and documentation: "licence" as a noun, "behaviour", "organisation", "serialise".
  Identifiers, upstream names and quotations keep their original spelling.
- Do not include secrets, credentials, patient-level rows or large raw datasets in commits, issues or fixtures.
  Test data must be synthetic, with one exception: the Eunomia MIMIC dataset, the open-access 100-patient MIMIC-IV
  demo, is kept under its ODbL licence (see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)). Do not add any other
  real or credentialed data, and never the full MIMIC-IV. See [`docs/test-data.md`](docs/test-data.md).
- Do not put machine-specific paths (such as `C:\Users\...`) in documentation or scripts. Use environment variables.

## Commits and pull requests

- Make one logical change per commit. Keep refactors separate from behaviour changes.
- Write the subject in the imperative mood, in sentence case, at 72 characters or fewer, for example
  `Add DuckDB to the dialect allow-list`. Use the body to explain why the change is needed and what it affects.
- Fill in the pull request template, especially "Parity impact".
- Keep CI green. The workflows are in `.github/workflows/`: `ci.yml` (formatting, clippy, tests, builds, crate
  package checks and PostgreSQL scenarios), `security.yml` (`cargo deny` and `gitleaks`) and `release.yml`.

## Reporting bugs

Use the issue templates. For a suspected security problem, follow [SECURITY.md](SECURITY.md) instead of opening an
issue. If your result differs from the R package, say so in the bug report and include the inputs, with any patient
data removed.
