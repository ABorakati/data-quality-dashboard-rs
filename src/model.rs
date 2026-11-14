use anyhow::{Result, bail};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{IgnoredAny, MapAccess, Visitor},
};
use serde_json::{Map, Value};
use std::{fmt, path::PathBuf};

pub type Row = Map<String, Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub description: Row,
    pub parameters: Row,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionConfig {
    /// SQLite: the database file (DatabaseConnector `server`).
    #[serde(alias = "path")]
    pub sqlite_path: Option<PathBuf>,
    /// PostgreSQL: `host/database`, the DatabaseConnector convention.
    pub server: Option<String>,
    /// PostgreSQL: port, 5432 when absent.
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
}

/// Execution arguments mirror executeDqChecks. Empty strings represent arguments
/// required by the R function, not invented default schema or source names.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub cdm_database_schema: String,
    pub results_database_schema: String,
    pub vocab_database_schema: String,
    pub cdm_source_name: String,
    pub num_threads: f64,
    pub sql_only: bool,
    pub sql_only_union_count: f64,
    pub sql_only_incremental_insert: bool,
    pub output_folder: PathBuf,
    pub output_file: String,
    pub verbose_mode: bool,
    pub write_to_table: bool,
    pub write_table_name: String,
    pub write_to_csv: bool,
    pub csv_file: String,
    pub check_levels: Vec<String>,
    pub check_names: Vec<String>,
    pub check_severity: Vec<String>,
    pub cohort_definition_id: Vec<Value>,
    pub cohort_database_schema: String,
    pub cohort_table_name: String,
    pub tables_to_exclude: Vec<String>,
    pub cdm_version: String,
    pub table_check_threshold_loc: String,
    pub field_check_threshold_loc: String,
    pub concept_check_threshold_loc: String,
    pub dbms: String,
    pub connection: ConnectionConfig,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cdm_database_schema: String::new(),
            results_database_schema: String::new(),
            vocab_database_schema: String::new(),
            cdm_source_name: String::new(),
            num_threads: default_num_threads(),
            sql_only: false,
            sql_only_union_count: 1.0,
            sql_only_incremental_insert: false,
            output_folder: PathBuf::new(),
            output_file: String::new(),
            verbose_mode: false,
            write_to_table: true,
            write_table_name: "dqdashboard_results".into(),
            write_to_csv: false,
            csv_file: String::new(),
            check_levels: ["TABLE", "FIELD", "CONCEPT"].map(String::from).into(),
            check_names: Vec::new(),
            check_severity: ["fatal", "convention", "characterization"]
                .map(String::from)
                .into(),
            cohort_definition_id: Vec::new(),
            cohort_database_schema: String::new(),
            cohort_table_name: "cohort".into(),
            tables_to_exclude: [
                "CONCEPT",
                "VOCABULARY",
                "CONCEPT_ANCESTOR",
                "CONCEPT_RELATIONSHIP",
                "CONCEPT_CLASS",
                "CONCEPT_SYNONYM",
                "RELATIONSHIP",
                "DOMAIN",
                "PACK_CONTENT",
                "CONCEPT_METADATA",
                "CONCEPT_RELATIONSHIP_METADATA",
            ]
            .map(String::from)
            .into(),
            cdm_version: "5.3".into(),
            table_check_threshold_loc: "default".into(),
            field_check_threshold_loc: "default".into(),
            concept_check_threshold_loc: "default".into(),
            dbms: "sql server".into(),
            connection: ConnectionConfig::default(),
        }
    }
}

/// Default worker count when neither the options file nor the CLI sets `numThreads`: 8, the
/// fastest setting measured on SQLite (`docs/performance.md`), capped at the machine's available
/// parallelism. This deliberately diverges from upstream's `numThreads = 1`; results do not
/// depend on the thread count.
pub const PREFERRED_NUM_THREADS: usize = 8;

pub fn default_num_threads() -> f64 {
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    PREFERRED_NUM_THREADS.min(available) as f64
}

impl<'de> Deserialize<'de> for Options {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct OptionsVisitor;
        impl<'de> Visitor<'de> for OptionsVisitor {
            type Value = Options;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an executeDqChecks options object")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Options, M::Error> {
                let mut options = Options::default();
                let mut vocab_present = false;
                let mut cohort_schema_present = false;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "cdmDatabaseSchema" => options.cdm_database_schema = map.next_value()?,
                        "resultsDatabaseSchema" => {
                            options.results_database_schema = map.next_value()?
                        }
                        "vocabDatabaseSchema" => {
                            options.vocab_database_schema = map.next_value()?;
                            vocab_present = true;
                        }
                        "cdmSourceName" => options.cdm_source_name = map.next_value()?,
                        "numThreads" => options.num_threads = map.next_value()?,
                        "sqlOnly" => options.sql_only = map.next_value()?,
                        "sqlOnlyUnionCount" => options.sql_only_union_count = map.next_value()?,
                        "sqlOnlyIncrementalInsert" => {
                            options.sql_only_incremental_insert = map.next_value()?
                        }
                        "outputFolder" => options.output_folder = map.next_value()?,
                        "outputFile" => options.output_file = map.next_value()?,
                        "verboseMode" => options.verbose_mode = map.next_value()?,
                        "writeToTable" => options.write_to_table = map.next_value()?,
                        "writeTableName" => options.write_table_name = map.next_value()?,
                        "writeToCsv" => options.write_to_csv = map.next_value()?,
                        "csvFile" => options.csv_file = map.next_value()?,
                        "checkLevels" => {
                            let value: Value = map.next_value()?;
                            options.check_levels = character_vector(value, false)?;
                        }
                        "checkNames" => {
                            let value: Value = map.next_value()?;
                            options.check_names = character_vector(value, true)?;
                        }
                        "checkSeverity" => {
                            let value: Value = map.next_value()?;
                            options.check_severity = character_vector(value, false)?;
                        }
                        "cohortDefinitionId" => {
                            let value: Value = map.next_value()?;
                            options.cohort_definition_id = match value {
                                Value::Null => vec![Value::Null],
                                Value::Array(values) => values,
                                value => vec![value],
                            };
                        }
                        "cohortDatabaseSchema" => {
                            options.cohort_database_schema = map.next_value()?;
                            cohort_schema_present = true;
                        }
                        "cohortTableName" => options.cohort_table_name = map.next_value()?,
                        "tablesToExclude" => {
                            let value: Value = map.next_value()?;
                            options.tables_to_exclude = character_vector(value, true)?;
                        }
                        "cdmVersion" => options.cdm_version = map.next_value()?,
                        "tableCheckThresholdLoc" => {
                            options.table_check_threshold_loc = map.next_value()?
                        }
                        "fieldCheckThresholdLoc" => {
                            options.field_check_threshold_loc = map.next_value()?
                        }
                        "conceptCheckThresholdLoc" => {
                            options.concept_check_threshold_loc = map.next_value()?
                        }
                        "dbms" => options.dbms = map.next_value()?,
                        "connection" | "connectionDetails" => {
                            options.connection = map.next_value()?
                        }
                        _ => {
                            let _: IgnoredAny = map.next_value()?;
                        }
                    }
                }
                if !vocab_present {
                    options.vocab_database_schema = options.cdm_database_schema.clone();
                }
                if !cohort_schema_present {
                    options.cohort_database_schema = options.results_database_schema.clone();
                }
                Ok(options)
            }
        }
        deserializer.deserialize_map(OptionsVisitor)
    }
}

fn character_vector<E: serde::de::Error>(
    value: Value,
    null_as_empty: bool,
) -> std::result::Result<Vec<String>, E> {
    match value {
        Value::String(value) => Ok(vec![value]),
        Value::Array(values) => values
            .into_iter()
            .map(|value| match value {
                Value::String(value) => Ok(value),
                _ => Err(E::custom("expected string elements in character vector")),
            })
            .collect(),
        Value::Null if null_as_empty => Ok(Vec::new()),
        _ => Err(E::custom("expected a string or an array of strings")),
    }
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        // Preserve the upstream regex's unescaped dot (^5.[2-5]$).
        let mut version = self.cdm_version.chars();
        if version.next() != Some('5')
            || version.next().is_none()
            || !matches!(version.next(), Some('2'..='5'))
            || version.next().is_some()
        {
            bail!(
                "cdmVersion must contain a version of the form '5.X' where X is an integer between 2 and 5 inclusive."
            );
        }
        if self.sql_only_incremental_insert && !self.sql_only {
            bail!("Set `sqlOnly` to TRUE in order to use `sqlOnlyIncrementalInsert` mode.");
        }
        if !(self.sql_only_union_count > 0.0) {
            bail!("sqlOnlyUnionCount must be numeric and greater than zero");
        }
        if self
            .check_levels
            .iter()
            .any(|v| !["TABLE", "FIELD", "CONCEPT"].contains(&v.as_str()))
        {
            bail!(
                "checkLevels argument must be a subset of c(\"TABLE\", \"FIELD\", \"CONCEPT\"). You passed in {}",
                self.check_levels.join(", ")
            );
        }
        if self
            .check_severity
            .iter()
            .any(|v| !["fatal", "convention", "characterization"].contains(&v.as_str()))
        {
            bail!(
                "checkSeverity argument must be a subset of c(\"fatal\", \"convention\", \"characterization\"). You passed in {}",
                self.check_severity.join(", ")
            );
        }
        if !matches!(self.dbms.as_str(), "sqlite" | "postgresql") {
            bail!(
                "Unsupported dbms '{}': only 'sqlite' and 'postgresql' are supported.",
                self.dbms
            );
        }
        Ok(())
    }

    pub fn cohort_enabled(&self) -> bool {
        // Upstream tests length(cohortDefinitionId > 0), not whether IDs are positive.
        !self.cohort_definition_id.is_empty()
    }

    pub fn thread_count(&self) -> Result<usize> {
        r_positive_index(self.num_threads)
    }

    /// Reproduce R's ustart:uend slices, including fractional numeric counts.
    pub fn sql_union_batches(&self, len: usize) -> Result<Vec<std::ops::Range<usize>>> {
        let count = self.sql_only_union_count;
        if !(count > 0.0) {
            bail!("sqlOnlyUnionCount must be numeric and greater than zero");
        }
        let mut batches = Vec::new();
        let mut start = 1.0_f64;
        while start <= len as f64 {
            let end = (start + count - 1.0).min(len as f64);
            let sequence_len = ((end - start).abs() + 1e-7).floor() as usize + 1;
            let begin_index = start.trunc() as usize - 1;
            batches.push(begin_index..(begin_index + sequence_len).min(len));
            let next = start + count;
            if next == start {
                bail!("sqlOnlyUnionCount is too small to advance R indexing");
            }
            start = next;
        }
        Ok(batches)
    }
}

pub fn r_positive_index(value: f64) -> Result<usize> {
    if !value.is_finite() || value < 1.0 || value > i32::MAX as f64 {
        bail!("invalid value for numberOfThreads");
    }
    Ok(value.trunc() as usize)
}

/// R's character representation used by SqlRender and check identifiers.
pub fn value_text(value: &Value) -> String {
    match value {
        Value::Null => "NA".into(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => {
            if *value {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Value::Number(number) => number
            .as_i64()
            .map(|value| value.to_string())
            .or_else(|| {
                number.as_u64().map(|value| value.to_string()).or_else(|| {
                    number.as_f64().map(|value| {
                        if value.fract() == 0.0 {
                            format!("{value:.0}")
                        } else {
                            value.to_string()
                        }
                    })
                })
            })
            .unwrap_or_else(|| number.to_string()),
        Value::Array(values) => values.iter().map(value_text).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

pub fn check_id(level: &Value, name: &Value, parameters: &Row) -> String {
    [
        Some(level),
        Some(name),
        parameters.get("cdmTableName"),
        parameters.get("cdmFieldName"),
        parameters.get("conceptId"),
        parameters.get("unitConceptId"),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.is_null())
    .map(|value| value_text(value).replace(' ', ""))
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join("_")
    .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schemas_inherit_only_when_omitted() {
        let inherited: Options =
            serde_json::from_value(json!({"cdmDatabaseSchema":"cdm","resultsDatabaseSchema":"dq"}))
                .unwrap();
        assert_eq!(inherited.vocab_database_schema, "cdm");
        assert_eq!(inherited.cohort_database_schema, "dq");
        let explicit: Options =
            serde_json::from_value(json!({"cdmDatabaseSchema":"cdm","vocabDatabaseSchema":""}))
                .unwrap();
        assert_eq!(explicit.vocab_database_schema, "");
    }

    #[test]
    fn num_threads_defaults_to_eight_capped_by_cpus_and_explicit_values_win() {
        let available = std::thread::available_parallelism().map_or(1, usize::from);
        let unset: Options = serde_json::from_value(json!({})).unwrap();
        assert_eq!(unset.num_threads, 8.min(available) as f64);
        assert_eq!(Options::default().num_threads, unset.num_threads);
        let explicit: Options = serde_json::from_value(json!({"numThreads": 1})).unwrap();
        assert_eq!(explicit.num_threads, 1.0);
    }

    #[test]
    fn cohort_activation_checks_length_not_id_sign() {
        let options: Options = serde_json::from_value(json!({"cohortDefinitionId":0})).unwrap();
        assert!(options.cohort_enabled());
        assert_eq!(options.cohort_definition_id, [Value::from(0)]);
    }

    #[test]
    fn character_vector_options_accept_scalar_strings_and_preserve_empty_null_behavior() {
        let scalar: Options = serde_json::from_value(json!({
            "checkLevels": "FIELD",
            "checkNames": "cdmField",
            "checkSeverity": "fatal",
            "tablesToExclude": "PERSON"
        }))
        .unwrap();
        assert_eq!(scalar.check_levels, ["FIELD"]);
        assert_eq!(scalar.check_names, ["cdmField"]);
        assert_eq!(scalar.check_severity, ["fatal"]);
        assert_eq!(scalar.tables_to_exclude, ["PERSON"]);

        let empty: Options = serde_json::from_value(json!({
            "checkLevels": [],
            "checkNames": [],
            "checkSeverity": [],
            "tablesToExclude": []
        }))
        .unwrap();
        assert!(empty.check_levels.is_empty());
        assert!(empty.check_names.is_empty());
        assert!(empty.check_severity.is_empty());
        assert!(empty.tables_to_exclude.is_empty());

        let null_empty: Options = serde_json::from_value(json!({
            "checkNames": null,
            "tablesToExclude": null
        }))
        .unwrap();
        assert!(null_empty.check_names.is_empty());
        assert!(null_empty.tables_to_exclude.is_empty());
        assert!(serde_json::from_value::<Options>(json!({"checkLevels": null})).is_err());
        assert!(serde_json::from_value::<Options>(json!({"checkSeverity": null})).is_err());
    }
}
