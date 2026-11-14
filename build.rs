use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Collects every file below `dir` as a `/`-separated path relative to `base`.
fn collect_assets(base: &Path, dir: &Path, found: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("reading {dir:?}: {error}"));
    for entry in entries {
        let path = entry
            .expect("reading a viewer asset directory entry")
            .path();
        let hidden = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            collect_assets(base, &path, found);
        } else {
            let relative = path.strip_prefix(base).expect("asset below its base");
            let name: Vec<_> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect();
            found.push((name.join("/"), path));
        }
    }
}

/// Writes `$OUT_DIR/viewer_assets.rs`: the browser assets of `resources/viewer` as
/// `include_bytes!` entries sorted by name, so the binary serves them without the source tree.
fn embed_viewer_assets() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let base = manifest.join("resources/viewer");
    println!("cargo:rerun-if-changed=resources/viewer");
    let mut assets = Vec::new();
    collect_assets(&base, &base, &mut assets);
    assets.sort();
    let mut source = String::from("pub static VIEWER_ASSETS: &[(&str, &[u8])] = &[\n");
    for (name, path) in &assets {
        source.push_str(&format!(
            "    ({name:?}, include_bytes!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    source.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("viewer_assets.rs");
    fs::write(&out, source).unwrap_or_else(|error| panic!("writing {out:?}: {error}"));
}

fn main() {
    embed_viewer_assets();
    // DuckDB comes as DuckDB's prebuilt duckdb.dll (libduckdb-sys `download-lib`). Delay-load it, so
    // Windows loads it on the first DuckDB connection rather than at start-up: SQLite and
    // PostgreSQL runs then work without the DLL. `dqd-db` checks the DLL is findable before use.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/DELAYLOAD:duckdb.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
}
