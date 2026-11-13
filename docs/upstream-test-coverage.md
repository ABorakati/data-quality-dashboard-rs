# Upstream test coverage

This maps every `test_that` block in the pinned upstream suite
(`tests/testthat/` at DataQualityDashboard commit `b9d2a9b4a4736f7ea81415d974c72119b6f0bd11`)
to the Rust test that covers it.

## How the coverage works

The upstream tests assert a few properties of each run, for example "no SQL errors" or
"`numViolatedRows > 0`". The Rust suite is stricter. `tests/oracle/upstream-testthat-cases.R`
replays each upstream scenario through the pinned R package and records three things:

- the exact SQLite statements that set up the scenario,
- the options,
- every output: the results JSON, the result folder listing, the tables written, the SQL files, the
  converted files, and the warnings.

`tests/upstream.rs` then applies the same statements to a fresh copy of the same GiBleed database.
It runs the Rust port and compares all of those outputs with the oracle, not only the upstream
assertions. `upstream_assertions_hold_for_rust_results` also re-checks the upstream assertions
themselves.

To regenerate the fixtures (about 4 minutes):

```bash
Rscript --vanilla tests/oracle/upstream-testthat-cases.R C:/Users/abora/tmp/dqd-upstream tests/oracle/generated/upstream tests/oracle/generated/gibleed/sqlite/gibleed-copy.sqlite
```

Run it with the environment that `scripts/parity.ps1` sets (`JAVA_HOME`, `DQD_R_LIBRARY`). The
cases are keyed by name in `tests/oracle/generated/upstream/expected.json`.

## Mapping

### test-calculateNotApplicableStatus.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| measurePersonCompleteness should not be marked as not applicable when table is empty | `apply-not-applicable` (mock rows) | `upstream::apply_not_applicable_matches_upstream_mock_rows` |
| measurePersonCompleteness should be marked as not applicable when table is missing | `apply-not-applicable` | same |
| Not Applicable status Table Empty | `na-device-exposure-empty` (`DELETE FROM DEVICE_EXPOSURE`) | `upstream::not_applicable_when_device_exposure_empty` |
| measureConditionEraCompleteness Not Applicable if condition_occurrence empty | `na-condition-occurrence-empty` | `upstream::condition_era_not_applicable_when_condition_occurrence_empty` |
| measureConditionEraCompleteness Fails if condition_era empty | `na-condition-era-empty` | `upstream::condition_era_fails_when_condition_era_empty` |
| measurePersonCompleteness NOT marked as Not Applicable when table is empty | `na-observation-period-empty` | `upstream::person_completeness_applies_when_observation_period_empty` |
| NA applied correctly when table or field is missing | `apply-not-applicable` | `upstream::apply_not_applicable_matches_upstream_mock_rows` |
| .applyNotApplicable handles cdmTable and cdmField correctly | `apply-not-applicable` | same |

The `apply-not-applicable` case also holds all 64 flag combinations for 5 check names (320 rows)
beyond the 10 upstream mocks. The Rust side calls `evaluate::apply_not_applicable_flags`, the
port's equivalent of `.applyNotApplicable`. The join that produces the flags is covered by
`parity::threshold_and_applicability_states_match_upstream` (`evaluation.json`).

### test-convertResultsCase.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| Camel correctly converted to snake and back | `convert-case` | `upstream::convert_results_case_round_trip_matches_upstream` |
| Invalid case throws error | `convert-case.conversions.invalidCase` | same |
| Output folder required when writing to file | `convert-case.conversions.missingFolder` | same |

`parity::result_export_modes_match_upstream_json_csv_and_sql_files` also covers conversion of a
synthetic file (`exports/expected.json`).

### test-evaluateThresholds.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| evaluateThresholds handles missing table/field errors correctly for cdmField/cdmTable | `evaluate-thresholds-errors` | `upstream::evaluate_thresholds_error_classification_matches_upstream` |
| evaluateThresholds handles missing table/field errors correctly for other checks | same | same |
| evaluateThresholds handles non-missing table/field errors correctly | same | same |

These use upstream's PostgreSQL-style messages (`relation "foo" does not exist`).
`evaluation.json` adds SQLite, SQL Server and Oracle messages across 3 check names.

### test-executeDqChecks.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| Execute a single DQ check on Synthea/Eunomia | `single-check` (default `outputFile`, so the timestamped name is tested) | `upstream::single_check_with_default_output_file_and_its_warnings` |
| Execute all TABLE checks on Synthea/Eunomia | `level-table` | `upstream::execute_all_table_checks` |
| Execute FIELD checks on Synthea/Eunomia | `level-field` | `upstream::execute_field_checks` |
| Execute CONCEPT checks on Synthea/Eunomia | `level-concept` (with `unittest_OMOP_CDMv5.3_Concept_Level.csv`) | `upstream::execute_concept_checks_with_unittest_thresholds` |
| Execute observation period overlap check | `overlap-clean`, `overlap-overlapping`, `overlap-back-to-back` | `upstream::observation_period_overlap_*` |
| Execute a single DQ check on a cohort in Synthea/Eunomia | `cohort` | `upstream::single_check_on_cohort` |
| Execute a single DQ check on remote databases | — | see Explained gaps below |
| Check invalid cdm version | `invalid-cdm-version` | `upstream::invalid_cdm_versions_fail_like_upstream` |
| The vocabulary tables added in CDM v5.5 are excluded by default | `default-arguments` (every constant default of `executeDqChecks`) | `upstream::default_arguments_match_execute_dq_checks_formals` |
| Execute DQ checks and write to table | `write-to-table` | `upstream::write_results_to_named_table` |
| sqlOnly=TRUE, sqlOnlyUnionCount=4, sqlOnlyIncrementalInsert=TRUE (snapshot) | `sql-only-{sql_server,sqlite,postgresql}-union4-incremental1`, `snapshot-file` | `upstream::sql_only_files_match_upstream_for_each_dialect_and_mode`, `upstream::sql_only_oracle_reproduces_upstream_snapshots` |
| sqlOnly=TRUE, sqlOnlyUnionCount=1, sqlOnlyIncrementalInsert=TRUE (snapshot) | `sql-only-*-union1-incremental1` | same |
| sqlOnly=TRUE, sqlOnlyUnionCount=1, sqlOnlyIncrementalInsert=FALSE (snapshot) | `sql-only-*-union1-incremental0` | same |
| Incremental insert SQL is valid. | `incremental-insert-valid` | `upstream::incremental_insert_sql_is_valid_and_inserts_upstream_rows` |
| Multiple cdm_source rows triggers warning. | `multiple-cdm-source` | `upstream::multiple_cdm_source_rows_warn_and_use_the_first_row` (warnings checked on `dqd` stderr) |
| Execute checks on Synthea/Eunomia to test new variable executionTimeSeconds | `execution-time-seconds` | `upstream::execution_time_seconds_is_numeric` |
| checkNames are filtered by checkSeverity | `severity-fatal` (full run) | `upstream::check_names_filtered_by_severity` |
| Execute a single DQ check on DuckDB | — | see Explained gaps below |

The upstream snapshot tests use `dbms = "sql server"`. `dqd run` rejects that dialect, so the
Rust test uses the library's SQL-only export for it. The oracle's SQL Server output is also
checked against upstream's own `_snaps/executeDqChecks.md`, which proves that the oracle reproduces
upstream's snapshots. The same three modes also run for `sqlite` and `postgresql` through `dqd run`.

### test-listChecks.R

| Upstream `test_that` | Fixture | Rust test |
|---|---|---|
| listDqChecks works | `catalog/5.3.json` | `parity::list_checks_matches_upstream_raw_catalog_for_all_cdm_versions` |
| listDqChecks works for all supported CDM versions | `catalog/{5.2,5.3,5.4,5.5}.json` | same |
| v5.5 threshold files contain the tables and fields added in CDM v5.5 | `catalog/5.5.json` (the whole 5.5 catalog compared row by row) | same |

### test-plausibleAfterBirth.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| plausibleAfterBirth allows events on same day as birth (issue #561) | `plausible-after-birth` (TEXT datetimes written into REAL columns) | `upstream::plausible_after_birth_allows_same_day_events` |

### test-reEvaluateThresholds.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| Execute reEvaluateThresholds on Synthea/Eunomia | `re-evaluate` | `upstream::re_evaluate_thresholds_matches_upstream` |

### test-writeDBResultsTo.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| Write DB results to json | `write-db-results` | `upstream::write_db_results_to_json_matches_upstream` |

### test-writeJsonResultsTo.R

| Upstream `test_that` | Oracle case | Rust test |
|---|---|---|
| Write JSON results | `write-json-results` (`csv`, `tables.default`) | `upstream::write_json_results_to_csv_and_tables_match_upstream` |
| Write JSON results with singleTable parameter | `write-json-results` (`tables.single`, `tables.separate`) | same |

### test-helpers.R and setup.R

These files hold no `test_that` blocks. They set up devtools links and connection details.

## Explained gaps

- **Execute a single DQ check on remote databases.** Upstream loops over Oracle, PostgreSQL, SQL
  Server, Redshift, IRIS, Snowflake, Spark and BigQuery, and skips any without credentials. The
  port supports only `sqlite` and `postgresql`. PostgreSQL has full parity coverage: the 8
  scenarios, the preflight failures, and the full GiBleed run in `tests/parity.rs`, which run when
  `DQD_PG_PORT` is set (`scripts/parity.ps1 -Postgres`). The other dialects are out of scope by
  design (see `CHANGELOG.md`). Their SQL rendering and translation for `sql server` is still
  compared in `parity::every_check_template_renders_translates_and_splits_on_each_supported_dialect`.
- **Execute a single DQ check on DuckDB.** The port has no DuckDB driver. DuckDB is out of scope,
  as for the other unsupported dialects.

## What the upstream scenarios found

The replay found three real mismatches. All three are fixed, and the tests above guard them:

1. **`_row` in results JSON.** Upstream results keep R row names (`"1.1"`, `"1.2"`, …, one prefix
   per check description) when the run lacks the three Not Applicable prerequisite checks. jsonlite
   writes them as a `_row` field. Readers that use `as.data.frame` turn it into an `X_row`
   column: `reEvaluateThresholds`, and `writeJsonResultsToTable`, where it becomes `_x_row`.
   `convertJsonResultsFileCase` keeps it as row names. Earlier parity runs always included those
   checks, so they never saw it.
2. **Tables written from results JSON on SQLite.** RSQLite creates the `"schema.table"` table from
   the data frame's own columns. Rust copied every DDL column instead, so its inserts failed when
   the JSON lacked all-NA fields or carried `X_row`.
3. **SQL-only output.** The `.sql` files lacked R's Windows text-mode newlines (`\n` written as
   `\r\n`). The run log was also missing in SQL-only mode.
