//! What a person sees when they run `dqd run` on a database: a short summary, not the full results.

use std::{fs, process::Command};

mod common;

/// A copy of the synthetic GiBleed sample database (CDM 5.3) from the parity fixtures, with a config for it
/// that does not write anything into the database.
fn sample(dir: &std::path::Path) -> std::path::PathBuf {
    let database = dir.join("gibleed.sqlite");
    fs::copy(
        common::oracle().join("gibleed/sqlite/gibleed-copy.sqlite"),
        &database,
    )
    .unwrap();
    let config = dir.join("options.json");
    let options = serde_json::json!({
        "dbms": "sqlite",
        "connection": {"sqlitePath": database},
        "cdmDatabaseSchema": "main",
        "resultsDatabaseSchema": "main",
        "cdmSourceName": "GiBleed sample",
        "cdmVersion": "5.3",
        "numThreads": 4,
        "writeToTable": false,
        "outputFolder": dir.join("out"),
        "outputFile": "results.json",
    });
    fs::write(&config, serde_json::to_vec(&options).unwrap()).unwrap();
    config
}

#[test]
fn run_prints_a_short_summary_and_print_results_prints_the_full_json() {
    let dir = tempfile::tempdir().unwrap();
    let config = sample(dir.path());
    let dqd = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_dqd"))
            .args(["run", "--config"])
            .arg(&config)
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };

    let summary = dqd(&[]);
    assert!(summary.len() < 1_000, "{} bytes of output", summary.len());
    assert!(summary.contains("2002 checks:"), "{summary}");
    assert!(summary.contains("not applicable"), "{summary}");
    assert!(summary.contains("results.json"), "{summary}");
    assert!(summary.contains("dqd viewer --results-root"), "{summary}");
    assert!(
        !summary.contains("queryText"),
        "the summary has no results JSON"
    );
    // The results file is still written in full.
    let written: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("out/results.json")).unwrap()).unwrap();
    assert_eq!(written["CheckResults"].as_array().unwrap().len(), 2002);

    let full: serde_json::Value = serde_json::from_str(&dqd(&["--print-results"])).unwrap();
    assert_eq!(full["CheckResults"].as_array().unwrap().len(), 2002);
}
