# Real upstream DatabaseConnector/RSQLite execution. Every case gets an isolated database.
database_cases <- list()
db_root <- file.path(out, "database"); dir.create(db_root, recursive = TRUE, showWarnings = FALSE)
write_control <- function(path, header, rows) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  write.table(rbind(header, rows), path, sep = ",", row.names = FALSE, col.names = FALSE, quote = TRUE, na = "")
}
for (scenario in c("clean", "violating", "missing-table", "missing-field", "empty-table", "unpopulated-field", "cohort", "clean-write-table")) {
  folder <- file.path(db_root, scenario); dir.create(folder, recursive = TRUE, showWarnings = FALSE)
  sqlite <- file.path(folder, "cdm.sqlite")
  unlink(sqlite)
  db <- DBI::dbConnect(RSQLite::SQLite(), sqlite)
  DBI::dbExecute(db, "CREATE TABLE cdm_source (cdm_source_name TEXT, cdm_source_abbreviation TEXT, cdm_holder TEXT, source_description TEXT, source_documentation_reference TEXT, cdm_etl_reference TEXT, source_release_date TEXT, cdm_release_date TEXT, cdm_version TEXT, vocabulary_version TEXT, vocabulary_concept_id INTEGER)")
  DBI::dbExecute(db, "INSERT INTO cdm_source VALUES ('Parity','PAR','OHDSI','Fixture','','','','','5.3','','0')")
  if (scenario != "missing-table") {
    DBI::dbExecute(db, "CREATE TABLE PERSON (person_id INTEGER, value TEXT)")
    if (!scenario %in% c("empty-table", "unpopulated-field")) DBI::dbExecute(db, "INSERT INTO PERSON VALUES (1, 'present')")
    if (scenario == "violating") DBI::dbExecute(db, "INSERT INTO PERSON VALUES (NULL, NULL)")
    if (scenario == "unpopulated-field") DBI::dbExecute(db, "INSERT INTO PERSON VALUES (1, NULL)")
    if (scenario == "missing-field") DBI::dbExecute(db, "ALTER TABLE PERSON RENAME COLUMN person_id TO renamed_id")
  }
  if (scenario == "cohort") {
    DBI::dbExecute(db, "CREATE TABLE cohort (cohort_definition_id INTEGER, subject_id INTEGER, cohort_start_date TEXT, cohort_end_date TEXT)")
    DBI::dbExecute(db, "INSERT INTO cohort VALUES (7,1,'2000-01-01','2000-01-01')")
    DBI::dbExecute(db, "INSERT INTO cohort VALUES (8,1,'2000-01-01','2000-01-01')")
  }
  DBI::dbDisconnect(db)
  input_sqlite <- file.path(folder, "input.sqlite")
  file.copy(sqlite, input_sqlite, overwrite = TRUE)
  table_csv <- file.path(folder, "tables.csv")
  field_csv <- file.path(folder, "fields.csv")
  table_header <- c("cdmTableName", "schema", "cdmTable", "measurePersonCompleteness", "measurePersonCompletenessThreshold", "measurePersonCompletenessNotes")
  table_rows <- matrix(c("PERSON", "CDM", "Yes", "Yes", "0", "fixture"), nrow = 1, dimnames = list(NULL, table_header))
  write_control(table_csv, table_header, table_rows)
  field_header <- c("cdmTableName", "cdmFieldName", "cdmField", "isRequired", "isRequiredThreshold", "measureValueCompleteness", "measureValueCompletenessThreshold")
  field_rows <- matrix(c("PERSON", "person_id", "Yes", "Yes", "0", "Yes", "0", "PERSON", "value", "Yes", "Yes", "0", "Yes", "0"), nrow = 2, byrow = TRUE, dimnames = list(NULL, field_header))
  if (scenario == "missing-field") field_rows <- field_rows[field_rows[, "cdmFieldName"] == "person_id", , drop = FALSE]
  write_control(field_csv, field_header, field_rows)
  opts <- modifyList(base_options, list(cdmVersion = "5.3", checkLevels = c("TABLE", "FIELD"), checkNames = c("cdmTable", "cdmField", "isRequired", "measureValueCompleteness", "measurePersonCompleteness"), checkSeverity = c("fatal", "convention", "characterization"), tablesToExclude = character(), tableCheckThresholdLoc = table_csv, fieldCheckThresholdLoc = field_csv, cohortDefinitionId = if (scenario == "cohort") 7L else numeric(), cohortDatabaseSchema = "main", cohortTableName = "cohort", outputFile = "results.json", writeToCsv = TRUE, csvFile = "results.csv", writeToTable = identical(scenario, "clean-write-table")))
  details <- DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = sqlite)
  result_folder <- file.path(folder, "results")
  run <- outcome(function() execute(opts, result_folder, details))
  result_file <- file.path(result_folder, "results.json")
  result <- if (file.exists(result_file)) jsonlite::fromJSON(result_file, simplifyVector = FALSE) else NULL
  db <- DBI::dbConnect(RSQLite::SQLite(), sqlite)
  table_rows <- if ("main.dqdashboard_results" %in% DBI::dbListTables(db)) DBI::dbGetQuery(db, 'SELECT * FROM "main.dqdashboard_results"') else NULL
  DBI::dbDisconnect(db)
  db_json <- NULL
  db_json_run <- NULL
  if (!is.null(table_rows)) {
    json_folder <- file.path(folder, "db-json"); dir.create(json_folder, showWarnings = FALSE)
    db_connection <- DatabaseConnector::connect(connectionDetails = details)
    db_json_run <- outcome(function() writeDBResultsToJson(db_connection, "main", "main", "dqdashboard_results", json_folder, "from-database.json"))
    DatabaseConnector::disconnect(db_connection)
    json_path <- file.path(json_folder, "from-database.json")
    if (file.exists(json_path)) db_json <- jsonlite::fromJSON(json_path, simplifyVector = FALSE)
  }
  database_cases[[scenario]] <- list(scenario = scenario, sqlite = normalizePath(input_sqlite, winslash = "/"), options = opts, tableThresholds = table_csv, fieldThresholds = field_csv, expected = run, result = result, tableRows = table_rows, databaseJsonExpected = db_json_run, databaseJson = db_json, resultCsv = if (file.exists(file.path(result_folder, "results.csv"))) readLines(file.path(result_folder, "results.csv"), warn = FALSE) else NULL)
}
json(database_cases, file.path(out, "database", "expected.json"))
# Metadata and fatal preflight checks are actual upstream calls.
for (scenario in c("empty-cdm-source", "missing-cdm-source")) {
  folder <- file.path(db_root, scenario); dir.create(folder, recursive = TRUE, showWarnings = FALSE)
  sqlite <- file.path(folder, "cdm.sqlite")
  unlink(sqlite)
  db <- DBI::dbConnect(RSQLite::SQLite(), sqlite)
  if (scenario == "empty-cdm-source") DBI::dbExecute(db, "CREATE TABLE cdm_source (cdm_source_name TEXT, cdm_source_abbreviation TEXT)")
  DBI::dbDisconnect(db)
  details <- DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = sqlite)
  preflight <- outcome(function() execute(modifyList(base_options, list(cdmVersion = "5.3")), file.path(folder, "results"), details))
  database_cases[[scenario]] <- list(scenario = scenario, sqlite = normalizePath(sqlite, winslash = "/"), expected = preflight)
}
json(database_cases, file.path(db_root, "expected.json"))
