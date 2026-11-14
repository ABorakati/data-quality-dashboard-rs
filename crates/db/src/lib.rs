//! Native SQLite and PostgreSQL connections for the DQD Rust port.
//!
//! Behavior (row values, error text, warnings, insert typing) reproduces what the former JDBC bridge
//! produced with sqlite-jdbc 3.50.3.0 and the pgjdbc driver DatabaseConnector downloads (42.7.3).

#[cfg(feature = "duckdb")]
mod duckdb;
mod javacompat;
mod pg;
mod pgparse;
mod sqlite;
mod sqlite_codes;
mod sqlite_math;

use std::path::PathBuf;

pub use javacompat::DbError;

/// Same type as the main crate's `model::Row`.
pub type Row = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dbms {
    Sqlite,
    Postgresql,
    Duckdb,
}

#[derive(Debug, Clone)]
pub struct ConnectSpec {
    pub dbms: Dbms,
    /// sqlite: the database file (DatabaseConnector `server`).
    pub sqlite_path: Option<PathBuf>,
    /// postgresql: `"host/database"` (DatabaseConnector convention).
    pub server: Option<String>,
    /// postgresql: default 5432.
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
}

pub trait Connection: Send {
    /// Exactly one statement (split with `dqd_sqlrender::try_split_sql`); raw column labels.
    fn query(&mut self, sql: &str) -> anyhow::Result<Vec<Row>>;
    /// Split with `dqd_sqlrender::try_split_sql`, run each statement.
    fn execute(&mut self, sql: &str) -> anyhow::Result<()>;
    fn insert_rows(&mut self, table_name: &str, rows: &[Row]) -> anyhow::Result<()>;
    /// Warnings of the most recent `query`/`execute`/non-empty `insert_rows`.
    fn take_warnings(&mut self) -> Vec<String>;
}

/// Open a connection in auto-commit mode, as DatabaseConnector does.
pub fn connect(spec: &ConnectSpec) -> anyhow::Result<Box<dyn Connection>> {
    match spec.dbms {
        Dbms::Sqlite => {
            let path = spec
                .sqlite_path
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("connection.sqlitePath is required for sqlite"))?;
            Ok(Box::new(sqlite::SqliteConnection::open(path)?))
        }
        Dbms::Postgresql => Ok(Box::new(pg::PgConnection::connect(spec)?)),
        #[cfg(not(feature = "duckdb"))]
        Dbms::Duckdb => anyhow::bail!(
            "this build was compiled without DuckDB support; rebuild with the `duckdb` Cargo feature (enabled by default) to use dbms 'duckdb'"
        ),
        #[cfg(feature = "duckdb")]
        Dbms::Duckdb => {
            // DatabaseConnector passes the database file as `server`; `sqlite_path` is an alias.
            let path = spec
                .server
                .as_deref()
                .map(PathBuf::from)
                .or_else(|| spec.sqlite_path.clone())
                .ok_or_else(|| anyhow::anyhow!("connection.server is required for duckdb"))?;
            Ok(Box::new(duckdb::DuckDbConnection::open(&path)?))
        }
    }
}
