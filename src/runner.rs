use anyhow::{Context, Result, bail};
use chrono::Local;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

use crate::{
    catalog::Catalog,
    database::{self, Connection},
    evaluate,
    model::{Check, Options, Row, value_text},
    results,
    sql::{NativeSql, SqlEngine, parameters},
};

const DQD_VERSION: &str = env!("CARGO_PKG_VERSION");
static NULL: Value = Value::Null;

fn get<'a>(row: &'a Row, name: &str) -> &'a Value {
    row.get(name).unwrap_or(&NULL)
}
fn text(row: &Row, name: &str) -> String {
    value_text(get(row, name))
}
fn json_rows(rows: &[Row]) -> Value {
    results::rows_to_json(rows)
}

fn log_line(folder: &Path, source_name: &str, message: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join(format!("log_DqDashboard_{source_name}.txt")))
    {
        use std::io::Write;
        let _ = writeln!(
            file,
            "{} INFO {message}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        );
    }
}

fn render_sql(
    engine: &mut dyn SqlEngine,
    template: &str,
    params: &Row,
    dbms: &str,
) -> Result<String> {
    let rendered = engine.render(template, params)?;
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!("SqlRender warning: {}", warnings.join("; "));
    }
    let translated = if dbms.eq_ignore_ascii_case("sql server") {
        rendered
    } else {
        engine.translate(&rendered, dbms)?
    };
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!("SqlRender warning: {}", warnings.join("; "));
    }
    Ok(translated)
}

fn metadata(options: &Options) -> Result<Vec<Row>> {
    let mut engine = NativeSql::new();
    let params = parameters([("cdmDatabaseSchema", json!(options.cdm_database_schema))]);
    let sql = render_sql(
        &mut engine,
        "select * from @cdmDatabaseSchema.cdm_source;",
        &params,
        &options.dbms,
    )?;
    let mut connection = database::connect(options)?;
    let mut rows = connection.query(&sql).map_err(|error| {
        // DatabaseConnector reports a failed metadata query in the current directory.
        let path = std::env::current_dir()
            .unwrap_or_default()
            .join("errorReportSql.txt");
        anyhow::anyhow!(write_error_report(
            &path,
            &sql,
            &options.dbms,
            &format!("{error:#}")
        ))
    })?;
    let warnings = connection.take_warnings();
    if let Some(warning) = warnings.first() {
        eprintln!("{warning}");
    }
    database::normalize_columns_to_camel(&mut rows);
    if rows.is_empty() {
        bail!("Please populate the cdm_source table before executing data quality checks.");
    }
    if rows.len() > 1 {
        eprintln!(
            "The cdm_source table has more than 1 row. A single row from this table has been selected to populate DQD metadata."
        );
    }
    rows.truncate(1);
    rows[0].insert("dqdVersion".into(), Value::String(DQD_VERSION.to_owned()));
    Ok(rows)
}

fn record_result(
    check: &Check,
    query: &str,
    result: Option<&Row>,
    warning: Option<String>,
    error: Option<String>,
    execution_time: Option<String>,
    engine: &mut dyn SqlEngine,
) -> Result<Row> {
    let mut params = check.parameters.clone();
    for value in params.values_mut() {
        if !value.is_null() {
            *value = Value::String(value_text(value).to_uppercase());
        }
    }
    params.insert("warnOnMissingParameters".into(), Value::Bool(false));
    let description_template = get(&check.description, "checkDescription")
        .as_str()
        .context("Missing checkDescription")?;
    let description = engine.render(description_template, &params)?;
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while rendering check description: {}",
            warnings.join("; ")
        );
    }
    let mut row = Row::new();
    row.insert(
        "numViolatedRows".into(),
        result
            .and_then(|r| r.get("numViolatedRows"))
            .cloned()
            .unwrap_or(Value::Null),
    );
    row.insert(
        "pctViolatedRows".into(),
        result
            .and_then(|r| r.get("pctViolatedRows"))
            .cloned()
            .unwrap_or(Value::Null),
    );
    row.insert(
        "numDenominatorRows".into(),
        result
            .and_then(|r| r.get("numDenominatorRows"))
            .cloned()
            .unwrap_or(Value::Null),
    );
    row.insert(
        "executionTime".into(),
        execution_time.map(Value::String).unwrap_or(Value::Null),
    );
    row.insert("queryText".into(), Value::String(query.to_owned()));
    for key in ["checkName", "checkLevel", "sqlFile"] {
        row.insert(key.into(), get(&check.description, key).clone());
    }
    row.insert("checkDescription".into(), Value::String(description));
    for key in ["cdmTableName", "cdmFieldName", "conceptId", "unitConceptId"] {
        row.insert(key.into(), params.get(key).cloned().unwrap_or(Value::Null));
    }
    for (source, target) in [
        ("kahnCategory", "category"),
        ("kahnSubcategory", "subcategory"),
        ("kahnContext", "context"),
    ] {
        row.insert(target.into(), get(&check.description, source).clone());
    }
    row.insert(
        "warning".into(),
        warning.map(Value::String).unwrap_or(Value::Null),
    );
    row.insert(
        "error".into(),
        error.map(Value::String).unwrap_or(Value::Null),
    );
    row.insert(
        "checkId".into(),
        Value::String(crate::model::check_id(
            get(&check.description, "checkLevel"),
            get(&check.description, "checkName"),
            &params,
        )),
    );
    Ok(row)
}

/// The driver's message as DatabaseConnector shows it: sqlite-jdbc's inner detail for SQLite, and
/// the driver message without the `[SQLSTATE=..; vendorCode=..]` suffix for other databases.
fn native_error<'a>(dbms: &str, error: &'a str) -> &'a str {
    if dbms.eq_ignore_ascii_case("sqlite") {
        error
            .rsplit_once(" (")
            .and_then(|(_, detail)| {
                detail
                    .split_once(") [SQLSTATE=")
                    .map(|(message, _)| message)
            })
            .unwrap_or(error)
    } else {
        match error.rsplit_once(" [SQLSTATE=") {
            Some((message, rest)) if rest.ends_with(']') && rest.contains("; vendorCode=") => {
                message
            }
            _ => error,
        }
    }
}

/// Writes a DatabaseConnector-style SQL error report and returns the error text R records.
fn write_error_report(path: &Path, query: &str, dbms: &str, error: &str) -> String {
    let native_error = native_error(dbms, error);
    let _ = fs::write(
        path,
        format!("DBMS:\n{dbms}\n\nError:\n{native_error}\n\nSQL:\n{query}\n"),
    );
    let report_path = path.to_string_lossy().replace('\\', "/");
    format!(
        "Error executing SQL:\n{native_error}\nAn error report has been created at  {report_path}"
    )
}

fn error_report(folder: &Path, check: &Check, query: &str, dbms: &str, error: &str) -> String {
    let path = folder.join("errors").join(format!(
        "{}_{}_{}_{}.txt",
        text(&check.description, "checkLevel"),
        text(&check.description, "checkName"),
        text(&check.parameters, "cdmTableName"),
        text(&check.parameters, "cdmFieldName")
    ));
    write_error_report(&path, query, dbms, error)
}

fn execute_one(
    options: &Options,
    check: &Check,
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
) -> Result<Row> {
    let params = parameters(
        [
            ("dbms", json!(options.dbms)),
            ("sqlFilename", get(&check.description, "sqlFile").clone()),
            ("packageName", json!("DataQualityDashboard")),
            ("warnOnMissingParameters", json!(false)),
            ("cdmDatabaseSchema", json!(options.cdm_database_schema)),
            (
                "cohortDatabaseSchema",
                json!(options.cohort_database_schema),
            ),
            ("cohortTableName", json!(options.cohort_table_name)),
            (
                "cohortDefinitionId",
                Value::Array(options.cohort_definition_id.clone()),
            ),
            ("vocabDatabaseSchema", json!(options.vocab_database_schema)),
            ("cohort", json!(options.cohort_enabled())),
        ]
        .into_iter()
        .chain(
            check
                .parameters
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone())),
        ),
    );
    let filename = Path::new(&text(&check.description, "sqlFile"))
        .file_name()
        .and_then(|s| s.to_str())
        .context("Invalid check SQL file")?
        .to_owned();
    let sql = render_sql(
        engine,
        crate::catalog::embedded_sql(&filename)?,
        &params,
        &options.dbms,
    )?;
    let started = Instant::now();
    match connection.query(&sql) {
        Ok(mut rows) => {
            let warnings = connection.take_warnings();
            if let Some(warning) = warnings.first() {
                return record_result(check, &sql, None, Some(warning.clone()), None, None, engine);
            }
            database::normalize_columns_to_camel(&mut rows);
            let execution_time = format!("{:.6} secs", started.elapsed().as_secs_f64());
            record_result(
                check,
                &sql,
                rows.first(),
                None,
                None,
                Some(execution_time),
                engine,
            )
        }
        Err(error) => {
            connection.take_warnings();
            record_result(
                check,
                &sql,
                None,
                None,
                Some(format!("{error:#}")),
                None,
                engine,
            )
        }
    }
}

fn execute_checks(
    options: &Options,
    checks: &[Check],
    workers: usize,
    error_folder: &Path,
    source_name: &str,
) -> Result<Vec<Row>> {
    let next = AtomicUsize::new(0);
    let output = Mutex::new(Vec::<(usize, Row)>::with_capacity(checks.len()));
    let worker_error = Mutex::new(None::<String>);
    thread::scope(|scope| -> Result<()> {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            handles.push(scope.spawn(|| -> Result<()> {
                let mut engine = NativeSql::new();
                let mut connection = database::connect(options)?;
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= checks.len() { break; }
                    let check = &checks[index];
                    match execute_one(options, check, connection.as_mut(), &mut engine) {
                        Ok(mut row) => {
                            if let Some(error) = row.get("error").and_then(Value::as_str) {
                                let query = row
                                    .get("queryText")
                                    .and_then(Value::as_str)
                                    .unwrap_or("");
                                let error = error_report(
                                    error_folder,
                                    check,
                                    query,
                                    &options.dbms,
                                    error,
                                );
                                row.insert("error".into(), Value::String(error.clone()));
                                log_line(error_folder.parent().unwrap_or(error_folder), source_name, &format!("[Level: {}] [Check: {}] [CDM Table: {}] [CDM Field: {}] {error}", text(&check.description, "checkLevel"), text(&check.description, "checkName"), text(&check.parameters, "cdmTableName"), text(&check.parameters, "cdmFieldName")));
                            }
                            if let Some(warning) = row.get("warning").and_then(Value::as_str) {
                                let message = format!("[Level: {}] [Check: {}] [CDM Table: {}] [CDM Field: {}] {warning}", text(&check.description, "checkLevel"), text(&check.description, "checkName"), text(&check.parameters, "cdmTableName"), text(&check.parameters, "cdmFieldName"));
                                eprintln!("{message}");
                                log_line(error_folder.parent().unwrap_or(error_folder), source_name, &message);
                            }
                            output.lock().expect("runner result lock poisoned").push((index, row));
                        }
                        Err(error) => {
                            *worker_error.lock().expect("runner error lock poisoned") = Some(format!("{}: {error:#}", text(&check.description, "checkName")));
                            break;
                        }
                    }
                }
                Ok(())
            }));
        }
        for handle in handles {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("DQD worker panicked"))??;
        }
        Ok(())
    })?;
    if let Some(error) = worker_error
        .into_inner()
        .expect("runner error lock poisoned")
    {
        bail!("{error}");
    }
    let mut output = output.into_inner().expect("runner result lock poisoned");
    output.sort_by_key(|(index, _)| *index);
    Ok(output.into_iter().map(|(_, row)| row).collect())
}

fn warn_selected(descriptions: &[Row]) {
    let names: Vec<&str> = descriptions
        .iter()
        .filter_map(|row| row.get("checkName").and_then(Value::as_str))
        .collect();
    if !["cdmTable", "cdmField", "measureValueCompleteness"]
        .iter()
        .all(|name| names.contains(name))
    {
        eprintln!(
            "Missing check names to calculate the 'Not Applicable' status. Results will show pass/fail without Not Applicable status."
        );
    }
    for (name, warning) in [
        (
            "plausibleDuringLife",
            "DEPRECATION WARNING - The plausibleDuringLife check has been reimplemented with the plausibleBeforeDeath check.",
        ),
        (
            "plausibleTemporalAfter",
            "DEPRECATION WARNING - The plausibleTemporalAfter check has been reimplemented with the plausibleAfterBirth and plausibleStartBeforeEnd checks.",
        ),
        (
            "plausibleGender",
            "DEPRECATION WARNING - The plausibleGender check has been reimplemented with the plausibleGenderUseDescendants check.",
        ),
    ] {
        if names.contains(&name) {
            eprintln!("{warning}");
        }
    }
}

fn default_filename(metadata: &[Row], end: chrono::DateTime<Local>) -> String {
    let abbreviation = metadata
        .first()
        .and_then(|row| row.get("cdmSourceAbbreviation"))
        .map(value_text)
        .unwrap_or_else(|| "NA".into())
        .to_lowercase();
    format!("{abbreviation}-{}.json", end.format("%Y%m%d%H%M%S"))
}

pub fn list_dq_checks(options: &Options) -> Result<Value> {
    Ok(Catalog::load(options)?.as_list_value())
}

pub fn execute_dq_checks(options: &Options) -> Result<Value> {
    options.validate()?;
    let metadata = if options.sql_only {
        vec![parameters([
            ("dqdVersion", Value::String(DQD_VERSION.into())),
            (
                "cdmSourceName",
                Value::String(options.cdm_source_name.clone()),
            ),
        ])]
    } else {
        metadata(options)?
    };
    fs::create_dir_all(&options.output_folder)?;
    let error_folder = options.output_folder.join("errors");
    if error_folder.exists() {
        fs::remove_dir_all(&error_folder)?;
    }
    fs::create_dir_all(&error_folder)?;
    let start = Local::now();
    let raw = Catalog::load(options)?;
    let catalog = raw.prepared(options)?;
    let descriptions = catalog.selected_descriptions(options)?;
    warn_selected(&descriptions);
    let checks = catalog.checks_prepared(options)?;
    for description in &descriptions {
        let name = text(description, "checkName");
        let level = text(description, "checkLevel");
        if !checks.iter().any(|check| {
            text(&check.description, "checkName") == name
                && text(&check.description, "checkLevel") == level
        }) {
            eprintln!(
                "Warning: Evaluation resulted in no checks: {}Checks %>% dplyr::filter({})",
                level.to_lowercase(),
                text(description, "evaluationFilter")
            );
        }
    }
    if options.sql_only {
        let mut engine = NativeSql::new();
        crate::sql_only::export(&checks, &catalog, options, &mut engine)?;
        return Ok(Value::Null);
    }
    let mut check_results = execute_checks(
        options,
        &checks,
        options.thread_count()?,
        &options.output_folder,
        &options.cdm_source_name,
    )?;
    evaluate::evaluate_thresholds(&mut check_results, &catalog)?;
    let overview = evaluate::summarize(&check_results);
    let end = Local::now();
    let seconds = (end.timestamp_micros() - start.timestamp_micros()) as f64 / 1_000_000.0;
    let mut output = serde_json::Map::new();
    output.insert(
        "startTimestamp".into(),
        json!([start.format("%Y-%m-%d %H:%M:%S").to_string()]),
    );
    output.insert(
        "endTimestamp".into(),
        json!([end.format("%Y-%m-%d %H:%M:%S").to_string()]),
    );
    output.insert(
        "executionTime".into(),
        json!([format!("{} secs", seconds.round() as i64)]),
    );
    output.insert("executionTimeSeconds".into(), json!([seconds]));
    output.insert("CheckResults".into(), json_rows(&check_results));
    output.insert("Metadata".into(), json_rows(&metadata));
    output.insert(
        "Overview".into(),
        Value::Object(
            overview
                .into_iter()
                .map(|(key, value)| (key, Value::Array(vec![value])))
                .collect(),
        ),
    );
    let output_file = if options.output_file.is_empty() {
        default_filename(&metadata, end)
    } else {
        options.output_file.clone()
    };
    results::write_results_to_json(
        &Value::Object(output.clone()),
        &options.output_folder,
        &output_file,
    )?;
    log_line(
        &options.output_folder,
        &options.cdm_source_name,
        "Execution Complete",
    );
    if options.write_to_table {
        let mut connection = database::connect(options)?;
        let mut engine = NativeSql::new();
        if let Err(error) = results::write_results_to_table_for_cohorts(
            &check_results,
            connection.as_mut(),
            &mut engine,
            &options.dbms,
            &options.results_database_schema,
            &options.write_table_name,
            &options.cohort_definition_id,
        ) {
            log_line(
                &options.output_folder,
                &options.cdm_source_name,
                &format!("Writing table failed: {error:#}"),
            );
        }
    }
    if options.write_to_csv {
        if options.csv_file.is_empty() {
            // Upstream derives the name from basename(allResults$outputFile), but allResults has no
            // outputFile element, so executeDqChecks fails here, after the JSON and table writes.
            bail!("a character vector argument expected");
        }
        let path: PathBuf = options.output_folder.join(&options.csv_file);
        if let Err(error) = results::write_results_to_csv(&check_results, &path, None, b',') {
            log_line(
                &options.output_folder,
                &options.cdm_source_name,
                &format!("Writing to CSV file failed: {error:#}"),
            );
        }
    }
    Ok(Value::Object(output))
}
