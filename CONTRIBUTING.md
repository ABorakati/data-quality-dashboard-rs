# Contributing

Thank you for looking at this port. It aims to reproduce OHDSI DataQualityDashboard 2.9.0 exactly, so most of the
rules below exist to protect that. Read [README.md](README.md) first, in particular "Status and compatibility".

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

## Writing

- Use British English in prose and documentation: "licence" as a noun, "behaviour", "organisation", "serialise".
  Identifiers, upstream names and quotations keep their original spelling.
- Do not include secrets, credentials, patient-level rows or large raw datasets in commits, issues or fixtures.
  Test data must be synthetic. Eunomia datasets are acceptable.
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
