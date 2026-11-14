//! Parity on every scenario of the pinned upstream testthat suite (`tests/testthat/*.R`).
//!
//! `tests/oracle/upstream-testthat-cases.R` re-runs each `test_that` scenario through the pinned R
//! package on a copy of the Eunomia GiBleed fixture and records the exact setup statements, the
//! options, and every output. These tests apply the same statements to a fresh copy, run the Rust
//! port, and compare the outputs, not only the upstream assertions. `docs/upstream-test-coverage.md`
//! maps each upstream test to the case here.
use data_quality_dashboard::{
    catalog::Catalog,
    database, evaluate,
    model::{Options, Row},
    results, runner,
    sql::{NativeSql, SqlEngine},
    sql_only,
};
use serde_json::{Map, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

mod common;
use common::*;

fn cases() -> &'static Value {
    static CASES: OnceLock<Value> = OnceLock::new();
    CASES.get_or_init(|| fixture("upstream/expected.json"))
}

fn case(name: &str) -> &'static Value {
    let case = &cases()[name];
    assert!(!case.is_null(), "oracle case {name} is missing");
    case
}

fn gibleed_source() -> PathBuf {
    oracle().join("gibleed/sqlite/gibleed-copy.sqlite")
}

fn sqlite_options(db: &Path) -> Options {
    let mut options = Options {
        dbms: "sqlite".into(),
        ..Options::default()
    };
    options.connection.sqlite_path = Some(db.to_path_buf());
    options
}

/// Copies the GiBleed fixture and applies the scenario's setup statements, as R executed them.
fn fresh_gibleed(dir: &Path, name: &str, setup: &Value) -> PathBuf {
    let db = dir.join(format!("{name}.sqlite"));
    fs::copy(gibleed_source(), &db).unwrap();
    let mut connection = database::connect(&sqlite_options(&db)).unwrap();
    for statement in string_vector(setup) {
        connection
            .execute(&statement)
            .unwrap_or_else(|e| panic!("{name} setup `{statement}`: {e:#}"));
    }
    db
}

/// The R options with paths that point into the R library mapped onto the fixture copies.
fn case_options(case: &Value, db: &Path, folder: &Path) -> Options {
    let mut options: Options = serde_json::from_value(case["options"].clone()).expect("options");
    for location in [
        &mut options.table_check_threshold_loc,
        &mut options.field_check_threshold_loc,
        &mut options.concept_check_threshold_loc,
    ] {
        if location != "default" {
            let name = Path::new(location.as_str()).file_name().unwrap();
            *location = oracle()
                .join("upstream")
                .join(name)
                .to_string_lossy()
                .into_owned();
        }
    }
    options.dbms = "sqlite".into();
    options.connection.sqlite_path = Some(db.to_path_buf());
    options.output_folder = folder.to_path_buf();
    options
}

fn table_names(db: &Path) -> BTreeSet<String> {
    let mut connection = database::connect(&sqlite_options(db)).unwrap();
    connection
        .query("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .into_iter()
        .map(|row| row["name"].as_str().unwrap().to_owned())
        .collect()
}

/// Tables a scenario created (DQD output tables), with their rows; `execution_time` varies.
fn new_tables(db: &Path) -> Value {
    let original = table_names(&gibleed_source());
    let mut connection = database::connect(&sqlite_options(db)).unwrap();
    let mut tables = Map::new();
    for name in table_names(db) {
        if original.contains(&name) || name.ends_with("_BACK") {
            continue;
        }
        let rows = connection
            .query(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        tables.insert(name, rows_without_execution_time(rows));
    }
    Value::Object(tables)
}

fn rows_without_execution_time(rows: Vec<Row>) -> Value {
    Value::Array(
        rows.into_iter()
            .map(|mut row| {
                row.remove("execution_time");
                Value::Object(row)
            })
            .collect(),
    )
}

/// jsonlite writes an empty named list as `[]`.
fn object_or_empty(value: &Value) -> Value {
    match value {
        Value::Array(values) if values.is_empty() => Value::Object(Map::new()),
        Value::Object(_) => {
            let mut value = value.clone();
            for rows in value.as_object_mut().unwrap().values_mut() {
                for row in rows.as_array_mut().into_iter().flatten() {
                    row.as_object_mut().unwrap().remove("execution_time");
                }
            }
            value
        }
        other => panic!("expected a named list, got {other}"),
    }
}

/// Result folder listing with run timestamps in default file names masked.
fn result_files(folder: &Path) -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| mask_timestamp(&entry.unwrap().file_name().to_string_lossy()))
        .collect();
    files.sort();
    files
}

fn mask_timestamp(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if i + 14 <= bytes.len() && bytes[i..i + 14].iter().all(u8::is_ascii_digit) {
            out.push_str("<timestamp>");
            i += 14;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Result of one Rust run of an upstream scenario.
struct Run {
    _dir: tempfile::TempDir,
    db: PathBuf,
    folder: PathBuf,
    options: Options,
    value: Value,
    result_file: PathBuf,
}

/// Runs one executeDqChecks scenario and compares it in full with the oracle.
fn assert_run_case(name: &str) -> Run {
    let case = case(name);
    let dir = tempfile::tempdir().unwrap();
    let db = fresh_gibleed(dir.path(), name, &case["setup"]);
    let folder = dir.path().join(name);
    let options = case_options(case, &db, &folder);
    let expected = &case["expected"];
    let outcome = runner::execute_dq_checks(&options);
    if !expected["ok"].as_bool().unwrap() {
        assert_eq!(
            format!("{:#}", outcome.unwrap_err()),
            expected["error"].as_str().unwrap(),
            "{name} error"
        );
        return Run {
            _dir: dir,
            db,
            folder,
            options,
            value: Value::Null,
            result_file: PathBuf::new(),
        };
    }
    let value = outcome.unwrap_or_else(|e| panic!("{name}: {e:#}"));
    if case["executionTimeSecondsIsNumeric"] == true {
        assert!(
            value["executionTimeSeconds"].is_number()
                || value["executionTimeSeconds"][0].is_number(),
            "{name}: executionTimeSeconds must be numeric: {}",
            value["executionTimeSeconds"]
        );
    }
    let files = result_files(&folder);
    let expected_files: Vec<String> = string_vector(&case["resultFiles"])
        .iter()
        .map(|file| mask_timestamp(file))
        .collect();
    assert_eq!(files, expected_files, "{name} result folder");
    let result_name = if options.output_file.is_empty() {
        fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .find(|file| file.ends_with(".json"))
            .expect("default-named result file")
    } else {
        options.output_file.clone()
    };
    let result_file = folder.join(result_name);
    let actual = read_json(&result_file);
    assert_results_match(name, &actual, &case["result"]);
    assert_json_equal(
        &new_tables(&db),
        &object_or_empty(&case["newTables"]),
        &format!("{name} written tables"),
    );
    Run {
        _dir: dir,
        db,
        folder,
        options,
        value,
        result_file,
    }
}

macro_rules! run_cases {
    ($($test:ident => $name:literal),* $(,)?) => {$(
        #[test]
        fn $test() {
            assert_run_case($name);
        }
    )*};
}

run_cases! {
    // test-executeDqChecks.R
    execute_all_table_checks => "level-table",
    execute_field_checks => "level-field",
    execute_concept_checks_with_unittest_thresholds => "level-concept",
    observation_period_overlap_clean => "overlap-clean",
    observation_period_overlap_overlapping => "overlap-overlapping",
    observation_period_overlap_back_to_back => "overlap-back-to-back",
    single_check_on_cohort => "cohort",
    write_results_to_named_table => "write-to-table",
    execution_time_seconds_is_numeric => "execution-time-seconds",
    check_names_filtered_by_severity => "severity-fatal",
    // test-calculateNotApplicableStatus.R
    not_applicable_when_device_exposure_empty => "na-device-exposure-empty",
    condition_era_not_applicable_when_condition_occurrence_empty => "na-condition-occurrence-empty",
    condition_era_fails_when_condition_era_empty => "na-condition-era-empty",
    person_completeness_applies_when_observation_period_empty => "na-observation-period-empty",
    // test-plausibleAfterBirth.R
    plausible_after_birth_allows_same_day_events => "plausible-after-birth",
}

/// Upstream asserts on the scenarios above, re-checked on the Rust output so that the upstream
/// expectation itself is visible here, beyond equality with the oracle.
#[test]
fn upstream_assertions_hold_for_rust_results() {
    let rows = |name: &str| -> Vec<Value> {
        case(name)["result"]["CheckResults"]
            .as_array()
            .unwrap()
            .clone()
    };
    for name in ["level-table", "level-field", "level-concept"] {
        let rows = rows(name);
        assert!(!rows.is_empty(), "{name}");
        assert!(
            rows.iter().all(|row| row["error"].is_null()),
            "{name}: upstream expects no SQL errors"
        );
    }
    let overlap = |name: &str| rows(name)[0]["numViolatedRows"].as_f64().unwrap();
    assert_eq!(overlap("overlap-clean"), 0.0);
    assert!(overlap("overlap-overlapping") > 0.0);
    assert!(overlap("overlap-back-to-back") > 0.0);
    let fatal = [
        "cdmTable",
        "cdmField",
        "isRequired",
        "cdmDatatype",
        "isPrimaryKey",
        "isForeignKey",
        "measureObservationPeriodOverlap",
    ];
    assert!(
        rows("severity-fatal")
            .iter()
            .all(|row| fatal.contains(&row["checkName"].as_str().unwrap()))
    );
    assert!(
        rows("na-device-exposure-empty")
            .iter()
            .filter(|row| row["checkName"] == "measureValueCompleteness"
                && row["cdmTableName"] == "DEVICE_EXPOSURE")
            .all(|row| row["notApplicable"] == 1)
    );
    let era = |name: &str| {
        rows(name)
            .into_iter()
            .find(|row| row["checkName"] == "measureConditionEraCompleteness")
            .unwrap()
    };
    assert_eq!(era("na-condition-occurrence-empty")["notApplicable"], 1);
    assert_eq!(era("na-condition-era-empty")["failed"], 1);
    let person = rows("na-observation-period-empty")
        .into_iter()
        .find(|row| {
            row["checkName"] == "measurePersonCompleteness"
                && row["cdmTableName"] == "OBSERVATION_PERIOD"
        })
        .unwrap();
    assert_eq!(person["notApplicable"], 0);
    assert_eq!(person["failed"], 1);
    let visit = rows("plausible-after-birth")
        .into_iter()
        .find(|row| {
            row["cdmTableName"] == "VISIT_OCCURRENCE" && row["cdmFieldName"] == "VISIT_START_DATE"
        })
        .unwrap();
    assert_eq!(visit["numViolatedRows"], 0);
}

#[test]
fn apply_not_applicable_matches_upstream_mock_rows() {
    for (index, mock) in case("apply-not-applicable")["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        // A one-row data frame.
        let row = mock["row"][0].as_object().unwrap();
        let expected = mock["expected"]["value"].as_f64().unwrap() == 1.0;
        assert_eq!(
            evaluate::apply_not_applicable_flags(row),
            expected,
            "mock {index} ({}): {row:?}",
            mock["test"]
        );
    }
}

#[test]
fn evaluate_thresholds_error_classification_matches_upstream() {
    let empty = Catalog::from_rows(vec![], vec![], vec![], vec![]);
    for mock in case("evaluate-thresholds-errors")["cases"]
        .as_array()
        .unwrap()
    {
        let mut rows: Vec<Row> = mock["row"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.as_object().unwrap().clone())
            .collect();
        evaluate::evaluate_thresholds(&mut rows, &empty)
            .unwrap_or_else(|e| panic!("{}: {e:#}", mock["test"]));
        assert_json_equal(
            &Value::Array(rows.into_iter().map(Value::Object).collect()),
            &mock["expected"]["value"],
            mock["test"].as_str().unwrap(),
        );
    }
}

#[test]
fn invalid_cdm_versions_fail_like_upstream() {
    for version_case in case("invalid-cdm-version")["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = sqlite_options(&dir.path().join("unused.sqlite"));
        options.results_database_schema = "main".into();
        options.cdm_source_name = "Eunomia".into();
        options.check_names = vec!["measurePersonCompleteness".into()];
        options.output_folder = dir.path().join("out");
        options.write_to_table = false;
        options.cdm_version = version_case["cdmVersion"].as_str().unwrap().into();
        let expected = &version_case["expected"];
        assert!(!expected["ok"].as_bool().unwrap());
        assert_eq!(
            format!("{:#}", runner::execute_dq_checks(&options).unwrap_err()),
            expected["error"].as_str().unwrap(),
            "cdmVersion {}",
            options.cdm_version
        );
    }
}

#[test]
fn default_arguments_match_execute_dq_checks_formals() {
    let expected = &case("default-arguments")["defaults"];
    let actual = serde_json::to_value(Options::default()).unwrap();
    // Deliberate divergence: upstream defaults to 1 worker; the port defaults to 8 (capped at
    // the CPU count), its fastest setting (docs/performance.md). Results do not depend on it.
    assert_eq!(expected["numThreads"], 1, "upstream numThreads default");
    assert_eq!(
        actual["numThreads"],
        data_quality_dashboard::model::default_num_threads()
    );
    for (name, expected) in expected.as_object().unwrap() {
        if name == "numThreads" {
            continue;
        }
        let actual = &actual[name];
        // R's c() is NULL; the Rust default is an empty vector.
        let expected = match expected {
            Value::Null => Value::Array(vec![]),
            Value::String(text) if actual.is_array() => Value::Array(vec![text.clone().into()]),
            other => other.clone(),
        };
        assert_json_equal(actual, &expected, name);
    }
    let inherited = &case("default-arguments")["inherited"];
    assert_eq!(inherited["vocabDatabaseSchema"], "cdmDatabaseSchema");
    assert_eq!(inherited["cohortDatabaseSchema"], "resultsDatabaseSchema");
    let options: Options =
        serde_json::from_str(r#"{"cdmDatabaseSchema":"cdm","resultsDatabaseSchema":"res"}"#)
            .unwrap();
    assert_eq!(options.vocab_database_schema, "cdm");
    assert_eq!(options.cohort_database_schema, "res");
}

/// SQL-only export through the library entry point, which (unlike `dqd run`) also accepts
/// `sql server`, the dialect of upstream's snapshot tests.
fn export_sql_only(options: &Options) {
    let catalog = Catalog::load(options).unwrap().prepared(options).unwrap();
    let checks = catalog.checks(options).unwrap();
    let mut engine = NativeSql::new();
    sql_only::export(&checks, &catalog, options, &mut engine).unwrap();
}

fn sql_only_case_name(dbms: &str, union: u32, incremental: bool) -> String {
    format!(
        "sql-only-{}-union{union}-incremental{}",
        dbms.replace(' ', "_"),
        u8::from(incremental)
    )
}

#[test]
fn sql_only_files_match_upstream_for_each_dialect_and_mode() {
    for dbms in ["sql server", "sqlite", "postgresql"] {
        for (union, incremental) in [(4, true), (1, true), (1, false)] {
            let name = sql_only_case_name(dbms, union, incremental);
            let case = case(&name);
            assert!(case["expected"]["ok"].as_bool().unwrap(), "{name} oracle");
            let dir = tempfile::tempdir().unwrap();
            let mut options: Options =
                serde_json::from_value(case["options"].clone()).expect("options");
            options.dbms = dbms.into();
            options.output_folder = dir.path().join("out");
            fs::create_dir_all(&options.output_folder).unwrap();
            if dbms == "sql server" {
                export_sql_only(&options);
            } else {
                runner::execute_dq_checks(&options).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            }
            let sql_files = |files: Vec<String>| -> BTreeSet<String> {
                files.into_iter().filter(|f| f.ends_with(".sql")).collect()
            };
            assert_eq!(
                sql_files(result_files(&options.output_folder)),
                sql_files(string_vector(&case["resultFiles"])),
                "{name} SQL files"
            );
            if dbms != "sql server" {
                // `dqd run` also creates the errors folder and log file, as executeDqChecks does.
                let all = |files: Vec<String>| files.into_iter().collect::<BTreeSet<_>>();
                assert_eq!(
                    all(result_files(&options.output_folder)),
                    all(string_vector(&case["resultFiles"])),
                    "{name} files"
                );
            }
            for (file, expected) in case["files"].as_object().unwrap() {
                let actual = fs::read_to_string(options.output_folder.join(file)).unwrap();
                assert_eq!(
                    actual,
                    platform_text(expected.as_str().unwrap()),
                    "{name}/{file}"
                );
            }
        }
    }
}

/// The oracle's `sql server` output reproduces upstream's own `_snaps/executeDqChecks.md`.
#[test]
fn sql_only_oracle_reproduces_upstream_snapshots() {
    let lines = string_vector(&case("snapshot-file")["lines"]);
    let mut snapshots: Vec<(String, String)> = Vec::new();
    let mut in_output = false;
    for line in &lines {
        if let Some(title) = line.strip_prefix("# ") {
            snapshots.push((title.to_owned(), String::new()));
            in_output = false;
        } else if line == "    Output" {
            in_output = true;
        } else if let (true, Some((_, body))) = (in_output, snapshots.last_mut()) {
            if let Some(code) = line.strip_prefix("      ") {
                body.push_str(code);
                body.push('\n');
            } else if line.is_empty() && !body.is_empty() {
                body.push('\n');
            }
        }
    }
    assert_eq!(snapshots.len(), 3, "upstream snapshot count");
    let normalize = |text: &str| -> Vec<String> {
        text.replace("\r\n", "\n")
            .replace('\r', "")
            .lines()
            .map(|line| line.trim_end().to_owned())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .skip_while(String::is_empty)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    for (title, body) in snapshots {
        let (union, incremental, file) = if title.contains("sqlOnlyUnionCount=4") {
            (4, true, "TABLE_measurePersonCompleteness.sql")
        } else if title.contains("sqlOnlyIncrementalInsert=TRUE") {
            (1, true, "TABLE_measurePersonCompleteness.sql")
        } else {
            (1, false, "measurePersonCompleteness.sql")
        };
        let oracle = case(&sql_only_case_name("sql server", union, incremental))["files"][file]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(normalize(&oracle), normalize(&body), "{title}");
    }
}

#[test]
fn incremental_insert_sql_is_valid_and_inserts_upstream_rows() {
    let case = case("incremental-insert-valid");
    let dir = tempfile::tempdir().unwrap();
    let db = fresh_gibleed(dir.path(), "incremental", &Value::Null);
    let options = case_options(case, &db, &dir.path().join("out"));
    runner::execute_dq_checks(&options).unwrap();
    let ddl = fs::read_to_string(options.output_folder.join("ddlDqdResults.sql")).unwrap();
    let check = fs::read_to_string(
        options
            .output_folder
            .join("TABLE_measurePersonCompleteness.sql"),
    )
    .unwrap();
    // SqlRender::readSql returns the file's exact characters.
    assert_eq!(
        ddl,
        platform_text(case["files"]["ddl"].as_str().unwrap()),
        "ddlDqdResults.sql"
    );
    assert_eq!(
        check,
        platform_text(case["files"]["check"].as_str().unwrap()),
        "TABLE_measurePersonCompleteness.sql"
    );
    assert!(case["executed"]["ok"].as_bool().unwrap());
    let mut engine = NativeSql::new();
    let mut connection = database::connect(&sqlite_options(&db)).unwrap();
    for sql in [&ddl, &check] {
        for statement in engine.split(sql).unwrap() {
            connection.execute(&statement).unwrap();
        }
    }
    let rows = connection.query("SELECT * FROM main.dqd_results;").unwrap();
    let expected = &case["rows"]["value"];
    assert_eq!(rows.len(), 16, "upstream expects 16 rows");
    let rows: Vec<Value> = rows.into_iter().map(Value::Object).collect();
    assert_json_equal(&Value::Array(rows), expected, "dqd_results rows");
}

/// Runs the `dqd` CLI on a scenario and returns its stderr, where the R warnings go.
fn cli_stderr(case: &Value, name: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let db = fresh_gibleed(dir.path(), name, &case["setup"]);
    let options = case_options(case, &db, &dir.path().join("out"));
    let config = dir.path().join("options.json");
    fs::write(&config, serde_json::to_vec(&options).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dqd"))
        .args(["run", "--config"])
        .arg(&config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn single_check_with_default_output_file_and_its_warnings() {
    let run = assert_run_case("single-check");
    assert!(run.value["CheckResults"].as_array().unwrap().len() > 1);
    let case = case("single-check");
    let stderr = cli_stderr(case, "single-check");
    for warning in string_vector(&case["expected"]["warnings"]) {
        assert!(
            stderr.contains(&warning),
            "missing warning {warning:?} in {stderr}"
        );
    }
}

#[test]
fn multiple_cdm_source_rows_warn_and_use_the_first_row() {
    assert_run_case("multiple-cdm-source");
    let case = case("multiple-cdm-source");
    let warnings = string_vector(&case["expected"]["warnings"]);
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("Missing check names"))
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("The cdm_source table has"))
    );
    let stderr = cli_stderr(case, "multiple-cdm-source");
    for warning in warnings {
        assert!(
            stderr.contains(&warning),
            "missing warning {warning:?} in {stderr}"
        );
    }
}

#[test]
fn convert_results_case_round_trip_matches_upstream() {
    let run = assert_run_case("convert-case");
    let conversions = &case("convert-case")["conversions"];
    let folder = &run.folder;
    let foo = &run.result_file;
    // camel -> camel: a warning, nothing written, the input returned.
    let same = results::convert_json_results_file_case(foo, false, None, None, "camel").unwrap();
    assert!(
        conversions["sameCamel"]["expected"]["ok"]
            .as_bool()
            .unwrap()
    );
    assert!(
        string_vector(&conversions["sameCamel"]["expected"]["warnings"])[0]
            .starts_with("File is already in camelcase!")
    );
    assert!(
        conversions["sameCamel"]["returnedEqualsInput"]
            .as_bool()
            .unwrap()
    );
    let mut input = read_json(foo);
    let mut same = same;
    strip_volatile(&mut input);
    strip_volatile(&mut same);
    assert_json_equal(&same, &input, "same-case conversion returns the input");
    // camel -> snake, written to snake.json.
    let snake = results::convert_json_results_file_case(
        foo,
        true,
        Some(folder),
        Some("snake.json"),
        "snake",
    )
    .unwrap();
    let expected = &conversions["snake"];
    assert_eq!(
        snake.as_object().unwrap().len() as u64,
        expected["length"].as_u64().unwrap()
    );
    // R row names (`_row`) are not among names(df).
    let names: BTreeSet<String> = snake["CheckResults"][0]
        .as_object()
        .unwrap()
        .keys()
        .filter(|name| *name != "_row")
        .cloned()
        .collect();
    let expected_names: BTreeSet<String> = string_vector(&expected["checkResultNames"])
        .into_iter()
        .collect();
    assert_eq!(names, expected_names, "snake CheckResults names");
    let mut written = read_json(&folder.join("snake.json"));
    let mut expected_written: Value =
        serde_json::from_str(expected["written"].as_str().unwrap()).unwrap();
    strip_volatile(&mut written);
    strip_volatile(&mut expected_written);
    assert_json_equal(&written, &expected_written, "snake.json");
    // snake -> snake: a warning only.
    assert!(
        string_vector(&conversions["sameSnake"]["expected"]["warnings"])[0]
            .starts_with("File is already in snakecase!")
    );
    results::convert_json_results_file_case(&folder.join("snake.json"), false, None, None, "snake")
        .unwrap();
    // snake -> camel with the default output name.
    let camel = results::convert_json_results_file_case(
        &folder.join("snake.json"),
        true,
        Some(folder),
        None,
        "camel",
    )
    .unwrap();
    let expected = &conversions["camel"];
    assert_eq!(
        camel.as_object().unwrap().len() as u64,
        expected["length"].as_u64().unwrap()
    );
    let camel_file = folder.join("snake_camel.json");
    assert!(camel_file.exists(), "default output name snake_camel.json");
    let mut written = read_json(&camel_file);
    let mut expected_written: Value =
        serde_json::from_str(expected["written"].as_str().unwrap()).unwrap();
    strip_volatile(&mut written);
    strip_volatile(&mut expected_written);
    assert_json_equal(&written, &expected_written, "snake_camel.json");
    assert!(conversions["roundTripEqual"].as_bool().unwrap());
    let mut original = read_json(foo);
    let mut camel = camel;
    strip_volatile(&mut original);
    strip_volatile(&mut camel);
    assert_json_equal(&camel, &original, "camel round trip");
    for (key, target, write) in [
        ("invalidCase", "foo", false),
        ("missingFolder", "camel", true),
    ] {
        let error = results::convert_json_results_file_case(
            Path::new("bar.json"),
            write,
            None,
            None,
            target,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            conversions[key]["error"].as_str().unwrap(),
            "{key}"
        );
    }
}

#[test]
fn re_evaluate_thresholds_matches_upstream() {
    let run = assert_run_case("single-check");
    let case = case("re-evaluate");
    assert!(case["expected"]["ok"].as_bool().unwrap());
    assert_eq!(case["type"], "list");
    let value =
        results::re_evaluate_thresholds(&run.result_file, &run.folder, "reEvaluated.txt", &{
            let mut options = run.options.clone();
            options.cdm_version = "5.3".into();
            options.table_check_threshold_loc = "default".into();
            options.field_check_threshold_loc = "default".into();
            options.concept_check_threshold_loc = "default".into();
            options
        })
        .unwrap();
    let mut expected: Value = serde_json::from_str(case["value"].as_str().unwrap()).unwrap();
    let mut actual = value;
    strip_volatile(&mut expected);
    strip_volatile(&mut actual);
    assert_json_equal(&actual, &expected, "reEvaluateThresholds value");
    let mut written = read_json(&run.folder.join("reEvaluated.txt"));
    let mut expected_written: Value =
        serde_json::from_str(case["written"].as_str().unwrap()).unwrap();
    strip_volatile(&mut written);
    strip_volatile(&mut expected_written);
    assert_json_equal(&written, &expected_written, "reEvaluated.txt");
}

#[test]
fn write_json_results_to_csv_and_tables_match_upstream() {
    let run = assert_run_case("single-check");
    let case = case("write-json-results");
    let csv = run.folder.join("written.csv");
    results::write_json_results_to_csv(&run.result_file, &csv, None, b',').unwrap();
    assert!(case["csv"]["expected"]["ok"].as_bool().unwrap());
    assert_eq!(
        csv_records(&fs::read_to_string(&csv).unwrap()),
        csv_records(case["csv"]["written"].as_str().unwrap()),
        "writeJsonResultsToCsv"
    );
    for (variant, expected) in case["tables"].as_object().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_gibleed(dir.path(), variant, &Value::Null);
        let mut connection = database::connect(&sqlite_options(&db)).unwrap();
        let mut engine = NativeSql::new();
        results::write_json_results_to_table(
            connection.as_mut(),
            &mut engine,
            &run.result_file,
            "sqlite",
            "main",
            expected["writeTableName"].as_str().unwrap(),
            None,
            expected["singleTable"] == true,
        )
        .unwrap();
        drop(connection);
        assert!(expected["expected"]["ok"].as_bool().unwrap(), "{variant}");
        assert_json_equal(
            &new_tables(&db),
            &object_or_empty(&expected["newTables"]),
            &format!("writeJsonResultsToTable {variant}"),
        );
    }
}

#[test]
fn write_db_results_to_json_matches_upstream() {
    let run = assert_run_case("write-db-results");
    let export = &case("write-db-results")["export"];
    assert!(export["expected"]["ok"].as_bool().unwrap());
    let mut connection = database::connect(&run.options).unwrap();
    let mut engine = NativeSql::new();
    results::write_db_results_to_json(
        connection.as_mut(),
        &mut engine,
        "sqlite",
        "main",
        "main",
        "dqd_db_results",
        &run.folder,
        "dq-result-test.json",
    )
    .unwrap();
    let mut actual = read_json(&run.folder.join("dq-result-test.json"));
    let mut expected = export["json"].clone();
    strip_volatile(&mut actual);
    strip_volatile(&mut expected);
    assert_json_equal(&actual, &expected, "dq-result-test.json");
    let count = connection
        .query("select count(*) as count from main.dqd_db_results;")
        .unwrap();
    assert_json_equal(
        &count[0]["count"],
        &export["count"]["value"][0]["count(*)"],
        "count(*)",
    );
    assert_eq!(
        export["lengthMatchesCount"],
        Value::Bool(
            actual["CheckResults"].as_array().map_or(0, Vec::len) as f64
                == count[0]["count"].as_f64().unwrap()
        )
    );
    let _ = run.db;
}
