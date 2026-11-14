use crate::model::{Check, Options, Row, value_text};
use anyhow::{Context, Result, bail};
use serde_json::{Number, Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

#[derive(Debug, Clone, Default)]
struct CsvHeaders {
    descriptions: Vec<String>,
    tables: Vec<String>,
    fields: Vec<String>,
    concepts: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub descriptions: Vec<Row>,
    pub tables: Vec<Row>,
    pub fields: Vec<Row>,
    pub concepts: Vec<Row>,
    headers: CsvHeaders,
    prepared: bool,
}

impl Catalog {
    pub fn from_rows(
        descriptions: Vec<Row>,
        tables: Vec<Row>,
        fields: Vec<Row>,
        concepts: Vec<Row>,
    ) -> Self {
        Self {
            headers: CsvHeaders {
                descriptions: row_headers(&descriptions),
                tables: row_headers(&tables),
                fields: row_headers(&fields),
                concepts: row_headers(&concepts),
            },
            descriptions,
            tables,
            fields,
            concepts,
            prepared: false,
        }
    }

    pub fn load(options: &Options) -> Result<Self> {
        let version = &options.cdm_version;
        let (descriptions, description_headers) =
            parse_csv(embedded_csv(version, "Check_Descriptions")?, false)?;
        let (tables, table_headers) =
            load_control(version, "Table_Level", &options.table_check_threshold_loc)?;
        let (fields, field_headers) =
            load_control(version, "Field_Level", &options.field_check_threshold_loc)?;
        let (concepts, concept_headers) = load_control(
            version,
            "Concept_Level",
            &options.concept_check_threshold_loc,
        )?;
        Ok(Self {
            descriptions,
            tables,
            fields,
            concepts,
            headers: CsvHeaders {
                descriptions: description_headers,
                tables: table_headers,
                fields: field_headers,
                concepts: concept_headers,
            },
            prepared: false,
        })
    }

    /// Return unfiltered listDqChecks data. Execution preparation is separate.
    pub fn as_list_value(&self) -> Value {
        json!({"checkDescriptions": self.descriptions, "tableChecks": self.tables,
            "fieldChecks": self.fields, "conceptChecks": self.concepts})
    }

    pub fn prepared(&self, options: &Options) -> Result<Self> {
        if self.prepared {
            return Ok(self.clone());
        }
        let mut catalog = self.clone();
        let excluded: HashSet<String> = options
            .tables_to_exclude
            .iter()
            .map(|table| table.to_uppercase())
            .collect();
        let included = |row: &Row| match row.get("cdmTableName") {
            Some(Value::String(table)) => !excluded.contains(table),
            _ => true, // R %in% never yields NA.
        };
        catalog.tables.retain(included);
        catalog.fields.retain(included);
        catalog.concepts.retain(included);
        // R subset drops rows with NA in this comparison, not only offset rows.
        catalog.fields.retain(|row| {
            row.get("cdmFieldName")
                .is_some_and(|name| !name.is_null() && name != "offset")
        });
        if !catalog.headers.tables.iter().any(|name| name == "schema") {
            bail!("object 'schema' not found");
        }
        for table in &mut catalog.tables {
            let schema = match table.get("schema").and_then(Value::as_str) {
                Some("CDM") => &options.cdm_database_schema,
                Some("VOCAB") => &options.vocab_database_schema,
                Some("COHORT") => &options.cohort_database_schema,
                _ => &options.cdm_database_schema,
            };
            table.insert("schema".into(), Value::String(schema.clone()));
        }
        let mut schemas: HashMap<String, Vec<Value>> = HashMap::new();
        for table in &catalog.tables {
            schemas
                .entry(key(table.get("cdmTableName")))
                .or_default()
                .push(table.get("schema").cloned().unwrap_or(Value::Null));
        }
        let had_schema = catalog.headers.fields.iter().any(|name| name == "schema");
        let mut joined = Vec::with_capacity(catalog.fields.len());
        let null_schema = [Value::Null];
        for field in &catalog.fields {
            let matches = schemas
                .get(&key(field.get("cdmTableName")))
                .map(Vec::as_slice)
                .unwrap_or(&null_schema);
            for schema in matches {
                let mut field = field.clone();
                if had_schema {
                    let original = field.remove("schema").unwrap_or(Value::Null);
                    field.insert("schema.x".into(), original);
                    field.insert("schema.y".into(), schema.clone());
                } else {
                    field.insert("schema".into(), schema.clone());
                }
                joined.push(field);
            }
        }
        joined.sort_by(|a, b| {
            let a = a.get("cdmTableName");
            let b = b.get("cdmTableName");
            match (a, b) {
                (Some(Value::Null) | None, Some(Value::Null) | None) => std::cmp::Ordering::Equal,
                (Some(Value::Null) | None, _) => std::cmp::Ordering::Greater,
                (_, Some(Value::Null) | None) => std::cmp::Ordering::Less,
                (Some(a), Some(b)) => value_text(a).cmp(&value_text(b)),
            }
        });
        catalog.fields = joined;
        // merge.data.frame puts its by-column first, even for custom CSV order.
        catalog.headers.fields.retain(|name| name != "cdmTableName");
        catalog.headers.fields.insert(0, "cdmTableName".into());
        if had_schema {
            for name in &mut catalog.headers.fields {
                if name == "schema" {
                    *name = "schema.x".into();
                }
            }
            catalog.headers.fields.push("schema.y".into());
        } else {
            catalog.headers.fields.push("schema".into());
        }
        for row in catalog.fields.iter_mut().chain(catalog.concepts.iter_mut()) {
            if let Some(value) = row.get_mut("cdmFieldName")
                && !value.is_null()
            {
                *value = Value::String(value_text(value).to_uppercase());
            }
        }
        // Preserve the column order of R's merged data.frame.
        for row in &mut catalog.fields {
            let old = std::mem::take(row);
            for name in &catalog.headers.fields {
                row.insert(name.clone(), old.get(name).cloned().unwrap_or(Value::Null));
            }
        }
        catalog.prepared = true;
        Ok(catalog)
    }

    pub fn selected_descriptions(&self, options: &Options) -> Result<Vec<Row>> {
        let mut selected = Vec::new();
        for description in &self.descriptions {
            let level = text(description, "checkLevel")?;
            let name = text(description, "checkName")?;
            let severity = text(description, "severity")?;
            let filter = description
                .get("evaluationFilter")
                .and_then(Value::as_str)
                .unwrap_or("");
            let sql_file = description
                .get("sqlFile")
                .and_then(Value::as_str)
                .unwrap_or("");
            let included = [
                &self.headers.tables,
                &self.headers.fields,
                &self.headers.concepts,
            ]
            .iter()
            .any(|headers| dollar_column(headers, name).is_some());
            if options.check_levels.iter().any(|wanted| wanted == level)
                && options
                    .check_severity
                    .iter()
                    .any(|wanted| wanted == severity)
                && !filter.is_empty()
                && !sql_file.is_empty()
                && included
                && (options.check_names.is_empty()
                    || options.check_names.iter().any(|wanted| wanted == name))
            {
                selected.push(description.clone());
            }
        }
        if selected.is_empty() {
            bail!(
                "No checks are available based on excluded tables. Please review tablesToExclude."
            );
        }
        Ok(selected)
    }

    pub fn checks(&self, options: &Options) -> Result<Vec<Check>> {
        self.prepared(options)?.checks_prepared(options)
    }

    pub fn checks_prepared(&self, options: &Options) -> Result<Vec<Check>> {
        let descriptions = self.selected_descriptions(options)?;
        let mut checks = Vec::new();
        for description in descriptions {
            let rows = match text(&description, "checkLevel")? {
                "TABLE" => &self.tables,
                "FIELD" => &self.fields,
                "CONCEPT" => &self.concepts,
                level => bail!("Unknown check level: {level}"),
            };
            let expression = Filter::parse(text(&description, "evaluationFilter")?)?;
            for parameters in rows {
                if expression.matches(parameters)? {
                    checks.push(Check {
                        description: description.clone(),
                        parameters: parameters.clone(),
                    });
                }
            }
        }
        Ok(checks)
    }

    pub fn columns(&self, level: &str) -> &[String] {
        match level {
            "TABLE" => &self.headers.tables,
            "FIELD" => &self.headers.fields,
            "CONCEPT" => &self.headers.concepts,
            _ => &self.headers.descriptions,
        }
    }
}

fn row_headers(rows: &[Row]) -> Vec<String> {
    let mut headers = Vec::new();
    for row in rows {
        for name in row.keys() {
            if !headers.contains(name) {
                headers.push(name.clone());
            }
        }
    }
    headers
}

fn key(value: Option<&Value>) -> String {
    // Keep NA distinct from the literal string "NA" in merge keys.
    value.unwrap_or(&Value::Null).to_string()
}

fn text<'a>(row: &'a Row, name: &str) -> Result<&'a str> {
    row.get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("Missing or non-textual {name}"))
}

/// `$` matches an exact column first, then a unique column-name prefix.
fn dollar_column<'a>(headers: &'a [String], name: &str) -> Option<&'a str> {
    if let Some(column) = headers.iter().find(|column| column.as_str() == name) {
        return Some(column);
    }
    let mut matches = headers.iter().filter(|column| column.starts_with(name));
    let first = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn load_control(version: &str, level: &str, location: &str) -> Result<(Vec<Row>, Vec<String>)> {
    if location == "default" {
        parse_csv(embedded_csv(version, level)?, true)
    } else {
        let contents = fs::read(Path::new(location))
            .with_context(|| format!("Reading threshold file {location}"))?;
        parse_csv(&contents, true)
    }
}

#[derive(Clone, Copy)]
enum ColumnType {
    Character,
    Number,
    Logical,
}

fn infer_column(values: impl Iterator<Item = String>) -> ColumnType {
    let values: Vec<_> = values
        .filter(|value| !value.is_empty() && value != "NA")
        .collect();
    if values.iter().all(|value| parse_logical(value).is_some()) {
        ColumnType::Logical
    } else if values.iter().all(|value| parse_number(value).is_some()) {
        ColumnType::Number
    } else {
        ColumnType::Character
    }
}

fn parse_logical(value: &str) -> Option<bool> {
    match value {
        "TRUE" | "T" | "true" => Some(true),
        "FALSE" | "F" | "false" => Some(false),
        _ => None,
    }
}

fn parse_number(value: &str) -> Option<f64> {
    // readr's numeric parser does not treat grouping marks as decimal numbers.
    if value.contains(',') || value.starts_with("0x") {
        return None;
    }
    value.parse().ok()
}

fn trim_csv_field(value: &str) -> &str {
    value.trim_matches(|character| character == ' ' || character == '\t')
}

/// readThresholdFile guesses with spec_csv defaults, then reads with NA={" ",""}.
pub fn parse_csv(bytes: &[u8], threshold: bool) -> Result<(Vec<Row>, Vec<String>)> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::Headers)
        .from_reader(bytes);
    let original_headers = reader.headers()?.clone();
    let mut counts = HashMap::new();
    for name in &original_headers {
        *counts.entry(name.to_owned()).or_insert(0_usize) += 1;
    }
    let headers: Vec<String> = original_headers
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if name.is_empty() || counts[name] > 1 {
                format!("{name}...{}", i + 1)
            } else {
                name.to_owned()
            }
        })
        .collect();
    let records = reader
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let types: Vec<_> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if threshold && name == "plausibleUnitConceptIds" {
                ColumnType::Character
            } else {
                infer_column(
                    records
                        .iter()
                        .take(1000)
                        .map(|record| trim_csv_field(record.get(i).unwrap_or("")).to_owned()),
                )
            }
        })
        .collect();
    let mut rows = Vec::with_capacity(records.len());
    for record in records {
        let mut row = Row::new();
        for (i, name) in headers.iter().enumerate() {
            let raw = trim_csv_field(record.get(i).unwrap_or(""));
            let value = if raw.is_empty() || (!threshold && raw == "NA") {
                Value::Null
            } else {
                match types[i] {
                    ColumnType::Character => Value::String(raw.to_owned()),
                    ColumnType::Logical => {
                        parse_logical(raw).map(Value::Bool).unwrap_or(Value::Null)
                    }
                    ColumnType::Number => parse_number(raw)
                        .and_then(Number::from_f64)
                        .map(Value::Number)
                        .unwrap_or(Value::Null),
                }
            };
            row.insert(name.clone(), value);
        }
        rows.push(row);
    }
    Ok((rows, headers))
}

/// Canonical descriptions use only conjunctions of == and != comparisons.
/// A missing value produces R NA, which dplyr::filter drops.
#[derive(Debug)]
struct Filter(Vec<Comparison>);
#[derive(Debug)]
struct Comparison {
    field: String,
    equal: bool,
    expected: String,
}

impl Filter {
    fn parse(expression: &str) -> Result<Self> {
        let mut comparisons = Vec::new();
        for term in expression.split('&') {
            let (field, operator, expected) = if let Some((field, expected)) = term.split_once("==")
            {
                (field, true, expected)
            } else if let Some((field, expected)) = term.split_once("!=") {
                (field, false, expected)
            } else {
                bail!("Unsupported embedded evaluationFilter: {expression}");
            };
            let expected = expected.trim();
            // `len() >= 2`: a lone quote both starts and ends with one but has no content to strip.
            if expected.len() < 2 || !(expected.starts_with('\'') && expected.ends_with('\'')) {
                bail!("Invalid embedded evaluationFilter: {expression}");
            }
            comparisons.push(Comparison {
                field: field.trim().into(),
                equal: operator,
                expected: expected[1..expected.len() - 1].into(),
            });
        }
        Ok(Self(comparisons))
    }

    fn matches(&self, row: &Row) -> Result<bool> {
        let mut keep = true;
        // Evaluate every operand. R & does not short-circuit missing-column errors.
        for comparison in &self.0 {
            let value = row
                .get(&comparison.field)
                .with_context(|| format!("object '{}' not found", comparison.field))?;
            if value.is_null() || (value_text(value) == comparison.expected) != comparison.equal {
                keep = false;
            }
        }
        Ok(keep)
    }
}

pub fn embedded_csv(version: &str, level: &str) -> Result<&'static [u8]> {
    macro_rules! versions {
        ($($version:literal),+ $(,)?) => {
            match (version, level) {
                $(( $version, "Check_Descriptions") => Ok(include_bytes!(concat!("../resources/csv/OMOP_CDMv", $version, "_Check_Descriptions.csv")).as_slice()),
                ( $version, "Table_Level") => Ok(include_bytes!(concat!("../resources/csv/OMOP_CDMv", $version, "_Table_Level.csv")).as_slice()),
                ( $version, "Field_Level") => Ok(include_bytes!(concat!("../resources/csv/OMOP_CDMv", $version, "_Field_Level.csv")).as_slice()),
                ( $version, "Concept_Level") => Ok(include_bytes!(concat!("../resources/csv/OMOP_CDMv", $version, "_Concept_Level.csv")).as_slice()),)+
                _ => bail!("No upstream CSV resource OMOP_CDMv{version}_{level}.csv"),
            }
        }
    }
    versions!("5.2", "5.3", "5.4", "5.5")
}

pub fn embedded_sql(name: &str) -> Result<&'static str> {
    macro_rules! sql_files {
        ($($name:literal),+ $(,)?) => {
            match name {
                $($name => Ok(include_str!(concat!("../resources/sql/sql_server/", $name))),)+
                _ => bail!("No upstream SQL resource {name}"),
            }
        }
    }
    sql_files!(
        "concept_plausible_gender.sql",
        "concept_plausible_gender_use_descendants.sql",
        "concept_plausible_unit_concept_ids.sql",
        "concept_plausible_value_high.sql",
        "concept_plausible_value_low.sql",
        "field_cdm_datatype.sql",
        "field_cdm_field.sql",
        "field_concept_record_completeness.sql",
        "field_fk_class.sql",
        "field_fk_domain.sql",
        "field_is_not_nullable.sql",
        "field_is_primary_key.sql",
        "field_is_standard_valid_concept.sql",
        "field_measure_value_completeness.sql",
        "field_plausible_after_birth.sql",
        "field_plausible_before_death.sql",
        "field_plausible_during_life.sql",
        "field_plausible_start_before_end.sql",
        "field_plausible_temporal_after.sql",
        "field_plausible_value_high.sql",
        "field_plausible_value_low.sql",
        "field_source_value_completeness.sql",
        "field_within_visit_dates.sql",
        "is_foreign_key.sql",
        "result_dataframe_ddl.sql",
        "table_cdm_table.sql",
        "table_concept_completeness.sql",
        "table_condition_era_completeness.sql",
        "table_observation_period_overlap.sql",
        "table_person_completeness.sql",
        "sqlOnly/cte_sql_for_results_table.sql",
        "sqlOnly/insert_ctes_into_result_table.sql",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_filter_values_do_not_select_checks() {
        let filter = Filter::parse("isForeignKey=='Yes' & fkDomain!= ''").unwrap();
        let missing: Row =
            serde_json::from_value(json!({"isForeignKey":"Yes","fkDomain":null})).unwrap();
        assert!(!filter.matches(&missing).unwrap());
        let selected: Row =
            serde_json::from_value(json!({"isForeignKey":"Yes","fkDomain":"Condition"})).unwrap();
        assert!(filter.matches(&selected).unwrap());
    }

    #[test]
    fn filter_with_a_lone_quote_is_an_error_not_a_panic() {
        let error = Filter::parse("isForeignKey=='").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Invalid embedded evaluationFilter")
        );
        assert!(Filter::parse("isForeignKey==''").is_ok());
    }

    #[test]
    fn unit_lists_stay_text_and_na_policy_preserves_literal_na_in_character_columns() {
        let (rows, _) = parse_csv(
            b"plausibleUnitConceptIds,label,threshold\n123,NA,0\n\"123,456\",text,5\n",
            true,
        )
        .unwrap();
        assert_eq!(rows[0]["plausibleUnitConceptIds"], "123");
        assert_eq!(rows[1]["plausibleUnitConceptIds"], "123,456");
        assert_eq!(rows[0]["label"], "NA");
        assert_eq!(rows[1]["threshold"], 5.0);
    }

    #[test]
    fn quoted_multiline_csv_fields_trim_edge_spaces_without_losing_line_breaks() {
        let (rows, _) = parse_csv(b"field\n\"first\r\nsecond. \"\n", false).unwrap();
        assert_eq!(rows[0]["field"], "first\r\nsecond.");
    }

    #[test]
    fn requested_names_do_not_force_applicability_prerequisites() {
        let options = Options {
            check_names: vec!["isRequired".into()],
            ..Options::default()
        };
        let catalog = Catalog::load(&options).unwrap();
        let checks = catalog.checks(&options).unwrap();
        assert!(
            checks
                .iter()
                .all(|check| check.description["checkName"] == "isRequired")
        );
        assert!(
            checks
                .iter()
                .any(|check| check.parameters["cdmTableName"] == "PERSON"
                    && check.parameters["cdmFieldName"] == "PERSON_ID")
        );
    }
}
