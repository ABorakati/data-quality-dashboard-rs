# Upstream DQD 2.9.0 runs `basename(allResults$outputFile)` when writeToCsv = TRUE and csvFile = "",
# but allResults has no outputFile element, so executeDqChecks fails after the results JSON and
# the results table are written. This real upstream run records that outcome.
#
# Standalone: Rscript csv-default-case.R UPSTREAM OUTPUT
# Sourced by generate.R: reuses its `out`, `outcome`, `execute`, `json` and `write_control`.
if (!exists("execute", inherits = TRUE)) {
  args <- commandArgs(TRUE)
  if (length(args) != 2L) stop("Usage: Rscript csv-default-case.R UPSTREAM OUTPUT")
  upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
  out <- args[[2]]
  dir.create(out, recursive = TRUE, showWarnings = FALSE)
  .libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
  suppressPackageStartupMessages(library(DataQualityDashboard))
  pin <- "b9d2a9b4a4736f7ea81415d974c72119b6f0bd11"
  actual <- system2("git", c("-C", shQuote(upstream), "rev-parse", "HEAD"), stdout = TRUE)
  if (!identical(actual, pin)) stop("Upstream HEAD differs from the parity pin: ", paste(actual, collapse = " "))
  if (as.character(packageVersion("DataQualityDashboard")) != "2.9.0") stop("Wrong installed DQD version")
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
  base_options <- list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", vocabDatabaseSchema = "main", cdmSourceName = "Parity", writeToTable = FALSE, outputFile = "results.json", numThreads = 1)
  execute <- function(options, folder, connectionDetails) {
    options$dbms <- NULL; options$connection <- NULL
    run_options <- modifyList(base_options, options)
    run_options$outputFolder <- folder
    do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = connectionDetails), run_options))
  }
}
if (!exists("write_control", inherits = TRUE)) {
  write_control <- function(path, header, rows) {
    dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
    write.table(rbind(header, rows), path, sep = ",", row.names = FALSE, col.names = FALSE, quote = TRUE, na = "")
  }
}

# Same input as the "clean" scenario in database-cases.R, in its own isolated database.
csv_default_folder <- file.path(out, "database", "csv-default")
unlink(csv_default_folder, recursive = TRUE)
dir.create(csv_default_folder, recursive = TRUE, showWarnings = FALSE)
csv_default_sqlite <- file.path(csv_default_folder, "cdm.sqlite")
db <- DBI::dbConnect(RSQLite::SQLite(), csv_default_sqlite)
DBI::dbExecute(db, "CREATE TABLE cdm_source (cdm_source_name TEXT, cdm_source_abbreviation TEXT, cdm_holder TEXT, source_description TEXT, source_documentation_reference TEXT, cdm_etl_reference TEXT, source_release_date TEXT, cdm_release_date TEXT, cdm_version TEXT, vocabulary_version TEXT, vocabulary_concept_id INTEGER)")
DBI::dbExecute(db, "INSERT INTO cdm_source VALUES ('Parity','PAR','OHDSI','Fixture','','','','','5.3','','0')")
DBI::dbExecute(db, "CREATE TABLE PERSON (person_id INTEGER, value TEXT)")
DBI::dbExecute(db, "INSERT INTO PERSON VALUES (1, 'present')")
DBI::dbDisconnect(db)
csv_default_input <- file.path(csv_default_folder, "input.sqlite")
file.copy(csv_default_sqlite, csv_default_input, overwrite = TRUE)
table_csv <- file.path(csv_default_folder, "tables.csv")
field_csv <- file.path(csv_default_folder, "fields.csv")
table_header <- c("cdmTableName", "schema", "cdmTable", "measurePersonCompleteness", "measurePersonCompletenessThreshold", "measurePersonCompletenessNotes")
write_control(table_csv, table_header, matrix(c("PERSON", "CDM", "Yes", "Yes", "0", "fixture"), nrow = 1, dimnames = list(NULL, table_header)))
field_header <- c("cdmTableName", "cdmFieldName", "cdmField", "isRequired", "isRequiredThreshold", "measureValueCompleteness", "measureValueCompletenessThreshold")
write_control(field_csv, field_header, matrix(c("PERSON", "person_id", "Yes", "Yes", "0", "Yes", "0", "PERSON", "value", "Yes", "Yes", "0", "Yes", "0"), nrow = 2, byrow = TRUE, dimnames = list(NULL, field_header)))
csv_default_opts <- modifyList(base_options, list(cdmVersion = "5.3", checkLevels = c("TABLE", "FIELD"), checkNames = c("cdmTable", "cdmField", "isRequired", "measureValueCompleteness", "measurePersonCompleteness"), checkSeverity = c("fatal", "convention", "characterization"), tablesToExclude = character(), tableCheckThresholdLoc = table_csv, fieldCheckThresholdLoc = field_csv, cohortDefinitionId = numeric(), cohortDatabaseSchema = "main", cohortTableName = "cohort", outputFile = "results.json", writeToCsv = TRUE, csvFile = "", writeToTable = TRUE))
csv_default_details <- DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = csv_default_sqlite)
csv_default_result_folder <- file.path(csv_default_folder, "results")
csv_default_run <- outcome(function() execute(csv_default_opts, csv_default_result_folder, csv_default_details))
csv_default_result_file <- file.path(csv_default_result_folder, "results.json")
csv_default_result <- if (file.exists(csv_default_result_file)) jsonlite::fromJSON(csv_default_result_file, simplifyVector = FALSE) else NULL
db <- DBI::dbConnect(RSQLite::SQLite(), csv_default_sqlite)
csv_default_table_rows <- if ("main.dqdashboard_results" %in% DBI::dbListTables(db)) DBI::dbGetQuery(db, 'SELECT * FROM "main.dqdashboard_results"') else NULL
DBI::dbDisconnect(db)
json(list(
  scenario = "csv-default", sqlite = normalizePath(csv_default_input, winslash = "/"), options = csv_default_opts,
  tableThresholds = table_csv, fieldThresholds = field_csv, expected = csv_default_run,
  resultFileWritten = file.exists(csv_default_result_file), result = csv_default_result, tableRows = csv_default_table_rows,
  resultFolderFiles = sort(list.files(csv_default_result_folder, all.files = TRUE, no.. = TRUE)),
  csvFilesWritten = sort(list.files(csv_default_result_folder, pattern = "\\.csv$", recursive = TRUE))
), file.path(csv_default_folder, "expected.json"))
