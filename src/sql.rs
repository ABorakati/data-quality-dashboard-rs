use crate::model::Row;
use anyhow::{Result, anyhow};
use dqd_sqlrender::{Translator, try_render_sql, try_split_sql};
use serde_json::Value;
use std::sync::LazyLock;

pub trait SqlEngine {
    fn render(&mut self, sql: &str, parameters: &Row) -> Result<String>;
    fn translate(&mut self, sql: &str, dialect: &str) -> Result<String>;
    fn split(&mut self, sql: &str) -> Result<Vec<String>>;
    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

static TRANSLATOR: LazyLock<Translator> = LazyLock::new(Translator::new);

fn translator() -> &'static Translator {
    &TRANSLATOR
}

/// In-process SqlRender port: render, translate and split with the shared pattern set.
#[derive(Debug, Default)]
pub struct NativeSql {
    warnings: Vec<String>,
}

impl NativeSql {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_render_translate(
        &mut self,
        sql: &str,
        parameters: &Row,
        dialect: &str,
    ) -> Result<String> {
        let rendered = self.render(sql, parameters)?;
        if dialect == "sql server" {
            Ok(rendered)
        } else {
            self.translate(&rendered, dialect)
        }
    }
}

impl SqlEngine for NativeSql {
    fn render(&mut self, sql: &str, parameters: &Row) -> Result<String> {
        self.warnings.clear();
        let names: Vec<&str> = parameters.keys().map(String::as_str).collect();
        let values: Vec<String> = parameters.values().map(r_value_string).collect();
        let values: Vec<&str> = values.iter().map(String::as_str).collect();
        try_render_sql(sql, &names, &values).map_err(|error| anyhow!("{error}"))
    }

    fn translate(&mut self, sql: &str, dialect: &str) -> Result<String> {
        let translator = translator();
        self.warnings = translator.check(sql, dialect);
        translator
            .translate(sql, dialect, None)
            .map_err(|error| anyhow!("{error}"))
    }

    fn split(&mut self, sql: &str) -> Result<Vec<String>> {
        self.warnings.clear();
        try_split_sql(sql).map_err(|error| anyhow!("{error}"))
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Mirrors SqlRender's R wrapper `paste(parameter, collapse=",")` conversion.
pub fn r_value_string(value: &Value) -> String {
    match value {
        Value::Null => "NA".to_owned(),
        Value::Bool(true) => "TRUE".to_owned(),
        Value::Bool(false) => "FALSE".to_owned(),
        Value::String(s) => s.clone(),
        Value::Number(n) => r_number_string(n),
        Value::Array(values) => values
            .iter()
            .map(r_value_string)
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

fn r_number_string(value: &serde_json::Number) -> String {
    if let Some(number) = value.as_i64() {
        return number.to_string();
    }
    if let Some(number) = value.as_u64() {
        return number.to_string();
    }
    if let Some(number) = value.as_f64() {
        if number.is_finite() && number.fract() == 0.0 {
            return format!("{number:.0}");
        }
        return number.to_string();
    }
    value.to_string()
}

/// Collects named SQL render arguments into the insertion-ordered parameter map.
pub fn parameters<I, K>(values: I) -> Row
where
    I: IntoIterator<Item = (K, Value)>,
    K: Into<String>,
{
    values
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}
