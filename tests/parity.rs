use data_quality_dashboard::{
    catalog::Catalog,
    database, evaluate,
    model::{ConnectionConfig, Options, Row},
    results, runner,
    sql::{NativeSql, SqlEngine},
    sql_only,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
mod common;
use common::*;
fn opts(version: &str) -> Options {
    Options {
        cdm_version: version.into(),
        cdm_database_schema: "main".into(),
        results_database_schema: "main".into(),
        vocab_database_schema: "main".into(),
        cohort_database_schema: "main".into(),
        cdm_source_name: "Parity".into(),
        ..Options::default()
    }
}
fn parse_row(value: &Value) -> Row {
    value.as_object().expect("oracle row object").clone()
}
fn assert_catalog_matches(actual: &Value, expected: &Value, path: &str) {
    let actual = actual.as_object().expect("listDqChecks object");
    let names = [
        "checkDescriptions",
        "tableChecks",
        "fieldChecks",
        "conceptChecks",
    ];
    assert_eq!(
        actual.keys().map(String::as_str).collect::<Vec<_>>(),
        names,
        "{path} public list names"
    );
    let aliases = [
        ("checkDescriptions", "descriptions"),
        ("tableChecks", "tables"),
        ("fieldChecks", "fields"),
        ("conceptChecks", "concepts"),
    ];
    for (public_name, legacy_name) in aliases {
        let expected_value = expected
            .get(public_name)
            .or_else(|| expected.get(legacy_name))
            .unwrap_or_else(|| panic!("{path} oracle lacks {public_name}"));
        assert_json_equal(
            &actual[public_name],
            expected_value,
            &format!("{path}.{public_name}"),
        );
    }
}

fn oracle_check_id(description: &Value, parameters: &Value) -> String {
    let components = [
        description.get("checkLevel"),
        description.get("checkName"),
        parameters.get("cdmTableName"),
        parameters.get("cdmFieldName"),
        parameters.get("conceptId"),
        parameters.get("unitConceptId"),
    ];
    components
        .into_iter()
        .flatten()
        .filter_map(|value| match value {
            Value::Null => None,
            Value::String(value) => Some(value.clone()),
            Value::Bool(value) => Some(if *value { "TRUE" } else { "FALSE" }.to_owned()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .map(|value| value.replace(' ', ""))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("_")
        .to_lowercase()
}

#[test]
fn list_checks_matches_upstream_raw_catalog_for_all_cdm_versions() {
    for version in ["5.2", "5.3", "5.4", "5.5"] {
        let expected = fixture(&format!("catalog/{version}.json"));
        let actual = Catalog::load(&opts(version)).unwrap().as_list_value();
        assert_catalog_matches(&actual, &expected["catalog"], version);
        assert_catalog_matches(
            &runner::list_dq_checks(&opts(version)).unwrap(),
            &expected["catalog"],
            &format!("listDqChecks/{version}"),
        );
    }
}

fn set_variant(options: &mut Options, value: &Value) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (name, value) in object {
        match name.as_str() {
            "cdmVersion" => options.cdm_version = value.as_str().unwrap().into(),
            "tablesToExclude" => options.tables_to_exclude = string_vector(value),
            "checkLevels" => options.check_levels = string_vector(value),
            "checkSeverity" => options.check_severity = string_vector(value),
            "checkNames" => options.check_names = string_vector(value),
            _ => panic!("unmapped oracle option {name}"),
        }
    }
}

#[test]
fn check_enumeration_matches_upstream_order_parameters_and_recorded_ids() {
    for version in ["5.2", "5.3", "5.4", "5.5"] {
        for variant in [
            "default",
            "all_tables",
            "field_fatal",
            "table_convention",
            "concept_characterization",
            "named",
            "lowercase_exclusions",
            "no_checks",
            "no_severity",
        ] {
            let expected = fixture(&format!("enumeration/{version}-{variant}.json"));
            let mut options = opts(version);
            set_variant(&mut options, &expected["options"]);
            let actual = Catalog::load(&options).unwrap().checks(&options);
            if !expected["ok"].as_bool().unwrap() {
                assert_eq!(
                    actual.unwrap_err().to_string(),
                    expected["error"].as_str().unwrap(),
                    "{version}/{variant}"
                );
                continue;
            }
            let actual = actual.unwrap();
            let rows = expected["checks"].as_array().unwrap();
            assert_eq!(actual.len(), rows.len(), "{version}/{variant}");
            for (i, (check, reference)) in actual.iter().zip(rows).enumerate() {
                assert_json_equal(
                    &serde_json::to_value(&check.description).unwrap(),
                    &reference["description"],
                    &format!("{version}/{variant}[{i}].description"),
                );
                assert_json_equal(
                    &serde_json::to_value(&check.parameters).unwrap(),
                    &reference["parameters"],
                    &format!("{version}/{variant}[{i}].parameters"),
                );
                let id = data_quality_dashboard::model::check_id(
                    check.description.get("checkLevel").unwrap(),
                    check.description.get("checkName").unwrap(),
                    &check.parameters,
                );
                // The current observer fixture was captured with a matrix, not
                // the named vector passed to upstream .recordResult.
                assert_eq!(
                    id,
                    oracle_check_id(&reference["description"], &reference["parameters"]),
                    "{version}/{variant}[{i}].checkId"
                );
            }
        }
    }
}

#[test]
fn threshold_and_applicability_states_match_upstream() {
    let cases = fixture("evaluation.json");
    for case in cases.as_array().unwrap() {
        let mut rows: Vec<Row> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(parse_row)
            .collect();
        let catalog = if case["name"] == "threshold-zero-matches"
            || case["name"] == "threshold-no-header-empty"
        {
            let columns: Vec<String> = case["catalog"]["headers"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect();
            let directory =
                std::env::temp_dir().join(format!("dqd-empty-control-{}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            let control = directory.join("fields.csv");
            fs::write(&control, format!("{}\n", columns.join(","))).unwrap();
            let mut options = opts("5.3");
            options.field_check_threshold_loc = control.to_string_lossy().into_owned();
            let catalog = Catalog::load(&options).unwrap();
            fs::remove_dir_all(directory).unwrap();
            if case["name"] == "threshold-zero-matches" {
                assert!(
                    catalog
                        .columns("FIELD")
                        .contains(&"edgeThreshold".to_owned())
                );
                assert!(catalog.columns("FIELD").contains(&"edgeNotes".to_owned()));
            } else {
                assert!(
                    !catalog
                        .columns("FIELD")
                        .contains(&"edgeThreshold".to_owned())
                );
            }
            catalog
        } else {
            Catalog::from_rows(
                vec![],
                case["catalog"]["tables"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(parse_row)
                    .collect(),
                case["catalog"]["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(parse_row)
                    .collect(),
                case["catalog"]["concepts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(parse_row)
                    .collect(),
            )
        };
        let actual = match case["operation"].as_str().unwrap() {
            "evaluate" => evaluate::evaluate_thresholds(&mut rows, &catalog),
            "applicability" => evaluate::calculate_not_applicable(&mut rows),
            operation => panic!("unknown evaluation operation {operation}"),
        };
        let expected = &case["expected"];
        if case["name"] == "threshold-zero-matches" {
            assert_eq!(expected["error"], "replacement has 0 rows, data has 1");
        }
        if !expected["ok"].as_bool().unwrap() {
            assert_eq!(
                actual.unwrap_err().to_string(),
                expected["error"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            continue;
        }
        actual.unwrap_or_else(|e| panic!("{} unexpectedly errored: {e:#}", case["name"]));
        let expected_rows = expected["value"].as_array().expect("oracle result rows");
        let actual_value = Value::Array(rows.into_iter().map(Value::Object).collect());
        assert_json_equal(
            &actual_value,
            &Value::Array(expected_rows.clone()),
            case["name"].as_str().unwrap(),
        );
    }
}

#[test]
fn overview_counts_and_rounding_match_upstream() {
    for case in fixture("summary.json").as_array().unwrap() {
        let rows: Vec<Row> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(parse_row)
            .collect();
        let actual = evaluate::summarize(&rows);
        assert_json_equal(
            &Value::Object(actual),
            &case["expected"],
            case["name"].as_str().unwrap(),
        );
    }
}

#[test]
fn every_check_template_renders_translates_and_splits_on_each_supported_dialect() {
    const SUPPORTED: [&str; 3] = ["sql server", "sqlite", "postgresql"];
    let manifest = fixture("manifest.json");
    assert_eq!(
        manifest["upstreamCommit"],
        "b9d2a9b4a4736f7ea81415d974c72119b6f0bd11"
    );
    assert_eq!(manifest["dialects"].as_array().unwrap().len(), 16);
    assert_eq!(manifest["sqlRenderVersion"], "1.19.7");
    let cases = fixture("render.json");
    assert_eq!(cases.as_array().unwrap().len(), 27 * 16 * 2);
    let mut engine = NativeSql::new();
    let mut compared = 0;
    for case in cases
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| SUPPORTED.contains(&case["dialect"].as_str().unwrap()))
    {
        compared += 1;
        let name = format!(
            "{}/{}/{}/cohort={}",
            case["version"], case["description"]["checkName"], case["dialect"], case["cohort"]
        );
        let template = case["template"].as_str().unwrap();
        let parameters = case["parameters"].as_object().unwrap().clone();
        let rendered = engine.render(template, &parameters);
        if !case["rendered"]["ok"].as_bool().unwrap() {
            assert!(rendered.is_err(), "{name}");
            continue;
        }
        let rendered = rendered.unwrap_or_else(|e| panic!("render {name}: {e:#}"));
        assert_eq!(
            rendered,
            case["rendered"]["value"].as_str().unwrap(),
            "{name} render"
        );
        let dialect = case["dialect"].as_str().unwrap();
        let translated = if dialect == "sql server" {
            Ok(rendered.clone())
        } else {
            engine.translate(&rendered, dialect)
        };
        if !case["translated"]["ok"].as_bool().unwrap() {
            assert!(translated.is_err(), "{name}");
            continue;
        }
        let translated = translated.unwrap_or_else(|e| panic!("translate {name}: {e:#}"));
        assert_eq!(
            translated,
            case["translated"]["value"].as_str().unwrap(),
            "{name} translated SQL"
        );
        let statements = engine
            .split(&translated)
            .unwrap_or_else(|e| panic!("split {name}: {e:#}"));
        assert_json_equal(
            &serde_json::to_value(statements).unwrap(),
            &case["statements"]["value"],
            &format!("{name} statements"),
        );
    }
    assert_eq!(compared, 27 * SUPPORTED.len() * 2);
}

#[test]
fn result_export_modes_match_upstream_json_csv_and_sql_files() {
    let root = oracle();
    let cases = fixture("exports/expected.json");
    let tmp = std::env::temp_dir().join(format!("dqd-export-parity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if name.ends_with("-csv") {
            let input = root.join("exports/rows.json");
            let output = tmp.join(format!("{name}.csv"));
            let columns: Vec<String> = if name == "selected-columns" {
                vec![
                    "queryText".into(),
                    "error".into(),
                    "failed".into(),
                    "missingColumn".into(),
                ]
            } else {
                vec![]
            };
            results::write_json_results_to_csv(
                &input,
                &output,
                (name == "selected-columns").then_some(columns.as_slice()),
                if name == "semicolon-csv" { b';' } else { b',' },
            )
            .unwrap();
            if case["lines"].is_null() {
                assert!(!output.exists(), "{name} wrote a file after oracle failure");
                continue;
            }
            let actual = fs::read_to_string(output).unwrap();
            let expected = string_vector(&case["lines"]);
            assert_eq!(
                actual.lines().collect::<Vec<_>>(),
                expected.iter().map(String::as_str).collect::<Vec<_>>(),
                "{name}"
            );
        } else if name.starts_with("convert-") {
            let target = name.strip_prefix("convert-").unwrap();
            let output = tmp.join(format!("{name}.json"));
            let converted = results::convert_json_results_file_case(
                &root.join("exports/rows.json"),
                true,
                Some(&tmp),
                Some(&format!("{name}.json")),
                target,
            );
            if !case["expected"]["ok"].as_bool().unwrap() {
                assert_eq!(
                    converted.unwrap_err().to_string(),
                    case["expected"]["error"].as_str().unwrap(),
                    "{name}"
                );
                assert!(
                    !output.exists(),
                    "{name} wrote a file after conversion failed"
                );
                continue;
            }
            let value = converted.unwrap();
            let expected = &case["expected"]["value"];
            assert_json_equal(&value, expected, name);
            if case["bytes"].is_null() {
                assert!(
                    !output.exists(),
                    "{name} wrote a file after a no-op conversion"
                );
                continue;
            }
            let written = fs::read_to_string(output).unwrap();
            let expected_lines = string_vector(&case["bytes"]);
            assert_eq!(
                written.lines().collect::<Vec<_>>(),
                expected_lines
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{name} output lines"
            );
        } else if name == "re-evaluate-thresholds" {
            let mut options = opts("5.3");
            options.output_folder = tmp.clone();
            let output = tmp.join("reevaluated.json");
            let value = results::re_evaluate_thresholds(
                &root.join("exports/re-evaluate.json"),
                &tmp,
                "reevaluated.json",
                &options,
            )
            .unwrap();
            let expected: Value =
                serde_json::from_str(case["expectedValue"].as_str().unwrap()).unwrap();
            assert_json_equal(&value, &expected, name);
            let written = fs::read(output).unwrap();
            assert_eq!(
                written,
                platform_text(case["expectedWritten"].as_str().unwrap()).as_bytes(),
                "{name} exact jsonlite bytes"
            );
        }
    }
    for mode in ["sql-simple", "sql-incremental"] {
        let mut options = opts("5.3");
        options.check_names = vec!["cdmTable".into(), "isRequired".into()];
        options.check_levels = vec!["TABLE".into(), "FIELD".into()];
        options.tables_to_exclude.clear();
        options.sql_only = true;
        options.sql_only_incremental_insert = mode == "sql-incremental";
        options.sql_only_union_count = 2.0;
        options.output_folder = tmp.join(mode);
        options.dbms = "sqlite".into();
        fs::create_dir_all(&options.output_folder).unwrap();
        let catalog = Catalog::load(&options).unwrap().prepared(&options).unwrap();
        let checks = catalog.checks(&options).unwrap();
        let mut engine = NativeSql::new();
        sql_only::export(&checks, &catalog, &options, &mut engine).unwrap();
        let actual = sql_files(&options.output_folder);
        let oracle_case = cases
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == mode)
            .expect("upstream SQL export case");
        assert!(
            oracle_case["expected"]["ok"].as_bool().unwrap(),
            "{} oracle error: {}",
            mode,
            oracle_case["expected"]["error"]
        );
        let expected = &oracle_case["files"];
        assert!(
            expected.as_object().is_some_and(|files| !files.is_empty()),
            "{mode} oracle SQL files are empty"
        );
        assert!(
            actual.as_object().is_some_and(|files| !files.is_empty()),
            "{mode} Rust SQL files are empty"
        );
        assert_json_equal(&actual, expected, mode);
    }
    let database_result = root.join("database/clean/results/results.json");

    let converted =
        results::convert_json_results_file_case(&database_result, false, None, None, "snake")
            .unwrap();
    assert!(
        converted["Metadata"][0]
            .as_object()
            .unwrap()
            .contains_key("CDM_SOURCE_NAME")
    );
    let check = converted["CheckResults"][0].as_object().unwrap();
    assert!(check.contains_key("NUM_VIOLATED_ROWS"));
    assert!(check.contains_key("checkId"));
    let _ = fs::remove_dir_all(tmp);
}

fn sql_files(folder: &Path) -> Value {
    let mut values = serde_json::Map::new();
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "sql") {
            values.insert(
                path.file_name().unwrap().to_string_lossy().into_owned(),
                // The oracle holds R readLines output, which drops the `\r` of `\r\n` and
                // also the extra `\r` of the `\r\r\n` that R writes on Windows.
                json!(
                    fs::read_to_string(path)
                        .unwrap()
                        .lines()
                        .map(|line| line.trim_end_matches('\r'))
                        .collect::<Vec<_>>()
                ),
            );
        }
    }
    Value::Object(values)
}

/// Runs one scenario with the Rust runner and compares result JSON and result CSV to the oracle case.
fn assert_run_matches_case(name: &str, case: &Value, options: &Options) {
    let mut actual = run_and_read_results_file(name, options);
    let mut expected = case["result"].clone();
    strip_volatile(&mut actual);
    strip_volatile(&mut expected);
    assert_json_equal(&actual, &expected, name);
    let csv_path = options.output_folder.join(&options.csv_file);
    let actual_csv = fs::read_to_string(&csv_path).unwrap();
    let expected_csv = string_vector(&case["resultCsv"]).join("\n");
    assert_eq!(
        csv_records(&actual_csv),
        csv_records(&expected_csv),
        "{name} CSV"
    );
}

/// Compares the written results table and the database-to-JSON export with the oracle case.
fn assert_written_table_matches_case(
    case: &Value,
    options: &Options,
    select_all: &str,
    results_schema: &str,
    cdm_schema: &str,
) {
    let mut connection = database::connect(options).unwrap();
    let rows = connection.query(select_all).unwrap();
    let mut actual_rows = Value::Array(rows.into_iter().map(Value::Object).collect());
    let mut expected_rows = case["tableRows"].clone();
    for value in [&mut actual_rows, &mut expected_rows] {
        if let Some(rows) = value.as_array_mut() {
            for row in rows {
                if let Some(row) = row.as_object_mut() {
                    row.remove("execution_time");
                }
            }
        }
    }
    assert_json_equal(&actual_rows, &expected_rows, "write-to-table");
    let mut engine = NativeSql::new();
    let json_folder = options.output_folder.join("database-json");
    fs::create_dir_all(&json_folder).unwrap();
    results::write_db_results_to_json(
        connection.as_mut(),
        &mut engine,
        &options.dbms,
        results_schema,
        cdm_schema,
        "dqdashboard_results",
        &json_folder,
        "from-database.json",
    )
    .unwrap();
    let mut actual: Value =
        serde_json::from_slice(&fs::read(json_folder.join("from-database.json")).unwrap()).unwrap();
    let mut expected = case["databaseJson"].clone();
    for value in [&mut actual, &mut expected] {
        strip_volatile(value);
    }
    assert_json_equal(&actual, &expected, "write-db-results-to-json");
    assert!(
        case["databaseJsonExpected"]["ok"].as_bool().unwrap(),
        "upstream database-to-JSON export failed: {}",
        case["databaseJsonExpected"]["error"]
    );
}

const DATABASE_SCENARIOS: [&str; 8] = [
    "clean",
    "violating",
    "missing-table",
    "missing-field",
    "empty-table",
    "unpopulated-field",
    "cohort",
    "clean-write-table",
];

#[test]
fn real_sqlite_database_runs_match_upstream_check_rows_and_metadata() {
    let cases = fixture("database/expected.json");
    for name in DATABASE_SCENARIOS {
        let case = &cases[name];
        let source = fixture_path(case["sqlite"].as_str().unwrap());
        let temporary = tempfile::tempdir().unwrap();
        let working = temporary.path().join("cdm.sqlite");
        fs::copy(&source, &working).unwrap();
        let mut options: Options = case_options(&case["options"]);
        options.connection.sqlite_path = Some(working);
        options.output_folder = temporary.path().join("results");
        options.dbms = "sqlite".into();
        assert_run_matches_case(name, case, &options);
        if name == "clean-write-table" {
            assert_written_table_matches_case(
                case,
                &options,
                "SELECT * FROM \"main.dqdashboard_results\"",
                "main",
                "main",
            );
        }
    }
}

/// DatabaseConnector reports a failed query in `errorReportSql.txt` in the current directory, and
/// the error text embeds that absolute path. Compare it with the directory abstracted.
fn normalize_cwd_report_path(error: &str) -> String {
    let marker = "An error report has been created at  ";
    match error.split_once(marker) {
        Some((prefix, path)) if path.replace('\\', "/").ends_with("/errorReportSql.txt") => {
            format!("{prefix}{marker}<cwd>/errorReportSql.txt")
        }
        _ => error.to_owned(),
    }
}

/// Options for the `cdm_source` preflight scenarios, which the oracle ran with base options only.
fn preflight_options(schema: &str, dbms: &str, folder: &Path) -> Options {
    Options {
        cdm_database_schema: schema.into(),
        results_database_schema: schema.into(),
        vocab_database_schema: schema.into(),
        cohort_database_schema: schema.into(),
        cdm_source_name: "Parity".into(),
        cdm_version: "5.3".into(),
        output_folder: folder.join("results"),
        dbms: dbms.into(),
        ..Options::default()
    }
}

fn assert_preflight_failure(name: &str, case: &Value, options: &Options) {
    assert!(!case["expected"]["ok"].as_bool().unwrap(), "{name}");
    let expected = normalize_cwd_report_path(case["expected"]["error"].as_str().unwrap());
    let actual = format!("{:#}", runner::execute_dq_checks(options).unwrap_err());
    let _ = fs::remove_file("errorReportSql.txt");
    assert_eq!(normalize_cwd_report_path(&actual), expected, "{name}");
}

#[test]
fn real_sqlite_cdm_source_preflight_failures_match_upstream() {
    let cases = fixture("database/expected.json");
    for name in ["empty-cdm-source", "missing-cdm-source"] {
        let case = &cases[name];
        let temporary = tempfile::tempdir().unwrap();
        let working = temporary.path().join("cdm.sqlite");
        fs::copy(fixture_path(case["sqlite"].as_str().unwrap()), &working).unwrap();
        let mut options = preflight_options("main", "sqlite", temporary.path());
        options.connection.sqlite_path = Some(working);
        assert_preflight_failure(name, case, &options);
    }
}

/// Port of the private PostgreSQL oracle cluster, from `DQD_PG_PORT`. `None` skips PostgreSQL tests.
fn pg_port() -> Option<u16> {
    let value = std::env::var("DQD_PG_PORT").ok()?;
    Some(
        value
            .parse()
            .unwrap_or_else(|e| panic!("DQD_PG_PORT must be a port number: {e}")),
    )
}

/// PostgreSQL scenarios rebuilt on the oracle cluster; the cluster uses trust authentication.
fn pg_connection(case: &Value, port: u16) -> ConnectionConfig {
    ConnectionConfig {
        server: Some(case["server"].as_str().unwrap().to_owned()),
        port: Some(port),
        user: Some("postgres".into()),
        password: Some(String::new()),
        ..ConnectionConfig::default()
    }
}

#[test]
fn real_postgresql_database_runs_match_upstream_check_rows_and_metadata() {
    let Some(port) = pg_port() else {
        eprintln!("skipping PostgreSQL parity: DQD_PG_PORT is not set");
        return;
    };
    let cases = fixture("postgres/expected.json");
    for name in DATABASE_SCENARIOS {
        let case = &cases[name];
        assert_eq!(case["scenario"], name);
        let schema = case["schema"].as_str().unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let mut options: Options = case_options(&case["options"]);
        options.connection = pg_connection(case, port);
        options.output_folder = temporary.path().join("results");
        options.dbms = "postgresql".into();
        // Rebuild exactly the SQL the oracle ran, so the schema holds the oracle's input data.
        database::connect(&options)
            .unwrap()
            .execute(case["sql"].as_str().unwrap())
            .unwrap_or_else(|e| panic!("{name} setup: {e:#}"));
        assert_run_matches_case(name, case, &options);
        if name == "clean-write-table" {
            assert_written_table_matches_case(
                case,
                &options,
                &format!("SELECT * FROM {schema}.dqdashboard_results"),
                schema,
                schema,
            );
        }
    }
}

#[test]
fn postgresql_cdm_source_preflight_failures_match_upstream() {
    let Some(port) = pg_port() else {
        eprintln!("skipping PostgreSQL parity: DQD_PG_PORT is not set");
        return;
    };
    let cases = fixture("postgres/expected.json");
    for name in ["empty-cdm-source", "missing-cdm-source"] {
        let case = &cases[name];
        let temporary = tempfile::tempdir().unwrap();
        let mut options = preflight_options(
            case["schema"].as_str().unwrap(),
            "postgresql",
            temporary.path(),
        );
        options.connection = pg_connection(case, port);
        database::connect(&options)
            .unwrap()
            .execute(case["sql"].as_str().unwrap())
            .unwrap_or_else(|e| panic!("{name} setup: {e:#}"));
        assert_preflight_failure(name, case, &options);
    }
}

/// Compares the result CSV a GiBleed run wrote with the oracle's CSV of the same name.
fn assert_gibleed_csv_matches(name: &str, options: &Options, oracle_folder: &Path) {
    let actual_csv = fs::read_to_string(options.output_folder.join(&options.csv_file)).unwrap();
    let expected_csv = fs::read_to_string(oracle_folder.join(&options.csv_file)).unwrap();
    assert_eq!(
        csv_records(&actual_csv),
        csv_records(&expected_csv),
        "{name} CSV"
    );
}

/// Compares a full GiBleed run, check by check and metadata, ignoring only timings and timestamps.
fn assert_gibleed_run_matches(name: &str, options: &Options, expected: &Value) {
    let actual = run_and_read_results_file(name, options);
    assert!(
        !expected["CheckResults"].as_array().unwrap().is_empty(),
        "{name}: empty oracle results"
    );
    assert_results_match(name, &actual, expected);
}

#[test]
fn gibleed_sqlite_run_matches_upstream_results_and_metadata() {
    let folder = oracle().join("gibleed/sqlite");
    let read = |file: &str| -> Value {
        let path = folder.join(file);
        serde_json::from_slice(
            &fs::read(&path)
                .unwrap_or_else(|e| panic!("missing oracle fixture {}: {e}", path.display())),
        )
        .unwrap_or_else(|e| panic!("invalid oracle fixture {}: {e}", path.display()))
    };
    let expected = read("results.json");
    let template: Options = serde_json::from_value(read("options.json")).expect("decode options");
    // The oracle's own thread count, then a parallel run: results must not depend on scheduling.
    let mut thread_counts = vec![template.num_threads, 4.0];
    thread_counts.dedup();
    for threads in thread_counts {
        let temporary = tempfile::tempdir().unwrap();
        let working = temporary.path().join("gibleed.sqlite");
        fs::copy(folder.join("gibleed-copy.sqlite"), &working).unwrap();
        let mut options = template.clone();
        options.dbms = "sqlite".into();
        options.connection.sqlite_path = Some(working);
        options.output_folder = temporary.path().join("results");
        options.num_threads = threads;
        let name = format!("gibleed-sqlite-threads-{threads}");
        assert_gibleed_run_matches(&name, &options, &expected);
        assert_gibleed_csv_matches(&name, &options, &folder);
    }
}

#[test]
fn gibleed_postgresql_run_matches_upstream_results_and_metadata() {
    let Some(port) = pg_port() else {
        eprintln!("skipping PostgreSQL parity: DQD_PG_PORT is not set");
        return;
    };
    let folder = oracle().join("postgres/gibleed");
    let read = |file: &str| -> Value {
        let path = folder.join(file);
        serde_json::from_slice(
            &fs::read(&path)
                .unwrap_or_else(|e| panic!("missing oracle fixture {}: {e}", path.display())),
        )
        .unwrap_or_else(|e| panic!("invalid oracle fixture {}: {e}", path.display()))
    };
    let expected = read("results.json");
    let connection = read("timing.json")["connection"].clone();
    assert_eq!(connection["dbms"], "postgresql");
    // The oracle loaded schema `gibleed` on the cluster once; it is read-only input here.
    let mut options: Options =
        serde_json::from_value(read("options.json")).expect("decode options");
    assert_eq!(options.cdm_database_schema, connection["schema"]);
    let temporary = tempfile::tempdir().unwrap();
    options.dbms = "postgresql".into();
    options.connection = pg_connection(&connection, port);
    options.output_folder = temporary.path().join("results");
    assert_gibleed_run_matches("gibleed-postgresql", &options, &expected);
    assert_gibleed_csv_matches("gibleed-postgresql", &options, &folder);
}

#[test]
fn empty_csv_file_fails_after_json_and_table_writes_like_upstream() {
    let case = fixture("database/csv-default/expected.json");
    assert!(!case["expected"]["ok"].as_bool().unwrap());
    let temporary = tempfile::tempdir().unwrap();
    let working = temporary.path().join("cdm.sqlite");
    fs::copy(fixture_path(case["sqlite"].as_str().unwrap()), &working).unwrap();
    let mut options: Options = case_options(&case["options"]);
    assert!(options.write_to_csv && options.csv_file.is_empty() && options.write_to_table);
    options.connection.sqlite_path = Some(working);
    options.output_folder = temporary.path().join("results");
    options.dbms = "sqlite".into();
    let error = runner::execute_dq_checks(&options).unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        case["expected"]["error"].as_str().unwrap()
    );
    // Side effects that happened before the failure.
    assert!(case["resultFileWritten"].as_bool().unwrap());
    let mut actual: Value =
        serde_json::from_slice(&fs::read(options.output_folder.join("results.json")).unwrap())
            .unwrap();
    let mut expected = case["result"].clone();
    strip_volatile(&mut actual);
    strip_volatile(&mut expected);
    assert_json_equal(&actual, &expected, "csv-default results.json");
    let mut connection = database::connect(&options).unwrap();
    let rows = connection
        .query("SELECT * FROM \"main.dqdashboard_results\"")
        .unwrap();
    let mut actual_rows = Value::Array(rows.into_iter().map(Value::Object).collect());
    let mut expected_rows = case["tableRows"].clone();
    for value in [&mut actual_rows, &mut expected_rows] {
        for row in value.as_array_mut().unwrap() {
            row.as_object_mut().unwrap().remove("execution_time");
        }
    }
    assert_json_equal(&actual_rows, &expected_rows, "csv-default table rows");
    let mut files: Vec<String> = fs::read_dir(&options.output_folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, string_vector(&case["resultFolderFiles"]));
    assert!(
        string_vector(&case["csvFilesWritten"]).is_empty()
            && !files.iter().any(|name| name.ends_with(".csv")),
        "no CSV file may be written"
    );
}
