//! Helpers shared by the parity suites (`tests/parity.rs`, `tests/upstream.rs`, `tests/eunomia.rs`).
#![allow(dead_code)]

use data_quality_dashboard::{model::Options, runner};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Root of the generated oracle fixtures (`DQD_ORACLE_DIR` overrides it).
pub fn oracle() -> PathBuf {
    std::env::var_os("DQD_ORACLE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/generated"))
}

/// Reads one JSON fixture below the oracle root.
pub fn fixture(name: &str) -> Value {
    read_json(&oracle().join(name))
}

pub fn read_json(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap_or_else(|e| {
        panic!(
            "missing oracle fixture {}: {e}; run scripts/parity.ps1",
            path.display()
        )
    });
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("invalid oracle fixture {}: {e}", path.display()))
}

/// Structural JSON equality that compares numbers as f64, so `1` equals `1.0`.
pub fn assert_json_equal(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) if a.as_f64().is_some() && b.as_f64().is_some() => {
            assert_eq!(a.as_f64(), b.as_f64(), "{path}")
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "array length at {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_json_equal(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "object key count at {path}");
            for (key, a) in a {
                assert_json_equal(
                    a,
                    b.get(key).unwrap_or_else(|| panic!("missing {path}.{key}")),
                    &format!("{path}.{key}"),
                );
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

/// An R character vector as jsonlite writes it with `auto_unbox`: a string, an array, or null.
pub fn string_vector(value: &Value) -> Vec<String> {
    match value {
        Value::String(value) => vec![value.clone()],
        Value::Array(values) => values
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect(),
        Value::Null => Vec::new(),
        other => panic!("expected oracle string vector, got {other}"),
    }
}

/// Error text embeds the absolute path of the per-check error report; keep only its file name.
pub fn normalize_error_report_path(error: &str) -> String {
    let marker = "An error report has been created at  ";
    let Some((prefix, path)) = error.split_once(marker) else {
        return error.to_owned();
    };
    let path = path.replace('\\', "/");
    let Some((_, suffix)) = path.rsplit_once("/errors/") else {
        return error.to_owned();
    };
    format!("{prefix}{marker}<output>/errors/{suffix}")
}

/// Parses result CSV text, blanking `executionTime` and normalizing error report paths.
pub fn csv_records(text: &str) -> Vec<Vec<String>> {
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let headers = reader.headers().expect("CSV header").clone();
    let execution_time = headers.iter().position(|header| header == "executionTime");
    let error_message = headers.iter().position(|header| header == "error");
    let mut rows = vec![headers.iter().map(|header| header.to_owned()).collect()];
    for record in reader.records() {
        let mut fields = record
            .expect("valid parity CSV")
            .iter()
            .map(|field| field.to_owned())
            .collect::<Vec<_>>();
        if let Some(index) = execution_time {
            fields[index].clear();
        }
        if let Some(index) = error_message {
            fields[index] = normalize_error_report_path(&fields[index]);
        }
        rows.push(fields);
    }
    rows
}

/// Removes run-dependent values (timestamps, timings, output paths) from a result object.
pub fn strip_volatile(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in [
        "startTimestamp",
        "endTimestamp",
        "executionTime",
        "executionTimeSeconds",
        "outputFile",
    ] {
        object.remove(key);
    }
    if let Some(rows) = object.get_mut("CheckResults").and_then(Value::as_array_mut) {
        for row in rows {
            if let Some(row) = row.as_object_mut() {
                if let Some(Value::String(error)) = row.get_mut("error") {
                    *error = normalize_error_report_path(error);
                }
                row.remove("executionTime");
                row.remove("EXECUTION_TIME");
            }
        }
    }
}

/// Runs the Rust runner and returns the results file it wrote, which is what upstream's
/// `results.json` holds (jsonlite rounds to 4 digits there, unlike the in-memory return value).
pub fn run_and_read_results_file(name: &str, options: &Options) -> Value {
    runner::execute_dq_checks(options).unwrap_or_else(|e| panic!("{name}: {e:#}"));
    let path = options.output_folder.join(&options.output_file);
    serde_json::from_slice(
        &fs::read(&path).unwrap_or_else(|e| panic!("{name}: reading {}: {e}", path.display())),
    )
    .unwrap_or_else(|e| panic!("{name}: invalid {}: {e}", path.display()))
}

/// Compares two full result files check by check (clear failure location), then as a whole.
pub fn assert_results_match(name: &str, actual: &Value, expected: &Value) {
    let mut actual = actual.clone();
    let mut expected = expected.clone();
    strip_volatile(&mut actual);
    strip_volatile(&mut expected);
    let expected_rows = expected["CheckResults"].as_array().expect("oracle rows");
    let actual_rows = actual["CheckResults"].as_array().expect("Rust rows");
    assert_eq!(actual_rows.len(), expected_rows.len(), "{name} check count");
    for (index, (actual_row, expected_row)) in actual_rows.iter().zip(expected_rows).enumerate() {
        assert_json_equal(
            actual_row,
            expected_row,
            &format!("{name} CheckResults[{index}] {}", expected_row["checkId"]),
        );
    }
    assert_json_equal(
        &actual["Metadata"],
        &expected["Metadata"],
        &format!("{name} Metadata"),
    );
    assert_json_equal(&actual, &expected, name);
}
