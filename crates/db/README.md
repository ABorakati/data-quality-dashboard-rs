# dqd-db

Native SQLite, PostgreSQL and DuckDB connections for the DQD Rust port. They replace the former JDBC bridge and
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
  database is read like a pgjdbc URL (`sslmode`, `sslrootcert`, `sslcert`, `sslkey`, `connectTimeout`,
  `currentSchema`, `ApplicationName`, `user`, `password`; values are percent-decoded, and a `/` inside the query,
  as in `sslrootcert=/etc/ssl/root.crt`, belongs to the query). See [PostgreSQL TLS](#postgresql-tls).
  `user` without `password` fails with DatabaseConnector's `Connection propery 'password' is NULL.`
* The session mirrors pgjdbc: `DateStyle=ISO`, `extra_float_digits=3`, `TimeZone=<system zone>`,
  `application_name=PostgreSQL JDBC Driver`. `DQD_DB_TIMEZONE` overrides the detected IANA zone; the zone
  decides how `timestamp` values map to instants.

## Features

* `duckdb` (default): DuckDB support through the `duckdb` crate, which links DuckDB's prebuilt 2.0.0 shared
  library. With `default-features = false` the crate builds with no DuckDB library at build or run time, and
  `connect` for `Dbms::Duckdb` returns an error saying the build was compiled without DuckDB support.

## PostgreSQL TLS

TLS is provided by [rustls](https://crates.io/crates/rustls) (pure Rust, `ring` crypto) through the
`tokio-postgres-rustls` connector, so release binaries need no system OpenSSL. It is the Cargo feature `tls`, on by
default; `--no-default-features` builds without it, and then `sslmode=require`, `verify-ca` and `verify-full` fail with
`sslmode=<x> requires TLS, which this build of dqd-db does not support. Use sslmode=disable, allow or prefer.` while
`disable`, `allow` and `prefer` connect in plain text as before.

`sslmode` follows pgjdbc 42.7.3 (values are case-sensitive; anything else fails with `Invalid sslmode value: <x>`):

| `sslmode`     | Connection attempts, in order                                              | Server certificate checked |
|---------------|----------------------------------------------------------------------------|----------------------------|
| `disable`     | plain text                                                                 | no TLS                     |
| `allow`       | plain text; TLS only if the server refuses that (SQLSTATE 28000 or 08004)  | no                         |
| `prefer`      | TLS if the server offers it, otherwise plain text; plain text again if the TLS attempt is refused (28000, 08004) or the handshake fails | no |
| `require`     | TLS, no fallback                                                           | **no**                     |
| `verify-ca`   | TLS, no fallback                                                           | chain and validity         |
| `verify-full` | TLS, no fallback                                                           | chain, validity and host name |

`prefer` is the default, as in pgjdbc, so a server without TLS behaves exactly as before. As in pgjdbc, and unlike libpq,
**`require` encrypts but does not authenticate the server**: it accepts any certificate. Use `verify-ca` or
`verify-full` to be protected against an impersonating server. `ssl=true` without `sslmode` means `verify-full`.
Connection refused and timeouts are never retried, and neither are password failures (28P01).

* `sslrootcert=<file>`: PEM file with the trusted CA certificates, used by `verify-ca` and `verify-full` only (ignored
  by the other modes, as in pgjdbc). Without it, the operating system's trust store is used (the bundled Mozilla roots
  if the system store is empty); `sslrootcert=system` asks for that explicitly. Unlike pgjdbc and libpq there is no
  `~/.postgresql/root.crt` default.
* `sslcert=<file>` and `sslkey=<file>`: client certificate (PEM) and key (PEM PKCS#8, PKCS#1 or SEC1, or DER PKCS#8
  as pgjdbc's `.pk8`), for servers using `clientcert`/`cert` authentication. Both are needed together; there is no
  `~/.postgresql/postgresql.crt` default. Encrypted keys (`sslpassword`) are not supported: decrypt the key first,
  for example `openssl pkcs8 -topk8 -nocrypt -in key.pem -out key.pk8 -outform DER`.
* Not supported: `sslfactory`, `sslhostnameverifier`, `sslpasswordcallback`, `sslcrl` and other pgjdbc SSL options are
  ignored; `gssEncMode` and `channelBinding` are not read (SCRAM channel binding is negotiated when the server offers it).
* Failures read like pgjdbc: `The server does not support SSL.` (08004) and `SSL error: <cause>` (08006), where the
  cause is rustls' text, for example `invalid peer certificate: UnknownIssuer`.

## Tests

`cargo test -p dqd-db` needs no Java and no database server. The expected strings in `tests/native.rs` were captured from
the JDBC bridge (5727 SQLite and 501 PostgreSQL differential cases, all identical) before the bridge was removed, and
`charindex` from real `RSQLite::SQLite()` connections. End-to-end behavior against the R oracle is covered by the main
crate's `tests/parity.rs`.

`tests/pg_tls.rs` pins the `sslmode` attempt order against a scripted fake server on a loopback socket (no PostgreSQL
needed). Its server-backed tests are skipped unless `DQD_PG_TLS_PORT` is set: run
`crates/db/tests/pg_tls/setup.sh start /tmp/dqd-pg-tls` (needs `openssl` and the PostgreSQL server binaries; it starts
three throwaway servers on 127.0.0.1: TLS, TLS with a certificate for another host name, and plain), export the
variables it prints (`DQD_PG_TLS_DIR`, `DQD_PG_TLS_PORT`, `DQD_PG_TLS_WRONGNAME_PORT`, `DQD_PG_PLAIN_PORT`), run
`cargo test -p dqd-db --test pg_tls`, then `setup.sh stop /tmp/dqd-pg-tls`.

`cargo test -p dqd-db` needs no Java and no database server. The expected strings in `tests/native.rs` were captured from
the JDBC bridge (5727 SQLite and 501 PostgreSQL differential cases, all identical) before the bridge was removed, and
`charindex` from real `RSQLite::SQLite()` connections. End-to-end behavior against the R oracle is covered by the main
crate's `tests/parity.rs`.

## Differences that cannot be removed

* Bundled SQLite is 3.54.1 (`rusqlite 0.41`), newer than the reference's 3.50.3 (sqlite-jdbc 3.50.3.0), because the
  older releases carry a known SQLite CVE. The full parity suite passes on it. `PRAGMA compile_options` differs.
* sqlite-jdbc `backup to`/`restore from` commands are not implemented.
* The JDBC bridge printed non-ASCII text with the platform charset (corrupted on Windows); native output is UTF-8.
* PostgreSQL TLS needs the default `tls` feature; the pgjdbc SSL options listed under "PostgreSQL TLS" are not all supported.
* An empty password for a SCRAM role fails with the server's `28P01` text instead of pgjdbc's local message.
* pgjdbc JDBC escapes (`{fn ...}`, `{d ...}`) are not rewritten; `{...}` in SQL is passed through.
* Only `charindex` is provided from RSQLite's other extension functions (the string, aggregate, `regexp`, `series`,
  `csv` and `uuid` ones); DQD's SQL needs no others.
