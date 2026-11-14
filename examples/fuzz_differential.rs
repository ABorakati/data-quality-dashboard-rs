//! Differential data fuzzer: seeded mutations of an OMOP CDM SQLite database, run through both
//! upstream R DataQualityDashboard and this port, with `CheckResults` compared field by field.
//!
//! ```text
//! cargo run --release --example fuzz_differential -- generate --base DB --out DIR --seed N --count K
//! Rscript --vanilla fuzz/run_r_batch.R DIR          # upstream R, one process for the batch
//! cargo run --release --example fuzz_differential -- rust DIR
//! cargo run --release --example fuzz_differential -- compare DIR --reproducers fuzz/reproducers
//! ```
//!
//! `scripts/fuzz-differential.ps1` runs all four steps. Every case directory holds `cdm.sqlite`
//! (the mutated copy), `options.json` (executeDqChecks arguments), and `mutations.json` (what was
//! changed and the exact SQL). The same seed always yields the same cases.
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use data_quality_dashboard::{database, model::Options, runner};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create COUNT mutated copies of BASE under OUT.
    Generate {
        #[arg(long)]
        base: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value_t = 10)]
        count: u64,
        /// Comma-separated check names to run (all checks when omitted).
        #[arg(long, value_delimiter = ',')]
        checks: Vec<String>,
        #[arg(long, default_value = "5.3")]
        cdm_version: String,
        /// Mutations per case are drawn from 1..=MAX.
        #[arg(long, default_value_t = 3)]
        max_mutations: u64,
    },
    /// Run the Rust port on every case under DIR (output in CASE/rust).
    Rust { dir: PathBuf },
    /// Compare CASE/r with CASE/rust for every case; copy mismatches to REPRODUCERS.
    Compare {
        dir: PathBuf,
        #[arg(long)]
        reproducers: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Generate {
            base,
            out,
            seed,
            count,
            checks,
            cdm_version,
            max_mutations,
        } => generate(
            &base,
            &out,
            seed,
            count,
            &checks,
            &cdm_version,
            max_mutations,
        ),
        Command::Rust { dir } => run_rust(&dir),
        Command::Compare { dir, reproducers } => compare(&dir, reproducers.as_deref()),
    }
}

/// SplitMix64: small, deterministic, and good enough to pick mutations.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

/// A `.duckdb` file is fuzzed on DuckDB, anything else on SQLite.
fn is_duckdb(db: &Path) -> bool {
    db.extension().is_some_and(|ext| ext == "duckdb")
}

fn db_options(db: &Path) -> Options {
    let mut options = Options::default();
    if is_duckdb(db) {
        options.dbms = "duckdb".into();
        // DatabaseConnector's convention: the DuckDB file is the `server`.
        options.connection.server = Some(db.to_string_lossy().into_owned());
    } else {
        options.dbms = "sqlite".into();
        options.connection.sqlite_path = Some(db.to_path_buf());
    }
    options
}

/// The case database (`cdm.sqlite` or `cdm.duckdb`).
fn case_db(case: &Path) -> PathBuf {
    let duckdb = case.join("cdm.duckdb");
    if duckdb.exists() {
        duckdb
    } else {
        case.join("cdm.sqlite")
    }
}

/// CDM tables the checks read; vocabulary and metadata tables are left alone, except where a
/// mutation names them explicitly.
const CLINICAL_TABLES: &[&str] = &[
    "person",
    "observation_period",
    "visit_occurrence",
    "visit_detail",
    "condition_occurrence",
    "drug_exposure",
    "procedure_occurrence",
    "device_exposure",
    "measurement",
    "observation",
    "death",
    "note",
    "specimen",
    "location",
    "care_site",
    "provider",
    "payer_plan_period",
    "drug_era",
    "dose_era",
    "condition_era",
];

struct Column {
    name: String,
    not_null: bool,
}

struct Schema {
    tables: BTreeMap<String, Vec<Column>>,
    row_counts: BTreeMap<String, u64>,
    /// DuckDB stores CDM dates as DATE (SQLite here: epoch seconds), so date mutations differ.
    duckdb: bool,
}

fn schema(connection: &mut dyn database::Connection, duckdb: bool) -> Result<Schema> {
    let mut tables = BTreeMap::new();
    let mut row_counts = BTreeMap::new();
    for table in CLINICAL_TABLES {
        // SQLite returns no rows for a missing table; DuckDB raises an error.
        let columns: Vec<Column> = connection
            .query(&format!("PRAGMA table_info(\"{table}\")"))
            .unwrap_or_default()
            .into_iter()
            .map(|row| Column {
                name: row["name"].as_str().unwrap_or_default().to_owned(),
                not_null: row["notnull"].as_i64() == Some(1) || row["notnull"] == true,
            })
            .collect();
        if columns.is_empty() {
            continue;
        }
        let count = connection.query(&format!("SELECT COUNT(*) AS n FROM \"{table}\""))?[0]["n"]
            .as_u64()
            .unwrap_or(0);
        row_counts.insert(table.to_string(), count);
        tables.insert(table.to_string(), columns);
    }
    Ok(Schema {
        tables,
        row_counts,
        duckdb,
    })
}

/// One applied mutation: a label and the SQL statements that made it.
struct Mutation {
    kind: &'static str,
    target: String,
    sql: Vec<String>,
}

fn columns_matching<'a>(
    schema: &'a Schema,
    table: &str,
    test: impl Fn(&str) -> bool,
) -> Vec<&'a Column> {
    schema.tables[table]
        .iter()
        .filter(|c| test(&c.name))
        .collect()
}

/// SQL that recreates `table` without NOT NULL constraints (CREATE TABLE AS keeps rows and
/// column order, and keeps each column's affinity), so NULLs can be injected.
fn relax(table: &str) -> Vec<String> {
    vec![
        format!("CREATE TABLE \"{table}__fuzz\" AS SELECT * FROM \"{table}\""),
        format!("DROP TABLE \"{table}\""),
        format!("ALTER TABLE \"{table}__fuzz\" RENAME TO \"{table}\""),
    ]
}

/// `WHERE rowid IN (...)` over a deterministic slice of up to `k` rows.
fn some_rows(rng: &mut Rng, schema: &Schema, table: &str, k: u64) -> String {
    let rows = schema.row_counts[table].max(1);
    let offset = rng.below(rows);
    format!("rowid IN (SELECT rowid FROM \"{table}\" ORDER BY rowid LIMIT {k} OFFSET {offset})")
}

const MUTATIONS: &[&str] = &[
    "null_in_required",
    "date_as_text",
    "date_as_iso_integer",
    "date_with_fraction",
    "future_date",
    "ancient_date",
    "impossible_text_date",
    "unknown_concept",
    "zero_concept",
    "negative_concept",
    "non_standard_concept",
    "empty_table",
    "drop_table",
    "rename_column",
    "huge_number",
    "tiny_number",
    "text_in_numeric",
    "duplicate_row",
    "orphan_person",
    "extra_cdm_source_row",
    "unicode_source_value",
    "end_before_start",
];

fn mutate(rng: &mut Rng, schema: &Schema) -> Option<Mutation> {
    let kind = *rng.pick(MUTATIONS);
    // Row-level mutations change nothing on an empty table; table-level ones may hit any table.
    let tables: Vec<&String> = schema
        .tables
        .keys()
        .filter(|table| {
            matches!(kind, "empty_table" | "drop_table" | "rename_column")
                || schema.row_counts[*table] > 0
        })
        .collect();
    if tables.is_empty() {
        return None;
    }
    let table = rng.pick(&tables).as_str().to_owned();
    let k = 1 + rng.below(25);
    let pick_column = |rng: &mut Rng, test: &dyn Fn(&str) -> bool| -> Option<String> {
        let candidates = columns_matching(schema, &table, test);
        (!candidates.is_empty()).then(|| rng.pick(&candidates).name.clone())
    };
    let is_date = |c: &str| c.ends_with("_date") || c.ends_with("_datetime");
    let is_concept = |c: &str| c.ends_with("_concept_id");
    let is_number = |c: &str| {
        c.starts_with("value_as_number")
            || c.starts_with("range_")
            || matches!(c, "quantity" | "days_supply" | "refills" | "year_of_birth")
    };
    let (target, sql) = match kind {
        "null_in_required" => {
            let candidates: Vec<&Column> = schema.tables[&table].iter().filter(|c| c.not_null).collect();
            let column = if candidates.is_empty() {
                rng.pick(&schema.tables[&table]).name.clone()
            } else {
                rng.pick(&candidates).name.clone()
            };
            let mut sql = relax(&table);
            sql.push(format!("UPDATE \"{table}\" SET \"{column}\" = NULL WHERE {}", some_rows(rng, schema, &table, k)));
            (format!("{table}.{column}"), sql)
        }
        "date_as_text" | "date_as_iso_integer" | "date_with_fraction" | "future_date" | "ancient_date" | "impossible_text_date" => {
            let column = pick_column(rng, &is_date)?;
            let value = match kind {
                // DuckDB columns are typed DATE/TIMESTAMP: only in-range values of that type fit
                // (text, integers and impossible dates are rejected, and the mutation is skipped).
                "future_date" if schema.duckdb => "DATE '2099-12-31'".to_owned(),
                "ancient_date" if schema.duckdb => "DATE '1779-01-01'".to_owned(),
                "date_with_fraction" if schema.duckdb => format!("\"{column}\" + INTERVAL 12 HOUR"),
                // GiBleed stores dates as epoch seconds; R's loader would have written these forms.
                "date_as_text" => format!("date(\"{column}\", 'unixepoch')"),
                "date_as_iso_integer" => format!("CAST(strftime('%Y%m%d', \"{column}\", 'unixepoch') AS INTEGER)"),
                "date_with_fraction" => format!("\"{column}\" + 0.5"),
                "future_date" => "4102358400".to_owned(),
                "ancient_date" => "-6000000000".to_owned(),
                _ => "'2020-02-30'".to_owned(),
            };
            (format!("{table}.{column}"), vec![format!("UPDATE \"{table}\" SET \"{column}\" = {value} WHERE {}", some_rows(rng, schema, &table, k))])
        }
        "unknown_concept" | "zero_concept" | "negative_concept" | "non_standard_concept" => {
            let column = pick_column(rng, &is_concept)?;
            let value = match kind {
                "unknown_concept" => "999999999".to_owned(),
                "zero_concept" => "0".to_owned(),
                "negative_concept" => "-7".to_owned(),
                _ => format!(
                    "(SELECT concept_id FROM concept WHERE standard_concept IS NULL ORDER BY concept_id LIMIT 1 OFFSET {})",
                    rng.below(20)
                ),
            };
            (format!("{table}.{column}"), vec![format!("UPDATE \"{table}\" SET \"{column}\" = {value} WHERE {}", some_rows(rng, schema, &table, k))])
        }
        "empty_table" => (table.clone(), vec![format!("DELETE FROM \"{table}\"")]),
        "drop_table" => (table.clone(), vec![format!("DROP TABLE \"{table}\"")]),
        "rename_column" => {
            let column = rng.pick(&schema.tables[&table]).name.clone();
            (format!("{table}.{column}"), vec![format!("ALTER TABLE \"{table}\" RENAME COLUMN \"{column}\" TO \"{column}_renamed\"")])
        }
        "huge_number" | "tiny_number" | "text_in_numeric" => {
            let column = pick_column(rng, &is_number).or_else(|| pick_column(rng, &|c: &str| c.ends_with("_id")))?;
            let value = match kind {
                "huge_number" => rng.pick(&["1e308", "-1e308", "9223372036854775807", "-9223372036854775808"]).to_string(),
                "tiny_number" => rng.pick(&["4.9e-324", "-1e-300", "0.1"]).to_string(),
                _ => "'abc'".to_owned(),
            };
            (format!("{table}.{column}"), vec![format!("UPDATE \"{table}\" SET \"{column}\" = {value} WHERE {}", some_rows(rng, schema, &table, k))])
        }
        "duplicate_row" => {
            let mut sql = relax(&table);
            sql.push(format!("INSERT INTO \"{table}\" SELECT * FROM \"{table}\" WHERE {}", some_rows(rng, schema, &table, k)));
            (table.clone(), sql)
        }
        "orphan_person" => {
            if !schema.tables[&table].iter().any(|c| c.name == "person_id") || table == "person" {
                return None;
            }
            (format!("{table}.person_id"), vec![format!("UPDATE \"{table}\" SET person_id = 987654321 WHERE {}", some_rows(rng, schema, &table, k))])
        }
        "extra_cdm_source_row" => (
            "cdm_source".into(),
            vec!["INSERT INTO cdm_source (cdm_source_name, cdm_source_abbreviation) VALUES ('fuzz', 'FZ')".into()],
        ),
        "unicode_source_value" => {
            let column = pick_column(rng, &|c: &str| c.ends_with("_source_value"))?;
            (format!("{table}.{column}"), vec![format!("UPDATE \"{table}\" SET \"{column}\" = 'é漢字 ''q'' \"d\" ' || {}(10) || '%s' WHERE {}", if schema.duckdb { "chr" } else { "char" }, some_rows(rng, schema, &table, k))])
        }
        "end_before_start" => {
            let starts = columns_matching(schema, &table, |c| c.ends_with("_start_date"));
            let start = starts.first()?.name.clone();
            let end = start.replace("_start_date", "_end_date");
            if !schema.tables[&table].iter().any(|c| c.name == end) {
                return None;
            }
            (format!("{table}.{end}"), vec![format!("UPDATE \"{table}\" SET \"{end}\" = \"{start}\" - {} * {} WHERE {}", if schema.duckdb { "1" } else { "86400" }, 1 + rng.below(400), some_rows(rng, schema, &table, k))])
        }
        _ => unreachable!(),
    };
    Some(Mutation { kind, target, sql })
}

fn generate(
    base: &Path,
    out: &Path,
    seed: u64,
    count: u64,
    checks: &[String],
    cdm_version: &str,
    max_mutations: u64,
) -> Result<()> {
    fs::create_dir_all(out)?;
    for index in 0..count {
        let name = format!("case-{seed}-{index:03}");
        let folder = out.join(&name);
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }
        fs::create_dir_all(&folder)?;
        let db = folder.join(if is_duckdb(base) {
            "cdm.duckdb"
        } else {
            "cdm.sqlite"
        });
        fs::copy(base, &db).with_context(|| format!("copying {}", base.display()))?;
        let mut rng = Rng(seed.wrapping_mul(0x1000_0000_01B3) ^ index);
        let mut connection = database::connect(&db_options(&db))?;
        let wanted = 1 + rng.below(max_mutations.max(1));
        let mut applied = Vec::new();
        let mut attempts = 0;
        while (applied.len() as u64) < wanted && attempts < 50 {
            attempts += 1;
            let schema = schema(connection.as_mut(), is_duckdb(&db))?;
            let Some(mutation) = mutate(&mut rng, &schema) else {
                continue;
            };
            let mut ok = true;
            for statement in &mutation.sql {
                if let Err(error) = connection.execute(statement) {
                    // An inapplicable mutation (e.g. a column already renamed) is skipped whole.
                    eprintln!(
                        "{name}: skipped {} on {}: {error:#}",
                        mutation.kind, mutation.target
                    );
                    ok = false;
                    break;
                }
            }
            if ok {
                applied.push(
                    json!({"kind": mutation.kind, "target": mutation.target, "sql": mutation.sql}),
                );
            }
        }
        drop(connection);
        let mut options = Map::new();
        options.insert("cdmDatabaseSchema".into(), json!("main"));
        options.insert("resultsDatabaseSchema".into(), json!("main"));
        options.insert("cdmSourceName".into(), json!("Fuzz"));
        options.insert("cdmVersion".into(), json!(cdm_version));
        options.insert("numThreads".into(), json!(1));
        options.insert("outputFile".into(), json!("results.json"));
        options.insert("writeToTable".into(), json!(false));
        options.insert("writeToCsv".into(), json!(false));
        if !checks.is_empty() {
            options.insert("checkNames".into(), json!(checks));
        }
        fs::write(
            folder.join("options.json"),
            serde_json::to_vec_pretty(&options)?,
        )?;
        fs::write(
            folder.join("mutations.json"),
            serde_json::to_vec_pretty(
                &json!({"seed": seed, "index": index, "base": base, "mutations": applied}),
            )?,
        )?;
        println!(
            "{name}: {}",
            applied
                .iter()
                .map(|m| format!(
                    "{}({})",
                    m["kind"].as_str().unwrap(),
                    m["target"].as_str().unwrap()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

fn cases(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut cases: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("options.json").exists())
        .collect();
    cases.sort();
    Ok(cases)
}

fn run_rust(dir: &Path) -> Result<()> {
    for case in cases(dir)? {
        let mut options: Options = serde_json::from_slice(&fs::read(case.join("options.json"))?)?;
        let connection = db_options(&case_db(&case));
        options.dbms = connection.dbms;
        options.connection = connection.connection;
        options.output_folder = case.join("rust");
        let outcome = match runner::execute_dq_checks(&options) {
            Ok(_) => json!({"ok": true}),
            Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
        };
        fs::create_dir_all(&options.output_folder)?;
        fs::write(
            options.output_folder.join("outcome.json"),
            serde_json::to_vec_pretty(&outcome)?,
        )?;
        println!(
            "{}: rust {}",
            case.file_name().unwrap().to_string_lossy(),
            outcome
        );
    }
    Ok(())
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Error text embeds absolute report paths; keep the part that both sides share.
fn normalize_error(text: &str) -> String {
    let marker = "An error report has been created at  ";
    match text.split_once(marker) {
        Some((prefix, path)) => {
            let path = path.replace('\\', "/");
            let file = path.rsplit('/').next().unwrap_or_default().to_owned();
            format!("{prefix}{marker}<path>/{file}")
        }
        None => text.to_owned(),
    }
}

fn strip(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        for key in [
            "startTimestamp",
            "endTimestamp",
            "executionTime",
            "executionTimeSeconds",
        ] {
            object.remove(key);
        }
        if let Some(rows) = object.get_mut("CheckResults").and_then(Value::as_array_mut) {
            for row in rows.iter_mut().filter_map(Value::as_object_mut) {
                row.remove("executionTime");
                if let Some(Value::String(error)) = row.get_mut("error") {
                    *error = normalize_error(error);
                }
            }
        }
    }
}

/// Records every difference between two JSON values (numbers compared as f64).
fn diff(r: &Value, rust: &Value, path: &str, out: &mut Vec<String>) {
    match (r, rust) {
        (Value::Number(a), Value::Number(b)) if a.as_f64() == b.as_f64() => {}
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                match b.get(key) {
                    Some(other) => diff(value, other, &format!("{path}.{key}"), out),
                    None => out.push(format!("{path}.{key}: R {value} | Rust <absent>")),
                }
            }
            for key in b.keys().filter(|key| !a.contains_key(*key)) {
                out.push(format!("{path}.{key}: R <absent> | Rust {}", b[key]));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!(
                    "{path}: R {} items | Rust {} items",
                    a.len(),
                    b.len()
                ));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                let label = x
                    .get("checkId")
                    .and_then(Value::as_str)
                    .map(|id| format!("{path}[{i}]({id})"))
                    .unwrap_or_else(|| format!("{path}[{i}]"));
                diff(x, y, &label, out);
            }
        }
        _ if r == rust => {}
        _ => out.push(format!("{path}: R {r} | Rust {rust}")),
    }
}

fn compare(dir: &Path, reproducers: Option<&Path>) -> Result<()> {
    let mut summary = Vec::new();
    let mut mismatches = 0;
    for case in cases(dir)? {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        let Some(r_outcome) = read_json(&case.join("r/outcome.json")) else {
            summary.push(json!({"case": name, "status": "no R result"}));
            continue;
        };
        let rust_outcome =
            read_json(&case.join("rust/outcome.json")).context("run `rust` first")?;
        let mut differences = Vec::new();
        match (r_outcome["ok"].as_bool(), rust_outcome["ok"].as_bool()) {
            (Some(true), Some(true)) => {
                let mut r = read_json(&case.join("r/results.json")).context("R results.json")?;
                let mut rust =
                    read_json(&case.join("rust/results.json")).context("Rust results.json")?;
                strip(&mut r);
                strip(&mut rust);
                diff(&r, &rust, "$", &mut differences);
            }
            (Some(false), Some(false)) => {
                let (r, rust) = (
                    r_outcome["error"].as_str().unwrap_or_default(),
                    rust_outcome["error"].as_str().unwrap_or_default(),
                );
                if normalize_error(r) != normalize_error(rust) {
                    differences.push(format!("error: R {r:?} | Rust {rust:?}"));
                }
            }
            _ => differences.push(format!("outcome: R {r_outcome} | Rust {rust_outcome}")),
        }
        let checks = read_json(&case.join("r/results.json"))
            .and_then(|v| v["CheckResults"].as_array().map(Vec::len))
            .unwrap_or(0);
        let status = if differences.is_empty() {
            "match"
        } else {
            "MISMATCH"
        };
        println!(
            "{name}: {status} ({checks} checks, {} differences)",
            differences.len()
        );
        if !differences.is_empty() {
            mismatches += 1;
            fs::write(case.join("diff.txt"), differences.join("\n"))?;
            for line in differences.iter().take(10) {
                println!("  {line}");
            }
            if let Some(root) = reproducers {
                let target = root.join(&name);
                fs::create_dir_all(&target)?;
                let db = case_db(&case);
                fs::copy(&db, target.join(db.file_name().unwrap()))?;
                for file in ["options.json", "mutations.json", "diff.txt"] {
                    fs::copy(case.join(file), target.join(file))?;
                }
                for side in ["r", "rust"] {
                    for file in ["results.json", "outcome.json"] {
                        let source = case.join(side).join(file);
                        if source.exists() {
                            fs::copy(&source, target.join(format!("{side}-{file}")))?;
                        }
                    }
                }
            }
        }
        summary.push(json!({"case": name, "status": status, "checks": checks, "differences": differences.len(), "rOk": r_outcome["ok"], "mutations": read_json(&case.join("mutations.json")).map(|m| m["mutations"].clone())}));
    }
    fs::write(
        dir.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{} cases, {mismatches} mismatching", summary.len());
    if mismatches > 0 {
        bail!("{mismatches} mismatching cases");
    }
    Ok(())
}
