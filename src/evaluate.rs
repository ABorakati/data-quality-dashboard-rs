// Port of OHDSI DataQualityDashboard 2.9.0 evaluateThresholds.R,
// calculateNotApplicableStatus.R, and summarizeResults.R.
// Copyright 2026 Observational Health Data Sciences and Informatics.
// Licensed under the Apache License, Version 2.0.

use std::collections::HashMap;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::{catalog::Catalog, model::Row};

const NA_CHECKS: [&str; 3] = ["cdmTable", "cdmField", "measureValueCompleteness"];
const NULL: Value = Value::Null;

fn value<'a>(row: &'a Row, name: &str) -> &'a Value {
    row.get(name).unwrap_or(&NULL)
}

fn named(row: &Row, name: &str) -> bool {
    value(row, "checkName").as_str() == Some(name)
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(u8::from(*b) as f64),
        _ => None,
    }
}

fn r_text(value: &Value) -> String {
    match value {
        Value::Null => "NA".into(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        Value::Number(n) if n.as_f64().is_some_and(|n| n.fract() == 0.0) => {
            format!("{:.0}", n.as_f64().expect("JSON number"))
        }
        _ => value.to_string(),
    }
}

// R comparisons propagate NA. A character operand coerces its other operand.
fn equal(a: &Value, b: &Value) -> Option<bool> {
    if a.is_null() || b.is_null() {
        None
    } else if let (Value::String(a), Value::String(b)) = (a, b) {
        Some(a == b)
    } else if a.is_string() || b.is_string() {
        Some(r_text(a) == r_text(b))
    } else if let (Some(a), Some(b)) = (number(a), number(b)) {
        Some(a == b)
    } else {
        Some(a == b)
    }
}

// Upstream puts the result key inside quotes with sprintf("%s"). R NA
// therefore becomes the literal string "NA"; missing control metadata does
// not match that key.
fn lookup_equal(control: &Value, result: &Value) -> bool {
    let lookup_text = match result {
        Value::Null => "NA",
        Value::String(text) => text.as_str(),
        other => return !control.is_null() && r_text(control) == r_text(other),
    };
    match control {
        Value::Null => false,
        _ => r_text(control) == lookup_text,
    }
}

fn and(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

fn flag(row: &Row, name: &str) -> Option<bool> {
    equal(value(row, name), &Value::from(1))
}

fn set_flag(row: &mut Row, name: &str, flag: bool) {
    row.insert(name.into(), Value::from(u8::from(flag)));
}

/// Upstream `.hasNAchecks`: the results include cdmTable, cdmField and measureValueCompleteness.
pub fn has_na_checks(results: &[Row]) -> bool {
    NA_CHECKS
        .iter()
        .all(|name| results.iter().any(|row| named(row, name)))
}

fn threshold_values(row: &Row, catalog: &Catalog) -> Result<(Value, Value)> {
    let (controls, columns) = match value(row, "checkLevel").as_str() {
        Some("TABLE") => (&catalog.tables, catalog.columns("TABLE")),
        Some("FIELD") => (&catalog.fields, catalog.columns("FIELD")),
        Some("CONCEPT") => (&catalog.concepts, catalog.columns("CONCEPT")),
        _ => bail!("unknown checkLevel in threshold lookup"),
    };
    threshold_values_from(row, controls, columns)
}

fn threshold_values_from(
    row: &Row,
    controls: &[Row],
    columns: &[String],
) -> Result<(Value, Value)> {
    let name = value(row, "checkName").as_str().unwrap_or("NA");
    let threshold = format!("{name}Threshold");
    let notes = format!("{name}Notes");
    if !columns.iter().any(|column| column == &threshold) {
        return Ok((Value::Null, Value::Null));
    }
    let mut found = None;
    let mut count = 0;
    for control in controls {
        let mut selected = Some(lookup_equal(
            value(control, "cdmTableName"),
            value(row, "cdmTableName"),
        ));
        if value(row, "checkLevel") != "TABLE" {
            selected = and(
                selected,
                Some(lookup_equal(
                    value(control, "cdmFieldName"),
                    value(row, "cdmFieldName"),
                )),
            );
        }
        if value(row, "checkLevel") == "CONCEPT" {
            let concept = value(row, "conceptId");
            let concept = if value(row, "unitConceptId").is_null()
                && concept.as_str().is_some_and(|s| s.contains(','))
            {
                concept.clone()
            } else {
                match concept {
                    Value::Null => Value::Null,
                    Value::String(s) if s == "NA" || s == "NaN" => Value::Null,
                    Value::String(s) => Value::from(s.parse::<f64>().map_err(|_| {
                        anyhow::anyhow!("invalid conceptId in threshold filter: {s}")
                    })?),
                    other => other.clone(),
                }
            };
            selected = and(selected, equal(value(control, "conceptId"), &concept));
            if !value(row, "unitConceptId").is_null() {
                let unit = match value(row, "unitConceptId") {
                    Value::String(s) => s.parse::<f64>().ok(),
                    other => number(other),
                }
                .filter(|n| n.is_finite() && *n > i32::MIN as f64 && *n <= i32::MAX as f64)
                .map(|n| Value::String((n as i32).to_string()))
                .unwrap_or(Value::Null);
                selected = and(selected, equal(value(control, "unitConceptId"), &unit));
            }
        }
        if selected != Some(false) {
            count += 1;
            // The oracle uses the first result column when a Notes column is missing.
            let notes_value = if columns.iter().any(|column| column == &notes) {
                value(control, &notes).clone()
            } else {
                value(row, "numViolatedRows").clone()
            };
            found = Some(if selected.is_none() {
                (Value::Null, notes_value)
            } else {
                (value(control, &threshold).clone(), notes_value)
            });
        }
    }
    if count != 1 {
        bail!("replacement has {count} rows, data has 1");
    }
    Ok(found.expect("one selected threshold"))
}

/// Evaluates thresholds, then calculates applicability only when all three
/// prerequisite check names occur in the actual results. Joins can expand rows.
pub fn evaluate_thresholds(results: &mut Vec<Row>, catalog: &Catalog) -> Result<()> {
    if results.is_empty() {
        // Upstream assigns scalar columns before its 1:nrow loop.
        bail!("replacement has 1 row, data has 0");
    }
    for row in results.iter_mut() {
        for name in ["failed", "passed", "isError", "notApplicable"] {
            set_flag(row, name, false);
        }
        for name in ["notApplicableReason", "thresholdValue", "notesValue"] {
            row.insert(name.into(), Value::Null);
        }
    }
    for row in results.iter_mut() {
        let (threshold, notes) = threshold_values(row, catalog)?;
        row.insert("thresholdValue".into(), threshold.clone());
        row.insert("notesValue".into(), notes);
        if !value(row, "error").is_null() {
            let error = r_text(value(row, "error")).to_lowercase();
            let missing = [
                "does not exist",
                "missing",
                "not found",
                "invalid object name",
                "invalid column name",
                "ora-00942",
                "no such table",
                "no such column",
            ]
            .iter()
            .any(|pattern| error.contains(pattern));
            let existence = named(row, "cdmField") || named(row, "cdmTable");
            set_flag(row, "failed", missing && existence);
            set_flag(row, "isError", !(missing && existence));
        } else if threshold.is_null() || equal(&threshold, &Value::from(0)) == Some(true) {
            set_flag(
                row,
                "failed",
                number(value(row, "numViolatedRows")).is_some_and(|n| n > 0.0),
            );
        } else {
            let Some(percent) = number(value(row, "pctViolatedRows")) else {
                bail!("missing value where TRUE/FALSE needed");
            };
            let failed = if let Value::String(threshold) = &threshold {
                r_text(&Value::from(percent * 100.0)) > *threshold
            } else if let Some(threshold) = number(&threshold) {
                // Do not round the database fraction or this multiplication.
                percent * 100.0 > threshold
            } else {
                bail!("invalid threshold value");
            };
            set_flag(row, "failed", failed);
        }
    }
    if has_na_checks(results) {
        calculate_not_applicable(results)?;
    } else {
        for row in results.iter_mut() {
            let passed = flag(row, "failed") == Some(false) && flag(row, "isError") == Some(false);
            set_flag(row, "passed", passed);
        }
    }
    if results
        .iter()
        .any(|row| value(row, "notesValue").is_string())
    {
        for row in results.iter_mut() {
            if let Some(note) = row.get_mut("notesValue") {
                if note.is_number() || note.is_boolean() {
                    *note = Value::String(r_text(note));
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Key<'a> {
    Null,
    Text(&'a str),
    Number(u64),
    Bool(bool),
}

fn key(value: &Value) -> Key<'_> {
    match value {
        Value::Null => Key::Null,
        Value::String(s) => Key::Text(s),
        Value::Number(n) => {
            let n = n.as_f64().expect("JSON number");
            Key::Number(if n == 0.0 { 0 } else { n.to_bits() })
        }
        Value::Bool(b) => Key::Bool(*b),
        _ => Key::Null,
    }
}

fn table(row: &Row) -> Key<'_> {
    key(value(row, "cdmTableName"))
}

fn field(row: &Row) -> (Key<'_>, Key<'_>) {
    (table(row), key(value(row, "cdmFieldName")))
}

fn key_text(key: Key<'_>) -> String {
    match key {
        Key::Null => "NA".into(),
        Key::Text(s) => s.into(),
        Key::Number(n) => r_text(&Value::from(f64::from_bits(n))),
        Key::Bool(b) => r_text(&Value::Bool(b)),
    }
}

#[derive(Clone, Copy)]
struct Applicability {
    table_missing: bool,
    field_missing: bool,
    table_empty: bool,
    field_empty: bool,
}

fn apply_not_applicable(row: &Row, facts: Applicability, condition_era: Option<bool>) -> bool {
    if let Some(condition_era) = condition_era {
        return condition_era;
    }
    // conceptIsMissing | conceptAndUnitAreMissing: a CONCEPT check with a zero denominator.
    let concept_missing = value(row, "checkLevel") == "CONCEPT"
        && number(value(row, "numDenominatorRows")) == Some(0.0);
    not_applicable_rule(row, facts, concept_missing)
}

/// Upstream `.applyNotApplicable(x)` for one row that carries the flag columns upstream joins on
/// (`tableIsMissing`, `fieldIsMissing`, `tableIsEmpty`, `fieldIsEmpty`, `conceptIsMissing`,
/// `conceptAndUnitAreMissing`; 1/TRUE is set, anything else is not). The condition-era special
/// case is outside `.applyNotApplicable` upstream too.
pub fn apply_not_applicable_flags(row: &Row) -> bool {
    let set = |name: &str| flag(row, name) == Some(true);
    not_applicable_rule(
        row,
        Applicability {
            table_missing: set("tableIsMissing"),
            field_missing: set("fieldIsMissing"),
            table_empty: set("tableIsEmpty"),
            field_empty: set("fieldIsEmpty"),
        },
        set("conceptIsMissing") || set("conceptAndUnitAreMissing"),
    )
}

fn not_applicable_rule(row: &Row, facts: Applicability, concept_missing: bool) -> bool {
    if named(row, "measurePersonCompleteness") || named(row, "cdmField") {
        return facts.table_missing;
    }
    if named(row, "cdmTable") {
        return false;
    }
    if facts.table_missing || facts.field_missing {
        return true;
    }
    if flag(row, "isError") == Some(true) {
        return false;
    }
    if facts.table_empty {
        return true;
    }
    if named(row, "measureValueCompleteness") {
        return false;
    }
    facts.field_empty || concept_missing
}

fn finish_applicability(row: &mut Row, facts: Applicability, condition_era: Option<bool>) {
    let not_applicable = apply_not_applicable(row, facts, condition_era);
    let reason = if !not_applicable {
        Value::Null
    } else if condition_era == Some(true) {
        Value::String("Table CONDITION_OCCURRENCE is empty.".into())
    } else {
        let table = r_text(value(row, "cdmTableName"));
        let field = r_text(value(row, "cdmFieldName"));
        Value::String(if facts.table_missing {
            format!("Table {table} does not exist.")
        } else if facts.field_missing {
            format!("Field {table}.{field} does not exist.")
        } else if facts.table_empty {
            format!("Table {table} is empty.")
        } else if facts.field_empty {
            format!("Field {table}.{field} is not populated.")
        } else if value(row, "unitConceptId").is_null() {
            format!(
                "{field}={} is missing from the {table} table.",
                r_text(value(row, "conceptId"))
            )
        } else {
            format!(
                "Combination of {field}={}, unitConceptId={} and VALUE_AS_NUMBER IS NOT NULL is missing from the {table} table.",
                r_text(value(row, "conceptId")),
                r_text(value(row, "unitConceptId"))
            )
        })
    };
    set_flag(row, "notApplicable", not_applicable);
    row.insert("notApplicableReason".into(), reason);
    if not_applicable {
        set_flag(row, "failed", false);
    }
    let passed = and(
        and(
            equal(value(row, "failed"), &Value::from(0)),
            equal(value(row, "isError"), &Value::from(0)),
        ),
        Some(!not_applicable),
    );
    row.insert(
        "passed".into(),
        passed
            .map(|passed| Value::from(u8::from(passed)))
            .unwrap_or(Value::Null),
    );
    // dplyr::select drops these names even when present on the input.
    for name in [
        "tableIsMissing",
        "fieldIsMissing",
        "tableIsEmpty",
        "fieldIsEmpty",
        "conceptIsMissing",
        "conceptAndUnitAreMissing",
    ] {
        row.remove(name);
    }
}

/// Reproduces the four upstream left joins, including NA-key matching and
/// ordered duplicate field expansion. Table-level duplicate guards are errors.
pub fn calculate_not_applicable(results: &mut Vec<Row>) -> Result<()> {
    let mut missing_tables = HashMap::<Key<'_>, Vec<Option<bool>>>::new();
    let mut missing_fields = HashMap::<(Key<'_>, Key<'_>), Vec<Option<bool>>>::new();
    let mut empty_tables = HashMap::<Key<'_>, Vec<Option<bool>>>::new();
    let mut empty_fields = HashMap::<(Key<'_>, Key<'_>), Vec<Option<bool>>>::new();
    for row in results.iter() {
        if named(row, "cdmTable") {
            missing_tables
                .entry(table(row))
                .or_default()
                .push(flag(row, "failed"));
        }
        if named(row, "cdmField") {
            missing_fields
                .entry(field(row))
                .or_default()
                .push(flag(row, "failed"));
        }
        if named(row, "measureValueCompleteness") {
            let empty = equal(value(row, "numDenominatorRows"), &Value::from(0));
            let entries = empty_tables.entry(table(row)).or_default();
            if !entries.contains(&empty) {
                entries.push(empty);
            }
            empty_fields.entry(field(row)).or_default().push(equal(
                value(row, "numDenominatorRows"),
                value(row, "numViolatedRows"),
            ));
        }
    }
    for (lookup, message) in [
        (
            &missing_tables,
            "Duplicate table names in missingTables indicates an issue with check execution - cdmTable ran more than once for the same table: ",
        ),
        (
            &empty_tables,
            "Duplicate table names in emptyTables indicates an issue with check execution - measureValueCompleteness had different denominator counts for the same table: ",
        ),
    ] {
        let mut duplicates: Vec<_> = lookup
            .iter()
            .filter(|(_, entries)| entries.len() > 1)
            .map(|(key, _)| *key)
            .collect();
        if !duplicates.is_empty() {
            // dplyr::count orders character groups in C-locale order, NA last.
            duplicates.sort_by(|a, b| {
                (matches!(a, Key::Null), key_text(*a)).cmp(&(matches!(b, Key::Null), key_text(*b)))
            });
            bail!(
                "{message}{}",
                duplicates
                    .into_iter()
                    .map(key_text)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    let condition_era = if results
        .iter()
        .any(|row| named(row, "measureConditionEraCompleteness"))
    {
        let missing = missing_tables
            .get(&Key::Text("CONDITION_OCCURRENCE"))
            .map(|entries| entries[0].unwrap_or(false));
        let empty = empty_tables
            .get(&Key::Text("CONDITION_OCCURRENCE"))
            .map(|entries| entries[0].unwrap_or(false));
        match (missing, empty) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => bail!("missing value where TRUE/FALSE needed"),
        }
    } else {
        None
    };
    let absent = [None];
    let mut plan = Vec::with_capacity(results.len());
    for (index, row) in results.iter().enumerate() {
        let table_missing = missing_tables
            .get(&table(row))
            .and_then(|entries| entries[0])
            .unwrap_or(false);
        let table_empty = empty_tables
            .get(&table(row))
            .and_then(|entries| entries[0])
            .unwrap_or(false);
        let missing = missing_fields
            .get(&field(row))
            .map(Vec::as_slice)
            .unwrap_or(&absent);
        let empty = empty_fields
            .get(&field(row))
            .map(Vec::as_slice)
            .unwrap_or(&absent);
        for field_missing in missing {
            for field_empty in empty {
                plan.push((
                    index,
                    Applicability {
                        table_missing,
                        table_empty,
                        field_missing: field_missing.unwrap_or(false),
                        field_empty: field_empty.unwrap_or(false),
                    },
                ));
            }
        }
    }
    // Release borrowed lookup keys before moving rows. Clone only expanded rows.
    drop((missing_tables, missing_fields, empty_tables, empty_fields));
    if plan.len() == results.len() {
        for (row, (_, facts)) in results.iter_mut().zip(plan) {
            let special = if named(row, "measureConditionEraCompleteness") {
                condition_era
            } else {
                None
            };
            finish_applicability(row, facts, special);
        }
    } else {
        let mut expanded = Vec::with_capacity(plan.len());
        let mut plan = plan.into_iter().peekable();
        for (index, mut row) in std::mem::take(results).into_iter().enumerate() {
            while let Some((_, facts)) = plan.next() {
                let special = if named(&row, "measureConditionEraCompleteness") {
                    condition_era
                } else {
                    None
                };
                if plan.peek().is_some_and(|(next, _)| *next == index) {
                    let mut copy = row.clone();
                    finish_applicability(&mut copy, facts, special);
                    expanded.push(copy);
                } else {
                    finish_applicability(&mut row, facts, special);
                    expanded.push(row);
                    break;
                }
            }
        }
        *results = expanded;
    }
    Ok(())
}

// R >= 4.0 fround compares distances in the original scale, not rounded x*100.
#[doc(hidden)]
pub fn round_percent(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    let sign = x.signum();
    let x = x.abs();
    let scaled = x * 100.0;
    let floor = scaled.floor();
    let lower = floor / 100.0;
    let upper = scaled.ceil() / 100.0;
    let down = x - lower;
    let up = upper - x;
    Value::from(
        sign * if up < down || (up == down && floor % 2.0 == 1.0) {
            upper
        } else {
            lower
        },
    )
}

/// Preserves legacy overview counts: errors and not-applicable rows count as
/// overall passes when failed != 1. NA logical subscripts count NA rows in R.
pub fn summarize(results: &[Row]) -> Row {
    let has = |name: &str| results.iter().any(|row| row.contains_key(name));
    let count = |predicate: &dyn Fn(&Row) -> Option<bool>| -> i64 {
        results
            .iter()
            .filter(|row| predicate(row) != Some(false))
            .count() as i64
    };
    let total = results.len() as i64;
    let failed = if has("failed") {
        count(&|row| flag(row, "failed"))
    } else {
        0
    };
    let passed = total - failed;
    let error_failed = if has("error") {
        count(&|row| Some(!value(row, "error").is_null()))
    } else {
        0
    };
    let threshold_failed = if has("failed") && has("error") {
        count(&|row| and(flag(row, "failed"), Some(value(row, "error").is_null())))
    } else {
        0
    };
    let mut summary = Row::new();
    for (name, n) in [
        ("countTotal", total),
        ("countPassed", passed),
        ("countErrorFailed", error_failed),
        ("countThresholdFailed", threshold_failed),
        ("countOverallFailed", failed),
    ] {
        summary.insert(name.into(), Value::from(n));
    }
    summary.insert(
        "percentPassed".into(),
        round_percent(passed as f64 / total as f64 * 100.0),
    );
    summary.insert(
        "percentFailed".into(),
        round_percent(failed as f64 / total as f64 * 100.0),
    );
    for prefix in ["Total", "Failed", "Passed"] {
        for category in ["Plausibility", "Conformance", "Completeness"] {
            let n = if !has("category")
                || (prefix == "Failed" && !has("failed"))
                || (prefix == "Passed" && !has("passed"))
            {
                0
            } else {
                count(&|row| {
                    let category_matches = match value(row, "category") {
                        Value::Null => None,
                        Value::String(s) => Some(s == category),
                        other => Some(r_text(other) == category),
                    };
                    match prefix {
                        "Failed" => and(category_matches, flag(row, "failed")),
                        "Passed" => and(category_matches, flag(row, "passed")),
                        _ => category_matches,
                    }
                })
            };
            summary.insert(format!("count{prefix}{category}"), Value::from(n));
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(value: Value) -> Row {
        value.as_object().expect("row object").clone()
    }

    fn threshold_values_for_test(row: &Row, controls: &[Row]) -> Result<(Value, Value)> {
        let mut columns: Vec<_> = controls
            .iter()
            .flat_map(|control| control.keys().cloned())
            .collect();
        columns.sort();
        columns.dedup();
        threshold_values_from(row, controls, &columns)
    }

    #[test]
    fn missing_table_marks_regular_check_not_applicable_but_preserves_error() {
        let mut results = vec![
            row(
                json!({"checkName":"cdmTable","cdmTableName":"PERSON","checkLevel":"TABLE","failed":1,"isError":0}),
            ),
            row(
                json!({"checkName":"cdmField","cdmTableName":"PERSON","cdmFieldName":"person_id","checkLevel":"FIELD","failed":0,"isError":0}),
            ),
            row(
                json!({"checkName":"measureValueCompleteness","cdmTableName":"PERSON","cdmFieldName":"person_id","checkLevel":"FIELD","failed":0,"isError":0,"numDenominatorRows":0,"numViolatedRows":0}),
            ),
            row(
                json!({"checkName":"isRequired","cdmTableName":"PERSON","cdmFieldName":"person_id","checkLevel":"FIELD","failed":1,"isError":1}),
            ),
        ];
        calculate_not_applicable(&mut results).unwrap();
        let result = results.iter().find(|row| named(row, "isRequired")).unwrap();
        assert_eq!(value(result, "notApplicable"), &json!(1));
        assert_eq!(value(result, "failed"), &json!(0));
        assert_eq!(value(result, "isError"), &json!(1));
        assert_eq!(value(result, "passed"), &json!(0));
        assert_eq!(
            value(result, "notApplicableReason"),
            &json!("Table PERSON does not exist.")
        );
    }

    #[test]
    fn duplicate_field_prerequisites_expand_rows_in_left_join_order() {
        let mut results = vec![
            row(json!({"checkName":"cdmTable","cdmTableName":"PERSON","failed":0,"isError":0})),
            row(
                json!({"checkName":"cdmField","cdmTableName":"PERSON","cdmFieldName":"gender","failed":0,"isError":0}),
            ),
            row(
                json!({"checkName":"cdmField","cdmTableName":"PERSON","cdmFieldName":"gender","failed":1,"isError":0}),
            ),
            row(
                json!({"checkName":"measureValueCompleteness","cdmTableName":"PERSON","cdmFieldName":"gender","failed":0,"isError":0,"numDenominatorRows":4,"numViolatedRows":4}),
            ),
            row(
                json!({"checkName":"measureValueCompleteness","cdmTableName":"PERSON","cdmFieldName":"gender","failed":0,"isError":0,"numDenominatorRows":4,"numViolatedRows":0}),
            ),
            row(
                json!({"checkName":"isRequired","cdmTableName":"PERSON","cdmFieldName":"gender","failed":1,"isError":0}),
            ),
        ];
        calculate_not_applicable(&mut results).unwrap();
        let expanded: Vec<_> = results
            .iter()
            .filter(|row| named(row, "isRequired"))
            .collect();
        assert_eq!(expanded.len(), 4);
        assert_eq!(
            expanded
                .iter()
                .filter(|row| value(row, "notApplicable") == &json!(1))
                .count(),
            3
        );
        assert_eq!(
            expanded
                .iter()
                .filter(|row| value(row, "notApplicableReason")
                    == &json!("Field PERSON.gender does not exist."))
                .count(),
            2
        );
    }

    #[test]
    fn table_summary_retains_legacy_failed_and_category_counts() {
        let results = vec![
            row(json!({"category":"Plausibility","failed":1,"passed":0,"error":null})),
            row(json!({"category":"Conformance","failed":0,"passed":0,"error":"sql error"})),
            row(json!({"category":"Completeness","failed":0,"passed":1,"error":null})),
        ];
        let summary = summarize(&results);
        assert_eq!(value(&summary, "countPassed"), &json!(2));
        assert_eq!(value(&summary, "countOverallFailed"), &json!(1));
        assert_eq!(value(&summary, "countErrorFailed"), &json!(1));
        assert_eq!(value(&summary, "countThresholdFailed"), &json!(1));
        assert_eq!(value(&summary, "countPassedConformance"), &json!(0));
        assert_eq!(value(&summary, "countPassedCompleteness"), &json!(1));
        assert_eq!(value(&summary, "percentFailed"), &json!(33.33));
    }

    #[test]
    fn applicability_rejects_duplicate_missing_table_prerequisites() {
        let mut results = vec![
            row(json!({"checkName":"cdmTable","cdmTableName":"PERSON","failed":0})),
            row(json!({"checkName":"cdmTable","cdmTableName":"PERSON","failed":1})),
        ];
        assert!(
            calculate_not_applicable(&mut results)
                .unwrap_err()
                .to_string()
                .contains("Duplicate table names in missingTables")
        );
    }

    #[test]
    fn custom_threshold_lookup_matches_concept_units_and_notes() {
        let result = row(json!({
            "checkName":"plausibleValueLow",
            "checkLevel":"CONCEPT",
            "cdmTableName":"MEASUREMENT",
            "cdmFieldName":"value_as_number",
            "conceptId":"3000963",
            "unitConceptId":"8840"
        }));
        let controls = vec![
            row(
                json!({"cdmTableName":"MEASUREMENT","cdmFieldName":"value_as_number","conceptId":3000963,"unitConceptId":8837,"plausibleValueLowThreshold":2,"plausibleValueLowNotes":"wrong unit"}),
            ),
            row(
                json!({"cdmTableName":"MEASUREMENT","cdmFieldName":"value_as_number","conceptId":3000963,"unitConceptId":8840,"plausibleValueLowThreshold":7.5,"plausibleValueLowNotes":"custom"}),
            ),
        ];
        assert_eq!(
            threshold_values_for_test(&result, &controls).unwrap(),
            (json!(7.5), json!("custom"))
        );
    }

    #[test]
    fn duplicate_custom_threshold_keys_reproduce_r_assignment_error() {
        let result =
            row(json!({"checkName":"isRequired","checkLevel":"TABLE","cdmTableName":"PERSON"}));
        let controls = vec![
            row(json!({"cdmTableName":"PERSON","isRequiredThreshold":0})),
            row(json!({"cdmTableName":"PERSON","isRequiredThreshold":50})),
        ];
        assert_eq!(
            threshold_values_for_test(&result, &controls)
                .unwrap_err()
                .to_string(),
            "replacement has 2 rows, data has 1"
        );
    }

    #[test]
    fn comma_concept_identifiers_use_exact_character_lookup_without_unit() {
        let result = row(
            json!({"checkName":"plausibleValueLow","checkLevel":"CONCEPT","cdmTableName":"MEASUREMENT","cdmFieldName":"value_as_number","conceptId":"3000963,3000964","unitConceptId":null}),
        );
        let controls = vec![row(
            json!({"cdmTableName":"MEASUREMENT","cdmFieldName":"value_as_number","conceptId":"3000963,3000964","unitConceptId":null,"plausibleValueLowThreshold":10,"plausibleValueLowNotes":"pair"}),
        )];
        assert_eq!(
            threshold_values_for_test(&result, &controls).unwrap(),
            (json!(10), json!("pair"))
        );
    }

    #[test]
    fn null_table_result_key_matches_only_literal_na_catalog_metadata() {
        let result =
            row(json!({"checkName":"isRequired","checkLevel":"TABLE","cdmTableName":null}));
        let controls = vec![
            row(json!({"cdmTableName":null,"isRequiredThreshold":1,"isRequiredNotes":"null"})),
            row(json!({"cdmTableName":"NA","isRequiredThreshold":2,"isRequiredNotes":"literal"})),
        ];
        assert_eq!(
            threshold_values_for_test(&result, &controls).unwrap(),
            (json!(2), json!("literal"))
        );
    }

    #[test]
    fn null_field_result_key_matches_only_literal_na_catalog_metadata() {
        let result = row(
            json!({"checkName":"isRequired","checkLevel":"FIELD","cdmTableName":"PERSON","cdmFieldName":null}),
        );
        let controls = vec![
            row(
                json!({"cdmTableName":"PERSON","cdmFieldName":null,"isRequiredThreshold":1,"isRequiredNotes":"null"}),
            ),
            row(
                json!({"cdmTableName":"PERSON","cdmFieldName":"NA","isRequiredThreshold":2,"isRequiredNotes":"literal"}),
            ),
        ];
        assert_eq!(
            threshold_values_for_test(&result, &controls).unwrap(),
            (json!(2), json!("literal"))
        );
    }

    #[test]
    fn null_catalog_lookup_key_does_not_match_null_result_key() {
        let result =
            row(json!({"checkName":"isRequired","checkLevel":"TABLE","cdmTableName":null}));
        let controls = vec![row(
            json!({"cdmTableName":null,"isRequiredThreshold":1,"isRequiredNotes":"null"}),
        )];
        assert_eq!(
            threshold_values_for_test(&result, &controls)
                .unwrap_err()
                .to_string(),
            "replacement has 0 rows, data has 1"
        );
    }

    #[test]
    fn empty_control_csv_uses_preserved_header_for_threshold_presence() {
        let directory = tempfile::tempdir().unwrap();
        let table_file = directory.path().join("table.csv");
        let mut options = crate::model::Options::default();
        options.table_check_threshold_loc = table_file.to_string_lossy().into_owned();
        let result =
            row(json!({"checkName":"isRequired","checkLevel":"TABLE","cdmTableName":"PERSON"}));

        std::fs::write(&table_file, "cdmTableName,isRequiredThreshold\n").unwrap();
        let catalog = Catalog::load(&options).unwrap();
        assert!(catalog.tables.is_empty());
        assert!(
            threshold_values(&result, &catalog)
                .unwrap_err()
                .to_string()
                .contains("replacement has 0 rows")
        );

        std::fs::write(&table_file, "cdmTableName\n").unwrap();
        let catalog = Catalog::load(&options).unwrap();
        assert_eq!(
            threshold_values(&result, &catalog).unwrap(),
            (Value::Null, Value::Null)
        );
    }
}
