//! DuckDB backend smoke test: a temp database file, DDL, insert, and value mapping.

use dqd_db::{ConnectSpec, Dbms, Row, connect};
use serde_json::json;

#[test]
fn duckdb_value_mapping_and_insert() {
    let dir = std::env::temp_dir().join(format!("dqd-duckdb-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("smoke.duckdb");
    let _ = std::fs::remove_file(&file);

    let mut conn = connect(&ConnectSpec {
        dbms: Dbms::Duckdb,
        sqlite_path: None,
        server: Some(file.to_string_lossy().into_owned()),
        port: None,
        user: None,
        password: None,
    })
    .unwrap();

    conn.execute(
        "CREATE TABLE t (i INTEGER, b BIGINT, d DOUBLE, v VARCHAR, dt DATE, ts TIMESTAMP, f BOOLEAN, n INTEGER);\
         INSERT INTO t VALUES (1, 9000000000, 2.5, 'x', DATE '2020-02-29', TIMESTAMP '2020-02-29 01:02:03', true, NULL);",
    )
    .unwrap();

    let mut row = Row::new();
    row.insert("i".into(), json!(2));
    row.insert("b".into(), json!(-5));
    row.insert("d".into(), json!(0.25));
    row.insert("v".into(), json!("it's"));
    row.insert("dt".into(), json!("1999-12-31"));
    row.insert("ts".into(), json!("1999-12-31 23:59:59"));
    row.insert("f".into(), json!(false));
    row.insert("n".into(), json!(7));
    conn.insert_rows("t", &[row]).unwrap();

    let rows = conn
        .query("SELECT i, b, d, v, dt, ts, f, n FROM t ORDER BY i")
        .unwrap();
    assert_eq!(rows.len(), 2);
    let first = &rows[0];
    assert_eq!(first["i"], json!(1));
    assert_eq!(first["b"], json!(9_000_000_000i64));
    assert_eq!(first["d"], json!(2.5));
    assert_eq!(first["v"], json!("x"));
    assert_eq!(first["dt"], json!("2020-02-29"));
    assert_eq!(first["ts"], json!("2020-02-29 01:02:03"));
    assert_eq!(first["f"], json!(true));
    assert!(first["n"].is_null());
    let second = &rows[1];
    assert_eq!(second["v"], json!("it's"));
    assert_eq!(second["dt"], json!("1999-12-31"));
    assert_eq!(second["n"], json!(7));

    let counts = conn
        .query("SELECT COUNT(*) AS c, SUM(i) AS s, AVG(d) AS a FROM t")
        .unwrap();
    assert_eq!(counts[0]["c"], json!(2));
    assert_eq!(counts[0]["s"], json!(3)); // SUM(INTEGER) is HUGEINT
    assert_eq!(counts[0]["a"], json!(1.375));

    assert!(conn.query("SELECT * FROM missing_table").is_err());
    assert!(conn.take_warnings().is_empty());
    drop(conn);
    let _ = std::fs::remove_dir_all(&dir);
}
