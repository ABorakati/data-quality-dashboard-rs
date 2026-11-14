# Runs the pinned upstream executeDqChecks on every fuzz case under DIR, in one R process.
# Each CASE gets CASE/r/results.json (when the run succeeds) and CASE/r/outcome.json.
# Usage: Rscript --vanilla fuzz/run_r_batch.R DIR
args <- commandArgs(TRUE)
if (length(args) != 1L) stop("Usage: Rscript run_r_batch.R DIR")
.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
suppressPackageStartupMessages(library(DataQualityDashboard))
if (as.character(packageVersion("DataQualityDashboard")) != "2.9.0") stop("Wrong installed DQD version")
cases <- sort(list.dirs(args[[1]], recursive = FALSE))
cases <- cases[file.exists(file.path(cases, "options.json"))]
for (case in cases) {
  out <- file.path(case, "r")
  if (file.exists(file.path(out, "outcome.json"))) next # resumable batches
  options <- jsonlite::fromJSON(file.path(case, "options.json"), simplifyVector = TRUE)
  details <- DatabaseConnector::createConnectionDetails(dbms = "sqlite", server = normalizePath(file.path(case, "cdm.sqlite"), winslash = "/"))
  warnings <- character()
  started <- Sys.time()
  value <- tryCatch(
    withCallingHandlers(
      { do.call(DataQualityDashboard::executeDqChecks, c(list(connectionDetails = details, outputFolder = out), options)); NULL },
      warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") }
    ),
    error = function(e) conditionMessage(e)
  )
  dir.create(out, showWarnings = FALSE, recursive = TRUE)
  outcome <- list(ok = is.null(value), error = value, warnings = unique(warnings), seconds = as.numeric(difftime(Sys.time(), started, units = "secs")))
  writeLines(jsonlite::toJSON(outcome, auto_unbox = TRUE, null = "null"), file.path(out, "outcome.json"), useBytes = TRUE)
  cat(basename(case), if (is.null(value)) "ok" else paste("error:", value), sprintf("%.0fs", outcome$seconds), "\n")
}
