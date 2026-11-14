use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use crate::{
    catalog::{Catalog, embedded_sql},
    model::{Check, Options, Row, check_id, value_text},
    sql::{SqlEngine, parameters},
};

fn field<'a>(row: &'a Row, name: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    row.get(name).unwrap_or(&NULL)
}

fn string(row: &Row, name: &str) -> String {
    value_text(field(row, name))
}

fn scalar_params(check: &Check, options: &Options) -> Row {
    let mut values = vec![
        ("dbms", json!(options.dbms)),
        ("sqlFilename", field(&check.description, "sqlFile").clone()),
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
    ];
    values.extend(
        check
            .parameters
            .iter()
            .map(|(key, value)| (key.as_str(), value.clone())),
    );
    parameters(values)
}

fn rendered_check(check: &Check, options: &Options, engine: &mut dyn SqlEngine) -> Result<String> {
    let sql_file = string(&check.description, "sqlFile");
    let filename = Path::new(&sql_file)
        .file_name()
        .and_then(|name| name.to_str())
        .context("Invalid SQL filename in check catalog")?;
    let template = embedded_sql(filename)?;
    let sql = engine.render(template, &scalar_params(check, options))?;
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while rendering {filename}: {}",
            warnings.join("; ")
        );
    }
    let translated = if options.dbms.eq_ignore_ascii_case("sql server") {
        sql
    } else {
        engine.translate(&sql, &options.dbms)?
    };
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while translating {filename}: {}",
            warnings.join("; ")
        );
    }
    Ok(translated)
}

fn recorded_params(check: &Check) -> Row {
    let mut row = check.parameters.clone();
    for name in ["cdmTableName", "cdmFieldName", "conceptId", "unitConceptId"] {
        if let Some(value) = row.get_mut(name)
            && !value.is_null()
        {
            *value = Value::String(value_text(value));
        }
    }
    row
}

fn threshold(check: &Check, catalog: &Catalog) -> Result<Value> {
    let name = format!("{}Threshold", string(&check.description, "checkName"));
    let level = string(&check.description, "checkLevel");
    let rows = match level.as_str() {
        "TABLE" => &catalog.tables,
        "FIELD" => &catalog.fields,
        "CONCEPT" => &catalog.concepts,
        _ => bail!("Unknown check level {level}"),
    };
    let Some(value) = rows
        .iter()
        .find(|row| {
            row.get(&name).is_some()
                && ["cdmTableName", "cdmFieldName", "conceptId", "unitConceptId"]
                    .iter()
                    .all(|key| {
                        !check.parameters.contains_key(*key)
                            || row.get(*key) == check.parameters.get(*key)
                    })
        })
        .and_then(|row| row.get(&name))
        .cloned()
    else {
        return Ok(Value::from(0));
    };
    if value.is_null() {
        Ok(Value::from(0))
    } else {
        Ok(value)
    }
}

fn record_shell(
    check: &Check,
    query: &str,
    catalog: &Catalog,
    engine: &mut dyn SqlEngine,
) -> Result<Row> {
    let mut description_params = check.parameters.clone();
    for value in description_params.values_mut() {
        if !value.is_null() {
            *value = Value::String(value_text(value).to_uppercase());
        }
    }
    description_params.insert("warnOnMissingParameters".into(), Value::Bool(false));
    let description_template = field(&check.description, "checkDescription")
        .as_str()
        .context("Missing checkDescription")?;
    let description = engine.render(description_template, &description_params)?;
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while rendering check description: {}",
            warnings.join("; ")
        );
    }
    let params = recorded_params(check);
    let mut row = Row::new();
    row.insert("queryText".into(), Value::String(query.to_owned()));
    for key in [
        "checkName",
        "checkLevel",
        "sqlFile",
        "kahnCategory",
        "kahnSubcategory",
        "kahnContext",
    ] {
        let out = match key {
            "kahnCategory" => "category",
            "kahnSubcategory" => "subcategory",
            "kahnContext" => "context",
            _ => key,
        };
        row.insert(out.into(), field(&check.description, key).clone());
    }
    for key in ["cdmTableName", "cdmFieldName", "conceptId", "unitConceptId"] {
        row.insert(key.into(), params.get(key).cloned().unwrap_or(Value::Null));
    }
    row.insert("checkDescription".into(), Value::String(description));
    row.insert(
        "checkId".into(),
        Value::String(check_id(
            field(&check.description, "checkLevel"),
            field(&check.description, "checkName"),
            &params,
        )),
    );
    row.insert("thresholdValue".into(), threshold(check, catalog)?);
    Ok(row)
}

fn cte_for(
    check: &Check,
    catalog: &Catalog,
    options: &Options,
    engine: &mut dyn SqlEngine,
) -> Result<String> {
    let sql = rendered_check(check, options, engine)?;
    let mut shell = record_shell(check, &sql, catalog, engine)?;
    let q = sql.replace(';', "");
    shell.insert("queryText".into(), Value::String(q.clone()));
    let description = string(&shell, "checkDescription")
        .replace(['\t', '\r', '\n'], " ")
        .replace('\'', "''");
    let mut params = shell.clone();
    params.insert("queryText".into(), Value::String(q));
    params.insert(
        "renderedCheckDescription".into(),
        Value::String(description),
    );
    params.insert(
        "thresholdValue".into(),
        shell
            .get("thresholdValue")
            .cloned()
            .unwrap_or(Value::from(0)),
    );
    let template = embedded_sql("sqlOnly/cte_sql_for_results_table.sql")?;
    let rendered = engine.render(template, &params)?;
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while rendering incremental check: {}",
            warnings.join("; ")
        );
    }
    let translated = if options.dbms.eq_ignore_ascii_case("sql server") {
        rendered
    } else {
        engine.translate(&rendered, &options.dbms)?
    };
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while translating incremental check: {}",
            warnings.join("; ")
        );
    }
    Ok(translated)
}

/// Writes `text` and a newline as R's `write()` and `SqlRender::writeSql` do: through a text-mode
/// connection, which turns every `\n` into `\r\n` on Windows (so a template's own `\r\n` becomes
/// `\r\r\n` there, as in upstream's output).
fn write_r_line(file: &mut fs::File, text: &str) -> Result<()> {
    file.write_all(crate::results::text_mode_newlines(format!("{text}\n")).as_bytes())?;
    Ok(())
}

fn export_simple(checks: &[Check], options: &Options, engine: &mut dyn SqlEngine) -> Result<()> {
    for check in checks {
        let sql = rendered_check(check, options, engine)?;
        let filename = format!("{}.sql", string(&check.description, "checkName"));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(options.output_folder.join(filename))?;
        write_r_line(&mut file, &sql)?;
    }
    Ok(())
}

fn export_incremental(
    checks: &[Check],
    catalog: &Catalog,
    options: &Options,
    engine: &mut dyn SqlEngine,
) -> Result<()> {
    let descriptions = catalog.selected_descriptions(options)?;
    for description in descriptions {
        let level = string(&description, "checkLevel");
        let name = string(&description, "checkName");
        let matching: Vec<&Check> = checks
            .iter()
            .filter(|check| {
                string(&check.description, "checkName") == name
                    && string(&check.description, "checkLevel") == level
            })
            .collect();
        if matching.is_empty() {
            continue;
        }
        let mut queries = Vec::with_capacity(matching.len());
        for check in matching {
            queries.push(cte_for(check, catalog, options, engine)?);
        }
        let path = options.output_folder.join(format!("{level}_{name}.sql"));
        if path.exists() {
            fs::remove_file(&path).with_context(|| format!("Removing {}", path.display()))?;
        }
        for batch in options.sql_union_batches(queries.len())? {
            let union = queries[batch].join(" UNION ALL ");
            let params = parameters([
                (
                    "resultsDatabaseSchema",
                    json!(options.results_database_schema),
                ),
                ("tableName", json!(options.write_table_name)),
                ("queryText", Value::String(union)),
            ]);
            let template = embedded_sql("sqlOnly/insert_ctes_into_result_table.sql")?;
            let rendered = engine.render(template, &params)?;
            let warnings = engine.take_warnings();
            if !warnings.is_empty() {
                bail!(
                    "SqlRender warning while rendering SQL batch: {}",
                    warnings.join("; ")
                );
            }
            let sql = if options.dbms.eq_ignore_ascii_case("sql server") {
                rendered
            } else {
                engine.translate(&rendered, &options.dbms)?
            };
            let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
            write_r_line(&mut file, &sql)?;
        }
    }
    Ok(())
}

pub fn export(
    checks: &[Check],
    catalog: &Catalog,
    options: &Options,
    engine: &mut dyn SqlEngine,
) -> Result<()> {
    fs::create_dir_all(&options.output_folder)?;
    if options.sql_only_incremental_insert {
        export_incremental(checks, catalog, options, engine)?;
    } else {
        export_simple(checks, options, engine)?;
    }
    let table = format!(
        "{}.{}",
        options.results_database_schema, options.write_table_name
    );
    let ddl_params = parameters([("tableName", Value::String(table))]);
    let template = embedded_sql("result_dataframe_ddl.sql")?;
    let rendered = engine.render(template, &ddl_params)?;
    let warnings = engine.take_warnings();
    if !warnings.is_empty() {
        bail!(
            "SqlRender warning while rendering DQD result DDL: {}",
            warnings.join("; ")
        );
    }
    let ddl = if options.dbms.eq_ignore_ascii_case("sql server") {
        rendered
    } else {
        engine.translate(&rendered, &options.dbms)?
    };
    let mut ddl_file = fs::File::create(options.output_folder.join("ddlDqdResults.sql"))?;
    write_r_line(&mut ddl_file, &ddl)?;

    Ok(())
}
