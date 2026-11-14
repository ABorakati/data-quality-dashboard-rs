use crate::model::{Options, Row};
use anyhow::{Result, bail};
use dqd_db::{ConnectSpec, Dbms};

pub use dqd_db::Connection;

/// Opens an auto-commit native connection for the options' `dbms` and `connection` settings.
pub fn connect(options: &Options) -> Result<Box<dyn Connection>> {
    let config = &options.connection;
    let dbms = match options.dbms.as_str() {
        "sqlite" => Dbms::Sqlite,
        "postgresql" => Dbms::Postgresql,
        "duckdb" => Dbms::Duckdb,
        other => bail!(
            "Unsupported dbms '{other}': only 'sqlite', 'postgresql' and 'duckdb' are supported."
        ),
    };
    match dbms {
        Dbms::Sqlite if config.sqlite_path.is_none() => {
            bail!("connection.sqlitePath is required when dbms is 'sqlite'")
        }
        Dbms::Postgresql if config.server.is_none() => {
            bail!("connection.server (host/database) is required when dbms is 'postgresql'")
        }
        Dbms::Duckdb if config.server.is_none() && config.sqlite_path.is_none() => {
            bail!("connection.server (database file) is required when dbms is 'duckdb'")
        }
        _ => {}
    }
    dqd_db::connect(&ConnectSpec {
        dbms,
        sqlite_path: config.sqlite_path.clone(),
        server: config.server.clone(),
        port: config.port,
        user: config.user.clone(),
        password: config.password.clone(),
    })
}

pub fn insert_rows(connection: &mut dyn Connection, table_name: &str, rows: &[Row]) -> Result<()> {
    connection.insert_rows(table_name, rows)
}

/// SqlRender::snakeCaseToCamelCase; callers must apply it only to raw snake-case labels.
pub fn snake_case_to_camel_case(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '_'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_lowercase() || chars[i + 1].is_ascii_digit())
        {
            if chars[i + 1].is_ascii_lowercase() {
                output.push(chars[i + 1].to_ascii_uppercase());
            } else {
                output.push(chars[i + 1]);
            }
            i += 2;
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }
    output
}

/// SqlRender::camelCaseToSnakeCase, including digits that follow lowercase letters.
pub fn camel_case_to_snake_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 4);
    for ch in value.chars() {
        if ch.is_ascii_uppercase() {
            output.push('_');
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push(ch);
        }
    }
    let chars: Vec<char> = output.chars().collect();
    let mut normalized = String::with_capacity(output.len() + 2);
    for i in 0..chars.len() {
        normalized.push(chars[i]);
        if chars[i].is_ascii_lowercase() && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            normalized.push('_');
        }
    }
    normalized
}

pub fn normalize_columns_to_camel(rows: &mut [Row]) {
    for row in rows {
        let old = std::mem::take(row);
        for (key, value) in old {
            row.insert(snake_case_to_camel_case(&key), value);
        }
    }
}
