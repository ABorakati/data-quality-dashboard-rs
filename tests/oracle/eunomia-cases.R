# Execute pinned OHDSI code on one Eunomia dataset: a full executeDqChecks run on SQLite.
# Generalises gibleed-cases.R to any dataset name and CDM version.
# Usage: Rscript eunomia-cases.R UPSTREAM DB_SOURCE OUTPUT THREADS SOURCE_NAME CDM_VERSION
args <- commandArgs(TRUE)
if (length(args) != 6L) stop("Usage: Rscript eunomia-cases.R UPSTREAM DB_SOURCE OUTPUT THREADS SOURCE_NAME CDM_VERSION")
upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
db_source <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
out <- args[[3]]
threads <- as.integer(args[[4]])
source_name <- args[[5]]
cdm_version <- args[[6]]

dir.create(out, recursive = TRUE, showWarnings = FALSE)
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

# The run reads (never writes) this copy; the Rust test copies it again before its own run.
db_copy <- normalizePath(file.path(out, "cdm.sqlite"), winslash = "/", mustWork = FALSE)
file.copy(db_source, db_copy, overwrite = TRUE)
md5_before <- unname(tools::md5sum(db_copy))

run_options <- list(
  cdmDatabaseSchema = "main",
  resultsDatabaseSchema = "main",
  cdmSourceName = source_name,
  cdmVersion = cdm_version,
  numThreads = threads,
  writeToTable = FALSE,
  writeToCsv = TRUE,
  outputFolder = out,
  outputFile = "results.json",
  csvFile = "results.csv"
)
details <- DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = db_copy)
warnings <- character()
t <- system.time(withCallingHandlers(
  do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = details), run_options)),
  warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") }
))
if (!identical(unname(tools::md5sum(db_copy)), md5_before)) stop("executeDqChecks modified the input database")
results <- jsonlite::fromJSON(file.path(out, "results.json"))
check_results <- results$CheckResults
json(list(
  r_elapsed_seconds = t[["elapsed"]],
  total_checks = nrow(check_results),
  status_counts = list(
    passed = sum(check_results$passed == 1, na.rm = TRUE),
    failed = sum(check_results$failed == 1, na.rm = TRUE),
    not_applicable = sum(check_results$notApplicable == 1, na.rm = TRUE),
    error = sum(check_results$isError == 1, na.rm = TRUE)
  ),
  warnings = unique(warnings),
  source = basename(db_source),
  versions = list(
    DQD = as.character(packageVersion("DataQualityDashboard")),
    DatabaseConnector = as.character(packageVersion("DatabaseConnector")),
    SqlRender = as.character(packageVersion("SqlRender")),
    Eunomia = as.character(packageVersion("Eunomia")),
    R = R.version.string
  )
), file.path(out, "timing.json"))
run_options$outputFolder <- NULL
json(run_options, file.path(out, "options.json"))
