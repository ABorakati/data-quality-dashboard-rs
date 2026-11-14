# DataQualityDashboard Rust port

This crate ports OHDSI DataQualityDashboard 2.9.0 to Rust. The target is upstream commit `b9d2a9b4a4736f7ea81415d974c72119b6f0bd11`. Catalog, SQL templates, SqlRender patterns, and dashboard assets retain upstream source data where practical. SQL rendering, translation, and splitting run in Rust (`crates/sqlrender`, a port of OHDSI SqlRender 1.19.7), and database access uses native drivers (`crates/db`). No Java runtime and no JDBC driver are needed. Runtime R is not required.

## Status and compatibility

The command-line program and library expose check execution, SQL-only export, raw check listing, results conversion and export, threshold reevaluation, database result-table I/O, and the dashboard viewer. Check thresholds, database behavior, and result values require differential validation against the pinned R package. Do not treat a successful build as proof of exact parity.

Supported `dbms` values are `sqlite`, `postgresql` and `duckdb`. Any other value, including in SQL-only mode, fails with an error. This repository does not include database credentials.

## Requirements

- Rust toolchain compatible with the 2024 edition.
- A reachable SQLite file, DuckDB file or PostgreSQL server holding the CDM.
- For DuckDB, DuckDB's prebuilt 2.0.0 library. The build downloads it from DuckDB's GitHub
  release (`duckdb` crate, `download-lib`; `.cargo/config.toml` sets `DUCKDB_DOWNLOAD_LIB=1`), and
  `cargo run`/`cargo test` find it automatically. To run a built `dqd.exe` directly on DuckDB, put
  `duckdb.dll` from `duckdb-shared-libs-windows-amd64.tar.gz`
  ([v2.0.0](https://github.com/duckdb/duckdb/releases/tag/v2.0.0)) next to `dqd.exe` or on `PATH`.
  The DLL is delay-loaded, so SQLite and PostgreSQL runs do not need it.
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

The viewer serves the browser assets and exposes read-only APIs for result files. Use `--results-file` to select the initial file. Do not expose the service to untrusted networks without a separate access-control layer.

## Library

The crate exports `Options`, `Catalog`, `Check`, `Row`, database and SQL traits, threshold evaluation, execution, SQL-only export, results utilities, and viewer startup through `data_quality_dashboard`.

## Parity verification

The oracle uses the pinned upstream R package to capture JSON, SQL rendering (`sql server`, `sqlite`, `postgresql`), catalog enumeration, threshold behavior, exports, SQLite and PostgreSQL database scenarios, and full Eunomia GiBleed runs.

Generate fixtures once, then run parity tests against the existing fixtures. The Rust tests need no Java:

```powershell
pwsh -NoProfile -File scripts/parity.ps1 -Generate
pwsh -NoProfile -File scripts/parity.ps1
```

Add `-Postgres` to start the private oracle cluster (`scripts/postgres-oracle.ps1`, port 54329) and run the PostgreSQL scenarios. They are gated on the `DQD_PG_PORT` environment variable and skipped when it is unset. Use `-Refresh` to regenerate the fixtures and run parity in one command. Generation requires the pinned upstream checkout, isolated DQD R library, R 4.5.2, and Java (for R only).

## Provenance and license

`resources/provenance.json` records the pinned DQD source revision and retained resource paths. The OHDSI DataQualityDashboard source states Apache License 2.0. See `LICENSE` and `NOTICE`. The SqlRender pattern and dialect tables and viewer assets retain their own upstream notices where supplied.
