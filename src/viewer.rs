//! Read-only HTTP viewer for dashboard result files.
//!
//! The browser assets of `resources/viewer` are compiled into the binary (see `build.rs`), so an
//! installed `dqd` serves them from any directory. The only data read at run time are the JSON
//! result files below the configured results root.

use std::{
    collections::HashMap,
    fs,
    io::BufReader,
    net::{IpAddr, SocketAddr},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::SystemTime,
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{
    Deserialize, Deserializer,
    de::{IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::results::dashboard_results;

include!(concat!(env!("OUT_DIR"), "/viewer_assets.rs"));

/// Requests handled concurrently. Beyond this the viewer answers 503 instead of spawning more
/// threads, so a flood of stalled connections cannot exhaust the process.
const MAX_IN_FLIGHT_REQUESTS: usize = 64;

/// Sent with every response.
///
/// The dashboard loads only same-origin scripts, styles, fonts and images and fetches only the
/// same-origin JSON API. Two relaxations are unavoidable for the upstream components:
/// `'unsafe-eval'` because Handlebars compiles its templates with `new Function`, and
/// `'unsafe-inline'` for styles because DataTables and the Handlebars templates emit `style`
/// attributes. There are no inline scripts, so `'unsafe-inline'` is not granted for scripts.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self' 'unsafe-eval'; \
    style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; \
    base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

#[derive(Debug, Clone)]
pub struct ViewerOptions {
    pub results_root: PathBuf,
    pub results_file: Option<PathBuf>,
    pub host: String,
    pub port: u16,
    pub launch_browser: bool,
}

impl Default for ViewerOptions {
    fn default() -> Self {
        Self {
            results_root: PathBuf::from("."),
            results_file: None,
            host: "127.0.0.1".into(),
            port: 0,
            launch_browser: true,
        }
    }
}

/// A complete HTTP response, built by the handlers and written once by [`respond`].
struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    no_store: bool,
}

impl Reply {
    fn json(status: u16, value: &Value) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body: serde_json::to_vec(value)
                .unwrap_or_else(|_| br#"{"error":"Response could not be encoded"}"#.to_vec()),
            no_store: true,
        }
    }

    fn error(status: u16, message: impl std::fmt::Display) -> Self {
        Self::json(status, &json!({"error": message.to_string()}))
    }

    fn not_found() -> Self {
        Self {
            status: 404,
            content_type: "text/plain; charset=utf-8",
            body: b"Not found".to_vec(),
            no_store: false,
        }
    }
}

fn header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn respond(request: Request, reply: Reply) {
    let mut response = Response::from_data(reply.body)
        .with_status_code(StatusCode(reply.status))
        .with_header_opt(header("Content-Type", reply.content_type));
    for (name, value) in [
        ("Content-Security-Policy", CONTENT_SECURITY_POLICY),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
    ] {
        response = response.with_header_opt(header(name, value));
    }
    if reply.no_store {
        response = response.with_header_opt(header("Cache-Control", "no-store"));
    }
    if let Err(error) = request.respond(response) {
        eprintln!("Viewer response failed: {error}");
    }
}

trait WithHeaderOpt {
    fn with_header_opt(self, header: Option<Header>) -> Self;
}

impl<R: std::io::Read> WithHeaderOpt for Response<R> {
    fn with_header_opt(self, header: Option<Header>) -> Self {
        match header {
            Some(header) => self.with_header(header),
            None => self,
        }
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Percent-decodes `value` (and `+` as a space when `plus_is_space`); `None` when the escapes
/// are malformed or the result is not UTF-8.
fn percent_decode(value: &str, plus_is_space: bool) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' if plus_is_space => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = hex_value(*bytes.get(index + 1)?)?;
                let low = hex_value(*bytes.get(index + 2)?)?;
                decoded.push(high << 4 | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}

/// The decoded value of query parameter `key`: `Ok(None)` when absent, `Err` when malformed.
fn query_value(url: &str, key: &str) -> Result<Option<String>> {
    let Some((_, query)) = url.split_once('?') else {
        return Ok(None);
    };
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == key {
            return percent_decode(value, true).map(Some).with_context(|| {
                format!("Query parameter {key} is not valid percent-encoded UTF-8")
            });
        }
    }
    Ok(None)
}

/// Accepts a JSON object and discards its contents.
struct Object;

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = Object;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Object, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Object)
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

/// Accepts a JSON array of objects and discards its contents.
struct ObjectArray;

impl<'de> Deserialize<'de> for ObjectArray {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectArrayVisitor;
        impl<'de> Visitor<'de> for ObjectArrayVisitor {
            type Value = ObjectArray;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an array of objects")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ObjectArray, A::Error> {
                while seq.next_element::<Object>()?.is_some() {}
                Ok(ObjectArray)
            }
        }
        deserializer.deserialize_seq(ObjectArrayVisitor)
    }
}

/// The structural check of a DQD results file: a top-level object holding a `CheckResults`
/// array of objects and a `Metadata` array. Values are skipped, not built.
struct ResultsShape;

impl<'de> Deserialize<'de> for ResultsShape {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ShapeVisitor;
        impl<'de> Visitor<'de> for ShapeVisitor {
            type Value = ResultsShape;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a results object with CheckResults and Metadata")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ResultsShape, A::Error> {
                let (mut check_results, mut metadata) = (false, false);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "CheckResults" => {
                            map.next_value::<ObjectArray>()?;
                            check_results = true;
                        }
                        "Metadata" => {
                            map.next_value::<Vec<IgnoredAny>>()?;
                            metadata = true;
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                if check_results && metadata {
                    Ok(ResultsShape)
                } else {
                    Err(serde::de::Error::custom("missing CheckResults or Metadata"))
                }
            }
        }
        deserializer.deserialize_map(ShapeVisitor)
    }
}

/// Streams the file once without building a document tree.
fn looks_like_results(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(file));
    ResultsShape::deserialize(&mut deserializer).is_ok() && deserializer.end().is_ok()
}

type SniffCache = HashMap<PathBuf, (SystemTime, u64, bool)>;

fn is_json_file_name(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// The `*.json` files directly in `root` that are DQD results, sorted by name. A file is read
/// again only when its modification time or size changes.
fn result_files(root: &Path, cache: &Mutex<SniffCache>) -> Result<Vec<String>> {
    let root = root
        .canonicalize()
        .with_context(|| format!("Results root does not exist: {}", root.display()))?;
    let mut files = Vec::new();
    let mut seen = SniffCache::new();
    for entry in
        fs::read_dir(&root).with_context(|| format!("Reading results root {}", root.display()))?
    {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        // Names and files that cannot be listed (non-UTF-8, broken links, links leading out of
        // the root) are left out rather than failing the whole listing.
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_json_file_name(&path) {
            continue;
        }
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if !canonical.is_file() || canonical.parent() != Some(root.as_path()) {
            continue;
        }
        let Ok(metadata) = canonical.metadata() else {
            continue;
        };
        let stamp = metadata
            .modified()
            .ok()
            .map(|modified| (modified, metadata.len()));
        let cached = stamp.and_then(|(modified, length)| {
            let cache = cache.lock().ok()?;
            match cache.get(&canonical) {
                Some(&(seen_modified, seen_length, verdict))
                    if seen_modified == modified && seen_length == length =>
                {
                    Some(verdict)
                }
                _ => None,
            }
        });
        let verdict = cached.unwrap_or_else(|| looks_like_results(&canonical));
        if let Some((modified, length)) = stamp {
            seen.insert(canonical, (modified, length, verdict));
        }
        if verdict {
            files.push(name.to_owned());
        }
    }
    if let Ok(mut cache) = cache.lock() {
        *cache = seen;
    }
    files.sort();
    Ok(files)
}

fn safe_result_path(root: &Path, file: &str) -> Result<PathBuf> {
    let input = Path::new(file);
    if input
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
        || input.file_name().and_then(|value| value.to_str()) != Some(file)
    {
        bail!("Result file must be a basename within the results root");
    }
    let root = root.canonicalize()?;
    let path = root
        .join(input)
        .canonicalize()
        .with_context(|| format!("Results file does not exist: {file}"))?;
    if path.parent() != Some(root.as_path()) || !path.is_file() || !is_json_file_name(&path) {
        bail!("Result file must be a JSON file directly within the results root");
    }
    Ok(path)
}

fn mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// The embedded-asset name for a request path, or `None` for anything that is not a plain
/// relative path of non-empty segments. The path is percent-decoded first, so `%2e%2e` and `%2f`
/// are judged as what they decode to, and backslashes are refused for the sake of Windows
/// clients. Lookup in the embedded table is by exact name, so no request can reach a file
/// outside it either way.
fn asset_name(path: &str) -> Option<String> {
    let decoded = percent_decode(path, false)?;
    let relative = decoded.strip_prefix('/')?;
    if relative.is_empty() {
        return Some("index.html".to_owned());
    }
    if relative.contains(['\\', '\0'])
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    Some(relative.to_owned())
}

fn asset_reply(path: &str) -> Reply {
    let Some(name) = asset_name(path) else {
        return Reply::not_found();
    };
    match VIEWER_ASSETS.binary_search_by_key(&name.as_str(), |(asset, _)| *asset) {
        Ok(index) => Reply {
            status: 200,
            content_type: mime_type(Path::new(&name)),
            body: VIEWER_ASSETS[index].1.to_vec(),
            no_store: false,
        },
        Err(_) => Reply::not_found(),
    }
}

/// Which `Host` headers the viewer accepts, so that a page on another origin cannot read the
/// results through DNS rebinding (a name that first resolves to the attacker, then to this
/// address, arrives with the attacker's name in `Host`).
///
/// * Bound to a loopback address (the default): `127.0.0.1`, `localhost` and `[::1]`, plus the
///   `--host` value, each with the bound port.
/// * Bound to another address: the bound address and the `--host` value, with the bound port.
/// * Bound to a wildcard address (`0.0.0.0` or `::`): the machine's own names are unknown, so
///   Host checking is limited to the port and the kind of name. Loopback names and any IP
///   address literal are accepted (rebinding needs a DNS name, never an IP literal); any other
///   name is refused with 403 unless it was given as `--host`. Pass `--host <name>` to reach
///   the viewer by a DNS name. The caller prints a warning, as this exposes the results to the
///   network.
#[derive(Debug, Clone)]
struct HostPolicy {
    port: u16,
    names: Vec<String>,
    any_ip_literal: bool,
}

impl HostPolicy {
    fn new(bound: SocketAddr, configured_host: &str) -> Self {
        let mut names = Vec::new();
        let configured = configured_host
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase();
        let ip = bound.ip();
        if ip.is_loopback() || ip.is_unspecified() {
            names.extend(["localhost", "127.0.0.1", "::1"].map(str::to_owned));
        } else {
            names.push(ip.to_string());
        }
        if !configured.is_empty()
            && configured
                .parse::<IpAddr>()
                .map_or(true, |parsed| !parsed.is_unspecified())
        {
            names.push(configured);
        }
        Self {
            port: bound.port(),
            names,
            any_ip_literal: ip.is_unspecified(),
        }
    }

    /// Splits a `Host` header into a lower-case host (without IPv6 brackets) and a port
    /// (80 when absent).
    fn split(value: &str) -> Option<(String, u16)> {
        let value = value.trim().to_ascii_lowercase();
        let (host, port) = if let Some(rest) = value.strip_prefix('[') {
            let (host, after) = rest.split_once(']')?;
            let port = match after {
                "" => 80,
                after => after.strip_prefix(':')?.parse().ok()?,
            };
            (host.to_owned(), port)
        } else {
            match value.split_once(':') {
                None => (value, 80),
                Some((host, port)) => (host.to_owned(), port.parse().ok()?),
            }
        };
        (!host.is_empty()).then_some((host, port))
    }

    fn allows(&self, host_header: &str) -> bool {
        let Some((host, port)) = Self::split(host_header) else {
            return false;
        };
        port == self.port
            && (self.names.contains(&host)
                || (self.any_ip_literal && host.parse::<IpAddr>().is_ok()))
    }
}

struct Shared {
    root: PathBuf,
    default_file: Option<String>,
    hosts: HostPolicy,
    sniffed: Mutex<SniffCache>,
}

impl Shared {
    fn files(&self) -> Result<Vec<String>> {
        result_files(&self.root, &self.sniffed)
    }

    fn api_files(&self) -> Reply {
        let files = match self.files() {
            Ok(files) => files,
            Err(error) => return Reply::error(500, format!("{error:#}")),
        };
        let default = self
            .default_file
            .as_deref()
            .filter(|file| files.iter().any(|name| name == file))
            .map(str::to_owned)
            .or_else(|| files.first().cloned());
        Reply::json(200, &json!({"files": files, "default": default}))
    }

    fn api_results(&self, url: &str) -> Reply {
        let selected = match query_value(url, "file") {
            Ok(selected) => selected,
            Err(error) => return Reply::error(400, format!("{error:#}")),
        };
        let selected = selected.or_else(|| self.default_file.clone());
        let path = match selected {
            Some(file) => safe_result_path(&self.root, &file),
            None => match self.files() {
                Ok(files) => match files.first() {
                    Some(name) => safe_result_path(&self.root, name),
                    None => Err(anyhow!(
                        "No DQD result files exist in the configured results root"
                    )),
                },
                Err(error) => return Reply::error(500, format!("{error:#}")),
            },
        };
        let path = match path {
            Ok(path) => path,
            Err(error) => return Reply::error(404, format!("{error:#}")),
        };
        match dashboard_results(&path) {
            Ok(results) => Reply::json(200, &results),
            Err(error) => Reply::error(422, format!("{error:#}")),
        }
    }

    fn handle(&self, request: &Request) -> Reply {
        let mut hosts = request
            .headers()
            .iter()
            .filter(|header| header.field.equiv("Host"));
        let host_allowed = match (hosts.next(), hosts.next()) {
            (Some(host), None) => self.hosts.allows(host.value.as_str()),
            _ => false,
        };
        if !host_allowed {
            return Reply::error(
                403,
                "Host header is not an address this viewer is bound to (DNS rebinding protection)",
            );
        }
        if request.method() != &Method::Get {
            return Reply::error(405, "Only GET requests are supported");
        }
        let url = request.url();
        let path = url.split('?').next().unwrap_or("/");
        match path {
            "/api/files" => self.api_files(),
            "/api/results" => self.api_results(url),
            _ => asset_reply(path),
        }
    }
}

/// Decrements the in-flight counter when a request thread ends, however it ends.
struct InFlight(Arc<AtomicUsize>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Stops a running [`ViewerServer`] from another thread.
#[derive(Clone)]
pub struct ShutdownHandle {
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
}

impl ShutdownHandle {
    /// Makes [`ViewerServer::run`] return once its current wait ends. Requests already being
    /// handled finish on their own threads.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.server.unblock();
    }
}

/// A bound viewer: the socket is listening as soon as [`ViewerServer::bind`] returns, and
/// [`ViewerServer::run`] serves requests until shut down.
pub struct ViewerServer {
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
    address: SocketAddr,
}

impl ViewerServer {
    /// Validates `options` and binds the listening socket; the launch-browser flag is ignored.
    pub fn bind(options: &ViewerOptions) -> Result<Self> {
        let root = options.results_root.canonicalize().with_context(|| {
            format!(
                "Results root does not exist: {}",
                options.results_root.display()
            )
        })?;
        let default_file = if let Some(path) = &options.results_file {
            let path = path
                .canonicalize()
                .with_context(|| format!("Results file does not exist: {}", path.display()))?;
            if path.parent() != Some(root.as_path()) || !path.is_file() || !is_json_file_name(&path)
            {
                bail!(
                    "Selected results file must be a JSON file directly within the configured results root"
                );
            }
            if !looks_like_results(&path) {
                bail!(
                    "Selected results file is not a DQD results file (a JSON object with CheckResults and Metadata arrays): {}",
                    path.display()
                );
            }
            Some(
                path.file_name()
                    .context("Selected results file has no basename")?
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        };
        let server = Server::http((options.host.as_str(), options.port))
            .map_err(|error| anyhow!(error.to_string()))?;
        let address = server
            .server_addr()
            .to_ip()
            .context("Viewer host must resolve to an IP address")?;
        Ok(Self {
            server: Arc::new(server),
            stop: Arc::new(AtomicBool::new(false)),
            shared: Arc::new(Shared {
                root,
                default_file,
                hosts: HostPolicy::new(address, &options.host),
                sniffed: Mutex::new(SniffCache::new()),
            }),
            address,
        })
    }

    /// The socket address the viewer listens on (the real port when 0 was requested).
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// A URL for a browser on this machine. A wildcard bind is reached through loopback.
    pub fn url(&self) -> String {
        let address = match self.address.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => {
                SocketAddr::from(([127, 0, 0, 1], self.address.port()))
            }
            IpAddr::V6(ip) if ip.is_unspecified() => {
                SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], self.address.port()))
            }
            _ => self.address,
        };
        format!("http://{address}/")
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            server: Arc::clone(&self.server),
            stop: Arc::clone(&self.stop),
        }
    }

    /// Serves requests, each on its own short-lived thread, until [`ShutdownHandle::shutdown`].
    ///
    /// `tiny_http` reads each request head on its own pool thread, so a client that stalls
    /// before finishing its request line or headers never reaches this loop. A client that
    /// stops reading a large response can hold one request thread; the cap on concurrent
    /// requests keeps that from growing without bound. `tiny_http` offers no socket timeouts,
    /// so those threads end when the operating system gives up on the connection.
    pub fn run(self) -> Result<()> {
        let in_flight = Arc::new(AtomicUsize::new(0));
        loop {
            let request = match self.server.recv() {
                Ok(request) => request,
                Err(_) if self.stop.load(Ordering::SeqCst) => return Ok(()),
                Err(error) => return Err(anyhow!("Viewer listener failed: {error}")),
            };
            if self.stop.load(Ordering::SeqCst) {
                return Ok(());
            }
            if in_flight.fetch_add(1, Ordering::AcqRel) >= MAX_IN_FLIGHT_REQUESTS {
                in_flight.fetch_sub(1, Ordering::AcqRel);
                respond(
                    request,
                    Reply::error(503, "The viewer is busy; retry shortly"),
                );
                continue;
            }
            let guard = InFlight(Arc::clone(&in_flight));
            let shared = Arc::clone(&self.shared);
            let spawned = thread::Builder::new()
                .name("dqd-viewer-request".into())
                .spawn(move || {
                    let _guard = guard;
                    let handled = catch_unwind(AssertUnwindSafe(|| {
                        let reply = shared.handle(&request);
                        respond(request, reply);
                    }));
                    if handled.is_err() {
                        eprintln!("Viewer request handler panicked");
                    }
                });
            if let Err(error) = spawned {
                // The request was dropped with the closure, which answers 500.
                eprintln!("Viewer could not start a request thread: {error}");
            }
        }
    }
}

/// Host the original dashboard over HTTP and serve direct JSON files from `results_root`.
/// The default address is loopback. The HTTP API exposes no write operations.
pub fn serve(options: ViewerOptions) -> Result<()> {
    let viewer = ViewerServer::bind(&options)?;
    let url = viewer.url();
    eprintln!("Data Quality Dashboard listening at {url}");
    if !viewer.address().ip().is_loopback() {
        eprintln!(
            "Warning: the viewer listens on {} and serves result files, including the SQL of every check, \
             without authentication to anyone who can reach that address.",
            viewer.address()
        );
        if viewer.address().ip().is_unspecified() {
            eprintln!(
                "Warning: on a wildcard address only loopback names and IP addresses are accepted in the \
                 Host header; to use a DNS name, pass it as --host."
            );
        }
    }
    if options.launch_browser {
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("cmd").args(["/C", "start", "", &url]).spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(&url).spawn();
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = Command::new("xdg-open").arg(&url).spawn();
        }
    }
    viewer.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(bound: &str, host: &str) -> HostPolicy {
        HostPolicy::new(bound.parse().unwrap(), host)
    }

    #[test]
    fn loopback_policy_accepts_loopback_names_with_the_bound_port_only() {
        let policy = policy("127.0.0.1:8080", "127.0.0.1");
        for host in [
            "127.0.0.1:8080",
            "localhost:8080",
            "LOCALHOST:8080",
            "[::1]:8080",
        ] {
            assert!(policy.allows(host), "{host}");
        }
        for host in [
            "127.0.0.1:8081",
            "127.0.0.1",
            "evil.example:8080",
            "",
            "localhost:",
            "[::1",
        ] {
            assert!(!policy.allows(host), "{host}");
        }
    }

    #[test]
    fn explicit_host_is_accepted_and_other_names_are_not() {
        let policy = policy("192.0.2.7:9000", "viewer.internal");
        assert!(policy.allows("192.0.2.7:9000"));
        assert!(policy.allows("viewer.internal:9000"));
        assert!(!policy.allows("localhost:9000"));
        assert!(!policy.allows("192.0.2.8:9000"));
    }

    #[test]
    fn wildcard_policy_accepts_ip_literals_but_not_other_names() {
        let policy = policy("0.0.0.0:9000", "0.0.0.0");
        assert!(policy.allows("192.0.2.7:9000"));
        assert!(policy.allows("[2001:db8::1]:9000"));
        assert!(policy.allows("localhost:9000"));
        assert!(!policy.allows("attacker.example:9000"));
        assert!(!policy.allows("192.0.2.7:9001"));
        assert!(!policy.names.iter().any(|name| name == "0.0.0.0"));
    }

    #[test]
    fn asset_names_reject_anything_but_plain_relative_paths() {
        assert_eq!(asset_name("/").as_deref(), Some("index.html"));
        assert_eq!(asset_name("/js/bridge.js").as_deref(), Some("js/bridge.js"));
        assert_eq!(
            asset_name("/vendor/googlefonts/Muli-400%2C400i.css").as_deref(),
            Some("vendor/googlefonts/Muli-400,400i.css")
        );
        for path in [
            "/../Cargo.toml",
            "/%2e%2e/Cargo.toml",
            "/%2E%2E%2FCargo.toml",
            "/js/..%5cCargo.toml",
            "/js\\bridge.js",
            "//index.html",
            "/js//bridge.js",
            "/./index.html",
            "/%00",
            "/%zz",
            "/%ff",
            "no-slash",
        ] {
            assert_eq!(asset_name(path), None, "{path}");
        }
    }

    #[test]
    fn query_values_are_decoded_and_malformed_ones_are_errors() {
        assert_eq!(
            query_value("/api/results?file=a%20b+c.json", "file")
                .unwrap()
                .as_deref(),
            Some("a b c.json")
        );
        assert_eq!(query_value("/api/results", "file").unwrap(), None);
        assert_eq!(query_value("/api/results?other=1", "file").unwrap(), None);
        assert!(query_value("/api/results?file=%zz", "file").is_err());
    }

    #[test]
    fn embedded_assets_are_sorted_and_include_the_entry_page() {
        assert!(VIEWER_ASSETS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(VIEWER_ASSETS.iter().any(|(name, _)| *name == "index.html"));
    }
}
