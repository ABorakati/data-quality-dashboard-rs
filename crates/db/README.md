# dqd-db

Native SQLite and PostgreSQL connections for the DQD Rust port. They replace the former JDBC bridge and
reproduce its output, which was the data DatabaseConnector hands to DQD with sqlite-jdbc 3.50.3.0 and pgjdbc 42.7.3:
row values and JSON types, error text, warnings, `insert_rows` typing. The SQLite connection also provides
`charindex`, which RSQLite's bundled `extension-functions.c` gives DatabaseConnector's SQLite and which DQD's SQLite SQL uses.

```rust
let mut db = dqd_db::connect(&ConnectSpec { dbms: Dbms::Sqlite, sqlite_path: Some(path), server: None, port: None, user: None, password: None })?;
let rows = db.query("SELECT count(*) AS n FROM person")?;   // one statement, raw column labels
db.execute("CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1)")?;
db.insert_rows("t", &rows)?;
let warnings = db.take_warnings();                           // warnings of the latest request
```

* Errors are `DbError` values inside `anyhow::Error`; `Display` is `"{class}: {message} [SQLSTATE={state}; vendorCode={code}]"`.
* PostgreSQL `server` is `host/database` as in DatabaseConnector `createConnectionDetails`. A `?query` after the
  database is read like a pgjdbc URL (`sslmode`, `connectTimeout`, `currentSchema`, `ApplicationName`, `user`, `password`).
  `user` without `password` fails with DatabaseConnector's `Connection propery 'password' is NULL.`
* The session mirrors pgjdbc: `DateStyle=ISO`, `extra_float_digits=3`, `TimeZone=<system zone>`,
  `application_name=PostgreSQL JDBC Driver`. `DQD_DB_TIMEZONE` overrides the detected IANA zone; the zone
  decides how `timestamp` values map to instants.

## Tests

`cargo test -p dqd-db` needs no Java and no database server. The expected strings in `tests/native.rs` were captured from
the JDBC bridge (5727 SQLite and 501 PostgreSQL differential cases, all identical) before the bridge was removed, and
`charindex` from real `RSQLite::SQLite()` connections. End-to-end behavior against the R oracle is covered by the main
crate's `tests/parity.rs`.

## Differences that cannot be removed

* Bundled SQLite is 3.50.2 (`rusqlite 0.37`); no crates.io release bundles 3.50.3. `PRAGMA compile_options` differs.
* sqlite-jdbc `backup to`/`restore from` commands are not implemented.
* The JDBC bridge printed non-ASCII text with the platform charset (corrupted on Windows); native output is UTF-8.
* PostgreSQL TLS (`sslmode=require` and stronger) is not implemented.
* An empty password for a SCRAM role fails with the server's `28P01` text instead of pgjdbc's local message.
* pgjdbc JDBC escapes (`{fn ...}`, `{d ...}`) are not rewritten; `{...}` in SQL is passed through.
* Only `charindex` is provided from RSQLite's other extension functions (the string, aggregate, `regexp`, `series`,
  `csv` and `uuid` ones); DQD's SQL needs no others.
