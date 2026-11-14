//! DuckDB connection, mirroring DatabaseConnector 8.0.0's `dbms = "duckdb"` (R `duckdb` package,
//! DuckDB engine 2.0.0): `createConnectionDetails(dbms = "duckdb", server = "<file.duckdb>")`.
//!
//! Unlike SQLite and PostgreSQL there is no JDBC layer here: R talks to DuckDB through DBI, so rows
//! carry DuckDB's own types. Error text is DuckDB's own; fine-tuning against R output comes later.

use crate::javacompat::{civil_from_days, local_date_string, single_statement, split_statements};
use crate::{Connection, Row};
use anyhow::{Result, bail};
use duckdb::types::{TimeUnit, Value as DuckValue, ValueRef};
use duckdb::{Config, params_from_iter};
use serde_json::{Number, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

pub struct DuckDbConnection {
    // Declared first, so it is dropped before the shared database instance below.
    conn: duckdb::Connection,
    _database: Option<Arc<Database>>,
}

/// One open DuckDB database instance (file). DuckDB allows a file to be open in only one
/// instance at a time, so worker connections in this process are clones of one root connection
/// (`Connection::try_clone`), not separate opens. The instance closes when the last connection
/// that uses it is dropped.
struct Database {
    root: Mutex<duckdb::Connection>,
}

fn databases() -> &'static Mutex<HashMap<PathBuf, Weak<Database>>> {
    static DATABASES: OnceLock<Mutex<HashMap<PathBuf, Weak<Database>>>> = OnceLock::new();
    DATABASES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The `dqd` binary delay-loads duckdb.dll (see the root `build.rs`); a missing DLL would abort
/// the process on first use, so fail with an error instead. Windows looks in the executable's
/// folder, then the system folders, then `PATH`.
#[cfg(windows)]
fn check_duckdb_library() -> Result<()> {
    let mut folders: Vec<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .into_iter()
        .collect();
    if let Some(path) = std::env::var_os("PATH") {
        folders.extend(std::env::split_paths(&path));
    }
    if let Some(root) = std::env::var_os("SystemRoot") {
        folders.push(PathBuf::from(root).join("System32"));
    }
    if folders
        .iter()
        .any(|folder| folder.join("duckdb.dll").is_file())
    {
        Ok(())
    } else {
        bail!(
            "duckdb.dll (DuckDB 2.0.0) was not found next to the executable or on PATH. Download duckdb-shared-libs-windows-amd64.tar.gz from https://github.com/duckdb/duckdb/releases/tag/v2.0.0 and place duckdb.dll next to dqd.exe."
        )
    }
}

#[cfg(not(windows))]
fn check_duckdb_library() -> Result<()> {
    Ok(())
}

impl DuckDbConnection {
    /// Opens (creating if absent) the database file; `:memory:` gives a private in-memory database.
    pub fn open(path: &Path) -> Result<Self> {
        check_duckdb_library()?;
        if path.as_os_str() == ":memory:" {
            let conn = duckdb::Connection::open_in_memory_with_flags(Config::default())?;
            return Ok(DuckDbConnection {
                conn,
                _database: None,
            });
        }
        let key = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut open = databases().lock().expect("DuckDB registry lock poisoned");
        let database = match open.get(&key).and_then(Weak::upgrade) {
            Some(database) => database,
            None => {
                let root = duckdb::Connection::open_with_flags(&key, Config::default())?;
                let database = Arc::new(Database {
                    root: Mutex::new(root),
                });
                open.insert(key, Arc::downgrade(&database));
                database
            }
        };
        let conn = database
            .root
            .lock()
            .expect("DuckDB root connection lock poisoned")
            .try_clone()?;
        Ok(DuckDbConnection {
            conn,
            _database: Some(database),
        })
    }
}

fn double_value(value: f64) -> Value {
    // JSON has no NaN/Infinity; jsonlite would emit null for these as well.
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `YYYY-MM-DD HH:MM:SS[.ffffff]`, the text jsonlite produces for a UTC POSIXct.
fn timestamp_text(unit: TimeUnit, value: i64) -> String {
    let micros: i128 = match unit {
        TimeUnit::Second => value as i128 * 1_000_000,
        TimeUnit::Millisecond => value as i128 * 1_000,
        TimeUnit::Microsecond => value as i128,
        TimeUnit::Nanosecond => value as i128 / 1_000,
    };
    let secs = micros.div_euclid(1_000_000) as i64;
    let frac = micros.rem_euclid(1_000_000) as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let sod = secs.rem_euclid(86_400);
    let mut text = format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        sod / 3600,
        sod / 60 % 60,
        sod % 60
    );
    if frac != 0 {
        text.push_str(format!(".{frac:06}").trim_end_matches('0'));
    }
    text
}

/// Maps one DuckDB value to JSON. Integers (including HUGEINT/UBIGINT) and floating/decimal values
/// become JSON numbers, VARCHAR strings, BOOLEAN booleans, DATE `YYYY-MM-DD`, NULL null.
fn read_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Boolean(v) => Value::Bool(v),
        ValueRef::TinyInt(v) => Value::from(v),
        ValueRef::SmallInt(v) => Value::from(v),
        ValueRef::Int(v) => Value::from(v),
        ValueRef::BigInt(v) => Value::from(v),
        ValueRef::UTinyInt(v) => Value::from(v),
        ValueRef::USmallInt(v) => Value::from(v),
        ValueRef::UInt(v) => Value::from(v),
        ValueRef::UBigInt(v) => Value::from(v),
        // Beyond i64 a HUGEINT can only be a double, as R's `duckdb` returns it.
        ValueRef::HugeInt(v) => match i64::try_from(v) {
            Ok(small) => Value::from(small),
            Err(_) => double_value(v as f64),
        },
        ValueRef::Float(v) => double_value(v as f64),
        ValueRef::Double(v) => double_value(v),
        ValueRef::Decimal(v) => double_value(v.to_string().parse().unwrap_or(f64::NAN)),
        ValueRef::Date32(days) => {
            let (y, m, d) = civil_from_days(days as i64);
            Value::String(local_date_string(y, m, d))
        }
        ValueRef::Timestamp(unit, v) => Value::String(timestamp_text(unit, v)),
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            Value::String(String::from_utf8_lossy(bytes).into_owned())
        }
        // TIME, INTERVAL, nested and other rare types are not used by DQD queries.
        other => Value::String(format!("{:?}", DuckValue::from(other))),
    }
}

/// The message R's `duckdb` package raises for a failed statement: DuckDB's own text (`Catalog
/// Error: ...`, with the engine's `LINE n:` caret excerpt), then the client context and the error
/// type, e.g. `ℹ Context: rapi_prepare` and `ℹ Error type: CATALOG` (the type is the message's
/// `<Type> Error:` prefix, upper-cased, with spaces as underscores).
fn r_duckdb_error(error: duckdb::Error, context: &str) -> anyhow::Error {
    let message = match &error {
        duckdb::Error::DuckDBFailure(_, Some(message)) => message.clone(),
        other => other.to_string(),
    };
    let kind = message
        .split_once(" Error:")
        .map(|(kind, _)| kind.trim().to_uppercase().replace(' ', "_"));
    match kind {
        Some(kind) if !kind.contains('\n') => {
            anyhow::anyhow!("{message}\nℹ Context: {context}\nℹ Error type: {kind}")
        }
        _ => anyhow::anyhow!("{message}\nℹ Context: {context}"),
    }
}

impl Connection for DuckDbConnection {
    fn query(&mut self, sql: &str) -> Result<Vec<Row>> {
        let text = single_statement(sql).map_err(anyhow::Error::from)?;
        let mut stmt = self
            .conn
            .prepare(&text)
            .map_err(|e| r_duckdb_error(e, "rapi_prepare"))?;
        let mut cursor = stmt
            .query([])
            .map_err(|e| r_duckdb_error(e, "rapi_execute"))?;
        let mut rows = Vec::new();
        let mut labels: Option<Vec<String>> = None;
        while let Some(row) = cursor
            .next()
            .map_err(|e| r_duckdb_error(e, "rapi_execute"))?
        {
            let labels =
                labels.get_or_insert_with(|| row.as_ref().column_names().into_iter().collect());
            let mut out = Row::new();
            for (index, label) in labels.iter().enumerate() {
                out.insert(label.clone(), read_value(row.get_ref(index)?));
            }
            rows.push(out);
        }
        Ok(rows)
    }

    fn execute(&mut self, sql: &str) -> Result<()> {
        for statement in split_statements(sql).map_err(anyhow::Error::from)? {
            self.conn
                .execute_batch(&statement)
                .map_err(|e| r_duckdb_error(e, "rapi_prepare"))?;
        }
        Ok(())
    }

    fn insert_rows(&mut self, table_name: &str, rows: &[Row]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let columns: Vec<&String> = rows[0].keys().collect();
        if rows.iter().any(|row| {
            row.len() != columns.len() || columns.iter().any(|column| !row.contains_key(*column))
        }) {
            bail!("Inserted rows must have the same columns in the same shape")
        }
        let quoted: Vec<String> = columns
            .iter()
            .map(|column| format!("\"{}\"", column.replace('"', "\"\"")))
            .collect();
        let sql = format!(
            "INSERT INTO {table_name} ({}) VALUES ({})",
            quoted.join(","),
            vec!["?"; columns.len()].join(",")
        );
        // One transaction: per-row autocommit is very slow in DuckDB.
        self.conn.execute_batch("BEGIN TRANSACTION")?;
        let result = (|| -> Result<()> {
            let mut stmt = self.conn.prepare(&sql)?;
            for row in rows {
                let values = columns.iter().map(|column| to_duck(&row[*column]));
                stmt.execute(params_from_iter(values))?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
        Ok(())
    }

    fn take_warnings(&mut self) -> Vec<String> {
        Vec::new()
    }
}

fn to_duck(value: &Value) -> DuckValue {
    match value {
        Value::Null => DuckValue::Null,
        Value::Bool(v) => DuckValue::Boolean(*v),
        Value::Number(n) if n.is_i64() => DuckValue::BigInt(n.as_i64().unwrap_or_default()),
        Value::Number(n) if n.is_u64() => DuckValue::UBigInt(n.as_u64().unwrap_or_default()),
        Value::Number(n) => DuckValue::Double(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(text) => DuckValue::Text(text.clone()),
        other => DuckValue::Text(other.to_string()),
    }
}
