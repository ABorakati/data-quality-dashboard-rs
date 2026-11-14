# Real upstream DatabaseConnector/duckdb execution. Mirrors database-cases.R scenario for scenario,
# adds upstream's "Execute a single DQ check on DuckDB" test and a full GiBleed run, all on DuckDB.
# Upstream DataQualityDashboard, DatabaseConnector and duckdb run unmodified.
# Usage: Rscript duckdb-cases.R UPSTREAM OUTPUT GIBLEED_DUCKDB   (writes below OUTPUT/duckdb)
args <- commandArgs(TRUE)
if (length(args) != 3L) stop("Usage: Rscript duckdb-cases.R UPSTREAM OUTPUT GIBLEED_DUCKDB")
upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
out <- file.path(args[[2]], "duckdb")
gibleed <- normalizePath(args[[3]], winslash = "/", mustWork = TRUE)
dir.create(out, recursive = TRUE, showWarnings = FALSE)
out <- normalizePath(out, winslash = "/", mustWork = TRUE)
.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
suppressPackageStartupMessages(library(DataQualityDashboard))
pin <- "b9d2a9b4a4736f7ea81415d974c72119b6f0bd11"
actual <- system2("git", c("-C", shQuote(upstream), "rev-parse", "HEAD"), stdout = TRUE)
if (!identical(actual, pin)) stop("Upstream HEAD differs from the parity pin: ", paste(actual, collapse = " "))
if (as.character(packageVersion("DataQualityDashboard")) != "2.9.0") stop("Wrong installed DQD version")
for (relative in list.files(file.path(upstream, "inst"), recursive = TRUE)) {
  installed <- system.file(relative, package = "DataQualityDashboard")
  if (!nzchar(installed) || unname(tools::md5sum(installed)) != unname(tools::md5sum(file.path(upstream, "inst", relative)))) stop("Installed asset differs: ", relative)
}
json <- function(value, path) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  writeLines(jsonlite::toJSON(value, auto_unbox = TRUE, na = "null", null = "null", digits = 17, dataframe = "rows"), path, useBytes = TRUE)
}
outcome <- function(f) {
  warnings <- character()
  value <- tryCatch(withCallingHandlers(f(), warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") }), error = function(e) structure(list(message = conditionMessage(e)), class = "oracle_error"))
  if (inherits(value, "oracle_error")) list(ok = FALSE, error = value$message, warnings = warnings) else list(ok = TRUE, value = value, warnings = warnings)
}
status <- function(o) { o$value <- NULL; o }
# Default driver settings: DatabaseConnector keeps its own instance of each file alive, and a
# connection to that file must not ask for different instance settings.
duck <- function(path) DBI::dbConnect(duckdb::duckdb(), path)
duck_close <- function(db) DBI::dbDisconnect(db, shutdown = TRUE)
details_for <- function(path) DatabaseConnector::createConnectionDetails(dbms = "duckdb", server = path)
# DQD never writes the CDM, but DuckDB may checkpoint the file it opened read-write, so every run
# reads a scratch copy and the fixture keeps the untouched input.
scratch <- tempfile("duckdb-cases-"); dir.create(scratch)
run_copy <- function(path) { copy <- file.path(scratch, paste0(basename(dirname(path)), "-", basename(path))); file.copy(path, copy, overwrite = TRUE); copy }
base_options <- list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", vocabDatabaseSchema = "main", cdmSourceName = "Parity", writeToTable = FALSE, outputFile = "results.json", numThreads = 1)
versions <- list(DQD = as.character(packageVersion("DataQualityDashboard")), DatabaseConnector = as.character(packageVersion("DatabaseConnector")), duckdbPackage = as.character(packageVersion("duckdb")), SqlRender = as.character(packageVersion("SqlRender")), R = R.version.string)
local({
  db <- duck(":memory:"); on.exit(duck_close(db))
  versions$duckdbEngine <<- DBI::dbGetQuery(db, "SELECT library_version FROM pragma_version()")[1, 1]
})
cases <- list()

# --- Small scenarios, as database-cases.R -----------------------------------------------------
write_control <- function(path, header, rows) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  write.table(rbind(header, rows), path, sep = ",", row.names = FALSE, col.names = FALSE, quote = TRUE, na = "")
}
for (scenario in c("clean", "violating", "missing-table", "missing-field", "empty-table", "unpopulated-field", "cohort", "clean-write-table")) {
  folder <- file.path(out, scenario); unlink(folder, recursive = TRUE); dir.create(folder, recursive = TRUE)
  input <- file.path(folder, "input.duckdb")
  db <- duck(input)
  DBI::dbExecute(db, "CREATE TABLE cdm_source (cdm_source_name VARCHAR, cdm_source_abbreviation VARCHAR, cdm_holder VARCHAR, source_description VARCHAR, source_documentation_reference VARCHAR, cdm_etl_reference VARCHAR, source_release_date VARCHAR, cdm_release_date VARCHAR, cdm_version VARCHAR, vocabulary_version VARCHAR, vocabulary_concept_id INTEGER)")
  DBI::dbExecute(db, "INSERT INTO cdm_source VALUES ('Parity','PAR','OHDSI','Fixture','','','','','5.3','',0)")
  if (scenario != "missing-table") {
    DBI::dbExecute(db, "CREATE TABLE PERSON (person_id INTEGER, value VARCHAR)")
    if (!scenario %in% c("empty-table", "unpopulated-field")) DBI::dbExecute(db, "INSERT INTO PERSON VALUES (1, 'present')")
    if (scenario == "violating") DBI::dbExecute(db, "INSERT INTO PERSON VALUES (NULL, NULL)")
    if (scenario == "unpopulated-field") DBI::dbExecute(db, "INSERT INTO PERSON VALUES (1, NULL)")
    if (scenario == "missing-field") DBI::dbExecute(db, "ALTER TABLE PERSON RENAME COLUMN person_id TO renamed_id")
  }
  if (scenario == "cohort") {
    DBI::dbExecute(db, "CREATE TABLE cohort (cohort_definition_id INTEGER, subject_id INTEGER, cohort_start_date DATE, cohort_end_date DATE)")
    DBI::dbExecute(db, "INSERT INTO cohort VALUES (7, 1, DATE '2000-01-01', DATE '2000-01-01'), (8, 1, DATE '2000-01-01', DATE '2000-01-01')")
  }
  duck_close(db)
  table_csv <- file.path(folder, "tables.csv"); field_csv <- file.path(folder, "fields.csv")
  table_header <- c("cdmTableName", "schema", "cdmTable", "measurePersonCompleteness", "measurePersonCompletenessThreshold", "measurePersonCompletenessNotes")
  write_control(table_csv, table_header, matrix(c("PERSON", "CDM", "Yes", "Yes", "0", "fixture"), nrow = 1, dimnames = list(NULL, table_header)))
  field_header <- c("cdmTableName", "cdmFieldName", "cdmField", "isRequired", "isRequiredThreshold", "measureValueCompleteness", "measureValueCompletenessThreshold")
  field_rows <- matrix(c("PERSON", "person_id", "Yes", "Yes", "0", "Yes", "0", "PERSON", "value", "Yes", "Yes", "0", "Yes", "0"), nrow = 2, byrow = TRUE, dimnames = list(NULL, field_header))
  if (scenario == "missing-field") field_rows <- field_rows[field_rows[, "cdmFieldName"] == "person_id", , drop = FALSE]
  write_control(field_csv, field_header, field_rows)
  opts <- modifyList(base_options, list(cdmVersion = "5.3", checkLevels = c("TABLE", "FIELD"), checkNames = c("cdmTable", "cdmField", "isRequired", "measureValueCompleteness", "measurePersonCompleteness"), checkSeverity = c("fatal", "convention", "characterization"), tablesToExclude = character(), tableCheckThresholdLoc = table_csv, fieldCheckThresholdLoc = field_csv, cohortDefinitionId = if (scenario == "cohort") 7L else numeric(), cohortDatabaseSchema = "main", cohortTableName = "cohort", outputFile = "results.json", writeToCsv = TRUE, csvFile = "results.csv", writeToTable = identical(scenario, "clean-write-table")))
  working <- run_copy(input)
  result_folder <- file.path(folder, "results")
  run <- outcome(function() do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = details_for(working), outputFolder = result_folder), opts)))
  result_file <- file.path(result_folder, "results.json")
  db <- duck(working)
  tables <- DBI::dbGetQuery(db, "SELECT table_schema, table_name FROM information_schema.tables ORDER BY 1, 2")
  dqd_tables <- tables[grepl("dqdashboard", tables$table_name), , drop = FALSE]
  table_rows <- if (nrow(dqd_tables)) setNames(lapply(seq_len(nrow(dqd_tables)), function(i) DBI::dbGetQuery(db, sprintf('SELECT * FROM "%s"."%s"', dqd_tables$table_schema[i], dqd_tables$table_name[i]))), paste(dqd_tables$table_schema, dqd_tables$table_name, sep = ".")) else NULL
  duck_close(db)
  db_json <- NULL; db_json_run <- NULL
  if (!is.null(table_rows)) {
    json_folder <- file.path(folder, "db-json"); dir.create(json_folder, showWarnings = FALSE)
    connection <- DatabaseConnector::connect(details_for(working))
    db_json_run <- outcome(function() writeDBResultsToJson(connection, "main", "main", "dqdashboard_results", json_folder, "from-database.json"))
    DatabaseConnector::disconnect(connection)
    json_path <- file.path(json_folder, "from-database.json")
    if (file.exists(json_path)) db_json <- jsonlite::fromJSON(json_path, simplifyVector = FALSE)
  }
  cases[[scenario]] <- list(scenario = scenario, duckdb = paste0("duckdb/", scenario, "/input.duckdb"), options = opts, expected = status(run),
    result = if (file.exists(result_file)) jsonlite::fromJSON(result_file, simplifyVector = FALSE) else NULL,
    tableRows = table_rows, databaseJsonExpected = if (is.null(db_json_run)) NULL else status(db_json_run), databaseJson = db_json,
    resultCsv = if (file.exists(file.path(result_folder, "results.csv"))) readLines(file.path(result_folder, "results.csv"), warn = FALSE) else NULL)
  cat(scenario, if (isTRUE(run$ok)) "ok" else paste("error:", run$error), "\n")
}
for (scenario in c("empty-cdm-source", "missing-cdm-source")) {
  folder <- file.path(out, scenario); unlink(folder, recursive = TRUE); dir.create(folder, recursive = TRUE)
  input <- file.path(folder, "input.duckdb")
  db <- duck(input)
  if (scenario == "empty-cdm-source") DBI::dbExecute(db, "CREATE TABLE cdm_source (cdm_source_name VARCHAR, cdm_source_abbreviation VARCHAR)")
  duck_close(db)
  working <- run_copy(input)
  preflight <- outcome(function() do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = details_for(working), outputFolder = file.path(folder, "results")), modifyList(base_options, list(cdmVersion = "5.3")))))
  cases[[scenario]] <- list(scenario = scenario, duckdb = paste0("duckdb/", scenario, "/input.duckdb"), expected = status(preflight))
  cat(scenario, if (isTRUE(preflight$ok)) "ok" else paste("error:", preflight$error), "\n")
}

# --- test-executeDqChecks.R: "Execute a single DQ check on DuckDB" ----------------------------
local({
  folder <- file.path(out, "single-check"); unlink(folder, recursive = TRUE); dir.create(folder, recursive = TRUE)
  opts <- list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", cdmSourceName = "DuckDB Test", checkNames = "measurePersonCompleteness", outputFile = "results.json", writeToTable = FALSE)
  run <- outcome(function() do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = details_for(run_copy(gibleed)), outputFolder = file.path(folder, "results")), opts)))
  cases[["single-check"]] <<- list(scenario = "single-check", test = "Execute a single DQ check on DuckDB", options = opts, expected = status(run), result = jsonlite::fromJSON(file.path(folder, "results", "results.json"), simplifyVector = FALSE))
  cat("single-check", if (isTRUE(run$ok)) "ok" else paste("error:", run$error), "\n")
})
json(cases, file.path(out, "expected.json"))

# --- Full GiBleed run ----------------------------------------------------------------------------
local({
  folder <- file.path(out, "gibleed"); unlink(folder, recursive = TRUE); dir.create(folder, recursive = TRUE)
  file.copy(gibleed, file.path(folder, "cdm.duckdb"))
  opts <- list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", cdmSourceName = "GiBleed", cdmVersion = "5.3", numThreads = 1, writeToTable = FALSE, writeToCsv = TRUE, outputFile = "results.json", csvFile = "results.csv")
  warnings <- character()
  t <- system.time(withCallingHandlers(
    do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = details_for(run_copy(gibleed)), outputFolder = folder), opts)),
    warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") }))
  results <- jsonlite::fromJSON(file.path(folder, "results.json"))$CheckResults
  json(opts, file.path(folder, "options.json"))
  json(list(r_elapsed_seconds = t[["elapsed"]], total_checks = nrow(results), status_counts = list(passed = sum(results$passed == 1, na.rm = TRUE), failed = sum(results$failed == 1, na.rm = TRUE), not_applicable = sum(results$notApplicable == 1, na.rm = TRUE), error = sum(results$isError == 1, na.rm = TRUE)), warnings = unique(warnings), versions = versions), file.path(folder, "timing.json"))
  cat("gibleed", nrow(results), "checks,", sum(results$isError == 1, na.rm = TRUE), "errors\n")
})
json(list(upstreamCommit = pin, versions = versions), file.path(out, "manifest.json"))
unlink(scratch, recursive = TRUE)
