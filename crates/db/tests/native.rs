//! Checks of the native SQLite connection. The expected strings were captured from the former JDBC bridge
//! (sqlite-jdbc 3.50.3.0) in its differential suite, which was removed with the bridge.

use dqd_db::{ConnectSpec, Connection, Dbms, Row};
use serde_json::json;

fn open(name: &str) -> Box<dyn Connection> {
    let dir = std::env::temp_dir().join(format!("dqd-db-native-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dqd_db::connect(&ConnectSpec {
        dbms: Dbms::Sqlite,
        sqlite_path: Some(dir.join("db.sqlite")),
        server: None,
        port: None,
        user: None,
        password: None,
    })
    .unwrap()
}

fn error_of<T>(result: anyhow::Result<T>) -> String {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn error_text_matches_the_jdbc_bridge() {
    let mut db = open("errors");
    assert_eq!(
        error_of(db.query("SELECT * FROM nosuch")),
        "org.sqlite.SQLiteException: [SQLITE_ERROR] SQL error or missing database (no such table: nosuch) [SQLSTATE=; vendorCode=1]"
    );
    assert_eq!(
        error_of(db.query("CREATE TABLE a (x)")),
        "java.sql.SQLException: query does not return ResultSet [SQLSTATE=SQLITE_DONE; vendorCode=101]"
    );
    assert_eq!(
        error_of(db.query("SELECT 1; SELECT 2")),
        "java.lang.IllegalArgumentException: A query that returns a result can only consist of one SQL statement, but 2 statements were found [SQLSTATE=; vendorCode=0]"
    );
    assert_eq!(
        error_of(db.query("END")),
        "java.util.EmptyStackException: java.util.EmptyStackException [SQLSTATE=; vendorCode=0]"
    );
    db.execute("CREATE TABLE u (k INTEGER PRIMARY KEY)")
        .unwrap();
    db.execute("INSERT INTO u VALUES (1)").unwrap();
    assert_eq!(
        error_of(db.execute("INSERT INTO u VALUES (1)")),
        "org.sqlite.SQLiteException: [SQLITE_CONSTRAINT_PRIMARYKEY] A PRIMARY KEY constraint failed (UNIQUE constraint failed: u.k) [SQLSTATE=; vendorCode=19]"
    );
}

#[test]
fn value_mapping_matches_read_value() {
    let mut db = open("values");
    let rows = db
        .query("SELECT 1 AS i, 1.5 AS d, 'x' AS s, NULL AS n, x'41' AS b, 86400 AS start_date, 1700000000 AS Event_DateTime, '1700000000' AS txt_date, 'abc' AS bad_date")
        .unwrap();
    assert_eq!(
        serde_json::to_string(&rows[0]).unwrap(),
        r#"{"i":1,"d":1.5,"s":"x","n":null,"b":"A","start_date":"1970-01-02","Event_DateTime":"2023-11-14 22:13:20","txt_date":"2023-11-14","bad_date":null}"#
    );
    // Integers of 2^53 and more are refused, smaller 64-bit values are not.
    assert_eq!(
        db.query("SELECT 9007199254740991 AS v").unwrap()[0]["v"],
        json!(9007199254740991i64)
    );
    assert!(
        error_of(db.query("SELECT 9007199254740992 AS v"))
            .starts_with("java.lang.ArithmeticException: The data contains integers >= 2^53")
    );
    // Declared types drive the mapping: BOOLEAN reads as a boolean; NUMERIC affinity stores 2 as an integer.
    db.execute("CREATE TABLE t (b BOOLEAN, d DECIMAL(10,2), big BIGINT)")
        .unwrap();
    db.execute("INSERT INTO t VALUES (1, 2, 3)").unwrap();
    let typed = db.query("SELECT b, d, big FROM t").unwrap();
    assert_eq!(
        serde_json::to_string(&typed[0]).unwrap(),
        r#"{"b":true,"d":2,"big":3}"#
    );
    // The JDBC build has the math functions; the bundled one needs them registered.
    assert_eq!(
        db.query("SELECT sqrt(16) AS r, power(2, 10) AS p").unwrap()[0]["p"],
        json!(1024.0)
    );
}

/// Expected values come from real `RSQLite::SQLite()` connections (DatabaseConnector's SQLite),
/// which load extension-functions.c and so provide `charindex`; DQD's SQLite SQL needs it.
#[test]
fn charindex_matches_rsqlite() {
    let mut db = open("charindex");
    let cases: &[(&str, serde_json::Value)] = &[
        ("charindex('.', '12.5')", json!(3)),
        ("charindex('.', '125')", json!(0)),
        ("charindex('.', 12.5)", json!(3)),
        ("charindex('b', 'abcabc')", json!(2)),
        ("charindex('b', 'abcabc', 3)", json!(5)),
        ("charindex('b', 'abcabc', 0)", json!(2)),
        ("charindex('b', 'abcabc', -5)", json!(2)),
        ("charindex('b', 'abcabc', 7)", json!(0)),
        ("charindex('b', 'abcabc', 99)", json!(0)),
        ("charindex('', 'abc')", json!(0)),
        ("charindex('', 'abc', 2)", json!(0)),
        ("charindex('', '')", json!(0)),
        ("charindex('\u{e9}', 'a\u{e9}\u{e9}')", json!(2)),
        ("charindex('\u{e9}', 'a\u{e9}\u{e9}', 3)", json!(3)),
        ("charindex(NULL, 'abc')", json!(null)),
        ("charindex('a', NULL)", json!(null)),
        ("charindex('a', 'abc', NULL)", json!(1)),
        ("charindex('b', 'abc', '2')", json!(2)),
        ("charindex('b', 'abc', 2.9)", json!(2)),
        ("charindex(x'61', 'abc')", json!(1)),
        ("charindex('B', 'abc')", json!(0)),
        ("charindex('abc', 'ab')", json!(0)),
        ("charindex(1, 21)", json!(2)),
        ("charindex('c', 'abc', 3)", json!(3)),
        ("charindex('c', 'abc', 4)", json!(0)),
        ("charindex('', 'abc', 5)", json!(0)),
    ];
    for (expression, expected) in cases {
        let rows = db.query(&format!("SELECT {expression} AS v")).unwrap();
        assert_eq!(&rows[0]["v"], expected, "{expression}");
    }
}

#[test]
fn insert_rows_types_and_empty_input() {
    let mut db = open("insert");
    db.execute("CREATE TABLE r (a, b, c, d, e)").unwrap();
    let row: Row =
        serde_json::from_value(json!({"a": 1, "b": 0.5, "c": "s", "d": null, "e": true})).unwrap();
    db.insert_rows("r", &[row]).unwrap();
    db.insert_rows("r", &[]).unwrap();
    let rows = db
        .query("SELECT typeof(a) a, typeof(b) b, typeof(c) c, typeof(d) d, typeof(e) e FROM r")
        .unwrap();
    assert_eq!(
        serde_json::to_string(&rows[0]).unwrap(),
        r#"{"a":"integer","b":"real","c":"text","d":"null","e":"integer"}"#
    );
    let mismatched: Vec<Row> = vec![
        serde_json::from_value(json!({"a": 1})).unwrap(),
        serde_json::from_value(json!({"b": 1})).unwrap(),
    ];
    assert_eq!(
        error_of(db.insert_rows("r", &mismatched)),
        "Inserted rows must have the same columns in the same shape"
    );
}

#[test]
fn postgres_server_string_follows_database_connector() {
    let spec = |server: Option<&str>, user: Option<&str>, password: Option<&str>| ConnectSpec {
        dbms: Dbms::Postgresql,
        sqlite_path: None,
        server: server.map(str::to_owned),
        port: Some(1),
        user: user.map(str::to_owned),
        password: password.map(str::to_owned),
    };
    assert_eq!(
        error_of(dqd_db::connect(&spec(Some("localhost"), None, None))),
        "Error: database name not included in server string but is required for PostgreSQL. Please specify server as <host>/<database>"
    );
    assert_eq!(
        error_of(dqd_db::connect(&spec(
            Some("localhost/db"),
            Some("u"),
            None
        ))),
        "Connection propery 'password' is NULL."
    );
    // Nothing listens on port 1: the pgjdbc "refused" text, SQLSTATE 08001.
    assert_eq!(
        error_of(dqd_db::connect(&spec(
            Some("localhost/db"),
            Some("u"),
            Some("p")
        ))),
        "org.postgresql.util.PSQLException: Connection to localhost:1 refused. Check that the hostname and port are correct and that the postmaster is accepting TCP/IP connections. [SQLSTATE=08001; vendorCode=0]"
    );
}
