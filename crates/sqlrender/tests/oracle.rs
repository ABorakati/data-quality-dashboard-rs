//! Byte-for-byte parity against the SqlRender 1.19.7 oracle fixture
//! (`tests/oracle/generated/render.json`, produced by the real R/Java stack).

use dqd_sqlrender::{Translator, render_sql, split_sql};
use serde_json::Value;
use std::sync::LazyLock;

static TRANSLATOR: LazyLock<Translator> = LazyLock::new(Translator::new);

/// Same conversion as the main crate's `sql::r_value_string` (SqlRender's R
/// wrapper `paste(parameter, collapse=",")`).
fn r_value_string(value: &Value) -> String {
    match value {
        Value::Null => "NA".to_owned(),
        Value::Bool(true) => "TRUE".to_owned(),
        Value::Bool(false) => "FALSE".to_owned(),
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 {
                    format!("{f:.0}")
                } else {
                    f.to_string()
                }
            } else {
                n.to_string()
            }
        }
        Value::Array(values) => values
            .iter()
            .map(r_value_string)
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

fn cases() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/oracle/generated/render.json"
    );
    let text = std::fs::read_to_string(path).expect("render.json readable");
    let cases: Vec<Value> = serde_json::from_str(&text).expect("render.json parses");
    assert_eq!(cases.len(), 864);
    cases
}

fn label(case: &Value) -> String {
    format!(
        "version {} cohort {} dialect {} check {}",
        case["version"], case["cohort"], case["dialect"], case["description"]["checkName"]
    )
}

#[test]
fn render_translate_split_match_oracle() {
    let mut checked = [0usize; 3];
    let mut failures = Vec::new();
    for case in cases() {
        let dialect = case["dialect"].as_str().expect("dialect");
        if !matches!(dialect, "sql server" | "sqlite" | "postgresql") {
            continue;
        }
        let template = case["template"].as_str().expect("template");
        let parameters = case["parameters"].as_object().expect("parameters");
        let names: Vec<&str> = parameters.keys().map(String::as_str).collect();
        let values: Vec<String> = parameters.values().map(r_value_string).collect();
        let values: Vec<&str> = values.iter().map(String::as_str).collect();

        let expected_rendered = case["rendered"]["value"].as_str().expect("rendered");
        let rendered = render_sql(template, &names, &values);
        if rendered != expected_rendered {
            failures.push(format!("render mismatch: {}", label(&case)));
            continue;
        }
        checked[0] += 1;

        let expected_translated = case["translated"]["value"].as_str().expect("translated");
        if dialect != "sql server" {
            let translated = TRANSLATOR
                .translate(&rendered, dialect, None)
                .expect("supported dialect");
            if translated != expected_translated {
                failures.push(format!("translate mismatch: {}", label(&case)));
                continue;
            }
            checked[1] += 1;
        }

        let expected_statements: Vec<&str> = case["statements"]["value"]
            .as_array()
            .expect("statements")
            .iter()
            .map(|s| s.as_str().expect("statement"))
            .collect();
        let statements = split_sql(expected_translated);
        if statements != expected_statements {
            failures.push(format!("split mismatch: {}", label(&case)));
            continue;
        }
        checked[2] += 1;
    }
    println!(
        "oracle parity: render {} / translate {} / split {}",
        checked[0], checked[1], checked[2]
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // 27 templates x cohort on/off: 54 cases per dialect.
    assert_eq!(checked, [162, 108, 162]);
}
