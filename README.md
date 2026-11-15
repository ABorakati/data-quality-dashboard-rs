# DataQualityDashboard Rust port

> **Unofficial port.** Not affiliated with or endorsed by OHDSI. The OHDSI name and logo belong to OHDSI and identify the upstream project this port follows.

[![CI](https://github.com/ABorakati/data-quality-dashboard-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/ABorakati/data-quality-dashboard-rs/actions/workflows/ci.yml)
[![Security](https://github.com/ABorakati/data-quality-dashboard-rs/actions/workflows/security.yml/badge.svg)](https://github.com/ABorakati/data-quality-dashboard-rs/actions/workflows/security.yml)
[![Release](https://img.shields.io/github/v/release/ABorakati/data-quality-dashboard-rs?include_prereleases&label=release)](https://github.com/ABorakati/data-quality-dashboard-rs/releases)
[![Licence: Apache-2.0](https://img.shields.io/badge/licence-Apache--2.0-blue.svg)](LICENSE)
[![Rust 1.97+](https://img.shields.io/badge/rust-1.97%2B-orange.svg)](Cargo.toml)
[![Tracks DQD 2.9.0](https://img.shields.io/badge/tracks%20DQD-2.9.0-informational.svg)](https://github.com/OHDSI/DataQualityDashboard/tree/b9d2a9b4a4736f7ea81415d974c72119b6f0bd11)
[![Made with Claude Code](https://img.shields.io/badge/made%20with-Claude%20Code-D97757?logo=anthropic&logoColor=white)](https://claude.com/claude-code)
[![OMOP CDM 5.2 to 5.5](https://img.shields.io/badge/OMOP%20CDM-5.2%20to%205.5-lightgrey.svg)](docs/test-data.md)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.24243167.svg)](https://doi.org/10.5281/zenodo.24243167)

This crate ports OHDSI DataQualityDashboard 2.9.0 to Rust. The target is upstream commit `b9d2a9b4a4736f7ea81415d974c72119b6f0bd11`. Catalog, SQL templates, SqlRender patterns, and dashboard assets retain upstream source data where practical. SQL rendering, translation, and splitting run in Rust (`crates/sqlrender`, a port of OHDSI SqlRender 1.19.7), and database access uses native drivers (`crates/db`). No Java runtime and no JDBC driver are needed. Runtime R is not required.

It produces identical check results to the R package, verified on SQLite, DuckDB and PostgreSQL. Run-dependent fields (timestamps and timings) are the only values that cannot match between any two runs, and the tests leave them out. The evidence, the engines and dialects covered, and the intentional differences are listed under [Status and compatibility](#status-and-compatibility).

On SQLite and DuckDB it ran about 20x or more faster than R at default settings (8 threads against R's 1) and 4.5x to 34x faster single-threaded, measured on three sample datasets. The R timings are single runs on a busy machine, so treat them as rough; see [docs/performance.md](docs/performance.md).

## Status and compatibility

The command-line program and library expose check execution, SQL-only export, raw check listing, results conversion and export, threshold reevaluation, database result-table I/O, and the dashboard viewer.

A successful build is not proof of parity with upstream. The port is checked against outputs produced by the pinned R package, and the evidence is listed below. Supported `dbms` values are `sqlite`, `postgresql` and `duckdb`. Any other value, including in SQL-only mode, fails with an error. This repository does not include database credentials.

### What is verified

The reference outputs come from the pinned R package (DataQualityDashboard 2.9.0, R 4.5.2, DatabaseConnector 8.0.0, SqlRender 1.19.7). They are committed under `tests/oracle/generated`, and the Rust tests compare the port against them.

- **Full runs on synthetic Eunomia datasets, compared with R.**
  - SQLite: GiBleed (CDM 5.3, 2,002 checks), MIMIC (CDM 5.3, 2,002 checks) and Synthea27Nj (CDM 5.4, 2,384 checks), in `tests/parity.rs` and `tests/eunomia.rs`.
  - DuckDB: GiBleed at 1 and 8 threads, in `tests/duckdb.rs`. The MIMIC and Synthea27Nj DuckDB files gave the same results as R in the differential fuzzer (0 differences).
  - PostgreSQL: the eight database scenarios, the two `cdm_source` preflight failures, and a full GiBleed run. These run when `DQD_PG_PORT` is set, against a server the tests can reach.
- **Upstream test suite.** Every scenario of the pinned upstream testthat suite is replayed through R, and all outputs are compared with the port, not only upstream's assertions. `docs/upstream-test-coverage.md` maps each block to its Rust test.
- **SQL rendering.** Rendering and splitting match SqlRender 1.19.7 for `sql server`, `sqlite`, `postgresql` and `duckdb` (216 render and 216 split oracle cases). Translation matches for the three runnable dialects (162 cases). Each set covers all 27 check templates in both cohort modes.
- **Differential fuzzing.** `fuzz/README.md` records 49 mutated OMOP CDM databases from four seeds, on SQLite and DuckDB. 46 of them ran every check. None had a mismatch with R. Where upstream stops with a fatal error, the port stops with the same message.
- **Performance.** `docs/performance.md` reports the timings. The port is faster than R on every dataset and engine measured, but these timings do not show parity.

### What is not supported

- **Other DatabaseConnector dialects.** SQL Server, Oracle, Snowflake, BigQuery, Redshift, Spark, Impala, Netezza, Synapse, IRIS and the rest are not supported. `dqd run` rejects them. The library can render `sql server` SQL for a SQL-only export, and that rendering is checked against R, but the port cannot execute it.
- **JDBC.** There is no Java runtime and no JDBC driver. Connections use native drivers: `rusqlite` for SQLite, the `duckdb` crate, and the `postgres` crate.
- **Remote-database upstream tests.** Upstream's single-check tests against Oracle, SQL Server and other remote systems are not run. See the "Explained gaps" section of `docs/upstream-test-coverage.md`.

### Intentional differences from upstream

- **`numThreads` defaults to 8**, capped at the machine's available parallelism. Upstream defaults to 1. Results do not depend on the thread count. Set `"numThreads": 1` or `--num-threads 1` for upstream's single-worker behaviour.
- **Connection options.** `jdbcUrl`, `driverClass`, `java` and `driverClasspath` are removed. Use `sqlitePath`, or `server`, `port`, `user` and `password` (`--server`, `--port`).
- **PostgreSQL error text.** Errors recorded for checks omit the `[SQLSTATE=…; vendorCode=…]` suffix that the JDBC driver adds to PostgreSQL messages.
- **Viewer.** The viewer has no authentication and is meant for loopback use. See the Viewer section below.

### Reporting a parity discrepancy

Open a bug report and choose "Yes, the output differs from the pinned R package". Include the `dbms` and database version, the options file with the password removed, the check IDs whose values differ, and both outputs (the port's and R's) for the same inputs. Share aggregate counts and SQL only. Do not share patient-level rows. If you cannot share the data, describe the smallest schema that reproduces the difference. Security problems go to [SECURITY.md](SECURITY.md) instead.

## Installation

**Release archives.** Each release tag (`v*`) on the GitHub Releases page has archives for `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`, named `dqd-v<version>-<target>` (`.tar.gz`, or `.zip` on Windows). Each archive holds `dqd` (`dqd.exe` on Windows), DuckDB's shared library (`libduckdb.so`, `libduckdb.dylib` or `duckdb.dll`), `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md` and `README.md`. Check the archive against `SHA256SUMS` before you run it.

**Install from a checkout.** The DuckDB shared library is not copied by `cargo install`, so DuckDB runs need it on the loader path. Use a release archive, or build with `cargo build --release` and keep the library next to `target/release/dqd`. For SQLite and PostgreSQL only, install without DuckDB:

```sh
cargo install --path . --no-default-features
```

**Build from source.** The build downloads DuckDB's prebuilt library, so it needs network access:

```sh
git clone https://github.com/ABorakati/data-quality-dashboard-rs
cd data-quality-dashboard-rs
cargo build --release          # the binary is target/release/dqd
```

## Quick start

These commands use `examples/options.json`, which sets `"sqlOnly": true`. The first commands need no database.

```sh
cargo run --release -- dialects                                       # the supported dbms values
cargo run --release -- list-checks --config examples/options.json     # the check catalogue
cargo run --release -- run --config examples/options.json             # writes check SQL to ./out and does not connect
```

To run the checks, edit `examples/options.json` (schemas, `dbms` and `connection`; see [Run checks](#run-checks)), then run with `--sql-only false`:

```sh
cargo run --release -- run --config examples/options.json --sql-only false
```

The results are written to `outputFolder`, in the file named by `outputFile`.

## Requirements

- Rust 1.97 or later (edition 2024). This is the declared minimum (`rust-version`), tested in CI; no dependency needs more than 1.88.
- A reachable SQLite file, DuckDB file or PostgreSQL server holding the CDM.
- For DuckDB, DuckDB's prebuilt 2.0.0 shared library. DuckDB support is the Cargo feature `duckdb`, which
  is on by default. The build downloads the library from DuckDB's GitHub release (`duckdb` crate,
  `download-lib`; `.cargo/config.toml` sets `DUCKDB_DOWNLOAD_LIB=1`), and `cargo run`/`cargo test` find it
  automatically.
  - **Release archives** put the library next to the executable: `libduckdb.so` beside `dqd` on Linux,
    `libduckdb.dylib` beside `dqd` on macOS, `duckdb.dll` beside `dqd.exe` on Windows. Linux and macOS
    builds carry an `$ORIGIN` / `@loader_path` run path, so the executable finds the library in its own
    folder with no `LD_LIBRARY_PATH` or `DYLD_LIBRARY_PATH`; keep the two files together when moving them.
    The library is in `duckdb-shared-libs-<platform>.tar.gz` on the
    [DuckDB v2.0.0 release](https://github.com/duckdb/duckdb/releases/tag/v2.0.0).
  - On Windows the DLL is delay-loaded, so SQLite and PostgreSQL runs do not need it (it can also be on
    `PATH`). On Linux and macOS the library is a normal dependency of the executable and must be present
    even for SQLite and PostgreSQL runs.
  - **Without DuckDB**: `cargo build --release --no-default-features` builds a SQLite and PostgreSQL only
    binary that needs no DuckDB library at build or run time. `--dbms duckdb` then fails with
    "this build was compiled without DuckDB support". Dependent crates use `dqd-db` with
    `default-features = false` for the same effect.
- R is optional and is used only by the independent upstream parity oracle. R itself needs Java through rJava and DatabaseConnector, so Java is needed only to regenerate oracle fixtures.

## Run checks

Create an options JSON file using camelCase names from upstream `executeDqChecks`. Start with `examples/options.json`, then set the CDM schema, source name, `dbms`, and `connection`.

The `connection` object has these fields, matching DatabaseConnector connection details:

| Field | `sqlite` | `postgresql` | `duckdb` |
|---|---|---|---|
| `sqlitePath` | database file (required) | unused | alias for `server` |
| `server` | unused | `host/database` (required) | database file (required) |
| `port` | unused | port, default 5432 | unused |
| `user`, `password` | unused | credentials | unused |

```json
{ "dbms": "postgresql", "connection": { "server": "localhost/omop", "port": 5432, "user": "dqd", "password": "" } }
{ "dbms": "duckdb", "connection": { "server": "C:/data/cdm.duckdb" } }
```

### PostgreSQL TLS

`dqd` connects to PostgreSQL with TLS through rustls (pure Rust, no system OpenSSL). Choose the mode in the `server` string, pgjdbc-style: `host/database?sslmode=verify-full&sslrootcert=/path/ca.pem`. The default, `prefer`, tries TLS and falls back to plain text. `require` encrypts but does not check the server certificate, as in pgjdbc. Use `verify-ca` or `verify-full` to authenticate the server; `verify-full` also checks the host name. Without `sslrootcert` the operating system's trust store is used. Client certificates are supported through `sslcert` and `sslkey` (unencrypted PEM or DER keys). Building with `--no-default-features` removes TLS, and `require` and stronger then fail with a clear error. Details: [crates/db/README.md](crates/db/README.md).

DuckDB follows DatabaseConnector's `createConnectionDetails(dbms = "duckdb", server = <file>)`.
Worker threads share one DuckDB instance of the file, because DuckDB lets only one instance open
a file at a time.

```sh
cargo run --release -- run --config examples/options.json
```

Use `--sql-only true` to export check SQL without connecting to the CDM. Set `outputFolder`, `outputFile`, and `sqlOnlyIncrementalInsert` in the JSON file for the desired export. `sqlOnlyUnionCount` controls the upstream union grouping behavior.

The CLI accepts matching kebab-case overrides, such as `--cdm-version 5.4`, `--check-levels TABLE,FIELD`, `--num-threads 4`, `--sqlite-path cdm.sqlite`, and `--server localhost/omop --port 5432`. The JSON configuration uses camelCase. CLI values override JSON values.

`numThreads` defaults to 8, capped at the machine's available parallelism. Upstream's default is
1, but 8 was the fastest setting on SQLite in `docs/performance.md`, and results do not depend on
the thread count. Set `"numThreads": 1` or `--num-threads 1` for upstream's single-worker run.

```sh
cargo run -- list-checks --config examples/options.json
cargo run -- run --config examples/options.json --sql-only true
```

`list-checks` returns the unfiltered check catalog. It does not connect to a database.

`dialects` prints the supported `dbms` values (`sqlite`, `postgresql`, `duckdb`) with their SqlRender descriptions.

```sh
cargo run -- dialects
```


## Result tools

```sh
cargo run -- convert-case results.json --target-case snake --output results-snake.json
cargo run -- export-csv results.json results.csv
cargo run -- reevaluate results.json --config examples/options.json --output-folder out --output-file reevaluated.json
cargo run -- import-table results.json --config examples/options.json
cargo run -- table-to-json --config examples/options.json --output-folder out --output-file results.json
```

For CSV export, pass `--columns checkId,checkName,passed,failed` to select columns. `--delimiter` takes one byte and defaults to a comma.
SQLite and DuckDB result-table writes match upstream behavior: `schema.table` becomes one literal table name. `table-to-json` treats the dot as a schema separator and reads a different table.

## Viewer

Place result JSON files directly inside a results directory, then start the viewer:

```sh
cargo run -- viewer --results-root ./results --host 127.0.0.1 --port 8080
```

The viewer is self-contained: the browser assets are compiled into the binary, so `dqd viewer` works from any directory and needs no source tree. It exposes read-only APIs for result files. Use `--results-file` to select the initial file. The file list shows only valid DQD result files, so `options.json` and `timing.json` in the same folder do not appear.

It is meant for local use. Do not expose it to untrusted networks without a separate access-control layer, because it serves results, including each check's SQL, without authentication.

- **Host check.** The viewer returns 403 for any `Host` header that is not its bound address, as protection against DNS rebinding. With `--host 0.0.0.0` or `::` it accepts only loopback names and IP addresses and prints a warning. To reach it by a DNS name, pass that name as `--host`.
- **Headers.** Responses carry a Content-Security-Policy and other security headers, and `/api/*` responses are not cacheable.
- **Limits.** Each request runs on its own thread, up to 64 at once (503 above that). The HTTP library has no socket timeouts, so a client that never reads a large response holds one thread until the operating system drops the connection.

## Library

The crate exports `Options`, `Catalog`, `Check`, `Row`, database and SQL traits, threshold evaluation, execution, SQL-only export, results utilities, and viewer startup through `data_quality_dashboard`.

## Parity verification

The parity fixtures are committed in `tests/oracle/generated`. They cover the outputs listed under "What is verified": JSON results, SQL rendering, catalogue enumeration, threshold behaviour, exports, the SQLite and PostgreSQL database scenarios, and the full runs.

What the fixtures are built from, where the Eunomia datasets come from, their licences, and how to rebuild everything: [docs/test-data.md](docs/test-data.md).

The Rust tests need no R and no Java:

```sh
cargo test --workspace --release
```

The PostgreSQL scenarios run when `DQD_PG_PORT` is set to the port of a PostgreSQL server with trust authentication. They are skipped otherwise. See [CONTRIBUTING.md](CONTRIBUTING.md#test).

Regenerating the fixtures needs R 4.5.2 with the pinned package, Java (for R only) and PowerShell 7. Their setup is described in [CONTRIBUTING.md](CONTRIBUTING.md#regenerating-the-parity-fixtures). Do not edit the fixtures by hand.

## Citing

If you use this software, cite it using [`CITATION.cff`](CITATION.cff) (GitHub's "Cite this repository" button reads it).
The software is archived on Zenodo. [10.5281/zenodo.24243167](https://doi.org/10.5281/zenodo.24243167) always resolves to the
latest version, and v0.1.0 is [10.5281/zenodo.24243168](https://doi.org/10.5281/zenodo.24243168). This is an unofficial port,
so please also cite the paper that describes the upstream Data Quality Dashboard:

> Blacketer C, DeFalco FJ, Ryan PB, Rijnbeek PR. Increasing trust in real-world evidence through evaluation of observational
> data quality. *J Am Med Inform Assoc.* 2021;28(10):2251-2257. [doi:10.1093/jamia/ocab132](https://doi.org/10.1093/jamia/ocab132)

The MIMIC-IV demo database used in the parity tests has its own citation and licence: see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## AI-generated content

Substantial parts of this project, including code, tests, documentation, CI configuration and notices, were written by AI:
[Claude Code](https://claude.com/claude-code), Anthropic's coding assistant, working under the direction of the maintainer,
Aditya Borakati. Commits made by Claude Code carry a `Co-Authored-By: Claude` trailer and a link to the session.

What that means for you:

- **Correctness rests on tests.** The port is checked against the pinned R package by the parity tests (see
  [Status and compatibility](#status-and-compatibility)). Review the code as you would any other, and report anything wrong
  through an issue.
- **Licence and data-terms notes are not legal advice.** [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
  [docs/test-data.md](docs/test-data.md) record what the AI read from the sources they cite. Check the originals before you
  rely on them.
- **Authorship.** The maintainer is the author named in [`CITATION.cff`](CITATION.cff). The AI is acknowledged here and in the
  commit history, and is not listed as an author.

## Provenance and license

`resources/provenance.json` records the pinned DQD source revision and retained resource paths. The OHDSI DataQualityDashboard source states Apache License 2.0. See `LICENSE` and `NOTICE`. The SqlRender pattern and dialect tables and viewer assets retain their own upstream notices where supplied. Third-party components bundled with the viewer, with their licences, are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
