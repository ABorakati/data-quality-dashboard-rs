use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::results::dashboard_results;

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

fn send(request: Request, status: u16, content_type: &str, body: Vec<u8>) -> Result<()> {
    let header = Header::from_bytes("Content-Type", content_type).expect("static header is valid");
    request.respond(
        Response::from_data(body)
            .with_status_code(StatusCode(status))
            .with_header(header),
    )?;
    Ok(())
}

fn send_json(request: Request, status: u16, value: Value) -> Result<()> {
    send(
        request,
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(&value)?,
    )
}

fn decode_query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != key {
            continue;
        }
        let mut decoded = Vec::with_capacity(value.len());
        let bytes = value.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'+' => {
                    decoded.push(b' ');
                    index += 1;
                }
                b'%' if index + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                    decoded.push(u8::from_str_radix(hex, 16).ok()?);
                    index += 3;
                }
                byte => {
                    decoded.push(byte);
                    index += 1;
                }
            }
        }
        return String::from_utf8(decoded).ok();
    }
    None
}

fn result_files(root: &Path) -> Result<Vec<String>> {
    let root = root
        .canonicalize()
        .with_context(|| format!("Results root does not exist: {}", root.display()))?;
    let mut files = Vec::new();
    for entry in
        fs::read_dir(&root).with_context(|| format!("Reading results root {}", root.display()))?
    {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            && path.is_file()
            && path.canonicalize()?.parent() == Some(root.as_path())
        {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                files.push(name.to_owned());
            }
        }
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
    if path.parent() != Some(root.as_path())
        || !path.is_file()
        || !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        bail!("Result file must be a JSON file directly within the results root");
    }
    Ok(path)
}

fn read_api_file(
    request: Request,
    root: &Path,
    url: &str,
    default_file: Option<&str>,
) -> Result<()> {
    let selected = decode_query_value(url, "file");
    let selected = selected.as_deref().or(default_file);
    let path = match selected {
        Some(file) => safe_result_path(root, file),
        None => result_files(root)?
            .first()
            .map(|name| safe_result_path(root, name))
            .unwrap_or_else(|| bail!("No JSON result files exist in the configured results root")),
    };
    let path = match path {
        Ok(path) => path,
        Err(error) => return send_json(request, 404, json!({"error":format!("{error:#}")})),
    };
    match dashboard_results(&path) {
        Ok(results) => send_json(request, 200, results),
        Err(error) => send_json(request, 422, json!({"error":format!("{error:#}")})),
    }
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

fn send_asset(request: Request, path: &str) -> Result<()> {
    let relative = path.strip_prefix('/').unwrap_or(path);
    if Path::new(relative)
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return send(
            request,
            404,
            "text/plain; charset=utf-8",
            b"Not found".to_vec(),
        );
    }
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/viewer");
    let canonical_base = base.canonicalize()?;
    let candidate = base.join(relative);
    let asset = match candidate.canonicalize() {
        Ok(asset) if asset.starts_with(&canonical_base) && asset.is_file() => asset,
        _ => {
            return send(
                request,
                404,
                "text/plain; charset=utf-8",
                b"Not found".to_vec(),
            );
        }
    };
    let content_type = mime_type(&asset);
    send(request, 200, content_type, fs::read(asset)?)
}

fn route(request: Request, root: &Path, default_file: Option<&str>) -> Result<()> {
    if request.method() != &Method::Get {
        return send_json(
            request,
            405,
            json!({"error":"Only GET requests are supported"}),
        );
    }
    let url = request.url().to_owned();
    let path = url.split('?').next().unwrap_or("/");
    match path {
        "/api/files" => {
            let files = match result_files(root) {
                Ok(files) => files,
                Err(error) => {
                    return send_json(request, 500, json!({"error":format!("{error:#}")}));
                }
            };
            let default = default_file
                .filter(|file| files.iter().any(|name| name == file))
                .map(str::to_owned)
                .or_else(|| files.first().cloned());
            send_json(request, 200, json!({"files":files,"default":default}))
        }
        "/api/results" => read_api_file(request, root, &url, default_file),
        "/" => send_asset(request, "/index.html"),
        _ => send_asset(request, path),
    }
}

/// Host the original dashboard over HTTP and serve direct JSON files from `results_root`.
/// The default address is loopback. The HTTP API exposes no write operations.
pub fn serve(options: ViewerOptions) -> Result<()> {
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
        if path.parent() != Some(root.as_path())
            || !path.is_file()
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            bail!(
                "Selected results file must be a JSON file directly within the configured results root"
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
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let address = server
        .server_addr()
        .to_ip()
        .context("Viewer host must resolve to an IP address")?;
    let url = format!("http://{address}/");
    eprintln!("Data Quality Dashboard listening at {url}");
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
    for request in server.incoming_requests() {
        if let Err(error) = route(request, &root, default_file.as_deref()) {
            eprintln!("Viewer request failed: {error:#}");
        }
    }
    Ok(())
}
