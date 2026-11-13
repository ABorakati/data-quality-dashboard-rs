# Re-runs every scenario in the pinned upstream testthat suite (tests/testthat/*.R) through the
# pinned DataQualityDashboard package and records inputs and full outputs, so the Rust port can be
# compared with upstream on exactly the situations upstream tests. Mutations are recorded as the
# exact SQLite statements executed, so the Rust test applies the same statements to the same
# GiBleed copy. Usage: Rscript upstream-testthat-cases.R UPSTREAM OUTPUT GIBLEED_SQLITE
args <- commandArgs(TRUE)
if (length(args) != 3L) stop("Usage: Rscript upstream-testthat-cases.R UPSTREAM OUTPUT GIBLEED_SQLITE")
upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
out <- args[[2]]
gibleed <- normalizePath(args[[3]], winslash = "/", mustWork = TRUE)
dir.create(out, recursive = TRUE, showWarnings = FALSE)
.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
suppressPackageStartupMessages(library(DataQualityDashboard))
ns <- asNamespace("DataQualityDashboard")
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
# Outcome without the (large, in-memory) value.
status <- function(o) { o$value <- NULL; o }
read_file <- function(path) if (file.exists(path)) rawToChar(readBin(path, "raw", n = file.info(path)$size)) else NULL
read_json_file <- function(path) if (file.exists(path)) jsonlite::fromJSON(path, simplifyVector = FALSE) else NULL

scratch <- tempfile("upstream-testthat-"); dir.create(scratch)
on.exit(unlink(scratch, recursive = TRUE), add = TRUE)
unit_csv <- file.path(out, "unittest_OMOP_CDMv5.3_Concept_Level.csv")
file.copy(system.file("csv", "unittest_OMOP_CDMv5.3_Concept_Level.csv", package = "DataQualityDashboard"), unit_csv, overwrite = TRUE)
na_exclusions <- c("COST", "CONCEPT", "VOCABULARY", "CONCEPT_ANCESTOR", "CONCEPT_RELATIONSHIP", "CONCEPT_CLASS", "CONCEPT_SYNONYM", "RELATIONSHIP", "DOMAIN")

fresh_db <- function(name) {
  path <- file.path(scratch, paste0(name, ".sqlite"))
  file.copy(gibleed, path, overwrite = TRUE)
  path
}
sqlite_details <- function(path) DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = path)
# Statements exactly as DatabaseConnector::executeSql runs them (it splits with SqlRender::splitSql).
statements_of <- function(sql, translate = FALSE, ...) {
  if (translate) sql <- SqlRender::translate(SqlRender::render(sql, ...), targetDialect = "sqlite")
  SqlRender::splitSql(sql)
}
execute_statements <- function(path, statements) {
  if (!length(statements)) return(invisible())
  connection <- DatabaseConnector::connect(sqlite_details(path))
  on.exit(DatabaseConnector::disconnect(connection))
  for (statement in statements) DatabaseConnector::executeSql(connection, statement, progressBar = FALSE, reportOverallTime = FALSE)
}
table_names <- function(path) {
  db <- DBI::dbConnect(RSQLite::SQLite(), path)
  on.exit(DBI::dbDisconnect(db))
  sort(DBI::dbListTables(db))
}
table_rows <- function(path, table) {
  db <- DBI::dbConnect(RSQLite::SQLite(), path)
  on.exit(DBI::dbDisconnect(db))
  DBI::dbGetQuery(db, sprintf('SELECT * FROM "%s"', gsub('"', '""', table)))
}
original_tables <- table_names(gibleed)
new_tables <- function(path) {
  # Backup copies made by the scenario setup are input data, not DQD output.
  tables <- setdiff(table_names(path), original_tables)
  tables <- tables[!grepl("_BACK$", tables)]
  setNames(lapply(tables, function(t) table_rows(path, t)), tables)
}

cases <- list()
add <- function(name, value) {
  attr(value, "db") <- NULL
  attr(value, "folder") <- NULL
  cases[[name]] <<- value
  cat(name, if (isTRUE(value$expected$ok) || is.null(value$expected)) "ok" else paste("error:", value$expected$error), "\n")
}
# One executeDqChecks run on a fresh, optionally mutated GiBleed copy.
run_case <- function(name, test, setup = character(), options = list(), keep_db = FALSE) {
  db <- fresh_db(name)
  execute_statements(db, setup)
  folder <- file.path(scratch, name)
  opts <- modifyList(list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", cdmSourceName = "Eunomia", outputFile = "results.json", writeToTable = FALSE), options)
  run <- outcome(function() do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = sqlite_details(db), outputFolder = folder), opts)))
  files <- sort(list.files(folder))
  json_file <- if (nzchar(opts$outputFile)) opts$outputFile else grep("\\.json$", files, value = TRUE)[1]
  record <- list(test = test, setup = setup, options = opts, expected = status(run), resultFiles = files,
    resultFile = json_file, result = read_json_file(file.path(folder, json_file)),
    executionTimeSecondsIsNumeric = isTRUE(run$ok) && is.numeric(run$value$executionTimeSeconds),
    newTables = new_tables(db))
  if (keep_db) attr(record, "db") <- db
  attr(record, "folder") <- folder
  record
}
strip_attrs <- function(x) { attr(x, "db") <- NULL; attr(x, "folder") <- NULL; x }

# --- test-calculateNotApplicableStatus.R: .applyNotApplicable on the upstream mock rows ----------
mock <- function(checkName, cdmTableName, cdmFieldName, isError, tableIsMissing, fieldIsMissing, tableIsEmpty) {
  data.frame(checkName = checkName, cdmTableName = cdmTableName, cdmFieldName = cdmFieldName, isError = isError, tableIsMissing = tableIsMissing, fieldIsMissing = fieldIsMissing, tableIsEmpty = tableIsEmpty, fieldIsEmpty = FALSE, conceptIsMissing = FALSE, conceptAndUnitAreMissing = FALSE, stringsAsFactors = FALSE)
}
mocks <- list(
  list(test = "measurePersonCompleteness should not be marked as not applicable when table is empty", row = mock("measurePersonCompleteness", "DEVICE_EXPOSURE", NA, 0, FALSE, FALSE, TRUE)),
  list(test = "measurePersonCompleteness should be marked as not applicable when table is missing", row = mock("measurePersonCompleteness", "DEVICE_EXPOSURE", NA, 0, TRUE, FALSE, FALSE)),
  list(test = "NA applied correctly when table or field is missing", row = mock("measurePersonCompleteness", "FOO", NA, 1, TRUE, FALSE, FALSE)),
  list(test = "NA applied correctly when table or field is missing", row = mock("measureValueCompleteness", "OBSERVATION", "bar", 1, FALSE, TRUE, FALSE)),
  list(test = ".applyNotApplicable handles cdmTable and cdmField correctly", row = mock("cdmTable", "FOO", NA, 0, TRUE, FALSE, FALSE)),
  list(test = ".applyNotApplicable handles cdmTable and cdmField correctly", row = mock("cdmTable", "FOO", NA, 0, FALSE, FALSE, TRUE)),
  list(test = ".applyNotApplicable handles cdmTable and cdmField correctly", row = mock("cdmTable", "FOO", NA, 1, FALSE, FALSE, FALSE)),
  list(test = ".applyNotApplicable handles cdmTable and cdmField correctly", row = mock("cdmField", "OBSERVATION", "bar", 0, TRUE, FALSE, FALSE)),
  list(test = ".applyNotApplicable handles cdmTable and cdmField correctly", row = mock("cdmField", "OBSERVATION", "bar", 0, FALSE, TRUE, FALSE)),
  list(test = ".applyNotApplicable handles cdmTable and cdmField correctly", row = mock("cdmField", "OBSERVATION", "bar", 0, FALSE, FALSE, TRUE))
)
# Beyond the upstream mocks: every flag combination for each rule branch.
for (name in c("measurePersonCompleteness", "cdmTable", "cdmField", "measureValueCompleteness", "isRequired")) for (bits in 0:63) {
  b <- as.logical(intToBits(bits)[1:6])
  row <- mock(name, "T", "f", as.numeric(b[1]), b[2], b[3], b[4])
  row$fieldIsEmpty <- b[5]; row$conceptIsMissing <- b[6]
  mocks[[length(mocks) + 1L]] <- list(test = "exhaustive flag combinations", row = row)
}
add("apply-not-applicable", list(test = "test-calculateNotApplicableStatus.R (mock rows)", cases = lapply(mocks, function(m) {
  list(test = m$test, row = m$row, expected = outcome(function() get(".applyNotApplicable", ns)(m$row)))
})))

# --- test-evaluateThresholds.R: .evaluateThresholds with empty threshold frames ---------------------
threshold_rows <- list(
  list(test = "evaluateThresholds handles missing table/field errors correctly for cdmField/cdmTable", row = data.frame(checkName = "cdmField", cdmTableName = "FOO", cdmFieldName = "bar", checkLevel = "FIELD", error = "relation \"foo\" does not exist", failed = 0, isError = 0, numViolatedRows = 0, pctViolatedRows = 0, stringsAsFactors = FALSE)),
  list(test = "evaluateThresholds handles missing table/field errors correctly for cdmField/cdmTable", row = data.frame(checkName = "cdmTable", cdmTableName = "FOO", cdmFieldName = NA, checkLevel = "TABLE", error = "relation \"foo\" does not exist", failed = 0, isError = 0, numViolatedRows = 0, pctViolatedRows = 0, stringsAsFactors = FALSE)),
  list(test = "evaluateThresholds handles missing table/field errors correctly for other checks", row = data.frame(checkName = "measurePersonCompleteness", cdmTableName = "FOO", cdmFieldName = NA, checkLevel = "TABLE", error = "relation \"foo\" does not exist", failed = 0, isError = 0, numViolatedRows = 0, pctViolatedRows = 0, stringsAsFactors = FALSE)),
  list(test = "evaluateThresholds handles missing table/field errors correctly for other checks", row = data.frame(checkName = "measureValueCompleteness", cdmTableName = "OBSERVATION", cdmFieldName = "bar", checkLevel = "FIELD", error = "column \"bar\" does not exist", failed = 0, isError = 0, numViolatedRows = 0, pctViolatedRows = 0, stringsAsFactors = FALSE)),
  list(test = "evaluateThresholds handles non-missing table/field errors correctly", row = data.frame(checkName = "cdmTable", cdmTableName = "PERSON", cdmFieldName = NA, checkLevel = "TABLE", error = "syntax error at or near \"SELECT\"", failed = 0, isError = 0, numViolatedRows = 0, pctViolatedRows = 0, stringsAsFactors = FALSE))
)
add("evaluate-thresholds-errors", list(test = "test-evaluateThresholds.R", cases = lapply(threshold_rows, function(m) {
  list(test = m$test, row = m$row, expected = outcome(function() get(".evaluateThresholds", ns)(m$row, data.frame(), data.frame(), data.frame())))
})))

# --- test-executeDqChecks.R -------------------------------------------------------------------------
base <- run_case("single-check", "Execute a single DQ check on Synthea/Eunomia", options = list(checkNames = "measurePersonCompleteness", outputFile = ""))
add("single-check", strip_attrs(base))
for (level in c("TABLE", "FIELD")) {
  add(paste0("level-", tolower(level)), run_case(paste0("level-", tolower(level)), sprintf("Execute all %s checks on Synthea/Eunomia", level), options = list(checkLevels = level)))
}
add("level-concept", run_case("level-concept", "Execute CONCEPT checks on Synthea/Eunomia", options = list(checkLevels = "CONCEPT", conceptCheckThresholdLoc = system.file("csv", "unittest_OMOP_CDMv5.3_Concept_Level.csv", package = "DataQualityDashboard"))))
overlap_person <- 6 # SELECT person_id FROM observation_period LIMIT 1 on the GiBleed fixture
local({
  db <- fresh_db("overlap-probe")
  connection <- DatabaseConnector::connect(sqlite_details(db))
  on.exit(DatabaseConnector::disconnect(connection))
  if (DatabaseConnector::querySql(connection, "SELECT person_id FROM observation_period LIMIT 1;")[1, 1] != overlap_person) stop("GiBleed fixture changed")
})
overlap_insert <- "INSERT INTO observation_period (observation_period_id, person_id, observation_period_start_date, observation_period_end_date, period_type_concept_id)
     VALUES
     (999999, @person_id, strftime('%s', '2020-01-01'), strftime('%s', '2020-06-30'), 44814724),
     (999998, @person_id, strftime('%s', '2020-04-01'), strftime('%s', '2020-12-31'), 44814724);"
back_to_back_insert <- "INSERT INTO observation_period (observation_period_id, person_id, observation_period_start_date, observation_period_end_date, period_type_concept_id)
     VALUES
     (999997, @person_id, strftime('%s', '2020-01-01'), strftime('%s', '2020-06-30'), 44814724),
     (999996, @person_id, strftime('%s', '2020-07-01'), strftime('%s', '2020-12-31'), 44814724);"
overlap_test <- "Execute observation period overlap check"
add("overlap-clean", run_case("overlap-clean", overlap_test, options = list(checkNames = "measureObservationPeriodOverlap")))
add("overlap-overlapping", run_case("overlap-overlapping", overlap_test, statements_of(overlap_insert, TRUE, person_id = overlap_person), list(checkNames = "measureObservationPeriodOverlap")))
add("overlap-back-to-back", run_case("overlap-back-to-back", overlap_test, statements_of(back_to_back_insert, TRUE, person_id = overlap_person), list(checkNames = "measureObservationPeriodOverlap")))
add("cohort", run_case("cohort", "Execute a single DQ check on a cohort in Synthea/Eunomia",
  statements_of("INSERT INTO @results_schema.cohort SELECT @cohort_id, person_id, observation_period_start_date, observation_period_end_date FROM @cdm_schema.observation_period LIMIT 10;", TRUE, results_schema = "main", cohort_id = 123, cdm_schema = "main"),
  list(checkNames = "measurePersonCompleteness", cohortTableName = "cohort", cohortDefinitionId = 123)))
add("invalid-cdm-version", list(test = "Check invalid cdm version", cases = lapply(c("5.2.3.1", "5.6", "5.1"), function(version) {
  list(cdmVersion = version, expected = status(outcome(function() executeDqChecks(connectionDetails = sqlite_details(fresh_db("invalid-version")), resultsDatabaseSchema = "main", cdmSourceName = "Eunomia", checkNames = "measurePersonCompleteness", outputFolder = file.path(scratch, "invalid-version"), writeToTable = FALSE, cdmVersion = version))))
})))
defaults <- formals(executeDqChecks)
constant <- c("numThreads", "sqlOnly", "sqlOnlyUnionCount", "sqlOnlyIncrementalInsert", "outputFile", "verboseMode", "writeToTable", "writeTableName", "writeToCsv", "csvFile", "checkLevels", "checkNames", "checkSeverity", "cohortDefinitionId", "cohortTableName", "tablesToExclude", "cdmVersion", "tableCheckThresholdLoc", "fieldCheckThresholdLoc", "conceptCheckThresholdLoc")
add("default-arguments", list(test = "The vocabulary tables added in CDM v5.5 are excluded by default", defaults = lapply(setNames(constant, constant), function(name) eval(defaults[[name]])), inherited = list(vocabDatabaseSchema = deparse(defaults$vocabDatabaseSchema), cohortDatabaseSchema = deparse(defaults$cohortDatabaseSchema))))
add("write-to-table", run_case("write-to-table", "Execute DQ checks and write to table", options = list(checkNames = "measurePersonCompleteness", writeToTable = TRUE, writeTableName = "dqd_results")))
for (dbms in c("sql server", "sqlite", "postgresql")) for (variant in list(c(4, 1), c(1, 1), c(1, 0))) {
  name <- sprintf("sql-only-%s-union%d-incremental%d", gsub(" ", "_", dbms), variant[1], variant[2])
  folder <- file.path(scratch, name)
  details <- if (dbms == "sqlite") sqlite_details(file.path(scratch, "unused.sqlite")) else DatabaseConnector::createConnectionDetails(dbms = dbms, pathToDriver = "/")
  opts <- list(cdmDatabaseSchema = "@yourCdmSchema", resultsDatabaseSchema = "@yourResultsSchema", cdmSourceName = "Eunomia", checkNames = "measurePersonCompleteness", writeToTable = FALSE, sqlOnly = TRUE, sqlOnlyUnionCount = variant[1], sqlOnlyIncrementalInsert = as.logical(variant[2]), writeTableName = "dqdashboard_results")
  run <- outcome(function() do.call(executeDqChecks, c(list(connectionDetails = details, outputFolder = folder), opts)))
  sql_files <- sort(list.files(folder, pattern = "\\.sql$"))
  add(name, list(test = "Execute DQ checks using sqlOnly=TRUE (expect_snapshot)", dbms = dbms, options = opts, expected = status(run), resultFiles = sort(list.files(folder)), files = setNames(lapply(sql_files, function(f) read_file(file.path(folder, f))), sql_files)))
}
# Upstream snapshots are taken with dbms = "sql server": confirm the oracle reproduces them.
snapshot <- readLines(file.path(upstream, "tests", "testthat", "_snaps", "executeDqChecks.md"), warn = FALSE)
add("snapshot-file", list(test = "_snaps/executeDqChecks.md", lines = snapshot))
local({
  name <- "incremental-insert-valid"
  folder <- file.path(scratch, name)
  db <- fresh_db(name)
  opts <- list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", cdmSourceName = "Eunomia", checkNames = "measurePersonCompleteness", writeToTable = FALSE, sqlOnly = TRUE, sqlOnlyUnionCount = 4, sqlOnlyIncrementalInsert = TRUE, writeTableName = "dqd_results")
  run <- outcome(function() do.call(executeDqChecks, c(list(connectionDetails = sqlite_details(db), outputFolder = folder), opts)))
  ddl <- SqlRender::readSql(file.path(folder, "ddlDqdResults.sql"))
  check <- SqlRender::readSql(file.path(folder, "TABLE_measurePersonCompleteness.sql"))
  connection <- DatabaseConnector::connect(sqlite_details(db))
  executed <- outcome(function() { DatabaseConnector::executeSql(connection = connection, sql = ddl); DatabaseConnector::executeSql(connection = connection, sql = check) })
  rows <- outcome(function() DatabaseConnector::renderTranslateQuerySql(connection, "SELECT * FROM @database_schema.dqd_results;", database_schema = "main"))
  DatabaseConnector::disconnect(connection)
  add(name, list(test = "Incremental insert SQL is valid.", options = opts, expected = status(run), files = list(ddl = ddl, check = check), executed = status(executed), rows = rows))
})
add("multiple-cdm-source", run_case("multiple-cdm-source", "Multiple cdm_source rows triggers warning.",
  statements_of("INSERT INTO @database_schema.cdm_source VALUES ('foo',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);", TRUE, database_schema = "main"),
  list(checkNames = "measurePersonCompleteness")))
add("execution-time-seconds", run_case("execution-time-seconds", "Execute checks on Synthea/Eunomia to test new variable executionTimeSeconds",
  options = list(checkNames = "measurePersonCompleteness", conceptCheckThresholdLoc = system.file("csv", "unittest_OMOP_CDMv5.3_Concept_Level.csv", package = "DataQualityDashboard"))))
add("severity-fatal", run_case("severity-fatal", "checkNames are filtered by checkSeverity", options = list(checkSeverity = "fatal")))

# --- test-calculateNotApplicableStatus.R: real runs on emptied tables -------------------------------
na_options <- function(checks) list(checkNames = checks, tablesToExclude = na_exclusions)
add("na-device-exposure-empty", run_case("na-device-exposure-empty", "Not Applicable status Table Empty",
  statements_of("DELETE FROM DEVICE_EXPOSURE;"), na_options(c("cdmTable", "cdmField", "measureValueCompleteness"))))
add("na-condition-occurrence-empty", run_case("na-condition-occurrence-empty", "measureConditionEraCompleteness Not Applicable if condition_occurrence empty",
  statements_of("CREATE TABLE CONDITION_OCCURRENCE_BACK AS SELECT * FROM CONDITION_OCCURRENCE; DELETE FROM CONDITION_OCCURRENCE;"), na_options(c("cdmTable", "cdmField", "measureValueCompleteness", "measureConditionEraCompleteness"))))
add("na-condition-era-empty", run_case("na-condition-era-empty", "measureConditionEraCompleteness Fails if condition_era empty",
  statements_of("CREATE TABLE CONDITION_ERA_BACK AS SELECT * FROM CONDITION_ERA; DELETE FROM CONDITION_ERA;"), na_options(c("cdmTable", "cdmField", "measureValueCompleteness", "measureConditionEraCompleteness"))))
add("na-observation-period-empty", run_case("na-observation-period-empty", "measurePersonCompleteness NOT marked as Not Applicable when table is empty",
  statements_of("CREATE TABLE OBSERVATION_PERIOD_BACK AS SELECT * FROM OBSERVATION_PERIOD; DELETE FROM OBSERVATION_PERIOD;"), na_options(c("cdmTable", "cdmField", "measureValueCompleteness", "measurePersonCompleteness"))))

# --- test-plausibleAfterBirth.R ---------------------------------------------------------------------
add("plausible-after-birth", run_case("plausible-after-birth", "plausibleAfterBirth allows events on same day as birth (issue #561)",
  c(statements_of("
    -- Update person 1 to have a specific birth_datetime
    UPDATE person
    SET birth_datetime = '1990-01-15 10:00:00'
    WHERE person_id = 1;
  "), statements_of("
    DELETE FROM visit_occurrence;

    INSERT INTO visit_occurrence
    (visit_occurrence_id, person_id, visit_concept_id, visit_start_date, visit_start_datetime, visit_end_date, visit_end_datetime, visit_type_concept_id)
    VALUES
    (1, 1, 9201, '1990-01-15', '1990-01-15 10:00:00', '1990-01-15', '1990-01-15 10:00:00', 44818517);
  ")),
  list(checkNames = "plausibleAfterBirth", tablesToExclude = na_exclusions)))

# --- test-convertResultsCase.R ----------------------------------------------------------------------
local({
  record <- run_case("convert-case", "Camel correctly converted to snake and back", options = list(checkNames = "measurePersonCompleteness", outputFile = "foo.json"))
  folder <- attr(record, "folder")
  foo <- file.path(folder, "foo.json")
  same_camel <- outcome(function() convertJsonResultsFileCase(foo, writeToFile = FALSE, targetCase = "camel"))
  snake <- outcome(function() convertJsonResultsFileCase(foo, writeToFile = TRUE, folder, outputFile = "snake.json", targetCase = "snake"))
  snake_file <- file.path(folder, "snake.json")
  same_snake <- outcome(function() convertJsonResultsFileCase(snake_file, writeToFile = FALSE, targetCase = "snake"))
  camel <- outcome(function() convertJsonResultsFileCase(snake_file, writeToFile = TRUE, folder, targetCase = "camel"))
  record$conversions <- list(
    sameCamel = list(expected = status(same_camel), returnedEqualsInput = isTRUE(same_camel$ok) && identical(jsonlite::toJSON(same_camel$value), jsonlite::toJSON(jsonlite::fromJSON(foo)))),
    snake = list(expected = status(snake), length = length(snake$value), checkResultNames = names(snake$value$CheckResults), written = read_file(snake_file)),
    sameSnake = list(expected = status(same_snake)),
    camel = list(expected = status(camel), length = length(camel$value), checkResultNames = names(camel$value$CheckResults), written = read_file(file.path(folder, "snake_camel.json"))),
    roundTripEqual = isTRUE(camel$ok) && identical(jsonlite::toJSON(jsonlite::fromJSON(foo)), jsonlite::toJSON(camel$value)),
    invalidCase = status(outcome(function() convertJsonResultsFileCase("bar.json", writeToFile = FALSE, targetCase = "foo"))),
    missingFolder = status(outcome(function() convertJsonResultsFileCase("bar.json", writeToFile = TRUE, targetCase = "camel")))
  )
  add("convert-case", strip_attrs(record))
})

# --- test-reEvaluateThresholds.R --------------------------------------------------------------------
local({
  folder <- attr(base, "folder")
  json_path <- list.files(folder, ".json", full.names = TRUE)
  re <- outcome(function() reEvaluateThresholds(jsonFilePath = json_path, outputFolder = folder, outputFile = "reEvaluated.txt"))
  add("re-evaluate", list(test = "Execute reEvaluateThresholds on Synthea/Eunomia", input = basename(json_path), expected = status(re), type = typeof(re$value), value = if (isTRUE(re$ok)) as.character(jsonlite::toJSON(re$value, digits = NA)) else NULL, written = read_file(file.path(folder, "reEvaluated.txt"))))
})

# --- test-writeJsonResultsTo.R ----------------------------------------------------------------------
local({
  folder <- attr(base, "folder")
  json_path <- list.files(folder, ".json", full.names = TRUE)
  csv_path <- file.path(scratch, "write-json-results.csv")
  csv <- outcome(function() writeJsonResultsToCsv(jsonPath = json_path, csvPath = csv_path))
  tables <- list()
  for (variant in list(list(name = "default", table = "dqd_json_results", single = NULL), list(name = "single", table = "dqd_single_table", single = TRUE), list(name = "separate", table = "dqd_separate_tables", single = FALSE))) {
    db <- fresh_db(paste0("json-to-table-", variant$name))
    arguments <- list(connectionDetails = sqlite_details(db), resultsDatabaseSchema = "main", jsonFilePath = json_path, writeTableName = variant$table)
    if (!is.null(variant$single)) arguments$singleTable <- variant$single
    run <- outcome(function() do.call(DataQualityDashboard::writeJsonResultsToTable, arguments))
    connection <- DatabaseConnector::connect(sqlite_details(db))
    names <- DatabaseConnector::getTableNames(connection = connection, databaseSchema = "main")
    DatabaseConnector::disconnect(connection)
    tables[[variant$name]] <- list(writeTableName = variant$table, singleTable = variant$single, expected = status(run), getTableNames = names, newTables = new_tables(db))
  }
  add("write-json-results", list(test = "Write JSON results / Write JSON results with singleTable parameter", input = basename(json_path), csv = list(expected = status(csv), written = read_file(csv_path)), tables = tables))
})

# --- test-writeDBResultsTo.R ------------------------------------------------------------------------
local({
  record <- run_case("write-db-results", "Write DB results to json", options = list(checkNames = "measurePersonCompleteness", writeToTable = TRUE, writeTableName = "dqd_db_results"), keep_db = TRUE)
  db <- attr(record, "db")
  folder <- attr(record, "folder")
  connection <- DatabaseConnector::connect(sqlite_details(db))
  exported <- outcome(function() DataQualityDashboard::writeDBResultsToJson(connection, "main", "main", "dqd_db_results", folder, "dq-result-test.json"))
  count <- outcome(function() DatabaseConnector::renderTranslateQuerySql(connection, sql = "select count(*) from @resultsDatabaseSchema.@writeTableName;", resultsDatabaseSchema = "main", writeTableName = "dqd_db_results", snakeCaseToCamelCase = TRUE))
  DatabaseConnector::disconnect(connection)
  exported_json <- read_json_file(file.path(folder, "dq-result-test.json"))
  record$export <- list(expected = status(exported), exists = file.exists(file.path(folder, "dq-result-test.json")), json = exported_json,
    count = count, lengthMatchesCount = isTRUE(count$ok) && length(jsonlite::fromJSON(file.path(folder, "dq-result-test.json"))$CheckResults) == count$value[1, 1])
  add("write-db-results", strip_attrs(record))
})

json(cases, file.path(out, "expected.json"))
json(list(upstreamCommit = pin, dqdVersion = as.character(packageVersion("DataQualityDashboard")), databaseConnectorVersion = as.character(packageVersion("DatabaseConnector")), sqlRenderVersion = as.character(packageVersion("SqlRender")), gibleed = "gibleed/sqlite/gibleed-copy.sqlite", gibleedMd5 = unname(tools::md5sum(gibleed))), file.path(out, "manifest.json"))
