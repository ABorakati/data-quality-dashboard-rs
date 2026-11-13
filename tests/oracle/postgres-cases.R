# Real upstream DatabaseConnector/PostgreSQL execution against the private oracle cluster
# (scripts/postgres-oracle.ps1, port 54329). Mirrors database-cases.R scenario for scenario.
# Upstream DataQualityDashboard, DatabaseConnector, SqlRender and CommonDataModel run unmodified:
# this file only calls their exported functions and records the outcomes.
args <- commandArgs(TRUE)
if (length(args) != 2L) stop("Usage: Rscript postgres-cases.R UPSTREAM OUTPUT")
upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
dir.create(args[[2]], recursive = TRUE, showWarnings = FALSE)
out <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
suppressPackageStartupMessages(library(DataQualityDashboard))

# Pin checks, exactly as generate.R.
pin <- "b9d2a9b4a4736f7ea81415d974c72119b6f0bd11"
actual <- system2("git", c("-C", shQuote(upstream), "rev-parse", "HEAD"), stdout = TRUE)
if (!identical(actual, pin)) stop("Upstream HEAD differs from the parity pin: ", paste(actual, collapse = " "))
if (as.character(packageVersion("DataQualityDashboard")) != "2.9.0") stop("Wrong installed DQD version")
for (relative in list.files(file.path(upstream, "inst"), recursive = TRUE)) {
  installed <- system.file(relative, package = "DataQualityDashboard")
  if (!nzchar(installed) || unname(tools::md5sum(installed)) != unname(tools::md5sum(file.path(upstream, "inst", relative)))) stop("Installed asset differs: ", relative)
}

# Helpers copied from generate.R.
json <- function(value, path, default = FALSE) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  text <- if (default) jsonlite::toJSON(value) else jsonlite::toJSON(value, auto_unbox = TRUE, na = "null", null = "null", digits = 17, dataframe = "rows")
  writeLines(text, path, useBytes = TRUE)
}
outcome <- function(f) {
  warnings <- character()
  value <- tryCatch(withCallingHandlers(f(), warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") }), error = function(e) structure(list(message = conditionMessage(e)), class = "oracle_error"))
  if (inherits(value, "oracle_error")) list(ok = FALSE, error = value$message, warnings = warnings) else list(ok = TRUE, value = value, warnings = warnings)
}
# generate.R's base_options without the SQLite "main" schemas: every PostgreSQL case names its own schema.
base_options <- list(cdmSourceName = "Parity", writeToTable = FALSE, outputFile = "results.json", numThreads = 1)
execute <- function(options, folder, connectionDetails) {
  options$dbms <- NULL; options$connection <- NULL
  run_options <- modifyList(base_options, options)
  run_options$outputFolder <- folder
  arguments <- c(list(connectionDetails = connectionDetails), run_options)
  if (anyDuplicated(names(arguments))) stop("duplicate executeDqChecks argument names: ", paste(names(arguments)[duplicated(names(arguments))], collapse = ", "))
  do.call(DataQualityDashboard::executeDqChecks, arguments)
}
write_control <- function(path, header, rows) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  write.table(rbind(header, rows), path, sep = ",", row.names = FALSE, col.names = FALSE, quote = TRUE, na = "")
}

server <- "localhost/postgres"
port <- 54329
details <- do.call(DatabaseConnector::createConnectionDetails, list(dbms = "postgresql", server = server, port = port, user = "postgres", password = "", pathToDriver = "C:/Users/abora/tmp/dqd-jdbc-drivers"))

pg_root <- file.path(out, "postgres")
unlink(pg_root, recursive = TRUE)
dir.create(pg_root, recursive = TRUE, showWarnings = FALSE)
db <- DatabaseConnector::connect(details)

database_cases <- list()
for (scenario in c("clean", "violating", "missing-table", "missing-field", "empty-table", "unpopulated-field", "cohort", "clean-write-table")) {
  schema <- gsub("-", "_", scenario)
  folder <- file.path(pg_root, scenario); dir.create(folder, recursive = TRUE, showWarnings = FALSE)
  statements <- c(
    sprintf("DROP SCHEMA IF EXISTS %s CASCADE", schema),
    sprintf("CREATE SCHEMA %s", schema),
    sprintf("CREATE TABLE %s.cdm_source (cdm_source_name VARCHAR(255), cdm_source_abbreviation VARCHAR(25), cdm_holder VARCHAR(255), source_description TEXT, source_documentation_reference VARCHAR(255), cdm_etl_reference VARCHAR(255), source_release_date DATE, cdm_release_date DATE, cdm_version VARCHAR(10), vocabulary_version VARCHAR(20), vocabulary_concept_id INTEGER)", schema),
    sprintf("INSERT INTO %s.cdm_source VALUES ('Parity','PAR','OHDSI','Fixture','','',NULL,NULL,'5.3','',0)", schema)
  )
  if (scenario != "missing-table") {
    statements <- c(statements, sprintf("CREATE TABLE %s.person (person_id INTEGER, value VARCHAR(255))", schema))
    if (!scenario %in% c("empty-table", "unpopulated-field")) statements <- c(statements, sprintf("INSERT INTO %s.person VALUES (1, 'present')", schema))
    if (scenario == "violating") statements <- c(statements, sprintf("INSERT INTO %s.person VALUES (NULL, NULL)", schema))
    if (scenario == "unpopulated-field") statements <- c(statements, sprintf("INSERT INTO %s.person VALUES (1, NULL)", schema))
    if (scenario == "missing-field") statements <- c(statements, sprintf("ALTER TABLE %s.person RENAME COLUMN person_id TO renamed_id", schema))
  }
  if (scenario == "cohort") {
    statements <- c(statements,
      sprintf("CREATE TABLE %s.cohort (cohort_definition_id INTEGER, subject_id INTEGER, cohort_start_date DATE, cohort_end_date DATE)", schema),
      sprintf("INSERT INTO %s.cohort VALUES (7,1,'2000-01-01','2000-01-01')", schema),
      sprintf("INSERT INTO %s.cohort VALUES (8,1,'2000-01-01','2000-01-01')", schema))
  }
  sql <- paste0(statements, ";", collapse = "\n")
  DatabaseConnector::executeSql(db, sql, progressBar = FALSE, reportOverallTime = FALSE)

  table_csv <- file.path(folder, "tables.csv")
  field_csv <- file.path(folder, "fields.csv")
  table_header <- c("cdmTableName", "schema", "cdmTable", "measurePersonCompleteness", "measurePersonCompletenessThreshold", "measurePersonCompletenessNotes")
  table_rows <- matrix(c("PERSON", "CDM", "Yes", "Yes", "0", "fixture"), nrow = 1, dimnames = list(NULL, table_header))
  write_control(table_csv, table_header, table_rows)
  field_header <- c("cdmTableName", "cdmFieldName", "cdmField", "isRequired", "isRequiredThreshold", "measureValueCompleteness", "measureValueCompletenessThreshold")
  field_rows <- matrix(c("PERSON", "person_id", "Yes", "Yes", "0", "Yes", "0", "PERSON", "value", "Yes", "Yes", "0", "Yes", "0"), nrow = 2, byrow = TRUE, dimnames = list(NULL, field_header))
  if (scenario == "missing-field") field_rows <- field_rows[field_rows[, "cdmFieldName"] == "person_id", , drop = FALSE]
  write_control(field_csv, field_header, field_rows)

  opts <- modifyList(base_options, list(
    cdmDatabaseSchema = schema, resultsDatabaseSchema = schema, vocabDatabaseSchema = schema,
    cdmVersion = "5.3", checkLevels = c("TABLE", "FIELD"),
    checkNames = c("cdmTable", "cdmField", "isRequired", "measureValueCompleteness", "measurePersonCompleteness"),
    checkSeverity = c("fatal", "convention", "characterization"), tablesToExclude = character(),
    tableCheckThresholdLoc = table_csv, fieldCheckThresholdLoc = field_csv,
    cohortDefinitionId = if (scenario == "cohort") 7L else numeric(), cohortDatabaseSchema = schema, cohortTableName = "cohort",
    outputFile = "results.json", writeToCsv = TRUE, csvFile = "results.csv", writeToTable = identical(scenario, "clean-write-table")))

  result_folder <- file.path(folder, "results")
  run <- outcome(function() execute(opts, result_folder, details))
  result_file <- file.path(result_folder, "results.json")
  result <- if (file.exists(result_file)) jsonlite::fromJSON(result_file, simplifyVector = FALSE) else NULL
  table_rows_query <- if ("dqdashboard_results" %in% tolower(DatabaseConnector::getTableNames(db, schema))) {
    DatabaseConnector::querySql(db, sprintf("SELECT * FROM %s.dqdashboard_results", schema))
  } else NULL
  db_json <- NULL
  db_json_run <- NULL
  if (!is.null(table_rows_query)) {
    json_folder <- file.path(folder, "db-json"); dir.create(json_folder, showWarnings = FALSE)
    db_json_run <- outcome(function() writeDBResultsToJson(db, schema, schema, "dqdashboard_results", json_folder, "from-database.json"))
    json_path <- file.path(json_folder, "from-database.json")
    if (file.exists(json_path)) db_json <- jsonlite::fromJSON(json_path, simplifyVector = FALSE)
  }
  database_cases[[scenario]] <- list(
    scenario = scenario, server = server, port = port, schema = schema, sql = sql,
    options = opts, tableThresholds = table_csv, fieldThresholds = field_csv,
    expected = run, result = result, tableRows = table_rows_query, databaseJsonExpected = db_json_run, databaseJson = db_json,
    resultCsv = if (file.exists(file.path(result_folder, "results.csv"))) readLines(file.path(result_folder, "results.csv"), warn = FALSE) else NULL)
}
json(database_cases, file.path(pg_root, "expected.json"))

# Metadata and fatal preflight checks are actual upstream calls.
for (scenario in c("empty-cdm-source", "missing-cdm-source")) {
  schema <- gsub("-", "_", scenario)
  folder <- file.path(pg_root, scenario); dir.create(folder, recursive = TRUE, showWarnings = FALSE)
  statements <- c(sprintf("DROP SCHEMA IF EXISTS %s CASCADE", schema), sprintf("CREATE SCHEMA %s", schema))
  if (scenario == "empty-cdm-source") statements <- c(statements, sprintf("CREATE TABLE %s.cdm_source (cdm_source_name VARCHAR(255), cdm_source_abbreviation VARCHAR(25))", schema))
  sql <- paste0(statements, ";", collapse = "\n")
  DatabaseConnector::executeSql(db, sql, progressBar = FALSE, reportOverallTime = FALSE)
  opts <- modifyList(base_options, list(cdmDatabaseSchema = schema, resultsDatabaseSchema = schema, vocabDatabaseSchema = schema, cdmVersion = "5.3"))
  preflight <- outcome(function() execute(opts, file.path(folder, "results"), details))
  database_cases[[scenario]] <- list(scenario = scenario, server = server, port = port, schema = schema, sql = sql, options = opts, expected = preflight)
}
json(database_cases, file.path(pg_root, "expected.json"))
DatabaseConnector::disconnect(db)
cat(sprintf("Generated %d scenarios\n", length(database_cases)))

# ---- GiBleed: full default DQD on Eunomia GiBleed (CDM 5.3) in PostgreSQL schema "gibleed" ----
gibleed_folder <- file.path(pg_root, "gibleed")
dir.create(gibleed_folder, recursive = TRUE, showWarnings = FALSE)
work <- "C:/Users/abora/tmp/dqd-pg-oracle"
sqlite_copy <- file.path(work, "gibleed-source.sqlite")
if (!file.copy("C:/Users/abora/tmp/dqd-bench/gibleed-source.sqlite", sqlite_copy, overwrite = TRUE)) stop("Could not copy GiBleed source")

db <- DatabaseConnector::connect(details)
DatabaseConnector::executeSql(db, "DROP SCHEMA IF EXISTS gibleed CASCADE; CREATE SCHEMA gibleed;", progressBar = FALSE, reportOverallTime = FALSE)
CommonDataModel::executeDdl(connectionDetails = details, cdmVersion = "5.3", cdmDatabaseSchema = "gibleed", executePrimaryKey = FALSE, executeForeignKey = FALSE)

sqlite_db <- DatabaseConnector::connect(do.call(DatabaseConnector::createConnectionDetails, list(dbms = "sqlite", server = sqlite_copy)))
for (tbl in DatabaseConnector::getTableNames(sqlite_db, "main")) {
  data <- DatabaseConnector::querySql(sqlite_db, sprintf("SELECT * FROM main.%s", tbl), snakeCaseToCamelCase = FALSE)
  names(data) <- tolower(names(data))
  # The Eunomia SQLite file stores the CDM VARCHAR(10) *_time columns as epoch-seconds text such as "907372800.0" or
  # "-1645920000" (11+ characters), which PostgreSQL rejects. Loader-side conversion only: the intended CDM value is a
  # time of day, so write it as HH:MM:SS (UTC). NULLs stay NULL.
  for (col in grep("_time$", names(data), value = TRUE)) if (is.character(data[[col]])) {
    seconds <- suppressWarnings(as.numeric(data[[col]]))
    data[[col]] <- ifelse(is.na(seconds), data[[col]], format(as.POSIXct(seconds, origin = "1970-01-01", tz = "UTC"), "%H:%M:%S"))
  }
  # relationship.is_hierarchical/defines_ancestry are VARCHAR(1) flags in the CDM DDL but arrive from the SQLite file
  # as text such as "0.0". Loader-side conversion to '0'/'1'.
  for (col in intersect(c("is_hierarchical", "defines_ancestry"), names(data))) data[[col]] <- as.character(as.integer(as.numeric(data[[col]])))
  cat(sprintf("insert %s: %d rows\n", tbl, nrow(data)))
  if (nrow(data) > 0) DatabaseConnector::insertTable(connection = db, databaseSchema = "gibleed", tableName = tolower(tbl), data = data, dropTableIfExists = FALSE, createTable = FALSE, tempTable = FALSE)
}
DatabaseConnector::disconnect(sqlite_db)
DatabaseConnector::disconnect(db)

elapsed <- system.time({
  DataQualityDashboard::executeDqChecks(
    connectionDetails = details,
    cdmVersion = "5.3",
    numThreads = 1,
    cdmSourceName = "GiBleed",
    cdmDatabaseSchema = "gibleed",
    resultsDatabaseSchema = "gibleed",
    writeToTable = FALSE,
    writeToCsv = TRUE,
    outputFolder = gibleed_folder,
    outputFile = "results.json",
    csvFile = "results.csv"
  )
})

results <- jsonlite::fromJSON(file.path(gibleed_folder, "results.json"))
check_results <- results$CheckResults
json(list(cdmDatabaseSchema = "gibleed", resultsDatabaseSchema = "gibleed", cdmSourceName = "GiBleed", cdmVersion = "5.3", numThreads = 1, writeToTable = FALSE, writeToCsv = TRUE, outputFile = "results.json", csvFile = "results.csv"), file.path(gibleed_folder, "options.json"))
timing <- list(
  r_elapsed_seconds = elapsed[["elapsed"]],
  total_checks = nrow(check_results),
  status_counts = list(
    passed = sum(check_results$passed == 1, na.rm = TRUE),
    failed = sum(check_results$failed == 1, na.rm = TRUE),
    not_applicable = sum(check_results$notApplicable == 1, na.rm = TRUE),
    error = sum(check_results$isError == 1, na.rm = TRUE)
  ),
  versions = list(
    DQD = as.character(packageVersion("DataQualityDashboard")),
    DatabaseConnector = as.character(packageVersion("DatabaseConnector")),
    SqlRender = as.character(packageVersion("SqlRender")),
    CommonDataModel = as.character(packageVersion("CommonDataModel")),
    R = R.version.string
  ),
  connection = list(dbms = "postgresql", server = server, port = port, schema = "gibleed")
)
json(timing, file.path(gibleed_folder, "timing.json"))
cat(sprintf("GiBleed: %d checks in %.2f seconds\n", timing$total_checks, timing$r_elapsed_seconds))
