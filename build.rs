fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Everything here concerns DuckDB's prebuilt shared library (libduckdb-sys `download-lib`), so
    // a build without the `duckdb` feature links nothing extra.
    if std::env::var_os("CARGO_FEATURE_DUCKDB").is_none() {
        return;
    }

    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        // Windows: delay-load duckdb.dll, so Windows loads it on the first DuckDB connection rather
        // than at start-up: SQLite and PostgreSQL runs then work without the DLL. `dqd-db` checks the
        // DLL is findable before use.
        Ok("windows") if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") => {
            println!("cargo:rustc-link-arg=/DELAYLOAD:duckdb.dll");
            println!("cargo:rustc-link-lib=delayimp");
        }
        // Linux and other ELF targets: search the executable's own folder for libduckdb.so, so a
        // release archive that ships the library beside `dqd` runs without LD_LIBRARY_PATH.
        Ok("linux" | "freebsd" | "netbsd" | "openbsd" | "android") => {
            println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
        }
        // macOS: the same, for libduckdb.dylib.
        Ok("macos") => {
            println!("cargo:rustc-link-arg-bins=-Wl,-rpath,@loader_path");
        }
        _ => {}
    }
}
