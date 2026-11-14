# Performance

The first part covers the SQLite tuning. The last section compares SQLite with DuckDB, and the
port with upstream R.

## SQLite tuning

Measured on 2026-11-14 on a 16-core Windows 11 machine with 31 GB of RAM. Other sessions shared
the machine, so absolute times are inflated and noisy. Every cell is the median of 3 runs of a
release `dqd run` (all checks, `writeToCsv = false`). For each database, thread count and
repetition, the binaries ran back to back, so changes in machine load hit all variants alike.
Every run of every variant produced byte-identical `CheckResults` after removing timings (one MD5
per database across all 216 runs).

- **QUOD**: PIONEER QUOD liver synthetic, CDM 5.4, 42 MB, 2,384 checks.
- **GiBleed**: Eunomia, CDM 5.3, 20 MB, 2,002 checks.

## Result

| DB | threads | before | + mmap/cache | + MEMSTATUS off (adopted) |
|---|---|---|---|---|
| QUOD | 1 | 36.6 s | 27.0 s | 34.1 s ¹ |
| QUOD | 4 | 31.5 s | 25.5 s | 10.1 s |
| QUOD | 8 | 41.2 s | 33.0 s | **8.6 s** |
| QUOD | 16 | 64.8 s | 61.6 s | 15.0 s |
| GiBleed | 1 | 33.6 s | 26.6 s | 25.1 s |
| GiBleed | 4 | 22.1 s | 20.3 s | 7.3 s |
| GiBleed | 8 | 28.1 s | 26.5 s | **5.1 s** |
| GiBleed | 16 | 53.7 s | 51.9 s | 6.6 s |

¹ The single-thread QUOD runs swung between 21 s and 57 s across passes as the machine load
changed. Compare within a row of one pass, not across passes.

**Fastest thread count: 8 on both databases.** Best before against best after: QUOD 31.5 s
(4 threads) → 8.6 s (8 threads), 3.7× faster; GiBleed 22.1 s (4 threads) → 5.1 s (8 threads),
4.3× faster. Against the default single-threaded run before the change, the gain is 4.3× for QUOD
and 6.6× for GiBleed. So the port's default is now `numThreads = 8`, capped at
`std::thread::available_parallelism()` so that small machines are not oversubscribed. This
deliberately diverges from upstream's `executeDqChecks` default of 1, and
`upstream::default_arguments_match_execute_dq_checks_formals` records the divergence. An explicit
`numThreads` or `--num-threads` still wins. Results do not depend on the thread count: `parity::gibleed_sqlite_run_matches_upstream_results_and_metadata`
runs at 1 and 4 threads.

## What changed

All changes are in `crates/db/src/sqlite.rs` and apply to every connection the runner opens
(one per worker).

1. **`SQLITE_CONFIG_MEMSTATUS` off.** This is the big one. SQLite keeps global memory statistics
   behind one process-wide mutex that every allocation takes, so worker connections serialise on
   it. Before the change, more than 4 threads made runs slower; after it, 8 threads beat 4. The
   statistics are never read.
2. **`PRAGMA mmap_size = 1 GiB` and `PRAGMA cache_size = -65536` (64 MiB).** Workers read pages
   through the shared OS page cache instead of copying them into small private caches. With
   MEMSTATUS off, removing these two pragmas costs 1.3× to 1.8× (QUOD at 8 threads: 8.6 s → 15.8 s).

## Tried and not adopted

- **`PRAGMA temp_store = MEMORY`.** There was no consistent gain (QUOD at 4 threads: 25.5 s →
  26.6 s; GiBleed at 16 threads: 51.9 s → 53.6 s). It also moves temporary B-trees into RAM: on
  MIMIC a run spills about 1 GB, and with up to 16 connections and about 4.5 GB of free RAM that
  is a real risk.
- **`query_only`.** Not tried. `writeToTable` writes through the same connections.
- **`[profile.release] lto = "fat"`, `codegen-units = 1`.** Not adopted. The time is spent
  inside SQLite's C code, which Rust LTO does not reach without cross-language LTO.
- **Literal-token prefilter in `dqd-sqlrender` `search_and_replace`.** Not done. The whole
  SQL-only run of QUOD (render and translate only) takes 2.9 s single-threaded, and in a full run
  the work spreads across the workers. Now that full runs take 5 to 9 s, it is the next candidate
  if more speed is wanted.

## Smaller change

The results JSON and CSV writers now use a `BufWriter`. `serde_json::to_writer` on a bare `File`
issued one system call per token.

## Reproduce

The benchmark scripts were throwaway. Each binary was run as:

```bash
dqd run --config <options.json> --dbms sqlite --sqlite-path <db> --num-threads N --output-folder <out> --write-to-csv false
```

Each run was timed with `date +%s.%N` and checked with an MD5 of the results JSON after removing
timing fields.

## SQLite vs DuckDB, and the port vs R

Measured on 2026-11-14 on the same machine, which was quieter than during the tuning runs above.
The Rust times are medians of 3 runs of a release `dqd run`, with all checks and
`writeToCsv = false`; the two engines ran back to back for each thread count and repetition.
Every run returned the full check set with 0 SQL errors. The R times are single runs of the
pinned package (`numThreads = 1`), taken from oracle generation or the comparison run. The machine
was busy then, so treat them as rough.

| Dataset | Engine | R, 1 thread | Rust, 1 thread | Rust, 8 threads | Rust (8 threads) vs R |
|---|---|---|---|---|---|
| GiBleed (CDM 5.3, 2,002 checks) | SQLite | 60.5 s | 13.4 s | 2.9 s | 21× |
| | DuckDB | 280 s | 8.2 s | **1.7 s** | 165× |
| MIMIC (CDM 5.3, 2,002 checks) | SQLite | 137 s | 20.3 s | 3.9 s | 35× |
| | DuckDB | 114 s | 8.5 s | **2.0 s** | 57× |
| Synthea27Nj (CDM 5.4, 2,384 checks) | SQLite | 132 s | 4.6 s | **0.96 s** | 138× |
| | DuckDB | 150 s | 10.9 s | 1.9 s | 79× |

- **DuckDB is faster on GiBleed and MIMIC, slower on Synthea27Nj.** In the port DuckDB beats
  SQLite on GiBleed (1.6× at 1 thread, 1.7× at 8) and on MIMIC (2.4× and 2.0×). It loses about
  2× on Synthea27Nj, which has only 28 people: there DuckDB's fixed per-query cost, over about
  2,400 queries, outweighs its faster scans.
- **The port beats R on every dataset and engine.** At 1 thread it is 4.5× to 34× faster. At its
  default 8 threads it is 21× to 165× faster.
- **R and the port agree on the MIMIC and Synthea27Nj DuckDB files.** The fuzz harness compared
  them: 2,002 and 2,384 checks, 0 differences. GiBleed on DuckDB is covered by `tests/duckdb.rs`.

How the DuckDB files were built: GiBleed and Synthea27Nj with `Eunomia::getDatabaseFile(...,
dbms = "duckdb")`. MIMIC does not load into DuckDB that way: its ids (up to about 7.5e18)
overflow the CDM DDL's `INTEGER` columns, which SQLite does not enforce. Its DuckDB file comes
from Eunomia's own `loadDataFiles`, run after widening `integer` columns to `BIGINT` in the DDL
that `CommonDataModel::writeDdl` writes.
