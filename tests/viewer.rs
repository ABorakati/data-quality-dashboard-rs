//! Integration tests of the read-only result viewer: each test binds a real server on a free
//! loopback port and talks to it over raw TCP, so request targets and headers reach the server
//! exactly as written (an HTTP client library would normalise `/../` away).
use data_quality_dashboard::viewer::{ShutdownHandle, ViewerOptions, ViewerServer};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    thread::JoinHandle,
    time::Duration,
};
use tempfile::TempDir;

mod common;

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("JSON body")
    }
}

struct Viewer {
    address: SocketAddr,
    handle: ShutdownHandle,
    thread: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Viewer {
    fn start(root: &Path) -> Self {
        Self::start_with(ViewerOptions {
            results_root: root.to_path_buf(),
            launch_browser: false,
            ..ViewerOptions::default()
        })
    }

    fn start_with(options: ViewerOptions) -> Self {
        let server = ViewerServer::bind(&options).expect("bind viewer");
        let address = server.address();
        let handle = server.shutdown_handle();
        let thread = Some(std::thread::spawn(move || server.run()));
        Self {
            address,
            handle,
            thread,
        }
    }

    /// Sends `raw` (a complete request, `\r\n` line endings) and reads the whole reply.
    fn raw(&self, raw: &str) -> Response {
        let mut stream = TcpStream::connect(self.address).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).expect("read reply");
        parse(&bytes)
    }

    fn request(&self, method: &str, target: &str, host: &str) -> Response {
        self.raw(&format!(
            "{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        ))
    }

    fn get(&self, target: &str) -> Response {
        self.request("GET", target, &self.address.to_string())
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.handle.shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn parse(bytes: &[u8]) -> Response {
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status line");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let mut body = bytes[split + 4..].to_vec();
    let chunked = head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    if chunked {
        body = dechunk(&body);
    }
    Response {
        status,
        headers,
        body,
    }
}

fn dechunk(mut rest: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let line_end = rest.windows(2).position(|pair| pair == b"\r\n").unwrap();
        let size =
            usize::from_str_radix(std::str::from_utf8(&rest[..line_end]).unwrap().trim(), 16)
                .unwrap();
        rest = &rest[line_end + 2..];
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

/// A results directory with one real result file, a copy of it under another name, and the
/// kinds of file a DQD output folder also holds.
fn results_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    let fixture = common::oracle().join("gibleed/sqlite/results.json");
    fs::copy(&fixture, dir.path().join("b-results.json")).unwrap();
    fs::copy(&fixture, dir.path().join("c-results.JSON")).unwrap();
    fs::write(
        dir.path().join("a-options.json"),
        r#"{"cdmSourceName":"x"}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("a-timing.json"),
        r#"[{"check":"x","seconds":1}]"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("a-truncated.json"),
        r#"{"CheckResults":[{"a":1},"#,
    )
    .unwrap();
    fs::write(dir.path().join("a-empty.json"), "").unwrap();
    fs::write(
        dir.path().join("a-no-metadata.json"),
        r#"{"CheckResults":[{"checkName":"x"}]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("a-bad-rows.json"),
        r#"{"CheckResults":[1,2],"Metadata":[{}]}"#,
    )
    .unwrap();
    fs::write(dir.path().join("a-notes.txt"), "not json").unwrap();
    fs::create_dir(dir.path().join("a-dir.json")).unwrap();
    dir
}

#[test]
fn index_is_served_from_the_embedded_assets() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    for target in ["/", "/index.html"] {
        let response = viewer.get(target);
        assert_eq!(response.status, 200, "{target}");
        assert_eq!(
            response.header("Content-Type"),
            Some("text/html; charset=utf-8")
        );
        let source =
            fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/viewer/index.html"))
                .unwrap();
        assert_eq!(response.body, source);
    }
    for (target, content_type) in [
        ("/js/bridge.js", "application/javascript; charset=utf-8"),
        ("/css/bridge.css", "text/css; charset=utf-8"),
        ("/img/ohdsi.png", "image/png"),
        ("/favicon.ico", "image/x-icon"),
        (
            "/vendor/fontawesome-free/webfonts/fa-brands-400.woff2",
            "font/woff2",
        ),
        (
            "/vendor/googlefonts/Muli-400%2C400i%2C800%2C800i.css",
            "text/css; charset=utf-8",
        ),
    ] {
        let response = viewer.get(target);
        assert_eq!(response.status, 200, "{target}");
        assert_eq!(
            response.header("Content-Type"),
            Some(content_type),
            "{target}"
        );
        assert!(!response.body.is_empty(), "{target}");
    }
    assert_eq!(viewer.get("/missing.js").status, 404);
    assert_eq!(viewer.get("/js").status, 404);
}

#[test]
fn every_embedded_file_matches_the_source_tree() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/viewer");
    let mut stack = vec![base.clone()];
    let mut checked = 0;
    while let Some(folder) = stack.pop() {
        for entry in fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .strip_prefix(&base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let response = viewer.get(&format!("/{name}"));
            assert_eq!(response.status, 200, "{name}");
            assert_eq!(response.body, fs::read(&path).unwrap(), "{name}");
            checked += 1;
        }
    }
    assert!(checked > 30, "only {checked} assets found");
}

#[test]
fn traversal_attempts_are_not_found() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    for target in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/%2E%2E/%2E%2E/Cargo.toml",
        "/..%2fCargo.toml",
        "/..%2FCargo.toml",
        "/js/../../Cargo.toml",
        "/js/%2e%2e/%2e%2e/Cargo.toml",
        "/..\\Cargo.toml",
        "/..%5cCargo.toml",
        "/%5c..%5cCargo.toml",
        "/js\\..\\..\\Cargo.toml",
        "/%2e%2e%5cCargo.toml",
        "//etc/passwd",
        "/%00",
        "/Cargo.toml",
        "/build.rs",
        "/resources/viewer/index.html",
    ] {
        let response = viewer.get(target);
        assert_eq!(response.status, 404, "{target}");
        assert_eq!(response.body, b"Not found", "{target}");
    }
}

#[test]
fn only_get_is_allowed() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    let host = viewer.address.to_string();
    for method in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"] {
        let response = viewer.request(method, "/api/files", &host);
        assert_eq!(response.status, 405, "{method}");
        if method != "HEAD" {
            assert!(response.json()["error"].is_string(), "{method}");
        }
    }
    let with_body = viewer.raw(&format!(
        "POST /api/files HTTP/1.1\r\nHost: {host}\r\nContent-Length: 4\r\nConnection: close\r\n\r\nabcd"
    ));
    assert_eq!(with_body.status, 405);
}

#[test]
fn unexpected_host_headers_are_forbidden() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    let port = viewer.address.port();
    for host in [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("LocalHost:{port}"),
        format!("[::1]:{port}"),
    ] {
        assert_eq!(
            viewer.request("GET", "/api/files", &host).status,
            200,
            "{host}"
        );
    }
    for host in [
        format!("evil.example:{port}"),
        "evil.example".to_owned(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        "127.0.0.1".to_owned(),
        format!("localhost.evil.example:{port}"),
        format!("127.0.0.1.nip.io:{port}"),
        String::new(),
    ] {
        for target in ["/", "/api/files", "/api/results", "/js/bridge.js"] {
            let response = viewer.request("GET", target, &host);
            assert_eq!(response.status, 403, "{host:?} {target}");
            assert!(response.json()["error"].as_str().unwrap().contains("Host"));
        }
    }
    // Missing Host header, and two of them.
    let none = viewer.raw("GET /api/files HTTP/1.0\r\n\r\n");
    assert_eq!(none.status, 403);
    let two = viewer.raw(&format!(
        "GET /api/files HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nHost: evil.example\r\nConnection: close\r\n\r\n"
    ));
    assert_eq!(two.status, 403);
}

#[test]
fn explicit_host_name_is_accepted_when_given_as_host_option() {
    let dir = results_dir();
    let viewer = Viewer::start_with(ViewerOptions {
        results_root: dir.path().to_path_buf(),
        launch_browser: false,
        host: "localhost".into(),
        ..ViewerOptions::default()
    });
    let port = viewer.address.port();
    assert_eq!(
        viewer
            .request("GET", "/api/files", &format!("localhost:{port}"))
            .status,
        200
    );
    assert_eq!(
        viewer
            .request("GET", "/api/files", &format!("other.example:{port}"))
            .status,
        403
    );
}

#[test]
fn wildcard_bind_refuses_dns_names_but_accepts_ip_literals() {
    let dir = results_dir();
    let viewer = Viewer::start_with(ViewerOptions {
        results_root: dir.path().to_path_buf(),
        launch_browser: false,
        host: "0.0.0.0".into(),
        ..ViewerOptions::default()
    });
    let port = viewer.address.port();
    let connect = |host: &str| -> u16 {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        write!(
            stream,
            "GET /api/files HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        parse(&bytes).status
    };
    assert_eq!(connect(&format!("127.0.0.1:{port}")), 200);
    assert_eq!(connect(&format!("192.0.2.1:{port}")), 200);
    assert_eq!(connect(&format!("localhost:{port}")), 200);
    assert_eq!(connect(&format!("attacker.example:{port}")), 403);
    assert_eq!(connect(&format!("192.0.2.1:{}", port.wrapping_add(1))), 403);
}

#[test]
fn file_listing_contains_only_result_files() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    let response = viewer.get("/api/files");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.json(),
        json!({"files": ["b-results.json", "c-results.JSON"], "default": "b-results.json"})
    );
    // Editing a file is noticed (the structural check is cached by modification time and size).
    fs::write(
        dir.path().join("a-options.json"),
        r#"{"CheckResults":[],"Metadata":[{}]}"#,
    )
    .unwrap();
    let response = viewer.get("/api/files");
    assert_eq!(
        response.json()["files"],
        json!(["a-options.json", "b-results.json", "c-results.JSON"])
    );
    fs::write(
        dir.path().join("a-options.json"),
        r#"{"cdmSourceName":"changed"}"#,
    )
    .unwrap();
    assert_eq!(
        viewer.get("/api/files").json()["files"],
        json!(["b-results.json", "c-results.JSON"])
    );
}

#[test]
fn empty_results_root_lists_nothing_and_results_are_not_found() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("timing.json"), "[]").unwrap();
    let viewer = Viewer::start(dir.path());
    assert_eq!(
        viewer.get("/api/files").json(),
        json!({"files": [], "default": null})
    );
    assert_eq!(viewer.get("/api/results").status, 404);
}

#[test]
fn selected_results_file_is_the_default_and_must_be_a_results_file() {
    let dir = results_dir();
    let viewer = Viewer::start_with(ViewerOptions {
        results_root: dir.path().to_path_buf(),
        results_file: Some(dir.path().join("c-results.JSON")),
        launch_browser: false,
        ..ViewerOptions::default()
    });
    assert_eq!(viewer.get("/api/files").json()["default"], "c-results.JSON");

    let error = ViewerServer::bind(&ViewerOptions {
        results_root: dir.path().to_path_buf(),
        results_file: Some(dir.path().join("a-options.json")),
        launch_browser: false,
        ..ViewerOptions::default()
    })
    .err()
    .expect("options.json is not a results file");
    assert!(
        error.to_string().contains("not a DQD results file"),
        "{error:#}"
    );
}

#[test]
fn results_are_served_and_traversal_in_the_file_parameter_is_refused() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    let response = viewer.get("/api/results");
    assert_eq!(response.status, 200);
    assert!(
        response.json()["CheckResults"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    let named = viewer.get("/api/results?file=c-results.JSON");
    assert_eq!(named.status, 200);
    assert_eq!(named.json(), response.json());

    let outside = TempDir::new().unwrap();
    fs::copy(
        common::oracle().join("gibleed/sqlite/results.json"),
        outside.path().join("outside.json"),
    )
    .unwrap();
    let outside_name = outside
        .path()
        .join("outside.json")
        .to_string_lossy()
        .replace('\\', "/");
    for file in [
        "../results.json".to_owned(),
        "..%2fb-results.json".to_owned(),
        "%2e%2e%2fCargo.toml".to_owned(),
        "..%5cb-results.json".to_owned(),
        "sub%2f..%2fb-results.json".to_owned(),
        "./b-results.json".to_owned(),
        "b-results.json%00.json".to_owned(),
        "/etc/passwd".to_owned(),
        outside_name.replace('/', "%2f"),
        "a-dir.json".to_owned(),
        "a-notes.txt".to_owned(),
        "missing.json".to_owned(),
        String::new(),
    ] {
        let response = viewer.get(&format!("/api/results?file={file}"));
        assert!(
            matches!(response.status, 400 | 404),
            "{file:?} gave {}",
            response.status
        );
        assert!(response.json()["error"].is_string(), "{file:?}");
    }
    assert_eq!(viewer.get("/api/results?file=%zz").status, 400);
    // A non-results JSON file in the root is readable by name but fails validation.
    assert_eq!(viewer.get("/api/results?file=a-options.json").status, 422);
}

#[test]
fn symlinks_leaving_the_results_root_are_not_listed_or_served() {
    #[cfg(unix)]
    {
        let dir = results_dir();
        let outside = TempDir::new().unwrap();
        let target = outside.path().join("outside.json");
        fs::copy(
            common::oracle().join("gibleed/sqlite/results.json"),
            &target,
        )
        .unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("linked.json")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("gone.json"), dir.path().join("broken.json"))
            .unwrap();
        let viewer = Viewer::start(dir.path());
        assert_eq!(
            viewer.get("/api/files").json()["files"],
            json!(["b-results.json", "c-results.JSON"])
        );
        assert_eq!(viewer.get("/api/results?file=linked.json").status, 404);
    }
}

#[test]
fn security_headers_are_on_every_response() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    let host = viewer.address.to_string();
    let responses = [
        ("/", viewer.get("/")),
        ("/js/bridge.js", viewer.get("/js/bridge.js")),
        ("/missing", viewer.get("/missing")),
        ("/../Cargo.toml", viewer.get("/../Cargo.toml")),
        ("/api/files", viewer.get("/api/files")),
        ("/api/results", viewer.get("/api/results")),
        (
            "/api/results?file=nope.json",
            viewer.get("/api/results?file=nope.json"),
        ),
        ("POST", viewer.request("POST", "/", &host)),
        ("bad host", viewer.request("GET", "/", "evil.example")),
    ];
    for (label, response) in &responses {
        assert_eq!(
            response.header("X-Content-Type-Options"),
            Some("nosniff"),
            "{label}"
        );
        assert_eq!(
            response.header("Referrer-Policy"),
            Some("no-referrer"),
            "{label}"
        );
        let policy = response
            .header("Content-Security-Policy")
            .unwrap_or_default();
        for directive in [
            "default-src 'none'",
            "script-src 'self'",
            "frame-ancestors 'none'",
            "base-uri 'none'",
        ] {
            assert!(policy.contains(directive), "{label}: {policy}");
        }
        assert!(
            !policy.contains("script-src 'self' 'unsafe-inline'"),
            "{label}"
        );
    }
    for (label, response) in &responses {
        if label.starts_with("/api/") {
            assert_eq!(
                response.header("Cache-Control"),
                Some("no-store"),
                "{label}"
            );
        }
    }
}

#[test]
fn stalled_and_malformed_requests_do_not_block_other_clients() {
    let dir = results_dir();
    let viewer = Viewer::start(dir.path());
    // Connections that say nothing, stop mid request line, or stop mid headers.
    let mut stalled = vec![TcpStream::connect(viewer.address).unwrap()];
    let mut partial_line = TcpStream::connect(viewer.address).unwrap();
    partial_line.write_all(b"GET /api/fi").unwrap();
    stalled.push(partial_line);
    let mut partial_headers = TcpStream::connect(viewer.address).unwrap();
    partial_headers
        .write_all(
            format!(
                "GET /api/files HTTP/1.1\r\nHost: {}\r\nX-Slow: ",
                viewer.address
            )
            .as_bytes(),
        )
        .unwrap();
    stalled.push(partial_headers);
    // A request that never reads its (large) reply.
    let mut unread = TcpStream::connect(viewer.address).unwrap();
    unread
        .write_all(
            format!(
                "GET /api/results HTTP/1.1\r\nHost: {}\r\n\r\n",
                viewer.address
            )
            .as_bytes(),
        )
        .unwrap();
    stalled.push(unread);

    for garbage in [
        "\r\n\r\n".to_owned(),
        "NOT HTTP AT ALL\r\n\r\n".to_owned(),
        "GET / HTTP/9.9\r\n\r\n".to_owned(),
        format!(
            "GET /api/files HTTP/1.1\r\nHost: {}\r\nBad Header\r\n\r\n",
            viewer.address
        ),
        format!(
            "GET /%ff%fe HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            viewer.address
        ),
        format!(
            "GET /api/results?file=%ff HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            viewer.address
        ),
    ] {
        let mut stream = TcpStream::connect(viewer.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let _ = stream.write_all(garbage.as_bytes());
        let mut sink = Vec::new();
        let _ = stream.read_to_end(&mut sink);
    }
    for _ in 0..3 {
        assert_eq!(viewer.get("/api/files").status, 200);
        assert_eq!(viewer.get("/").status, 200);
    }
    drop(stalled);
}

#[test]
fn shutdown_ends_run() {
    let dir = results_dir();
    let mut viewer = Viewer::start(dir.path());
    assert_eq!(viewer.get("/api/files").status, 200);
    viewer.handle.shutdown();
    let thread = viewer.thread.take().unwrap();
    assert!(thread.join().unwrap().is_ok());
}
