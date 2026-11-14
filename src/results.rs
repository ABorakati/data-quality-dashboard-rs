use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::Path,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::{Map, Value, json};

pub use crate::database::{camel_case_to_snake_case, snake_case_to_camel_case};
use crate::{
    catalog::{Catalog, embedded_sql},
    database::Connection,
    evaluate,
    model::{Options, Row},
    sql::SqlEngine,
};

pub const DEFAULT_CSV_COLUMNS: &[&str] = &[
    "checkId",
    "failed",
    "passed",
    "isError",
    "notApplicable",
    "checkName",
    "checkDescription",
    "thresholdValue",
    "notesValue",
    "checkLevel",
    "category",
    "subcategory",
    "context",
    "checkLevel",
    "cdmTableName",
    "cdmFieldName",
    "conceptId",
    "unitConceptId",
    "numViolatedRows",
    "pctViolatedRows",
    "numDenominatorRows",
    "executionTime",
    "notApplicableReason",
    "error",
    "queryText",
];
const MANDATORY_CSV_COLUMNS: &[&str] = &["checkId", "failed", "passed", "isError", "notApplicable"];

fn read_results(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).with_context(|| format!("Reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("Parsing {}", path.display()))
}

pub fn check_results(value: &Value) -> Result<Vec<Row>> {
    let array = value
        .get("CheckResults")
        .and_then(Value::as_array)
        .context("Results must contain a CheckResults array")?;
    let rows: Vec<Row> = array
        .iter()
        .map(|value| {
            value
                .as_object()
                .cloned()
                .context("Every CheckResults entry must be an object")
        })
        .collect::<Result<_>>()?;
    Ok(normalize_dataframe_rows(rows))
}

/// CheckResults as upstream's `jsonlite::read_json` + `as.data.frame(cr)` + `plyr::rbind.fill`
/// readers see them (`reEvaluateThresholds`, `writeJsonResultsToCsv`, `writeJsonResultsToTable`).
/// `as.data.frame` applies `make.names`, so the `_row` field that jsonlite writes for R row names
/// becomes an ordinary column `X_row`.
fn data_frame_rows(value: &Value) -> Result<Vec<Row>> {
    Ok(check_results(value)?
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(name, value)| (r_make_name(name), value))
                .collect()
        })
        .collect())
}

/// R `make.names` for the result field names that can occur: a name that starts with `_` or a
/// digit is not syntactic and gains an `X` prefix. (CheckResults names are otherwise syntactic.)
fn r_make_name(name: String) -> String {
    if name.starts_with('_') || name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("X{name}")
    } else {
        name
    }
}

/// jsonlite simplifies mixed atomic columns to one R vector type. In particular,
/// a literal "NA" keeps a character column character while JSON null stays missing.
fn normalize_dataframe_rows(mut rows: Vec<Row>) -> Vec<Row> {
    let columns: HashSet<String> = rows.iter().flat_map(|row| row.keys().cloned()).collect();
    for column in columns {
        let has_string = rows
            .iter()
            .any(|row| row.get(&column).is_some_and(Value::is_string));
        let has_number = rows
            .iter()
            .any(|row| row.get(&column).is_some_and(Value::is_number));
        if has_string {
            for value in rows.iter_mut().filter_map(|row| row.get_mut(&column)) {
                match value {
                    Value::Number(number) => {
                        let text = number.to_string();
                        *value = Value::String(text);
                    }
                    Value::Bool(boolean) => {
                        *value = Value::String(if *boolean { "TRUE" } else { "FALSE" }.to_owned())
                    }
                    _ => {}
                }
            }
        } else if has_number {
            for value in rows.iter_mut().filter_map(|row| row.get_mut(&column)) {
                if let Value::Bool(boolean) = value {
                    let integer = u8::from(*boolean);
                    *value = Value::from(integer);
                }
            }
        }
    }
    rows
}

fn normalize_dataframe_section(value: &mut Value) -> Result<()> {
    let rows = value
        .as_array()
        .context("Results section must be an array of objects")?;
    let rows: Vec<Row> = rows
        .iter()
        .map(|value| {
            value
                .as_object()
                .cloned()
                .context("Results section entries must be objects")
        })
        .collect::<Result<_>>()?;
    *value = Value::Array(
        normalize_dataframe_rows(rows)
            .into_iter()
            .map(Value::Object)
            .collect(),
    );
    Ok(())
}

fn jsonlite_value(value: &Value) -> Value {
    match value {
        Value::Number(number) if number.is_f64() => {
            let Some(number) = number.as_f64() else {
                return value.clone();
            };
            let rounded = (number * 10_000.0).round_ties_even() / 10_000.0;
            if rounded.fract() == 0.0
                && rounded >= i64::MIN as f64
                && rounded < 9_223_372_036_854_775_808.0
            {
                Value::Number((rounded as i64).into())
            } else {
                serde_json::Number::from_f64(rounded)
                    .map(Value::Number)
                    .unwrap_or_else(|| value.clone())
            }
        }
        Value::Array(values) => Value::Array(values.iter().map(jsonlite_value).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), jsonlite_value(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn jsonlite_results_value(value: &Value) -> Value {
    let mut results = value.clone();
    if let Some(object) = results.as_object_mut() {
        for section in ["Metadata", "CheckResults"] {
            if let Some(section_value) = object.get_mut(section) {
                if let Value::Array(values) = section_value {
                    if let Some(rows) = values
                        .iter()
                        .map(|value| value.as_object().cloned())
                        .collect::<Option<Vec<_>>>()
                    {
                        *section_value = rows_to_json(&rows);
                    }
                }
            }
        }
        if let Some(Value::Object(overview)) = object.get_mut("Overview") {
            for value in overview.values_mut() {
                if !value.is_array() {
                    *value = Value::Array(vec![value.clone()]);
                }
            }
            let count_total = overview
                .get("countTotal")
                .and_then(|value| {
                    value
                        .as_array()
                        .and_then(|values| values.first())
                        .or(Some(value))
                })
                .and_then(Value::as_i64);
            if count_total == Some(0) {
                for name in ["percentPassed", "percentFailed"] {
                    if let Some(Value::Array(values)) = overview.get_mut(name)
                        && matches!(values.first(), Some(Value::Null))
                    {
                        values[0] = Value::String("NaN".to_owned());
                    }
                }
            }
        }
        for field in ["startTimestamp", "endTimestamp"] {
            if let Some(value) = object.get_mut(field) {
                if !value.is_array() {
                    *value = Value::Array(vec![value.clone()]);
                }
            }
        }
    }
    jsonlite_value(&results)
}

pub fn rows_to_json(rows: &[Row]) -> Value {
    rows_to_json_inner(rows, true)
}

fn rows_to_json_inner(rows: &[Row], omit_nulls: bool) -> Value {
    let mut seen = HashSet::new();
    let mut columns = Vec::new();
    for row in rows {
        for column in row.keys() {
            if seen.insert(column.as_str()) {
                columns.push(column.as_str());
            }
        }
    }
    let column_types: Vec<(&str, bool, bool)> = columns
        .into_iter()
        .map(|column| {
            let has_string = rows
                .iter()
                .any(|row| row.get(column).is_some_and(Value::is_string));
            let has_number = rows
                .iter()
                .any(|row| row.get(column).is_some_and(Value::is_number));
            (column, has_string, has_number)
        })
        .collect();
    Value::Array(
        rows.iter()
            .map(|row| {
                Value::Object(
                    column_types
                        .iter()
                        .filter_map(|(column, has_string, has_number)| {
                            let mut value = row.get(*column).cloned().unwrap_or(Value::Null);
                            if *has_string {
                                match value {
                                    Value::Number(number) => {
                                        value = Value::String(number.to_string());
                                    }
                                    Value::Bool(boolean) => {
                                        value = Value::String(
                                            if boolean { "TRUE" } else { "FALSE" }.to_owned(),
                                        );
                                    }
                                    _ => {}
                                }
                            } else if *has_number && let Value::Bool(boolean) = value {
                                value = Value::from(u8::from(boolean));
                            }
                            (!omit_nulls || !value.is_null()).then(|| ((*column).to_owned(), value))
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn serialize_dataframe_section(results: &mut Value, name: &str) -> Result<()> {
    let Some(section) = results.get_mut(name) else {
        bail!("no applicable method for 'rename_with' applied to an object of class \"NULL\"");
    };
    normalize_dataframe_section(section)
}

fn rename_rows(value: &mut Value, target_case: &str, preserve_check_id: bool) -> Result<()> {
    let array = value
        .as_array_mut()
        .context("Results section must be an array of objects")?;
    for value in array {
        let row = value
            .as_object_mut()
            .context("Results section entries must be objects")?;
        let previous = std::mem::take(row);
        for (key, value) in previous {
            // jsonlite::fromJSON turns `_row` back into R row names, which rename_with leaves
            // alone and toJSON writes again as `_row`.
            let renamed = if (preserve_check_id && key == "checkId") || key == "_row" {
                key
            } else if target_case == "camel" {
                snake_case_to_camel_case(&key)
            } else {
                camel_case_to_snake_case(&key).to_uppercase()
            };
            if row.contains_key(&renamed) {
                bail!("Converted results contain duplicate column '{renamed}'");
            }
            row.insert(renamed, value);
        }
    }
    Ok(())
}

/// Convert only Metadata and CheckResults column names. Top-level names and
/// Overview remain unchanged. An already-converted file returns before writing.
pub fn convert_json_results_file_case(
    json_file_path: &Path,
    write_to_file: bool,
    output_folder: Option<&Path>,
    output_file: Option<&str>,
    target_case: &str,
) -> Result<Value> {
    if target_case != "camel" && target_case != "snake" {
        bail!("targetCase must be either 'camel' or 'snake'.");
    }
    if write_to_file && output_folder.is_none() {
        bail!("You must specify an output folder if writing to file.");
    }
    let mut results = read_results(json_file_path)?;
    serialize_dataframe_section(&mut results, "Metadata")?;
    serialize_dataframe_section(&mut results, "CheckResults")?;
    let rows = check_results(&results)?;
    let marker = if target_case == "camel" {
        "numViolatedRows"
    } else {
        "NUM_VIOLATED_ROWS"
    };
    if rows.iter().any(|row| row.contains_key(marker)) {
        eprintln!("File is already in {target_case}case! No conversion will be performed.");
        return Ok(results);
    }
    rename_rows(
        results
            .get_mut("Metadata")
            .context("Results must contain Metadata")?,
        target_case,
        false,
    )?;
    rename_rows(
        results
            .get_mut("CheckResults")
            .context("Results must contain CheckResults")?,
        target_case,
        true,
    )?;
    if write_to_file {
        let generated;
        let filename = match output_file.filter(|file| !file.is_empty()) {
            Some(file) => file,
            None => {
                generated = format!(
                    "{}_{}.json",
                    json_file_path
                        .file_stem()
                        .context("Input file has no basename")?
                        .to_string_lossy(),
                    target_case
                );
                &generated
            }
        };
        write_results_to_json(
            &results,
            output_folder.expect("output folder checked above"),
            filename,
        )?;
    }
    Ok(results)
}

/// Normalize the browser input exactly where Shiny calls the conversion helper.
pub fn dashboard_results(json_file_path: &Path) -> Result<Value> {
    let mut results = read_results(json_file_path)?;
    serialize_dataframe_section(&mut results, "Metadata")?;
    serialize_dataframe_section(&mut results, "CheckResults")?;
    let rows = check_results(&results)?;
    if rows.iter().any(|row| row.contains_key("numViolatedRows")) {
        return Ok(jsonlite_results_value(&results));
    }
    let converted = convert_json_results_file_case(json_file_path, false, None, None, "camel")?;
    Ok(jsonlite_results_value(&converted))
}

pub fn write_results_to_json(value: &Value, output_folder: &Path, output_file: &str) -> Result<()> {
    let path = output_folder.join(output_file);
    let file = fs::File::create(&path).with_context(|| format!("Writing {}", path.display()))?;
    // serde_json writes token by token; unbuffered, that is one system call per token.
    let mut file = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut file, &jsonlite_results_value(value))?;
    #[cfg(windows)]
    file.write_all(b"\r\n")?;
    #[cfg(not(windows))]
    file.write_all(b"\n")?;
    file.flush()
        .with_context(|| format!("Writing {}", path.display()))?;
    Ok(())
}

fn csv_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_owned(),
        Value::Number(number) => match number.as_f64() {
            Some(double) if !number.is_i64() && !number.is_u64() && double.is_finite() => {
                r_double_text(double)
            }
            _ => crate::model::value_text(value),
        },
        other => other.to_string(),
    }
}

/// `write.csv` formats doubles with 15 significant digits, in fixed notation unless scientific is
/// strictly narrower (R's `formatReal`).
fn r_double_text(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let scientific = format!("{value:.14e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("scientific notation");
    let exponent: i32 = exponent.parse().expect("exponent");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let significant = digits.trim_end_matches('0').len().max(1);
    let negative = usize::from(value < 0.0);
    let decimals = (significant as i32 - exponent - 1).max(0) as usize;
    let left = if exponent >= 0 {
        exponent as usize + 1
    } else {
        1
    };
    let fixed_width = negative + left + if decimals > 0 { decimals + 1 } else { 0 };
    let exponent_digits = exponent.unsigned_abs().to_string().len().max(2);
    let scientific_width =
        negative + if significant > 1 { significant + 1 } else { 1 } + 2 + exponent_digits;
    if fixed_width <= scientific_width {
        format!("{value:.decimals$}")
    } else {
        let mantissa = format!("{:.*e}", significant - 1, value);
        let (mantissa, _) = mantissa.split_once('e').expect("scientific notation");
        format!(
            "{mantissa}e{}{:0>2}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        )
    }
}

/// R writes CSV files in text mode, which turns every `\n` into `\r\n` on Windows.
pub(crate) fn text_mode_newlines(text: String) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

fn quoted_r_text(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

/// CSV errors are logged rather than propagated, as in the exported R helper.
pub fn write_json_results_to_csv(
    json_path: &Path,
    csv_path: &Path,
    columns: Option<&[String]>,
    delimiter: u8,
) -> Result<()> {
    let result = (|| {
        let rows = data_frame_rows(&read_results(json_path)?)?;
        write_results_to_csv_inner(&rows, csv_path, columns, delimiter)
    })();
    if let Err(error) = result {
        eprintln!("Writing to CSV file failed: {error:#}");
    }
    Ok(())
}

/// Export normalized rows with upstream default/custom columns and status columns.
pub fn write_results_to_csv(
    rows: &[Row],
    csv_path: &Path,
    columns: Option<&[String]>,
    delimiter: u8,
) -> Result<()> {
    write_results_to_csv_inner(rows, csv_path, columns, delimiter)
}
fn write_results_to_csv_inner(
    rows: &[Row],
    csv_path: &Path,
    columns: Option<&[String]>,
    delimiter: u8,
) -> Result<()> {
    let default_columns: Vec<String>;
    let columns = match columns {
        Some(columns) => columns,
        None => {
            default_columns = DEFAULT_CSV_COLUMNS
                .iter()
                .map(|column| (*column).to_owned())
                .collect();
            &default_columns
        }
    };
    let mut included = HashSet::new();
    let columns: Vec<&str> = MANDATORY_CSV_COLUMNS
        .iter()
        .copied()
        .chain(columns.iter().map(String::as_str))
        .filter(|column| {
            included.insert(*column) && rows.iter().any(|row| row.contains_key(*column))
        })
        .collect();
    let separator = char::from(delimiter).to_string();
    let mut file = std::io::BufWriter::new(
        fs::File::create(csv_path).with_context(|| format!("Writing {}", csv_path.display()))?,
    );
    write!(
        file,
        "{}",
        text_mode_newlines(format!(
            "{}\n",
            columns
                .iter()
                .map(|column| quoted_r_text(column))
                .collect::<Vec<_>>()
                .join(&separator)
        ))
    )?;
    for row in rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|column| {
                let value = row.get(*column).unwrap_or(&Value::Null);
                let mut text = csv_text(value);
                if *column == "queryText" || *column == "error" {
                    text = text
                        .replace('\n', " ")
                        .replace('\r', " ")
                        .replace('\t', " ");
                }
                if value.is_string() {
                    quoted_r_text(&text)
                } else {
                    text
                }
            })
            .collect();
        write!(
            file,
            "{}",
            text_mode_newlines(format!("{}\n", cells.join(&separator)))
        )?;
    }
    file.flush()?;
    Ok(())
}

pub fn re_evaluate_thresholds(
    json_file_path: &Path,
    output_folder: &Path,
    output_file: &str,
    options: &Options,
) -> Result<Value> {
    let mut results = read_results(json_file_path)?;
    let mut rows = data_frame_rows(&results)?;
    for row in &mut rows {
        for field in ["cdmFieldName", "error", "conceptId", "unitConceptId"] {
            row.entry(field).or_insert(Value::Null);
        }
    }
    let mut catalog = Catalog::load(options)?;
    for row in catalog.fields.iter_mut().chain(catalog.concepts.iter_mut()) {
        if let Some(Value::String(field)) = row.get_mut("cdmFieldName") {
            *field = field.to_uppercase();
        }
    }
    evaluate::evaluate_thresholds(&mut rows, &catalog)?;
    results["Overview"] = Value::Object(
        evaluate::summarize(&rows)
            .into_iter()
            .map(|(name, value)| (name, Value::Array(vec![value])))
            .collect(),
    );
    results["CheckResults"] = rows_to_json(&rows);
    // R's jsonlite reader keeps metadata cells as length-one columns in this flow.
    if let Some(Value::Array(metadata_rows)) = results.get_mut("Metadata") {
        for row in metadata_rows {
            if let Some(row) = row.as_object_mut() {
                for value in row.values_mut() {
                    if !value.is_array() {
                        *value = Value::Array(vec![std::mem::replace(value, Value::Null)]);
                    }
                }
            }
        }
    }
    write_results_to_json(&results, output_folder, output_file)?;
    Ok(results)
}

fn translate_if_needed(engine: &mut dyn SqlEngine, sql: String, dbms: &str) -> Result<String> {
    if dbms == "sql server" {
        Ok(sql)
    } else {
        engine.translate(&sql, dbms)
    }
}

/// DDL errors propagate. Insert errors warn and stop insertion, matching R's
/// tryCatch around DatabaseConnector::insertTable after the DDL execution.
pub fn write_results_to_table(
    rows: &[Row],
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    dbms: &str,
    schema: &str,
    table: &str,
    cohort_id: Option<i64>,
) -> Result<()> {
    let table_name = match cohort_id {
        Some(id) => format!("{schema}.{table}_{id}"),
        None => format!("{schema}.{table}"),
    };
    write_results_to_table_named(rows, connection, engine, dbms, &table_name)
}

fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// RSQLite's column type for an R column holding these JSON values: character -> TEXT,
/// double -> REAL, integer -> INTEGER; an all-NA column is logical, stored as INTEGER.
fn r_sqlite_type(rows: &[Row], column: &str) -> &'static str {
    let values = || rows.iter().filter_map(|row| row.get(column));
    if values().any(Value::is_string) {
        "TEXT"
    } else if values().any(Value::is_f64) {
        "REAL"
    } else {
        "INTEGER"
    }
}

fn write_results_to_table_named(
    rows: &[Row],
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    dbms: &str,
    table_name: &str,
) -> Result<()> {
    let parameters =
        Row::from_iter([("tableName".to_owned(), Value::String(table_name.to_owned()))]);
    let ddl = engine.render(embedded_sql("result_dataframe_ddl.sql")?, &parameters)?;
    let ddl = translate_if_needed(engine, ddl, dbms)?;
    for statement in engine.split(&ddl)? {
        connection.execute(&statement)?;
    }
    let database_rows: Vec<Row> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(column, value)| {
                    let name = if column == "checkId" {
                        "checkid".to_owned()
                    } else {
                        camel_case_to_snake_case(column)
                    };
                    (name, value.clone())
                })
                .collect()
        })
        .collect();
    let insert_result = (|| {
        // DatabaseConnector writes SQLite and DuckDB tables through DBI's dbWriteTable, which
        // takes "schema.table" as one literal table name (in the default schema).
        let insert_table_name = if dbms == "sqlite" || dbms == "duckdb" {
            let literal_table_name = quote_sqlite_identifier(table_name);
            let source_table_name = table_name
                .split_once('.')
                .map(|(schema, table)| {
                    format!(
                        "{}.{}",
                        quote_sqlite_identifier(schema),
                        quote_sqlite_identifier(table)
                    )
                })
                .unwrap_or_else(|| quote_sqlite_identifier(table_name));
            // RSQLite's dbWriteTable(append = TRUE) creates the missing table from the data
            // frame's own columns, not from the DDL: a results JSON without some fields (all-NA
            // columns are absent there) or with `X_row` yields a different column set.
            let (schema, table) = table_name.split_once('.').unwrap_or(("main", table_name));
            let pragma = if dbms == "duckdb" {
                format!(
                    "SELECT column_name AS name, data_type AS type FROM information_schema.columns WHERE table_schema = '{}' AND table_name = '{}'",
                    schema.replace('\'', "''"),
                    table.replace('\'', "''")
                )
            } else {
                format!(
                    "PRAGMA {}.table_info({})",
                    quote_sqlite_identifier(schema),
                    quote_sqlite_identifier(table)
                )
            };
            let ddl_types: HashMap<String, String> = connection
                .query(&pragma)?
                .into_iter()
                .filter_map(|row| {
                    Some((
                        row.get("name")?.as_str()?.to_owned(),
                        row.get("type")?.as_str()?.to_owned(),
                    ))
                })
                .collect();
            let mut columns: Vec<&str> = Vec::new();
            for row in &database_rows {
                for column in row.keys() {
                    if !columns.contains(&column.as_str()) {
                        columns.push(column);
                    }
                }
            }
            let definitions: Vec<String> = columns
                .iter()
                .map(|column| {
                    let declared = ddl_types.get(*column).cloned().unwrap_or_else(|| {
                        let sqlite = r_sqlite_type(&database_rows, column);
                        // duckdb's dbDataType: character VARCHAR, double DOUBLE, integer INTEGER.
                        match (dbms, sqlite) {
                            ("duckdb", "TEXT") => "VARCHAR",
                            ("duckdb", "REAL") => "DOUBLE",
                            (_, other) => other,
                        }
                        .to_owned()
                    });
                    format!("{} {declared}", quote_sqlite_identifier(column))
                })
                .collect();
            if definitions.is_empty() {
                // No rows, hence no column names here (R would still have them): use the DDL's.
                connection.execute(&format!(
                    "CREATE TABLE IF NOT EXISTS {literal_table_name} AS SELECT * FROM {source_table_name} WHERE 1 = 0"
                ))?;
            } else {
                connection.execute(&format!(
                    "CREATE TABLE IF NOT EXISTS {literal_table_name} ({})",
                    definitions.join(", ")
                ))?;
            }
            literal_table_name
        } else {
            table_name.to_owned()
        };
        connection.insert_rows(&insert_table_name, &database_rows)
    })();
    if let Err(error) = insert_result {
        eprintln!("Writing table failed: {error:#}");
    }
    Ok(())
}

pub fn write_results_to_table_for_cohorts(
    rows: &[Row],
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    dbms: &str,
    schema: &str,
    table: &str,
    cohort_ids: &[Value],
) -> Result<()> {
    let table_name = if cohort_ids.is_empty() {
        format!("{schema}.{table}")
    } else {
        format!(
            "{schema}.{table}_{}",
            cohort_ids
                .iter()
                .map(crate::model::value_text)
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    write_results_to_table_named(rows, connection, engine, dbms, &table_name)
}

pub fn write_json_results_to_table(
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    json_file_path: &Path,
    dbms: &str,
    results_database_schema: &str,
    write_table_name: &str,
    cohort_definition_id: Option<i64>,
    single_table: bool,
) -> Result<()> {
    let rows = data_frame_rows(&read_results(json_file_path)?)?;
    if single_table {
        return write_results_to_table(
            &rows,
            connection,
            engine,
            dbms,
            results_database_schema,
            write_table_name,
            cohort_definition_id,
        );
    }
    eprintln!(
        "Writing to 3 separate tables by check level is deprecated and will be removed in a future version. Use singleTable = TRUE to write DQD results to a single table."
    );
    for (level, suffix) in [
        ("TABLE", "table"),
        ("FIELD", "field"),
        ("CONCEPT", "concept"),
    ] {
        let selected: Vec<Row> = rows
            .iter()
            .filter(|row| row.get("checkLevel").and_then(Value::as_str) == Some(level))
            .cloned()
            .collect();
        if !selected.is_empty() {
            write_results_to_table(
                &selected,
                connection,
                engine,
                dbms,
                results_database_schema,
                &format!("{write_table_name}_{suffix}"),
                cohort_definition_id,
            )?;
        }
    }
    Ok(())
}

pub fn write_json_results_to_table_for_cohorts(
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    json_file_path: &Path,
    dbms: &str,
    results_database_schema: &str,
    write_table_name: &str,
    cohort_ids: &[Value],
    single_table: bool,
) -> Result<()> {
    let rows = data_frame_rows(&read_results(json_file_path)?)?;
    let levels: &[(&str, &str)] = if single_table {
        &[("", "")]
    } else {
        eprintln!(
            "Writing to 3 separate tables by check level is deprecated and will be removed in a future version. Use singleTable = TRUE to write DQD results to a single table."
        );
        &[
            ("TABLE", "table"),
            ("FIELD", "field"),
            ("CONCEPT", "concept"),
        ]
    };
    for (level, suffix) in levels {
        let selected: Vec<Row> = if level.is_empty() {
            rows.clone()
        } else {
            rows.iter()
                .filter(|row| row.get("checkLevel").and_then(Value::as_str) == Some(level))
                .cloned()
                .collect()
        };
        if selected.is_empty() {
            continue;
        }
        let table = if suffix.is_empty() {
            write_table_name.to_owned()
        } else {
            format!("{write_table_name}_{suffix}")
        };
        write_results_to_table_for_cohorts(
            &selected,
            connection,
            engine,
            dbms,
            results_database_schema,
            &table,
            cohort_ids,
        )?;
    }
    Ok(())
}

fn query_results(
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    dbms: &str,
    sql: &str,
    parameters: Row,
) -> Result<Vec<Row>> {
    let rendered = engine.render(sql, &parameters)?;
    let translated = translate_if_needed(engine, rendered, dbms)?;
    let rows = connection.query(&translated)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(column, value)| (snake_case_to_camel_case(&column), value))
                .collect()
        })
        .collect())
}

pub fn write_db_results_to_json(
    connection: &mut dyn Connection,
    engine: &mut dyn SqlEngine,
    dbms: &str,
    results_database_schema: &str,
    cdm_database_schema: &str,
    write_table_name: &str,
    output_folder: &Path,
    output_file: &str,
) -> Result<()> {
    let metadata = query_results(
        connection,
        engine,
        dbms,
        "select * from @cdmDatabaseSchema.cdm_source;",
        Row::from_iter([(
            "cdmDatabaseSchema".to_owned(),
            Value::String(cdm_database_schema.to_owned()),
        )]),
    )?;
    let mut rows = query_results(
        connection,
        engine,
        dbms,
        "select * from @resultsDatabaseSchema.@writeTableName;",
        Row::from_iter([
            (
                "resultsDatabaseSchema".to_owned(),
                Value::String(results_database_schema.to_owned()),
            ),
            (
                "writeTableName".to_owned(),
                Value::String(write_table_name.to_owned()),
            ),
        ]),
    )?;
    for row in &mut rows {
        for column in ["error", "warning"] {
            if row.get(column).and_then(Value::as_str) == Some("") {
                row.insert(column.to_owned(), Value::Null);
            }
        }
        for (column, replacement) in [
            ("executionTime", "0 secs"),
            ("queryText", "[Generated via SQL Only]"),
        ] {
            if row.get(column).and_then(Value::as_str) == Some("") {
                row.insert(column.to_owned(), Value::String(replacement.to_owned()));
            }
        }
        if let Some(id) = row.remove("checkid") {
            row.insert("checkId".to_owned(), id);
        }
    }
    let results = json!({
        "startTimestamp": [Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        "endTimestamp": [Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()],
        "executionTime": ["0 secs"],
        "CheckResults": rows_to_json(&rows),
        "Metadata": rows_to_json(&metadata),
        "Overview": Value::Object(evaluate::summarize(&rows).into_iter().map(|(name, value)| (name, Value::Array(vec![value]))).collect::<Map<String, Value>>()),
    });
    write_results_to_json(&results, output_folder, output_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_data_frame_names_rename_the_jsonlite_row_names_field() {
        let value = json!({"CheckResults": [{"checkId": "x", "_row": "1.1"}]});
        let rows = data_frame_rows(&value).unwrap();
        assert_eq!(
            rows[0].keys().collect::<Vec<_>>(),
            ["checkId", "X_row"],
            "as.data.frame applies make.names in place"
        );
        assert_eq!(camel_case_to_snake_case("X_row"), "_x_row");
    }

    #[test]
    fn r_sqlite_types_follow_the_r_column_class() {
        let rows: Vec<Row> = vec![
            json!({"a": "text", "b": 1, "c": 0.5, "d": null})
                .as_object()
                .unwrap()
                .clone(),
            json!({"a": 2, "b": 3, "c": 1}).as_object().unwrap().clone(),
        ];
        assert_eq!(r_sqlite_type(&rows, "a"), "TEXT");
        assert_eq!(r_sqlite_type(&rows, "b"), "INTEGER");
        assert_eq!(r_sqlite_type(&rows, "c"), "REAL");
        assert_eq!(r_sqlite_type(&rows, "d"), "INTEGER");
    }
}
