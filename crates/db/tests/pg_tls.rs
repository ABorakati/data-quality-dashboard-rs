//! PostgreSQL TLS (`sslmode`) behaviour of `dqd_db::connect`.
//!
//! * The first group runs a scripted fake server on a loopback socket: it records which packets the client
//!   sends (an `SSLRequest` or a startup message) and so pins the attempt order of each `sslmode` without
//!   a PostgreSQL server.
//! * The second group needs real servers with TLS and is skipped unless `DQD_PG_TLS_PORT` is set. Create
//!   them with `crates/db/tests/pg_tls/setup.sh start <dir>`, which prints the variables to export
//!   (`DQD_PG_TLS_DIR`, `DQD_PG_TLS_PORT`, `DQD_PG_TLS_WRONGNAME_PORT`, `DQD_PG_PLAIN_PORT`).

use dqd_db::{ConnectSpec, Dbms};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::Duration;

fn spec(host_and_query: &str, port: u16) -> ConnectSpec {
    ConnectSpec {
        dbms: Dbms::Postgresql,
        sqlite_path: None,
        server: Some(host_and_query.to_owned()),
        port: Some(port),
        user: Some("dqd".into()),
        password: Some("secret".into()),
    }
}

fn error_of(spec: &ConnectSpec) -> String {
    match dqd_db::connect(spec) {
        Ok(_) => panic!("expected {:?} to fail", spec.server),
        Err(error) => error.to_string(),
    }
}

// ---- scripted server -------------------------------------------------------------------------------

#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "tls"), allow(dead_code))]
enum Reply {
    /// `N`: no TLS here.
    No,
    /// `S`, then bytes that are not a TLS handshake.
    YesThenGarbage,
}

/// What the client sent first on one connection.
#[derive(Debug, PartialEq, Eq)]
enum First {
    SslRequest,
    Startup,
}

const SSL_REQUEST_CODE: u32 = 80_877_103;

/// Serve `replies.len()` connections, answering each `SSLRequest` per `replies` and closing after the
/// startup message. Returns the port and, once joined, what each connection started with.
fn fake_server(replies: Vec<Reply>, connections: usize) -> (u16, JoinHandle<Vec<First>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut replies = replies.into_iter();
        for _ in 0..connections {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut header = [0u8; 8];
            if stream.read_exact(&mut header).is_err() {
                continue;
            }
            let code = u32::from_be_bytes(header[4..8].try_into().unwrap());
            if code == SSL_REQUEST_CODE {
                seen.push(First::SslRequest);
                match replies.next().expect("a reply per SSLRequest") {
                    Reply::No => {
                        stream.write_all(b"N").unwrap();
                        // The client carries on in plain text with its startup message.
                        let mut startup = [0u8; 8];
                        if stream.read_exact(&mut startup).is_ok() {
                            assert_eq!(
                                u32::from_be_bytes(startup[4..8].try_into().unwrap()),
                                196_608,
                                "protocol 3.0 startup after N"
                            );
                            seen.push(First::Startup);
                        }
                    }
                    Reply::YesThenGarbage => {
                        stream.write_all(b"S").unwrap();
                        stream.write_all(&[0x15, 3, 3, 0, 2, 2, 40]).unwrap(); // a fatal TLS alert
                    }
                }
            } else {
                seen.push(First::Startup);
            }
        }
        seen
    });
    (port, handle)
}

fn connect_error(query: &str, replies: Vec<Reply>, connections: usize) -> (String, Vec<First>) {
    let (port, server) = fake_server(replies, connections);
    let error = error_of(&spec(&format!("127.0.0.1/db?{query}"), port));
    (error, server.join().unwrap())
}

#[test]
fn disable_and_allow_start_in_plain_text() {
    for mode in ["disable", "allow"] {
        let (_, seen) = connect_error(&format!("sslmode={mode}"), vec![], 1);
        assert_eq!(seen, vec![First::Startup], "sslmode={mode}");
    }
}

#[cfg(feature = "tls")]
#[test]
fn prefer_asks_for_tls_and_carries_on_in_plain_text_after_n() {
    let (_, seen) = connect_error("sslmode=prefer", vec![Reply::No], 1);
    assert_eq!(seen, vec![First::SslRequest, First::Startup]);
    // The default sslmode is prefer.
    let (_, seen) = connect_error("connectTimeout=5", vec![Reply::No], 1);
    assert_eq!(seen, vec![First::SslRequest, First::Startup]);
}

#[cfg(not(feature = "tls"))]
#[test]
fn without_tls_prefer_stays_plain() {
    let (_, seen) = connect_error("sslmode=prefer", vec![], 1);
    assert_eq!(seen, vec![First::Startup]);
}

#[cfg(feature = "tls")]
#[test]
fn prefer_retries_in_plain_text_when_the_tls_handshake_fails() {
    let (_, seen) = connect_error("sslmode=prefer", vec![Reply::YesThenGarbage], 2);
    assert_eq!(seen, vec![First::SslRequest, First::Startup]);
}

#[cfg(feature = "tls")]
#[test]
fn strict_modes_never_fall_back() {
    for mode in ["require", "verify-ca", "verify-full"] {
        let query = format!("sslmode={mode}&sslrootcert=system");
        let (error, seen) = connect_error(&query, vec![Reply::No], 1);
        assert_eq!(seen, vec![First::SslRequest], "sslmode={mode}");
        assert!(
            error.contains("The server does not support SSL.") && error.contains("SQLSTATE=08004"),
            "{error}"
        );
        let (error, seen) = connect_error(&query, vec![Reply::YesThenGarbage], 1);
        assert_eq!(seen, vec![First::SslRequest], "sslmode={mode}");
        assert!(
            error.contains("SSL error:") && error.contains("SQLSTATE=08006"),
            "{error}"
        );
    }
}

#[test]
fn unknown_sslmode_is_rejected_before_connecting() {
    let error = error_of(&spec("127.0.0.1/db?sslmode=bogus", 1));
    assert!(error.contains("Invalid sslmode value: bogus"), "{error}");
    // pgjdbc compares case-sensitively.
    let error = error_of(&spec("127.0.0.1/db?sslmode=Require", 1));
    assert!(error.contains("Invalid sslmode value: Require"), "{error}");
}

#[cfg(not(feature = "tls"))]
#[test]
fn without_tls_the_strict_modes_keep_the_historical_error() {
    for mode in ["require", "verify-ca", "verify-full"] {
        let error = error_of(&spec(&format!("127.0.0.1/db?sslmode={mode}"), 1));
        assert!(
            error.contains(&format!(
                "sslmode={mode} requires TLS, which this build of dqd-db does not support. Use sslmode=disable, allow or prefer."
            )),
            "{error}"
        );
    }
}

#[cfg(feature = "tls")]
#[test]
fn bad_tls_files_are_reported_before_connecting() {
    let error = error_of(&spec(
        "127.0.0.1/db?sslmode=verify-ca&sslrootcert=/nonexistent/root.crt",
        1,
    ));
    assert!(
        error.contains("Could not open SSL root certificate file /nonexistent/root.crt."),
        "{error}"
    );
    // The root file is not consulted by the modes that do not verify.
    let error = error_of(&spec(
        "127.0.0.1/db?sslmode=require&sslrootcert=/nonexistent/root.crt",
        1,
    ));
    assert!(error.contains("refused"), "{error}");
    let error = error_of(&spec(
        "127.0.0.1/db?sslmode=require&sslcert=/nonexistent/c.crt",
        1,
    ));
    assert!(
        error.contains("sslcert and sslkey must be given together"),
        "{error}"
    );
    let error = error_of(&spec(
        "127.0.0.1/db?sslmode=require&sslcert=/nonexistent/c.crt&sslkey=/nonexistent/c.key",
        1,
    ));
    assert!(
        error.contains("Could not open SSL certificate file /nonexistent/c.crt."),
        "{error}"
    );
}

// ---- real servers ----------------------------------------------------------------------------------

struct Servers {
    dir: String,
    good: u16,
    wrong_name: u16,
    plain: u16,
}

fn servers() -> Option<Servers> {
    let port = |name: &str| {
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} must be set together with DQD_PG_TLS_PORT"))
            .parse()
            .unwrap_or_else(|e| panic!("{name} must be a port number: {e}"))
    };
    if !cfg!(feature = "tls") {
        eprintln!("skipping PostgreSQL TLS tests: built without the `tls` feature");
        return None;
    }
    let Ok(good) = std::env::var("DQD_PG_TLS_PORT") else {
        eprintln!("skipping PostgreSQL TLS tests: DQD_PG_TLS_PORT is not set");
        return None;
    };
    Some(Servers {
        dir: std::env::var("DQD_PG_TLS_DIR")
            .expect("DQD_PG_TLS_DIR must be set with DQD_PG_TLS_PORT"),
        good: good.parse().expect("DQD_PG_TLS_PORT must be a port number"),
        wrong_name: port("DQD_PG_TLS_WRONGNAME_PORT"),
        plain: port("DQD_PG_PLAIN_PORT"),
    })
}

/// Connect as `user` and report whether the backend session is encrypted.
fn is_encrypted(user: &str, server: &str, port: u16) -> anyhow::Result<bool> {
    let mut spec = spec(server, port);
    spec.user = Some(user.into());
    let mut db = dqd_db::connect(&spec)?;
    let rows = db.query("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()")?;
    Ok(rows[0]["ssl"] == serde_json::Value::Bool(true))
}

fn ok(server: &str, port: u16) -> bool {
    is_encrypted("dqd", server, port).unwrap_or_else(|e| panic!("{server}: {e}"))
}

#[test]
fn modes_against_a_tls_server() {
    let Some(s) = servers() else { return };
    let ca = format!("sslrootcert={}/ca.crt", s.dir);
    let db = "localhost/postgres";
    assert!(!ok(&format!("{db}?sslmode=disable"), s.good));
    assert!(
        !ok(&format!("{db}?sslmode=allow"), s.good),
        "allow tries plain text first"
    );
    assert!(ok(&format!("{db}?sslmode=prefer"), s.good));
    assert!(ok(db, s.good), "prefer is the default");
    assert!(ok(&format!("{db}?sslmode=require"), s.good));
    // `require` does not look at the certificate, with or without a (wrong) root file.
    assert!(ok(
        &format!("{db}?sslmode=require&sslrootcert={}/other-ca.crt", s.dir),
        s.good
    ));
    assert!(ok(&format!("{db}?sslmode=verify-ca&{ca}"), s.good));
    assert!(ok(&format!("{db}?sslmode=verify-full&{ca}"), s.good));
    assert!(
        ok(
            &format!("127.0.0.1/postgres?sslmode=verify-full&{ca}"),
            s.good
        ),
        "IP SAN"
    );
    assert!(
        ok(&format!("{db}?ssl=true&{ca}"), s.good),
        "ssl=true is verify-full"
    );
    assert!(
        ok(&format!("{db}?sslmode=require&ssl=true"), s.good),
        "sslmode wins over ssl"
    );
}

#[test]
fn verification_failures_against_a_tls_server() {
    let Some(s) = servers() else { return };
    let other = format!("sslrootcert={}/other-ca.crt", s.dir);
    for mode in ["verify-ca", "verify-full"] {
        let error = is_encrypted(
            "dqd",
            &format!("localhost/postgres?sslmode={mode}&{other}"),
            s.good,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("SSL error:") && error.contains("UnknownIssuer"),
            "{mode}: {error}"
        );
        // The test CA is not in the system store either.
        let error = is_encrypted("dqd", &format!("localhost/postgres?sslmode={mode}"), s.good)
            .unwrap_err()
            .to_string();
        assert!(error.contains("UnknownIssuer"), "{mode}: {error}");
    }
}

#[test]
fn host_name_is_only_checked_by_verify_full() {
    let Some(s) = servers() else { return };
    let ca = format!("sslrootcert={}/ca.crt", s.dir);
    let db = "localhost/postgres";
    assert!(ok(&format!("{db}?sslmode=require"), s.wrong_name));
    assert!(ok(&format!("{db}?sslmode=verify-ca&{ca}"), s.wrong_name));
    let error = is_encrypted(
        "dqd",
        &format!("{db}?sslmode=verify-full&{ca}"),
        s.wrong_name,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("SSL error:") && error.contains("not valid for name"),
        "{error}"
    );
}

#[test]
fn server_that_only_accepts_tls() {
    let Some(s) = servers() else { return };
    let ca = format!("sslrootcert={}/ca.crt", s.dir);
    let db = "localhost/postgres";
    let rejected = is_encrypted("sslonly", &format!("{db}?sslmode=disable"), s.good)
        .unwrap_err()
        .to_string();
    assert!(rejected.contains("SQLSTATE=28000"), "{rejected}");
    // allow: plain text first, TLS after the server rejected it.
    let mut spec = spec(&format!("{db}?sslmode=allow"), s.good);
    spec.user = Some("sslonly".into());
    assert!(dqd_db::connect(&spec).is_ok());
    assert!(is_encrypted("sslonly", &format!("{db}?sslmode=prefer"), s.good).unwrap());
    assert!(is_encrypted("sslonly", &format!("{db}?sslmode=verify-full&{ca}"), s.good).unwrap());
    // A wrong password is not retried and is reported as the server's error.
    let mut wrong = spec.clone();
    wrong.password = Some("nope".into());
    let error = error_of(&wrong);
    assert!(error.contains("SQLSTATE=28P01"), "{error}");
}

#[test]
fn server_without_tls() {
    let Some(s) = servers() else { return };
    let db = "localhost/postgres";
    assert!(!ok(&format!("{db}?sslmode=disable"), s.plain));
    assert!(!ok(&format!("{db}?sslmode=allow"), s.plain));
    assert!(
        !ok(&format!("{db}?sslmode=prefer"), s.plain),
        "prefer falls back to plain text"
    );
    for mode in ["require", "verify-ca", "verify-full"] {
        let error = is_encrypted(
            "dqd",
            &format!("{db}?sslmode={mode}&sslrootcert=system"),
            s.plain,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("The server does not support SSL."),
            "{mode}: {error}"
        );
    }
}

#[test]
fn client_certificate_authentication() {
    let Some(s) = servers() else { return };
    let dir = &s.dir;
    let base = format!("localhost/postgres?sslmode=verify-full&sslrootcert={dir}/ca.crt");
    for key in ["client.key", "client.pk8"] {
        let url = format!("{base}&sslcert={dir}/client.crt&sslkey={dir}/{key}");
        assert!(is_encrypted("certuser", &url, s.good).unwrap(), "{key}");
    }
    // No certificate: the server refuses.
    assert!(is_encrypted("certuser", &base, s.good).is_err());
    let error = is_encrypted(
        "certuser",
        &format!("{base}&sslcert={dir}/client.crt&sslkey={dir}/client-encrypted.key"),
        s.good,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("sslpassword is not supported"), "{error}");
}
