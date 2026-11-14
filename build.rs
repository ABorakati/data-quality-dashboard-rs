fn main() {
    // DuckDB comes as DuckDB's prebuilt duckdb.dll (libduckdb-sys `download-lib`). Delay-load it, so
    // Windows loads it on the first DuckDB connection rather than at start-up: SQLite and
    // PostgreSQL runs then work without the DLL. `dqd-db` checks the DLL is findable before use.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/DELAYLOAD:duckdb.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
}
