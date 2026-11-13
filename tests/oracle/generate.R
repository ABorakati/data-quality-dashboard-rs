# Execute pinned OHDSI code. Instrumentation observes inputs or bypasses database work
# only for catalog enumeration. Evaluation, SQL, exports, and database runs are real.
args <- commandArgs(TRUE)
if (length(args) != 2L) stop("Usage: Rscript generate.R UPSTREAM OUTPUT")
upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
out <- args[[2]]
dir.create(out, recursive = TRUE, showWarnings = FALSE)
.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
suppressPackageStartupMessages(library(DataQualityDashboard))
ns <- asNamespace("DataQualityDashboard")
pin <- "b9d2a9b4a4736f7ea81415d974c72119b6f0bd11"
actual <- system2("git", c("-C", shQuote(upstream), "rev-parse", "HEAD"), stdout = TRUE)
if (!identical(actual, pin)) stop("Upstream HEAD differs from the parity pin: ", paste(actual, collapse = " "))
if (as.character(packageVersion("DataQualityDashboard")) != "2.9.0") stop("Wrong installed DQD version")
# Ensure installed R files and assets come from the supplied tree, not another package.
for (relative in list.files(file.path(upstream, "inst"), recursive = TRUE)) {
  installed <- system.file(relative, package = "DataQualityDashboard")
  if (!nzchar(installed) || unname(tools::md5sum(installed)) != unname(tools::md5sum(file.path(upstream, "inst", relative)))) stop("Installed asset differs: ", relative)
}
json <- function(value, path, default = FALSE) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  text <- if (default) jsonlite::toJSON(value) else jsonlite::toJSON(value, auto_unbox = TRUE, na = "null", null = "null", digits = 17, dataframe = "rows")
  writeLines(text, path, useBytes = TRUE)
}
replace <- function(env, name, value) {
  old <- get(name, env, inherits = FALSE)
  unlockBinding(name, env); assign(name, value, env); lockBinding(name, env)
  old
}
outcome <- function(f) {
  warnings <- character()
  value <- tryCatch(withCallingHandlers(f(), warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") }), error = function(e) structure(list(message = conditionMessage(e)), class = "oracle_error"))
  if (inherits(value, "oracle_error")) list(ok = FALSE, error = value$message, warnings = warnings) else list(ok = TRUE, value = value, warnings = warnings)
}
base_options <- list(cdmDatabaseSchema = "main", resultsDatabaseSchema = "main", vocabDatabaseSchema = "main", cdmSourceName = "Parity", writeToTable = FALSE, outputFile = "results.json", numThreads = 1)
details <- DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = tempfile(fileext = ".sqlite"))
execute <- function(options, folder, connectionDetails = details) {
  options$dbms <- NULL; options$connection <- NULL
  run_options <- modifyList(base_options, options)
  run_options$outputFolder <- folder
  arguments <- c(list(connectionDetails = connectionDetails), run_options)
  if (anyDuplicated(names(arguments))) stop("duplicate executeDqChecks argument names: ", paste(names(arguments)[duplicated(names(arguments))], collapse = ", "))
  do.call(DataQualityDashboard::executeDqChecks, arguments)
}
# Inject an observer after the original function's filter expression. The filter
# expression, schema merge, uppercase conversion, ordering and defaults stay upstream.
run_original <- get(".runCheck", ns)
run_observed <- run_original
body_parts <- as.list(body(run_original))
filter_pos <- which(vapply(body_parts, function(x) identical(x, quote(checks <- eval(parse(text = filterExpression)))), logical(1)))
if (length(filter_pos) != 1L) stop("Upstream .runCheck observer anchor changed")
body(run_observed) <- as.call(append(body_parts, list(quote(.parityCapture(checks, checkDescription)), quote(return(data.frame()))), after = filter_pos))
observer_env <- new.env(parent = ns)
captured <- list()
observer_env$.parityCapture <- function(checks, description) {
  if (!nrow(checks)) return(invisible(NULL))
  records <- lapply(seq_len(nrow(checks)), function(i) {
    check <- unlist(checks[i, , drop = FALSE], use.names = TRUE)
    get(".recordResult", ns)(check = check, checkDescription = description, sql = "")
  })
  for (i in seq_len(nrow(checks))) captured[[length(captured) + 1L]] <<- list(description = as.list(description[1, , drop = FALSE]), parameters = as.list(checks[i, , drop = FALSE]), record = as.list(records[[i]][1, , drop = FALSE]))
}
environment(run_observed) <- observer_env
old_ddl <- get(".writeDDL", ns)
replace(ns, ".runCheck", run_observed)
replace(ns, ".writeDDL", function(...) invisible(NULL))
versions <- c("5.2", "5.3", "5.4", "5.5")
variants <- list(
  default = list(), all_tables = list(tablesToExclude = character()),
  field_fatal = list(checkLevels = "FIELD", checkSeverity = "fatal"),
  table_convention = list(checkLevels = "TABLE", checkSeverity = "convention"),
  concept_characterization = list(checkLevels = "CONCEPT", checkSeverity = "characterization"),
  named = list(checkNames = c("cdmField", "isRequired", "plausibleGenderUseDescendants")),
  lowercase_exclusions = list(tablesToExclude = c("person", "measurement", "concept")),
  no_checks = list(checkNames = "unknownCheck"), no_severity = list(checkSeverity = character()))
all_catalogs <- list()
for (version in versions) {
  catalog <- DataQualityDashboard::listDqChecks(cdmVersion = version)
  all_catalogs[[version]] <- catalog
  json(list(version = version, catalog = catalog), file.path(out, "catalog", paste0(version, ".json")))
  for (name in names(variants)) {
    captured <- list()
    options <- c(list(cdmVersion = version), variants[[name]])
    folder <- tempfile("enum-"); dir.create(folder)
    result <- outcome(function() execute(c(options, list(sqlOnly = TRUE)), folder))
    json(list(name = name, options = options, ok = result$ok, error = if (!result$ok) result$error else NULL, checks = captured), file.path(out, "enumeration", paste0(version, "-", name, ".json")))
    unlink(folder, recursive = TRUE)
  }
}
replace(ns, ".runCheck", run_original)
replace(ns, ".writeDDL", old_ddl)
# Render one genuine control row per description, every CDM version and canonical dialect.
dialect_table <- read.csv(system.file("csv", "supportedDialects.csv", package = "SqlRender"), stringsAsFactors = FALSE)
dialects <- dialect_table[[1]]
if (length(dialects) != 16L) stop("SqlRender supported dialect set changed")
render_cases <- list()
for (version in "5.3") {
  enumeration <- jsonlite::fromJSON(file.path(out, "enumeration", paste0(version, "-all_tables.json")), simplifyVector = FALSE)$checks
  by_type <- split(enumeration, vapply(enumeration, function(x) paste(x$description$checkLevel, x$description$checkName), ""))
  for (group in by_type) {
    check <- group[[1]]
    parameters <- lapply(check$parameters, function(v) if (is.null(v)) NA else as.character(v))
    for (cohort in c(FALSE, TRUE)) for (dialect in dialects) {
      params <- c(list(cdmDatabaseSchema = "main", vocabDatabaseSchema = "main", cohortDatabaseSchema = "main", cohortTableName = "cohort", cohortDefinitionId = if (cohort) c(7L, 8L) else numeric(), cohort = cohort), parameters)
      template <- readChar(system.file("sql", "sql_server", check$description$sqlFile, package = "DataQualityDashboard"), nchars = 1000000, useBytes = TRUE)
      rendered <- outcome(function() do.call(SqlRender::render, c(list(sql = template, warnOnMissingParameters = FALSE), params)))
      translated <- if (rendered$ok) outcome(function() if (dialect == "sql server") rendered$value else SqlRender::translate(rendered$value, targetDialect = dialect)) else rendered
      split <- if (translated$ok) outcome(function() as.list(SqlRender::splitSql(translated$value))) else translated
      render_cases[[length(render_cases) + 1L]] <- list(version = version, description = check$description, cohort = cohort, dialect = dialect, parameters = params, template = template, rendered = rendered, translated = translated, statements = split)
    }
  }
}
json(render_cases, file.path(out, "render.json"))
script_arg <- grep("^--file=", commandArgs(), value = TRUE)
script_dir <- dirname(normalizePath(sub("^--file=", "", script_arg[[1]]), winslash = "/", mustWork = TRUE))
source(file.path(script_dir, "evaluation-cases.R"), local = TRUE)
source(file.path(script_dir, "export-cases.R"), local = TRUE)
source(file.path(script_dir, "database-cases.R"), local = TRUE)
source(file.path(script_dir, "csv-default-case.R"), local = TRUE)
json(list(upstreamCommit = pin, dqdVersion = as.character(packageVersion("DataQualityDashboard")), rVersion = as.character(getRversion()), sqlRenderVersion = as.character(packageVersion("SqlRender")), databaseConnectorVersion = as.character(packageVersion("DatabaseConnector")), dialects = dialects, cdmVersions = versions, normalization = c("executionTime", "executionTimeSeconds", "startTimestamp", "endTimestamp", "absolute output paths"), databaseBackend = "SQLite, real DatabaseConnector/RSQLite queries"), file.path(out, "manifest.json"))
