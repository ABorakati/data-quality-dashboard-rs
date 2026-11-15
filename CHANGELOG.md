# Changelog

## Unreleased

- Apply the pending Dependabot updates: `actions/checkout` 4 to 7, `actions/upload-artifact` 4 to 7, `actions/download-artifact` 4 to 8 and `gitleaks/gitleaks-action` 2 to 3 in the workflows, and `serde_json` 1.0.151 to 1.0.154 and the transitive `sha2` 0.11.0 to 0.11.1 in `Cargo.lock`. The parity tests pass unchanged. The release dry run now also runs the artifact download and checksum steps, so `download-artifact` is exercised without publishing.

- Release v0.1.0 is archived on Zenodo (concept DOI 10.5281/zenodo.24243167, v0.1.0 10.5281/zenodo.24243168). Add the DOI to the README badge, the "Citing" section and `CITATION.cff`.

- Build the release archives without a build cache. A restored cache kept the build output but not the DuckDB library that `libduckdb-sys` downloads, so the staging step could not find it.

- Let the release workflow publish from a manual run: give `publish_tag` (for example `v0.1.0`) and it creates the tag at the commit it built and publishes the release, the same as pushing the tag. Without it, a manual run is still a dry run.

- Let the release workflow be run by hand as a dry run, which builds the three archives and smoke-tests each one in a clean directory (version, and a DuckDB run that proves the bundled library loads) without creating a release. The publish job runs only for a tag.

- Add a "Made with Claude Code" badge and an "AI-generated content" section to the README, and a note on AI-assisted contributions to `CONTRIBUTING.md`.

- Add the speed-up over R to the README, with its scope (SQLite and DuckDB, three sample datasets) and the caveat that the R timings are rough.

- Describe parity as "identical check results" with its scope (SQLite, DuckDB and PostgreSQL, apart from timestamps and timings) in the README, `CITATION.cff`, `CONTRIBUTING.md`, the `dqd-sqlrender` README and the crate description, instead of looser wording.

- Run the CI and security workflows on pushes to `main` and `master`. The default branch was still `master` when the repository was prepared, so a trigger naming only `main` never fired; `master` can be dropped from both workflows once the branch is renamed to `main`.

- Reset the crate versions to `0.1.0` (the three crates), so the port is not mistaken for the official `2.9.0` release; the README, badges and `CITATION.cff` say it tracks DQD 2.9.0. Results files keep `dqdVersion` 2.9.0, now a constant in `src/runner.rs` that follows the upstream pin instead of the crate version, so the parity fixtures are unchanged. Earlier entries below name the version the crate carried then. Add the ORCID and affiliation to `CITATION.cff`.

- Add `CITATION.cff` (software citation plus the upstream Data Quality Dashboard paper), README badges (CI, security, release, licence, Rust version, tracked DQD version, CDM versions) and a "Citing" section, and document in `CONTRIBUTING.md` how to get a Zenodo DOI for a release. The DOI badge is a commented placeholder until the first release is archived.

- Document the test data: `docs/test-data.md` says what each parity fixture is built from, where the Eunomia datasets come from, what their licences allow, and how to rebuild the fixtures. Keep the Eunomia MIMIC database, the open-access MIMIC-IV demo, in the repository under its ODbL v1.0 licence, after reading the PhysioNet, ODbL, `EunomiaDatasets` and Synthea terms: add PhysioNet's `LICENSE.txt` and a `NOTICE.txt` (source, notice, citation) beside it, as ODbL section 4.2 asks, and record the terms in `THIRD_PARTY_NOTICES.md`. Add `scripts/compare-mimic-with-physionet.py`, which compares that database with PhysioNet's CSV files cell by cell: it is a derivative, not a verbatim copy (64-bit ids stored as doubles, dates as epoch seconds, 12 `operator_concept_id` values NULL), and the notices now say so; `CONTRIBUTING.md` names it as the one non-synthetic exception.

- Upgrade `rusqlite` from 0.37 to 0.41, which bundles SQLite 3.54.1 instead of 3.50.2 (`libsqlite3-sys` 0.35 to 0.39), to clear a known SQLite CVE. The reference, sqlite-jdbc 3.50.3.0, uses SQLite 3.50.3, so the engine is now newer than the oracle's. No code changed. The bundled build still defaults foreign keys on and still lacks `SQLITE_ENABLE_MATH_FUNCTIONS`, so the connection setup and the math and `charindex` emulation are unchanged. The parity suite passes unchanged: the SQLite and DuckDB scenarios, the upstream replay, and the full GiBleed, MIMIC and Synthea27Nj runs. Also update `ctutils` to 0.4.8 in the lockfile.

- **CI, governance and notices.** Preparation for making the repository public.
  - Add GitHub Actions: `ci.yml` (rustfmt, clippy and tests on Linux, macOS and Windows; default and `--no-default-features` builds; a crate package size and contents guard; PostgreSQL scenarios against a service container), `security.yml` (`cargo deny` and full-history `gitleaks`) and `release.yml` (tagged builds for three platforms, bundling DuckDB's shared library, with `SHA256SUMS`).
  - Add `deny.toml`, Dependabot settings, issue and pull request templates. The pull request template asks whether a change affects output compared with the pinned R package.
  - Add `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1) and `THIRD_PARTY_NOTICES.md`, which lists the viewer's vendored assets and their licences.
  - README: unofficial-port notice, the evidence behind the parity claims, unsupported dialects, intentional differences from upstream, installation and a quick start. The parity section no longer assumes PowerShell.
  - Remove the maintainer's Windows paths from the scripts, `fuzz/run_r_batch.R`, `CHANGELOG.md` and `docs/`. The scripts read `DQD_*` environment variables, listed in `CONTRIBUTING.md`. Fixtures under `tests/oracle` and the R generators there still contain such paths.
- DuckDB, part 1: `dqd-sqlrender` translates to the SqlRender `duckdb` dialect. It matches R SqlRender 1.19.7 on all 216 render, 162 translate and 216 split oracle cases, which covers every DQD template in both cohort modes. Only the dialect allow-list changed; the translation rules come from SqlRender's own `replacementPatterns.csv`. `tests/oracle/duckdb-cases.R` records the R oracle on DuckDB (R `duckdb` 2.0.0, engine v2.0.0): 8 small scenarios, 2 preflight failures, upstream's "Execute a single DQ check on DuckDB" test, and a full GiBleed run (2002 checks, 0 errors). The connection backend was not merged then.
- DuckDB, part 2: `--dbms duckdb` with `connection.server` set to the database file, as DatabaseConnector's `createConnectionDetails(dbms = "duckdb", server = ...)`.
  - The backend is the `duckdb` crate `=1.20000.2`, which binds engine 2.0.0, the engine R uses here. It links DuckDB's official prebuilt library (`download-lib`, `DUCKDB_DOWNLOAD_LIB=1` in `.cargo/config.toml`) instead of compiling the bundled C++ engine. That build needs over 5 GB of disk.
  - On MSVC, `duckdb.dll` is delay-loaded (root `build.rs`), so SQLite and PostgreSQL runs start without it. A DuckDB run without the DLL fails with an error that says where to get it.
  - Worker threads share one DuckDB instance of the file (`Connection::try_clone`). Opening the file once per worker fails, because DuckDB allows one instance per file.
  - Errors carry R's `ℹ Context: rapi_prepare` and `ℹ Error type: …` lines.
  - Result tables are written as the same literal `"schema.table"` table that DBI creates.
  - `tests/duckdb.rs` matches R on the 8 scenarios, including write-to-table and DB-to-JSON, on the 2 preflight error messages, on upstream's DuckDB `test_that` case, and on a full GiBleed run (2002 checks) at 1 and 8 threads.
  - The fuzzer accepts a `.duckdb` base database, and `fuzz/run_r_batch.R` picks the matching R connection.
- Remove the unused `gibleed/sqlite_run2` and `sqlite_run4` fixtures. No test read them.
- Default `numThreads` to 8, capped at the machine's available parallelism, when neither the options file nor the CLI sets it. This deliberately diverges from upstream's default of 1. It was the fastest setting on both benchmark databases, and results do not depend on the thread count. An explicit value still wins.

- Speed up SQLite runs without changing any result. SQLite's global memory-statistics mutex is now off (`SQLITE_CONFIG_MEMSTATUS`): it serialised every allocation across worker connections, so more than 4 threads made runs slower. Each connection also gets `mmap_size` = 1 GiB and a 64 MiB page cache. With 8 threads, the fastest setting on both test databases, a full QUOD run takes 8.6 s instead of 31.5 s and GiBleed takes 5.1 s instead of 22.1 s, each against the fastest setting before. `docs/performance.md` has the measurements and the options that were rejected. The results JSON and CSV writers are now buffered.

- Add `tests/upstream.rs`, which replays every scenario of the pinned upstream testthat suite through the R oracle (`tests/oracle/upstream-testthat-cases.R`) and compares all outputs. `docs/upstream-test-coverage.md` maps each upstream `test_that` block to its Rust test.
- Add full-run SQLite parity for the Eunomia MIMIC (CDM 5.3, 2002 checks) and Synthea27Nj (CDM 5.4, 2384 checks) datasets in `tests/eunomia.rs`, generated by `tests/oracle/eunomia-cases.R`. These are the datasets the installed Eunomia 2.1.0 offers beyond GiBleed. It has no SynPUF or empty CDM, its Synthea27NjParquet is the same data, and it builds only CDM 5.3 and 5.4.
- Write the `_row` field that upstream's results JSON carries when a run lacks the Not Applicable prerequisite checks. R keeps `rbind` row names (`"1.1"`, `"1.2"`, …, one prefix per check description) unless the Not Applicable join resets them, and jsonlite writes them when one is not all digits.
- Read results JSON as upstream's `as.data.frame` readers do: `_row` becomes an `X_row` column in `reevaluate`, `export-csv`, and `import-table` (`_x_row` in tables). Case conversion keeps `_row` unchanged, as R row names.
- On SQLite, create the `"schema.table"` results table from the written rows' own columns, as RSQLite's `dbWriteTable(append = TRUE)` does, instead of copying every DDL column. Writing a results JSON that lacks all-NA fields to a table no longer fails.
- Write SQL-only files through R's Windows text-mode newline translation, as the CSV writer already does, and write the run log in SQL-only mode, with the `CDM Tables skipped` and `Processing check description` lines upstream logs. Each run now deletes the previous log first, as upstream does.
- **Line endings: canonical upstream templates use LF.** Earlier, `queryText` and `checkDescription` held `\r\n`, which became `\r\r\n` in CSV on Windows. Against the old oracle library this matched R exactly. A CRAN-style DQD install gives plain `\n`. The evidence decides which is canonical:
  - In upstream git at the pin, all 32 `inst/sql/sql_server` templates are LF blobs (`git ls-files --eol`: `i/lf`). Of the 17 `inst/csv` files, 9 are CRLF blobs and 8 are LF.
  - The CRLF came from a Windows checkout of the pin with `core.autocrlf=true`. Both the oracle library (installed from that working tree) and this port's `resources/` were copied from it.
  - An R package built from git, as CRAN and `remotes` do, ships the blob bytes, so every install off this machine runs LF templates.

  So `resources/csv` and `resources/sql` now hold the exact git blob bytes. The 40 files that changed differ only in line endings. The oracle library was reinstalled from an LF checkout (`core.autocrlf=false`), and every oracle fixture was regenerated with `scripts/regenerate-oracles.ps1`. `scripts/parity.ps1` now uses the LF checkout. R's own Windows text-mode translation (`\n` written as `\r\n` in CSV and SQL files) is real platform behaviour, so the port keeps emulating it on Windows.
- Make the parity tests portable to non-Windows hosts. Fixture paths recorded on Windows now resolve below the oracle folder. Text that R wrote in Windows text mode is compared with `\r\n` normalised to `\n` when not on Windows, where neither R nor the port translates newlines.
- Remove Java entirely. SQL rendering, translation, and splitting run in-process through the `dqd-sqlrender` crate (a Rust port of SqlRender 1.19.7). Database access uses native drivers through the `dqd-db` crate. The `java/` bridge and `SqlRender.jar` are gone.
- Support only `dbms` `sqlite` and `postgresql`. Other values, including in SQL-only mode, fail with an error. The `dialects` command lists only these.
- Replace the JDBC connection fields. `connection` now takes `sqlitePath`, `server` (`host/database`), `port`, `user`, and `password`. `jdbcUrl`, `driverClass`, and the options `java` and `driverClasspath` are removed, along with `--jdbc-url`, `--driver-class`, `--java`, and `--driver-classpath`. Add `--server` and `--port`.
- Parity tests compare rendering for `sql server`, `sqlite`, and `postgresql`, and add PostgreSQL scenarios (`DQD_PG_PORT`) and full Eunomia GiBleed runs on SQLite and PostgreSQL. `scripts/parity.ps1` needs Java only for fixture generation and gains `-Postgres`.
- Record the differential evidence for the removed Java bridge. Before it was deleted, `dqd-db` matched it byte for byte on 5727 of 5727 SQLite and 501 of 501 PostgreSQL differential cases, and `dqd-sqlrender` matched SqlRender 1.19.7 on all 162 render, translate, and split cases for `sql server`, `sqlite`, and `postgresql`. The bridge-driven differential suites were removed with it; their expected values live on in `crates/db/tests/native.rs` and `tests/oracle/generated/render.json`.
- Provide `charindex` on SQLite connections, as RSQLite's bundled extension functions do for DatabaseConnector. DQD's SQLite `cdmDatatype` SQL needs it; without it 154 GiBleed checks errored.
- Write result CSV files as R does: doubles with 15 significant digits in R's fixed or scientific notation, and `\n` as `\r\n` on Windows (so an embedded `\r\n` becomes `\r\r\n`).
- Fail like upstream when `writeToCsv` is set with an empty `csvFile`: after the JSON and table writes, return the error `a character vector argument expected`, as `basename(allResults$outputFile)` does in R.
- Report a failed `cdm_source` metadata query as DatabaseConnector does: `Error executing SQL:` plus the driver message and an `errorReportSql.txt` path in the current directory. Strip the `[SQLSTATE=…; vendorCode=…]` suffix from PostgreSQL error text in recorded check errors.
- Packaging: DuckDB is now the optional Cargo feature `duckdb` (default on) of `dqd-db` and the root crate. `--no-default-features` builds a SQLite and PostgreSQL only `dqd` that needs no DuckDB library, and `--dbms duckdb` then fails with "this build was compiled without DuckDB support".
- Linux and macOS builds with DuckDB carry an `$ORIGIN` / `@loader_path` run path, so `dqd` finds `libduckdb.so` / `libduckdb.dylib` in its own folder.
- Move SqlRender's `replacementPatterns.csv` and `supportedDialects.csv`, with their NOTICE and provenance, from `resources/sqlrender/` to `crates/sqlrender/data/`, so `dqd-sqlrender` packages on its own. The main crate reads `dqd_sqlrender::SUPPORTED_DIALECTS_CSV`.
- Add crates.io metadata (repository, readme, keywords, categories, `rust-version = "1.97"`) and exclude the test fixtures, scripts and fuzz harness from the published packages.

## 2.9.0

- Pin the port to OHDSI DataQualityDashboard 2.9.0 commit `b9d2a9b4a4736f7ea81415d974c72119b6f0bd11`.
- Add the Rust library and `dqd` CLI for check execution, raw check listing, and SQL-only output.
- Add result conversion, threshold reevaluation, CSV export, database table import/export, and viewer commands.
- Retain upstream check catalogs, SQL templates, SqlRender assets, and dashboard browser assets with provenance records.
- Match upstream JSON serialization for null fields and threshold reevaluation.
- Preserve upstream SQLite result-table naming behavior.
- Match upstream SQL-only DDL file termination.
- Keep parity claims contingent on differential verification against the pinned upstream R package.
