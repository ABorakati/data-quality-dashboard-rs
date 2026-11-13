use anyhow::{Context, Result, bail};
use clap::{ArgAction, Args, Parser, Subcommand};
use data_quality_dashboard::{
    Options, database, results, runner,
    sql::NativeSql,
    viewer::{self, ViewerOptions},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Parser)]
#[command(
    name = "dqd",
    version,
    about = "OHDSI DataQualityDashboard 2.9.0 Rust implementation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Execute checks or export their SQL when sqlOnly is enabled.
    Run(RunArgs),
    /// Print the unfiltered upstream check catalog as JSON.
    ListChecks(RunArgs),
    /// List the pinned SqlRender dialect names and descriptions.
    Dialects,
    /// Convert result JSON metadata and result field names to camel or snake case.
    ConvertCase(ConvertCaseArgs),
    /// Recompute threshold status fields in a result JSON file.
    Reevaluate(ReevaluateArgs),
    /// Export result JSON CheckResults to CSV.
    ExportCsv(CsvArgs),
    /// Write result JSON CheckResults to a database table.
    ImportTable(ImportTableArgs),
    /// Read a results database table and write dashboard JSON.
    TableToJson(TableToJsonArgs),
    /// Serve the result viewer and its browser assets over HTTP.
    Viewer(ViewerArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Read camelCase R-compatible options from a JSON object before applying flags.
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(flatten)]
    options: OptionArgs,
}

#[derive(Debug, Args, Default)]
struct OptionArgs {
    #[arg(long)]
    cdm_database_schema: Option<String>,
    #[arg(long)]
    results_database_schema: Option<String>,
    #[arg(long)]
    vocab_database_schema: Option<String>,
    #[arg(long)]
    cdm_source_name: Option<String>,
    #[arg(long)]
    dbms: Option<String>,
    #[arg(long)]
    server: Option<String>,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    user: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    sqlite_path: Option<PathBuf>,
    #[arg(long)]
    num_threads: Option<f64>,
    #[arg(long)]
    sql_only: Option<bool>,
    #[arg(long)]
    sql_only_union_count: Option<f64>,
    #[arg(long)]
    sql_only_incremental_insert: Option<bool>,
    #[arg(long)]
    output_folder: Option<PathBuf>,
    #[arg(long)]
    output_file: Option<String>,
    #[arg(long)]
    verbose_mode: Option<bool>,
    #[arg(long)]
    write_to_table: Option<bool>,
    #[arg(long)]
    write_table_name: Option<String>,
    #[arg(long)]
    write_to_csv: Option<bool>,
    #[arg(long)]
    csv_file: Option<String>,
    #[arg(long, value_delimiter = ',')]
    check_levels: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    check_names: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    check_severity: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    cohort_definition_id: Option<Vec<Value>>,
    #[arg(long)]
    cohort_database_schema: Option<String>,
    #[arg(long)]
    cohort_table_name: Option<String>,
    #[arg(long, value_delimiter = ',')]
    tables_to_exclude: Option<Vec<String>>,
    #[arg(long)]
    cdm_version: Option<String>,
    #[arg(long)]
    table_check_threshold_loc: Option<String>,
    #[arg(long)]
    field_check_threshold_loc: Option<String>,
    #[arg(long)]
    concept_check_threshold_loc: Option<String>,
}

impl OptionArgs {
    fn apply(self, options: &mut Options) {
        macro_rules! set { ($($field:ident),* $(,)?) => { $(if let Some(value) = self.$field { options.$field = value; })* }; }
        set!(
            cdm_database_schema,
            results_database_schema,
            vocab_database_schema,
            cdm_source_name,
            dbms,
            num_threads,
            sql_only,
            sql_only_union_count,
            sql_only_incremental_insert,
            output_folder,
            output_file,
            verbose_mode,
            write_to_table,
            write_table_name,
            write_to_csv,
            csv_file,
            check_levels,
            check_names,
            check_severity,
            cohort_definition_id,
            cohort_database_schema,
            cohort_table_name,
            tables_to_exclude,
            cdm_version,
            table_check_threshold_loc,
            field_check_threshold_loc,
            concept_check_threshold_loc,
        );
        if let Some(value) = self.sqlite_path {
            options.connection.sqlite_path = Some(value);
        }
        if let Some(value) = self.server {
            options.connection.server = Some(value);
        }
        if let Some(value) = self.port {
            options.connection.port = Some(value);
        }
        if let Some(value) = self.user {
            options.connection.user = Some(value);
        }
        if let Some(value) = self.password {
            options.connection.password = Some(value);
        }
    }
}

#[derive(Debug, Args)]
struct ConvertCaseArgs {
    input: PathBuf,
    #[arg(long, value_parser = ["camel", "snake"])]
    target_case: String,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReevaluateArgs {
    input: PathBuf,
    #[arg(long)]
    output_folder: PathBuf,
    #[arg(long)]
    output_file: String,
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct CsvArgs {
    input: PathBuf,
    output: PathBuf,
    #[arg(long, value_delimiter = ',')]
    columns: Option<Vec<String>>,
    #[arg(long, default_value = ",")]
    delimiter: String,
}

#[derive(Debug, Args)]
struct ImportTableArgs {
    input: PathBuf,
    #[arg(long)]
    config: PathBuf,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    single_table: bool,
}

#[derive(Debug, Args)]
struct TableToJsonArgs {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    output_folder: PathBuf,
    #[arg(long)]
    output_file: String,
}

#[derive(Debug, Args)]
struct ViewerArgs {
    #[arg(long, default_value = ".")]
    results_root: PathBuf,
    #[arg(long)]
    results_file: Option<PathBuf>,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    launch_browser: bool,
}

fn read_options(path: Option<&Path>) -> Result<Options> {
    match path {
        Some(path) => serde_json::from_slice(
            &fs::read(path).with_context(|| format!("reading config {}", path.display()))?,
        )
        .with_context(|| format!("parsing camelCase options from {}", path.display())),
        None => Ok(Options::default()),
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Run(args) => {
            let mut options = read_options(args.config.as_deref())?;
            args.options.apply(&mut options);
            let output = runner::execute_dq_checks(&options)?;
            if !output.is_null() {
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        }
        Command::ListChecks(args) => {
            let mut options = read_options(args.config.as_deref())?;
            args.options.apply(&mut options);
            let output = runner::list_dq_checks(&options)?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Command::Dialects => {
            let mut reader = csv::Reader::from_reader(
                include_bytes!("../resources/sqlrender/supportedDialects.csv").as_slice(),
            );
            let dialects: Vec<Value> = reader
                .records()
                .filter_map(|record| match record {
                    Ok(record) if matches!(record.get(0), Some("sqlite" | "postgresql")) => {
                        Some(Ok(json!({"dialect": record.get(0).unwrap_or(""), "description": record.get(1).unwrap_or("")})))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<std::result::Result<_, csv::Error>>()?;
            println!("{}", serde_json::to_string_pretty(&dialects)?);
        }
        Command::ConvertCase(args) => {
            let (folder, filename) = match args.output.as_deref() {
                Some(output) => (
                    output.parent().unwrap_or(Path::new(".")),
                    output
                        .file_name()
                        .and_then(|s| s.to_str())
                        .context("output file name must be UTF-8")?,
                ),
                None => (
                    args.input.parent().unwrap_or(Path::new(".")),
                    args.input
                        .file_name()
                        .and_then(|s| s.to_str())
                        .context("input file name must be UTF-8")?,
                ),
            };
            let value = results::convert_json_results_file_case(
                &args.input,
                true,
                Some(folder),
                Some(filename),
                &args.target_case,
            )?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Reevaluate(args) => {
            let options = read_options(args.config.as_deref())?;
            let value = results::re_evaluate_thresholds(
                &args.input,
                &args.output_folder,
                &args.output_file,
                &options,
            )?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::ExportCsv(args) => {
            let delimiter = args.delimiter.as_bytes();
            if delimiter.len() != 1 {
                bail!("--delimiter must be one byte");
            }
            results::write_json_results_to_csv(
                &args.input,
                &args.output,
                args.columns.as_deref(),
                delimiter[0],
            )?;
        }
        Command::ImportTable(args) => {
            let options = read_options(Some(&args.config))?;
            let mut connection = database::connect(&options)?;
            let mut engine = NativeSql::new();
            results::write_json_results_to_table_for_cohorts(
                &mut *connection,
                &mut engine,
                &args.input,
                &options.dbms,
                &options.results_database_schema,
                &options.write_table_name,
                &options.cohort_definition_id,
                args.single_table,
            )?;
        }
        Command::TableToJson(args) => {
            let options = read_options(Some(&args.config))?;
            let mut connection = database::connect(&options)?;
            let mut engine = NativeSql::new();
            results::write_db_results_to_json(
                &mut *connection,
                &mut engine,
                &options.dbms,
                &options.results_database_schema,
                &options.cdm_database_schema,
                &options.write_table_name,
                &args.output_folder,
                &args.output_file,
            )?;
        }
        Command::Viewer(args) => viewer::serve(ViewerOptions {
            results_root: args.results_root,
            results_file: args.results_file,
            host: args.host,
            port: args.port,
            launch_browser: args.launch_browser,
        })?,
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
