//! SQLite connection reproducing `org.xerial:sqlite-jdbc` 3.50.3.0 as the former JDBC bridge drove it.

use crate::javacompat::{
    DbError, big_decimal_millis, check_bigint, double_decimal_text, epoch_millis_utc_date,
    epoch_millis_utc_datetime, hybrid_days, instant_string, local_date_of_epoch_millis,
    local_to_epoch_secs, single_statement, split_statements, timestamp_new,
};
use crate::sqlite_codes::SQLITE_ERROR_CODES;
use crate::{Connection, Row};
use rusqlite::ffi;
use serde_json::{Number, Value};
use std::ffi::{CStr, c_char, c_int};
use std::path::Path;
use std::ptr;
use std::sync::LazyLock;

pub struct SqliteConnection {
    conn: rusqlite::Connection,
    warnings: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Errors: org.sqlite.SQLiteException / DB.newSQLException
// ---------------------------------------------------------------------------------------------

/// `DB.newSQLException(errorCode, errorMessage)`.
fn sqlite_exception(code: c_int, message: &str) -> DbError {
    let entry = SQLITE_ERROR_CODES.iter().find(|entry| entry.1 == code);
    match entry {
        Some((name, entry_code, text)) => DbError::new(
            "org.sqlite.SQLiteException",
            format!("[{name}] {text} ({message})"),
            "",
            (*entry_code & 0xff) as i64,
        ),
        None => DbError::new(
            "org.sqlite.SQLiteException",
            format!("[UNKNOWN_ERROR] unknown error:{code} ({message})"),
            "",
            (-1i32 & 0xff) as i64,
        ),
    }
}

unsafe fn errmsg(db: *mut ffi::sqlite3) -> String {
    let message = unsafe { ffi::sqlite3_errmsg(db) };
    if message.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

/// Owns a prepared statement for the duration of one request (`sqlite3_finalize` on drop).
struct Stmt(*mut ffi::sqlite3_stmt);

impl Drop for Stmt {
    fn drop(&mut self) {
        unsafe { ffi::sqlite3_finalize(self.0) };
    }
}

// ---------------------------------------------------------------------------------------------
// JDBC type mapping: JDBC3ResultSet.getColumnType / getColumnTypeName
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JdbcType {
    Boolean,
    Tinyint,
    Smallint,
    Bigint,
    Date,
    Timestamp,
    Integer,
    Decimal,
    Double,
    Numeric,
    Real,
    Float,
    Char,
    Clob,
    Varchar,
    Binary,
    Blob,
}

static CAST_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    // Java: Pattern.compile("cast\\(.*?\\s+as\\s+(.*?)\\s*\\)") (no CASE_INSENSITIVE, `.` excludes line terminators)
    let dot = "[^\\n\\r\\u{85}\\u{2028}\\u{2029}]";
    let space = "[ \\t\\n\\x0B\\x0C\\r]";
    regex::Regex::new(&format!(
        "cast\\({dot}*?{space}+as{space}+({dot}*?){space}*\\)"
    ))
    .expect("cast pattern")
});

/// `getColumnDeclType`: declared type, else the type inside a lower-case `cast(.. as T)` column name.
fn decl_type(declared: Option<String>, column_name: &str) -> Option<String> {
    declared.or_else(|| {
        CAST_PATTERN
            .captures(column_name)
            .map(|captures| captures[1].to_owned())
    })
}

/// `getColumnTypeName`
fn type_name(decl: &Option<String>, value_type: c_int) -> String {
    match decl {
        Some(declared) => {
            let end = declared.find('(').unwrap_or(declared.len());
            declared[..end].to_uppercase()
        }
        None => match value_type {
            ffi::SQLITE_INTEGER => "INTEGER",
            ffi::SQLITE_FLOAT => "FLOAT",
            ffi::SQLITE_BLOB => "BLOB",
            ffi::SQLITE_TEXT => "TEXT",
            _ => "NUMERIC",
        }
        .to_owned(),
    }
}

/// `getColumnType`; `long_value` is only consulted for INTEGER values.
fn jdbc_type(name: &str, value_type: c_int, long_value: i64) -> JdbcType {
    if value_type == ffi::SQLITE_INTEGER || value_type == ffi::SQLITE_NULL {
        match name {
            "BOOLEAN" => return JdbcType::Boolean,
            "TINYINT" => return JdbcType::Tinyint,
            "SMALLINT" | "INT2" => return JdbcType::Smallint,
            "BIGINT" | "INT8" | "UNSIGNED BIG INT" => return JdbcType::Bigint,
            "DATE" | "DATETIME" => return JdbcType::Date,
            "TIMESTAMP" => return JdbcType::Timestamp,
            _ => {}
        }
        if value_type == ffi::SQLITE_INTEGER || matches!(name, "INT" | "INTEGER" | "MEDIUMINT") {
            return if long_value > i32::MAX as i64 || long_value < i32::MIN as i64 {
                JdbcType::Bigint
            } else {
                JdbcType::Integer
            };
        }
    }
    if value_type == ffi::SQLITE_FLOAT || value_type == ffi::SQLITE_NULL {
        match name {
            "DECIMAL" => return JdbcType::Decimal,
            "DOUBLE" | "DOUBLE PRECISION" => return JdbcType::Double,
            "NUMERIC" => return JdbcType::Numeric,
            "REAL" => return JdbcType::Real,
            _ => {}
        }
        if value_type == ffi::SQLITE_FLOAT || name == "FLOAT" {
            return JdbcType::Float;
        }
    }
    if value_type == ffi::SQLITE_TEXT || value_type == ffi::SQLITE_NULL {
        match name {
            "CHARACTER" | "NCHAR" | "NATIVE CHARACTER" | "CHAR" => return JdbcType::Char,
            "CLOB" => return JdbcType::Clob,
            "DATE" | "DATETIME" => return JdbcType::Date,
            "TIMESTAMP" => return JdbcType::Timestamp,
            _ => {}
        }
        if value_type == ffi::SQLITE_TEXT
            || matches!(name, "VARCHAR" | "VARYING CHARACTER" | "NVARCHAR" | "TEXT")
        {
            return JdbcType::Varchar;
        }
    }
    if value_type == ffi::SQLITE_BLOB || value_type == ffi::SQLITE_NULL {
        if name == "BINARY" {
            return JdbcType::Binary;
        }
        if value_type == ffi::SQLITE_BLOB || name == "BLOB" {
            return JdbcType::Blob;
        }
    }
    JdbcType::Numeric
}

fn double_value(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `FastDateFormat("yyyy-MM-dd HH:mm:ss.SSS").parse(text)` (lenient, prefix match) in the default zone,
/// as epoch milliseconds.
fn parse_date_text(text: &str) -> Option<i64> {
    static PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^(\d+)-(\d+)-(\d+) (\d+):(\d+):(\d+)\.(\d+)").expect("date pattern")
    });
    // sqlite-jdbc's FastDateParser.parse(String) appends ".000" to 19-character input.
    let normalized;
    let text = if text.encode_utf16().count() == 19 {
        normalized = format!("{text}.000");
        normalized.as_str()
    } else {
        text
    };
    let captures = PATTERN.captures(text)?;
    let field = |index: usize| captures[index].parse::<i64>().ok();
    let (year, month, day) = (field(1)?, field(2)?, field(3)?);
    let (hour, minute, second, milli) = (field(4)?, field(5)?, field(6)?, field(7)?);
    let days = hybrid_days(year, month, day);
    let wall = days * 86_400 + hour * 3600 + minute * 60 + second;
    let epoch = local_to_epoch_secs(wall.div_euclid(86_400), wall.rem_euclid(86_400));
    Some(epoch * 1000 + milli)
}

// ---------------------------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------------------------

impl SqliteConnection {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let mut file_name = path.to_string_lossy().into_owned();
        if !file_name.is_empty()
            && file_name != ":memory:"
            && !file_name.starts_with("file:")
            && !file_name.contains("mode=memory")
        {
            if let Ok(absolute) = std::path::absolute(&file_name) {
                file_name = absolute.to_string_lossy().into_owned();
            }
        }
        let bytes: Vec<u8> = file_name.bytes().take_while(|&b| b != 0).collect();
        let c_name = std::ffi::CString::new(bytes).expect("NUL removed");
        unsafe {
            let mut db: *mut ffi::sqlite3 = ptr::null_mut();
            // READWRITE | CREATE | URI, as SQLiteConfig sets by default.
            let flags = ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE | ffi::SQLITE_OPEN_URI;
            let rc = ffi::sqlite3_open_v2(c_name.as_ptr(), &mut db, flags, ptr::null());
            if rc != ffi::SQLITE_OK {
                let extended = ffi::sqlite3_extended_errcode(db);
                let message = if db.is_null() {
                    String::new()
                } else {
                    errmsg(db)
                };
                ffi::sqlite3_close(db);
                return Err(sqlite_exception(extended, &message));
            }
            ffi::sqlite3_extended_result_codes(db, 1);
            // The bundled build defaults foreign keys on; sqlite-jdbc's build does not.
            let mut previous: c_int = 0;
            ffi::sqlite3_db_config(
                db,
                ffi::SQLITE_DBCONFIG_ENABLE_FKEY,
                0 as c_int,
                &mut previous as *mut c_int,
            );
            // sqlite-jdbc is built with SQLITE_ENABLE_MATH_FUNCTIONS; the bundled build is not.
            crate::sqlite_math::register_math_functions(db);
            // DatabaseConnector connects through RSQLite, which loads extension-functions.c.
            crate::sqlite_math::register_extension_functions(db);
            // SQLiteConfig.apply: limits, then the 3000 ms default busy timeout.
            ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_ATTACHED, 10);
            ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_COLUMN, 2000);
            ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_FUNCTION_ARG, 100);
            ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_LENGTH, 1_000_000_000);
            ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_SQL_LENGTH, 1_000_000);
            ffi::sqlite3_busy_timeout(db, 3000);
            Ok(SqliteConnection {
                conn: rusqlite::Connection::from_handle_owned(db).expect("valid handle"),
                warnings: Vec::new(),
            })
        }
    }

    fn db(&self) -> *mut ffi::sqlite3 {
        unsafe { self.conn.handle() }
    }

    unsafe fn error(&self, code: c_int) -> DbError {
        sqlite_exception(code, &unsafe { errmsg(self.db()) })
    }

    unsafe fn prepare(&self, sql: &str) -> Result<Stmt, DbError> {
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        let rc = unsafe {
            ffi::sqlite3_prepare_v2(
                self.db(),
                sql.as_ptr() as *const c_char,
                sql.len() as c_int,
                &mut stmt,
                ptr::null_mut(),
            )
        };
        if rc != ffi::SQLITE_OK {
            return Err(unsafe { self.error(rc) });
        }
        Ok(Stmt(stmt))
    }

    fn run_query(&self, sql: &str) -> Result<Vec<Row>, DbError> {
        let text = single_statement(sql)?;
        unsafe {
            let stmt = self.prepare(&text)?;
            let raw = stmt.0;
            let first = ffi::sqlite3_step(raw);
            match first & 0xff {
                ffi::SQLITE_DONE => {
                    ffi::sqlite3_reset(raw);
                    if ffi::sqlite3_column_count(raw) == 0 {
                        return Err(DbError::new(
                            "java.sql.SQLException",
                            "query does not return ResultSet",
                            "SQLITE_DONE",
                            ffi::SQLITE_DONE as i64,
                        ));
                    }
                    return Ok(Vec::new());
                }
                ffi::SQLITE_ROW => {}
                _ => return Err(self.error(first)),
            }
            let count = ffi::sqlite3_column_count(raw);
            let mut labels = Vec::with_capacity(count as usize);
            let mut decls = Vec::with_capacity(count as usize);
            for column in 0..count {
                let name = ffi::sqlite3_column_name(raw, column);
                let label = if name.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(name).to_string_lossy().into_owned()
                };
                let declared = ffi::sqlite3_column_decltype(raw, column);
                let declared = if declared.is_null() {
                    None
                } else {
                    Some(CStr::from_ptr(declared).to_string_lossy().into_owned())
                };
                decls.push(decl_type(declared, &label));
                labels.push(label);
            }
            let mut rows = Vec::new();
            loop {
                let mut row = Row::new();
                for column in 0..count {
                    let value = read_value(
                        raw,
                        column,
                        &labels[column as usize],
                        &decls[column as usize],
                    )?;
                    row.insert(labels[column as usize].clone(), value);
                }
                rows.push(row);
                let rc = ffi::sqlite3_step(raw);
                match rc & 0xff {
                    ffi::SQLITE_ROW => {}
                    ffi::SQLITE_DONE => break,
                    _ => return Err(self.error(rc)),
                }
            }
            Ok(rows)
        }
    }

    fn run_execute(&self, sql: &str) -> Result<(), DbError> {
        for statement in split_statements(sql)? {
            let bytes: Vec<u8> = statement.bytes().take_while(|&b| b != 0).collect();
            let c_sql = std::ffi::CString::new(bytes).expect("NUL removed");
            unsafe {
                let rc = ffi::sqlite3_exec(
                    self.db(),
                    c_sql.as_ptr(),
                    None,
                    ptr::null_mut(),
                    ptr::null_mut(),
                );
                if rc != ffi::SQLITE_OK {
                    return Err(self.error(rc));
                }
            }
        }
        Ok(())
    }

    fn run_insert(&self, table_name: &str, rows: &[Row]) -> Result<(), DbError> {
        let columns: Vec<&String> = rows[0].keys().collect();
        let mut sql = format!("INSERT INTO {table_name} (");
        for (index, column) in columns.iter().enumerate() {
            if index != 0 {
                sql.push(',');
            }
            sql.push('"');
            sql.push_str(&column.replace('"', "\"\""));
            sql.push('"');
        }
        sql.push_str(") VALUES (");
        sql.push_str(&vec!["?"; columns.len()].join(","));
        sql.push(')');
        unsafe {
            let stmt = self.prepare(&sql)?;
            let raw = stmt.0;
            for row in rows {
                ffi::sqlite3_reset(raw);
                for (index, column) in columns.iter().enumerate() {
                    let position = index as c_int + 1;
                    let rc = match &row[*column] {
                        Value::Null => ffi::sqlite3_bind_null(raw, position),
                        Value::Bool(value) => ffi::sqlite3_bind_int(raw, position, *value as c_int),
                        Value::Number(number) if number.is_i64() || number.is_u64() => {
                            match number.to_string().parse::<i64>() {
                                Ok(value) => ffi::sqlite3_bind_int64(raw, position, value),
                                Err(_) => {
                                    return Err(DbError::java(
                                        "java.lang.NumberFormatException",
                                        format!("For input string: \"{number}\""),
                                    ));
                                }
                            }
                        }
                        Value::Number(number) => ffi::sqlite3_bind_double(
                            raw,
                            position,
                            number.as_f64().unwrap_or(f64::NAN),
                        ),
                        other => {
                            let text = match other {
                                Value::String(text) => text.clone(),
                                other => other.to_string(),
                            };
                            ffi::sqlite3_bind_text(
                                raw,
                                position,
                                text.as_ptr() as *const c_char,
                                text.len() as c_int,
                                ffi::SQLITE_TRANSIENT(),
                            )
                        }
                    };
                    if rc != ffi::SQLITE_OK {
                        return Err(self.error(rc));
                    }
                }
                let rc = ffi::sqlite3_step(raw);
                match rc & 0xff {
                    ffi::SQLITE_DONE => {
                        ffi::sqlite3_reset(raw);
                    }
                    ffi::SQLITE_ROW => {
                        ffi::sqlite3_reset(raw);
                        return Err(DbError::sql("query returns results"));
                    }
                    _ => return Err(self.error(rc)),
                }
            }
        }
        Ok(())
    }
}

/// The former bridge's `readValue` for the `sqlite` dbms.
unsafe fn read_value(
    stmt: *mut ffi::sqlite3_stmt,
    column: c_int,
    label: &str,
    decl: &Option<String>,
) -> Result<Value, DbError> {
    unsafe {
        let value_type = ffi::sqlite3_column_type(stmt, column);
        if value_type == ffi::SQLITE_NULL {
            return Ok(Value::Null);
        }
        let long = || ffi::sqlite3_column_int64(stmt, column);
        let text = || {
            let data = ffi::sqlite3_column_text(stmt, column);
            let len = ffi::sqlite3_column_bytes(stmt, column) as usize;
            if data.is_null() {
                String::new()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(data, len)).into_owned()
            }
        };
        let upper = label.to_uppercase();
        let as_datetime = upper.ends_with("DATETIME");
        if as_datetime || upper.ends_with("DATE") {
            // numericValue(getObject): Integer/Long/Double/String toString, BigDecimal parse.
            let rendered = match value_type {
                ffi::SQLITE_INTEGER => Some(long().to_string()),
                ffi::SQLITE_FLOAT => double_decimal_text(ffi::sqlite3_column_double(stmt, column)),
                ffi::SQLITE_TEXT => Some(text()),
                _ => None,
            };
            let Some(milliseconds) = rendered.as_deref().and_then(big_decimal_millis) else {
                return Ok(Value::Null);
            };
            return Ok(Value::String(if as_datetime {
                epoch_millis_utc_datetime(milliseconds)
            } else {
                epoch_millis_utc_date(milliseconds)
            }));
        }
        let name = type_name(decl, value_type);
        let kind = jdbc_type(
            &name,
            value_type,
            if value_type == ffi::SQLITE_INTEGER {
                long()
            } else {
                0
            },
        );
        match kind {
            JdbcType::Bigint => Ok(Value::from(check_bigint(long())?)),
            JdbcType::Integer | JdbcType::Smallint | JdbcType::Tinyint => Ok(Value::from(long())),
            JdbcType::Decimal
            | JdbcType::Numeric
            | JdbcType::Float
            | JdbcType::Real
            | JdbcType::Double => Ok(double_value(ffi::sqlite3_column_double(stmt, column))),
            JdbcType::Boolean => Ok(Value::Bool(ffi::sqlite3_column_int(stmt, column) != 0)),
            JdbcType::Date => {
                let milliseconds = match value_type {
                    ffi::SQLITE_TEXT => {
                        let source = text();
                        if source.is_empty() {
                            return Err(DbError::java(
                                "java.lang.NullPointerException",
                                "Cannot invoke \"java.sql.Date.toLocalDate()\" because the return value of \"java.sql.ResultSet.getDate(int)\" is null",
                            ));
                        }
                        parse_date_text(&source)
                            .ok_or_else(|| DbError::sql("Error parsing date"))?
                    }
                    _ => long(),
                };
                Ok(Value::String(local_date_of_epoch_millis(milliseconds)))
            }
            JdbcType::Timestamp => {
                let milliseconds = match value_type {
                    ffi::SQLITE_TEXT => {
                        let source = text();
                        if source.is_empty() {
                            return Err(DbError::java(
                                "java.lang.NullPointerException",
                                "Cannot invoke \"java.sql.Timestamp.toInstant()\" because the return value of \"java.sql.ResultSet.getTimestamp(int)\" is null",
                            ));
                        }
                        parse_date_text(&source)
                            .ok_or_else(|| DbError::sql("Error parsing time stamp"))?
                    }
                    _ => long(),
                };
                let (secs, nanos) = timestamp_new(milliseconds);
                Ok(Value::String(instant_string(secs, nanos)))
            }
            JdbcType::Char
            | JdbcType::Clob
            | JdbcType::Varchar
            | JdbcType::Binary
            | JdbcType::Blob => Ok(Value::String(text())),
        }
    }
}

impl Connection for SqliteConnection {
    fn query(&mut self, sql: &str) -> anyhow::Result<Vec<Row>> {
        self.warnings.clear();
        Ok(self.run_query(sql)?)
    }

    fn execute(&mut self, sql: &str) -> anyhow::Result<()> {
        self.warnings.clear();
        Ok(self.run_execute(sql)?)
    }

    fn insert_rows(&mut self, table_name: &str, rows: &[Row]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let columns: Vec<&String> = rows[0].keys().collect();
        if rows.iter().any(|row| {
            row.len() != columns.len() || columns.iter().any(|column| !row.contains_key(*column))
        }) {
            anyhow::bail!("Inserted rows must have the same columns in the same shape")
        }
        self.warnings.clear();
        Ok(self.run_insert(table_name, rows)?)
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}
