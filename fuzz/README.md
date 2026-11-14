# Differential data fuzzing

This harness checks the Rust port against upstream R DataQualityDashboard on databases that the
fixed parity fixtures never contain. It mutates copies of an OMOP CDM SQLite database with a fixed
seed, runs both implementations on each copy, and diffs `CheckResults` field by field. It ignores
only `executionTime`, `executionTimeSeconds`, the start and end timestamps, and absolute
error-report paths.

## Run a batch

```bash
pwsh -NoProfile -File scripts/fuzz-differential.ps1 -Seed 2 -Count 30
```

Options:

- `-Checks cdmField,isRequired,...` runs a check subset. A full GiBleed run takes R about 1 to 2
  minutes per case; a subset of 7 checks takes about 1 minute.
- `-Base <db>` mutates a different database. The default is the GiBleed oracle copy.
- `-CdmVersion` and `-MaxMutations` (default 3 mutations per case) change the cases.

The driver runs four steps, which can also run separately:

1. `cargo run --release --example fuzz_differential -- generate --base <db> --out fuzz/work/seed-N --seed N --count K`
2. `Rscript --vanilla fuzz/run_r_batch.R fuzz/work/seed-N`. One R process runs the whole batch.
   It can resume: cases that already have `r/outcome.json` are skipped.
3. `cargo run --release --example fuzz_differential -- rust fuzz/work/seed-N`
4. `cargo run --release --example fuzz_differential -- compare fuzz/work/seed-N --reproducers fuzz/reproducers`

The same seed always produces the same cases. `fuzz/work/` is scratch space and is ignored by git.

## Mutations

Each case applies 1 to `MaxMutations` mutations. Row-level mutations pick only non-empty clinical
tables. Each mutation is recorded with its exact SQL in `mutations.json`.

| Group | Mutations |
|---|---|
| Missing values | `null_in_required` (the table is rebuilt without NOT NULL first) |
| Date storage | `date_as_text` (`'YYYY-MM-DD'` in an epoch column), `date_as_iso_integer` (`20200131`), `date_with_fraction` (epoch + 0.5) |
| Impossible dates | `future_date` (2099-12-31), `ancient_date` (year 1779), `impossible_text_date` (`'2020-02-30'`), `end_before_start` |
| Concepts | `unknown_concept` (999999999), `zero_concept`, `negative_concept`, `non_standard_concept` |
| Structure | `empty_table` (zero denominators), `drop_table`, `rename_column`, `duplicate_row`, `extra_cdm_source_row` |
| Numbers | `huge_number` (±1e308, ±2^63), `tiny_number` (subnormal, -1e-300), `text_in_numeric` (`'abc'`) |
| Other | `orphan_person` (dangling foreign key), `unicode_source_value` (quotes, newline, `%s`, CJK) |

## Reproducers

`compare` copies each mismatching case to `fuzz/reproducers/<case>/`. The copy holds:

- `cdm.sqlite`, `options.json`, `mutations.json` and `diff.txt`;
- `r-results.json` and `rust-results.json`;
- `r-outcome.json` and `rust-outcome.json`.

The databases are ignored by git. Rebuild one with `generate` and the seed and index from
`mutations.json`.

## Batch log

| Date | Seed | Cases | Checks | Mutations | Mismatching cases | Notes |
|---|---|---|---|---|---|---|
| 2026-11-13 | 1 | 3 | 7 check names (1077 results each) | 5 | 0 | smoke test |
| 2026-11-13 | 2 | 30 | all (2002 results each, 56,056 in total) | 70, all 22 kinds | 0 | R took 52 to 130 s per case. Cases 010 and 024 (a renamed `value_as_number` / `dose_era_start_date` column) end in upstream's fatal "Duplicate table names in emptyTables" error, and the port raises the identical message. |
