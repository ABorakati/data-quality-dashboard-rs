//! PostgreSQL connection reproducing pgjdbc 42.7.3 (the driver DatabaseConnector downloads) as the
//! former JDBC bridge drove it.

use crate::javacompat::{
    DbError, check_bigint, default_zone_id, hybrid_days, instant_string, is_big_decimal,
    local_date_of_epoch_millis, local_to_epoch_secs, single_statement, split_statements,
    timestamp_new,
};
use crate::{ConnectSpec, Connection, Row};
use postgres::config::SslMode;
use postgres::error::DbError as ServerError;
use postgres::error::ErrorPosition;
use postgres::types::{IsNull, ToSql, Type, to_sql_checked};
use postgres::{Client, Config, NoTls, SimpleQueryMessage, Statement};
use serde_json::{Number, Value};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PSQL_EXCEPTION: &str = "org.postgresql.util.PSQLException";

const DATE_POSITIVE_INFINITY: i64 = 9_223_372_036_825_200_000;
const DATE_NEGATIVE_INFINITY: i64 = -9_223_372_036_832_400_000;

pub struct PgConnection {
    client: Client,
    notices: Arc<Mutex<Vec<String>>>,
    warnings: Vec<String>,
}

fn psql(message: impl Into<String>, state: &str) -> DbError {
    DbError::new(PSQL_EXCEPTION, message, state, 0)
}

/// `ServerErrorMessage.toString()`: severity, message, detail, hint, position, where.
fn server_message(error: &ServerError) -> String {
    let mut out = format!("{}: {}", error.severity(), error.message());
    if let Some(detail) = error.detail() {
        out.push_str(&format!("\n  Detail: {detail}"));
    }
    if let Some(hint) = error.hint() {
        out.push_str(&format!("\n  Hint: {hint}"));
    }
    if let Some(ErrorPosition::Original(position)) = error.position() {
        out.push_str(&format!("\n  Position: {position}"));
    }
    if let Some(location) = error.where_() {
        out.push_str(&format!("\n  Where: {location}"));
    }
    out
}

fn server_exception(error: &ServerError) -> DbError {
    psql(server_message(error), error.code().code())
}

fn connection_error(error: postgres::Error, host: &str, port: u16) -> DbError {
    if let Some(server) = error.as_db_error() {
        return server_exception(server);
    }
    let mut source: Option<&(dyn StdError + 'static)> = Some(&error);
    while let Some(cause) = source {
        #[allow(clippy::collapsible_if)] // kept nested to keep this hunk clear of the TLS changes
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::ConnectionRefused {
                return psql(
                    format!(
                        "Connection to {host}:{port} refused. Check that the hostname and port are correct and that the postmaster is accepting TCP/IP connections."
                    ),
                    "08001",
                );
            }
        }
        source = cause.source();
    }
    psql("The connection attempt failed.", "08001")
}

fn request_error(error: postgres::Error) -> DbError {
    if let Some(server) = error.as_db_error() {
        return server_exception(server);
    }
    if error.is_closed() {
        return psql("This connection has been closed.", "08003");
    }
    psql(
        "An I/O error occurred while sending to the backend.",
        "08006",
    )
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            // `get` rather than slicing: the two bytes after '%' may split a multi-byte character.
            && let Some(Ok(value)) = text.get(i + 1..i + 3).map(|hex| u8::from_str_radix(hex, 16))
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl PgConnection {
    pub fn connect(spec: &ConnectSpec) -> anyhow::Result<Self> {
        let server = spec
            .server
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("connection.server is required for postgresql"))?;
        // DatabaseConnector connectPostgreSql: server must be "<host>/<database>".
        if !server.contains('/') {
            anyhow::bail!(
                "Error: database name not included in server string but is required for PostgreSQL. Please specify server as <host>/<database>"
            );
        }
        // R: unlist(strsplit(server, "/")): drops a trailing empty piece; a missing database pastes as "NA".
        let mut parts: Vec<&str> = server.split('/').collect();
        while parts.last() == Some(&"") && parts.len() > 1 {
            parts.pop();
        }
        let host = parts[0].to_owned();
        let database_and_query = parts
            .get(1)
            .map(|s| (*s).to_owned())
            .unwrap_or_else(|| "NA".into());
        let port = spec.port.unwrap_or(5432);

        let (database, query) = match database_and_query.split_once('?') {
            Some((database, query)) => (database.to_owned(), query.to_owned()),
            None => (database_and_query, String::new()),
        };
        let mut user = spec.user.clone();
        let mut password = spec.password.clone();
        match (&user, &password) {
            (Some(_), None) => anyhow::bail!("Connection propery 'password' is NULL."),
            (None, _) => password = None,
            _ => {}
        }
        let mut ssl_mode = "prefer".to_owned();
        let mut connect_timeout = 10u64;
        let mut application_name = "PostgreSQL JDBC Driver".to_owned();
        let mut current_schema: Option<String> = None;
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = match pair.split_once('=') {
                Some((key, value)) => (percent_decode(key), percent_decode(value)),
                None => (percent_decode(pair), String::new()),
            };
            match key.as_str() {
                "user" => user = Some(value),
                "password" => password = Some(value),
                "sslmode" => ssl_mode = value,
                "ssl" if value.eq_ignore_ascii_case("true") || value.is_empty() => {
                    ssl_mode = "verify-full".into()
                }
                "connectTimeout" => connect_timeout = value.parse().unwrap_or(10),
                "ApplicationName" => application_name = value,
                "currentSchema" => current_schema = Some(value),
                _ => {}
            }
        }
        let tls_mode = match ssl_mode.as_str() {
            "disable" => SslMode::Disable,
            "allow" | "prefer" => SslMode::Prefer,
            other => {
                return Err(DbError::new(
                    PSQL_EXCEPTION,
                    format!("sslmode={other} requires TLS, which this build of dqd-db does not support. Use sslmode=disable, allow or prefer."),
                    "08004",
                    0,
                )
                .into());
            }
        };

        let host_name = if host.is_empty() {
            "localhost".to_owned()
        } else {
            percent_decode(&host)
        };
        let user_name = user.unwrap_or_else(|| {
            std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default()
        });
        let database_name = if database.is_empty() {
            user_name.clone()
        } else {
            percent_decode(&database)
        };

        let mut options = format!(
            "-c DateStyle=ISO -c extra_float_digits=3 -c TimeZone={}",
            default_zone_id()
        );
        if let Some(schema) = current_schema {
            options.push_str(&format!(
                " -c search_path={}",
                schema.replace('\\', "\\\\").replace(' ', "\\ ")
            ));
        }
        let notices: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&notices);
        let mut config = Config::new();
        config
            .host(&host_name)
            .port(port)
            .user(&user_name)
            .dbname(&database_name)
            .application_name(&application_name)
            .options(&options)
            .ssl_mode(tls_mode)
            .connect_timeout(Duration::from_secs(connect_timeout))
            .notice_callback(move |notice| {
                if let Ok(mut notices) = sink.lock() {
                    notices.push(notice.message().to_owned());
                }
            });
        if let Some(password) = &password {
            config.password(password);
        }
        let client = config
            .connect(NoTls)
            .map_err(|error| connection_error(error, &host_name, port))?;
        Ok(PgConnection {
            client,
            notices,
            warnings: Vec::new(),
        })
    }

    fn collect_notices(&mut self) {
        if let Ok(mut notices) = self.notices.lock() {
            self.warnings.append(&mut notices);
        }
    }

    fn discard_notices(&mut self) {
        if let Ok(mut notices) = self.notices.lock() {
            notices.clear();
        }
    }

    fn run_query(&mut self, sql: &str) -> Result<Vec<Row>, DbError> {
        let text = single_statement(sql)?;
        crate::pgparse::precheck(&text)?;
        // Describe for the column types; the values below come back as text, as they do for pgjdbc's
        // one-shot extended-protocol statements.
        let statement = self.client.prepare(&text).map_err(request_error)?;
        if !statement.params().is_empty() {
            // pgjdbc binds zero parameters; the server rejects the Bind message.
            return Err(psql(
                format!(
                    "ERROR: bind message supplies 0 parameters, but prepared statement \"\" requires {}",
                    statement.params().len()
                ),
                "08P01",
            ));
        }
        let oids: Vec<u32> = statement
            .columns()
            .iter()
            .map(|c| c.type_().oid())
            .collect();
        let messages = self.client.simple_query(&text).map_err(request_error)?;
        drop(statement);
        if oids.is_empty() {
            self.discard_notices();
            return Err(psql("No results were returned by the query.", "02000"));
        }
        let mut rows = Vec::new();
        for message in &messages {
            if let SimpleQueryMessage::Row(row) = message {
                let mut out = Row::new();
                for (index, column) in row.columns().iter().enumerate() {
                    let value = convert_value(oids[index], row.get(index))?;
                    out.insert(column.name().to_owned(), value);
                }
                rows.push(out);
            }
        }
        self.collect_notices();
        Ok(rows)
    }

    fn run_execute(&mut self, sql: &str) -> Result<(), DbError> {
        for statement in split_statements(sql)? {
            crate::pgparse::precheck(&statement)?;
            let messages = self
                .client
                .simple_query(&statement)
                .map_err(request_error)?;
            if messages.iter().any(|m| {
                matches!(
                    m,
                    SimpleQueryMessage::RowDescription(_) | SimpleQueryMessage::Row(_)
                )
            }) {
                self.discard_notices();
                return Err(psql(
                    "A result was returned when none was expected.",
                    "0100E",
                ));
            }
            self.collect_notices();
        }
        Ok(())
    }

    fn run_insert(&mut self, table_name: &str, rows: &[Row]) -> Result<(), DbError> {
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
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();
        sql.push_str(&placeholders.join(","));
        sql.push(')');
        let mut prepared: HashMap<Vec<u32>, Statement> = HashMap::new();
        for row in rows {
            let mut params = Vec::with_capacity(columns.len());
            for column in &columns {
                params.push(param_of(&row[*column])?);
            }
            // setBoolean/setLong/setDouble/setString/setObject(null): BOOL, INT8, FLOAT8, VARCHAR, unspecified.
            let types: Vec<Type> = params.iter().map(Param::declared_type).collect();
            let key: Vec<u32> = types.iter().map(Type::oid).collect();
            if !prepared.contains_key(&key) {
                let statement = self
                    .client
                    .prepare_typed(&sql, &types)
                    .map_err(request_error)?;
                prepared.insert(key.clone(), statement);
            }
            let statement = &prepared[&key];
            let refs: Vec<&(dyn ToSql + Sync)> =
                params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
            self.client
                .execute(statement, &refs)
                .map_err(request_error)?;
        }
        self.collect_notices();
        Ok(())
    }
}

#[derive(Debug)]
enum Param {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Param {
    fn declared_type(&self) -> Type {
        match self {
            Param::Null => Type::UNKNOWN,
            Param::Bool(_) => Type::BOOL,
            Param::Int(_) => Type::INT8,
            Param::Float(_) => Type::FLOAT8,
            Param::Text(_) => Type::VARCHAR,
        }
    }
}

impl ToSql for Param {
    fn to_sql(
        &self,
        _ty: &Type,
        out: &mut postgres::types::private::BytesMut,
    ) -> Result<IsNull, Box<dyn StdError + Sync + Send>> {
        match self {
            Param::Null => Ok(IsNull::Yes),
            Param::Bool(value) => value.to_sql(&Type::BOOL, out),
            Param::Int(value) => value.to_sql(&Type::INT8, out),
            Param::Float(value) => value.to_sql(&Type::FLOAT8, out),
            Param::Text(value) => value.as_str().to_sql(&Type::VARCHAR, out),
        }
    }

    fn accepts(_: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

/// `append_value` in the main crate plus the former bridge's `insert` value typing.
fn param_of(value: &Value) -> Result<Param, DbError> {
    Ok(match value {
        Value::Null => Param::Null,
        Value::Bool(value) => Param::Bool(*value),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            match number.to_string().parse::<i64>() {
                Ok(value) => Param::Int(value),
                Err(_) => {
                    return Err(DbError::java(
                        "java.lang.NumberFormatException",
                        format!("For input string: \"{number}\""),
                    ));
                }
            }
        }
        Value::Number(number) => Param::Float(number.as_f64().unwrap_or(f64::NAN)),
        Value::String(text) => Param::Text(text.clone()),
        other => Param::Text(other.to_string()),
    })
}

// ---------------------------------------------------------------------------------------------
// Value conversion of the former bridge's `readValue` over pgjdbc's ResultSet
// ---------------------------------------------------------------------------------------------

fn bad_value(kind: &str, text: &str) -> DbError {
    psql(format!("Bad value for type {kind} : {text}"), "22003")
}

/// `PgResultSet.trimMoney`
fn trim_money(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 || chars[0] > '-' {
        return text.to_owned();
    }
    match chars[0] {
        '(' => {
            // "-" + removePara(s).substring(1)
            let inner: String = text
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_owned();
            format!("-{}", inner.chars().skip(1).collect::<String>())
        }
        '$' => chars[1..].iter().collect(),
        '-' if chars[1] == '$' => format!("-{}", chars[2..].iter().collect::<String>()),
        _ => text.to_owned(),
    }
}

/// `Double.parseDouble(s.trim())`
fn java_parse_double(text: &str) -> Option<f64> {
    let trimmed = text.trim_matches(|c: char| c <= ' ');
    if let Ok(value) = trimmed.parse::<f64>() {
        // Rust accepts "inf"/"infinity"/"nan" in any case; Java only the exact spellings.
        let body = trimmed.trim_start_matches(['+', '-']);
        let lowered = body.to_ascii_lowercase();
        if matches!(lowered.as_str(), "inf" | "infinity" | "nan") {
            return match body {
                "Infinity" | "NaN" => Some(value),
                _ => None,
            };
        }
        return Some(value);
    }
    None
}

fn double_value(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `BooleanTypeUtil.fromString`
fn cast_to_boolean(text: &str) -> Result<bool, DbError> {
    let value = text.trim();
    let lowered = value.to_lowercase();
    if value == "1" || matches!(lowered.as_str(), "true" | "t" | "yes" | "y" | "on") {
        return Ok(true);
    }
    if value == "0" || matches!(lowered.as_str(), "false" | "f" | "no" | "n" | "off") {
        return Ok(false);
    }
    Err(psql(format!("Cannot cast to boolean: \"{text}\""), "42846"))
}

struct ParsedTimestamp {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    nanos: i64,
    offset: Option<i64>,
    bc: bool,
}

/// `TimestampUtils.parseBackendTimestamp`
fn parse_backend_timestamp(text: &str) -> Result<ParsedTimestamp, DbError> {
    let s: Vec<char> = text.chars().collect();
    let bad = || {
        psql(
            format!("Bad value for type timestamp/date/time: {text}"),
            "22007",
        )
    };
    let at = |i: usize| s.get(i).copied().unwrap_or('\0');
    let skip_ws = |mut i: usize| {
        while i < s.len() && s[i].is_whitespace() {
            i += 1;
        }
        i
    };
    let non_digit = |mut i: usize| {
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let number = |start: usize, end: usize| -> Result<i64, DbError> {
        if end <= start {
            return Err(bad());
        }
        let digits: String = s[start..end].iter().collect();
        digits.parse::<i32>().map(i64::from).map_err(|_| bad())
    };
    let mut result = ParsedTimestamp {
        year: 1970,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        nanos: 0,
        offset: None,
        bc: false,
    };
    let mut has_date = false;
    let mut has_time = false;
    let mut start = skip_ws(0);
    let mut end = non_digit(start);
    if at(end) == '-' {
        has_date = true;
        result.year = number(start, end)?;
        start = end + 1;
        end = non_digit(start);
        result.month = number(start, end)?;
        if at(end) != '-' {
            return Err(bad());
        }
        start = end + 1;
        end = non_digit(start);
        result.day = number(start, end)?;
        start = skip_ws(end);
    }
    if at(start).is_ascii_digit() {
        has_time = true;
        end = non_digit(start);
        result.hour = number(start, end)?;
        if at(end) != ':' {
            return Err(bad());
        }
        start = end + 1;
        end = non_digit(start);
        result.minute = number(start, end)?;
        if at(end) != ':' {
            return Err(bad());
        }
        start = end + 1;
        end = non_digit(start);
        result.second = number(start, end)?;
        start = end;
        if at(start) == '.' {
            end = non_digit(start + 1);
            let mut nanos = number(start + 1, end)?;
            for _ in (end - (start + 1))..9 {
                nanos *= 10;
            }
            result.nanos = nanos;
            start = end;
        }
        start = skip_ws(start);
    }
    let sep = at(start);
    if sep == '-' || sep == '+' {
        let sign: i64 = if sep == '-' { -1 } else { 1 };
        end = non_digit(start + 1);
        let hours = number(start + 1, end)?;
        start = end;
        let mut minutes = 0;
        if at(start) == ':' {
            end = non_digit(start + 1);
            minutes = number(start + 1, end)?;
            start = end;
        }
        let mut seconds = 0;
        if at(start) == ':' {
            end = non_digit(start + 1);
            seconds = number(start + 1, end)?;
            start = end;
        }
        result.offset = Some(sign * (hours * 3600 + minutes * 60 + seconds));
        start = skip_ws(start);
    }
    if has_date && start < s.len() {
        let rest: String = s[start..].iter().collect();
        if rest.starts_with("AD") {
            start += 2;
        } else if rest.starts_with("BC") {
            result.bc = true;
            start += 2;
        }
    }
    if start < s.len() || (!has_time && !has_date) {
        return Err(bad());
    }
    Ok(result)
}

/// `TimestampUtils.toTimestamp(null, text)` as (epoch seconds, nanos), or the infinity millisecond hack.
enum Stamp {
    Infinite(i64),
    At(i64, u32),
}

fn to_timestamp(text: &str) -> Result<Stamp, DbError> {
    if text == "infinity" {
        return Ok(Stamp::Infinite(DATE_POSITIVE_INFINITY));
    }
    if text == "-infinity" {
        return Ok(Stamp::Infinite(DATE_NEGATIVE_INFINITY));
    }
    let ts = parse_backend_timestamp(text)?;
    let year = if ts.bc { 1 - ts.year } else { ts.year };
    // Lenient hybrid Julian/Gregorian GregorianCalendar: month/day overflow normalizes.
    let days = hybrid_days(year, ts.month, ts.day);
    let wall = days * 86_400 + ts.hour * 3600 + ts.minute * 60 + ts.second;
    let secs = match ts.offset {
        Some(offset) => wall - offset,
        None => local_to_epoch_secs(wall.div_euclid(86_400), wall.rem_euclid(86_400)),
    };
    Ok(Stamp::At(secs, ts.nanos as u32))
}

fn convert_value(oid: u32, text: Option<&str>) -> Result<Value, DbError> {
    let Some(text) = text else {
        return Ok(Value::Null);
    };
    match oid {
        // int2, int4
        21 | 23 => text
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| bad_value("int", text)),
        // int8, oid
        20 | 26 => {
            let value = text
                .trim()
                .parse::<i64>()
                .map_err(|_| bad_value("long", text))?;
            Ok(Value::from(check_bigint(value)?))
        }
        // numeric
        1700 => {
            if !text.eq_ignore_ascii_case("NaN") && !is_big_decimal(trim_money(text).trim()) {
                return Err(bad_value("BigDecimal", trim_money(text).trim()));
            }
            let fixed = trim_money(text);
            java_parse_double(&fixed)
                .map(double_value)
                .ok_or_else(|| bad_value("double", fixed.trim()))
        }
        // float4: getObject() parses as float first
        700 => {
            let fixed = trim_money(text);
            if java_parse_double(&fixed).is_none() {
                return Err(bad_value("float", fixed.trim()));
            }
            java_parse_double(&fixed)
                .map(double_value)
                .ok_or_else(|| bad_value("double", fixed.trim()))
        }
        // float8, money
        701 | 790 => {
            let fixed = trim_money(text);
            java_parse_double(&fixed)
                .map(double_value)
                .ok_or_else(|| bad_value("double", fixed.trim()))
        }
        // bool
        16 => Ok(Value::Bool(text == "t")),
        // bit
        1560 => Ok(Value::Bool(cast_to_boolean(text)?)),
        // date
        1082 => Ok(Value::String(match to_timestamp(text)? {
            Stamp::Infinite(ms) => local_date_of_epoch_millis(ms),
            // Date(ms).toLocalDate() on the legacy hybrid calendar.
            Stamp::At(secs, _) => local_date_of_epoch_millis(secs * 1000),
        })),
        // timestamp, timestamptz
        1114 | 1184 => Ok(Value::String(match to_timestamp(text)? {
            Stamp::Infinite(ms) => {
                let (secs, nanos) = timestamp_new(ms);
                instant_string(secs, nanos)
            }
            Stamp::At(secs, nanos) => instant_string(secs, nanos),
        })),
        _ => Ok(Value::String(text.to_owned())),
    }
}

impl Connection for PgConnection {
    fn query(&mut self, sql: &str) -> anyhow::Result<Vec<Row>> {
        self.warnings.clear();
        self.discard_notices();
        Ok(self.run_query(sql)?)
    }

    fn execute(&mut self, sql: &str) -> anyhow::Result<()> {
        self.warnings.clear();
        self.discard_notices();
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
        self.discard_notices();
        Ok(self.run_insert(table_name, rows)?)
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}
