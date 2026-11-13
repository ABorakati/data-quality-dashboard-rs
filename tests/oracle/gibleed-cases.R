# Execute pinned OHDSI code.
args <- commandArgs(TRUE)
if (length(args) != 4L) stop("Usage: Rscript gibleed-cases.R UPSTREAM DB_SOURCE OUTPUT THREADS")
upstream <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
db_source <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
out <- args[[3]]
threads <- as.integer(args[[4]])

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

json <- function(value, path, default = FALSE) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  text <- if (default) jsonlite::toJSON(value) else jsonlite::toJSON(value, auto_unbox = TRUE, na = "null", null = "null", digits = 17, dataframe = "rows")
  writeLines(text, path, useBytes = TRUE)
}



db_copy <- normalizePath(file.path(out, "gibleed-copy.sqlite"), winslash = "/", mustWork = FALSE)
file.copy(db_source, db_copy, overwrite = TRUE)

details <- do.call(DatabaseConnector::createConnectionDetails, list(dbms = "sqlite", server = db_copy))
t <- system.time({
  DataQualityDashboard::executeDqChecks(
    connectionDetails = details,
    cdmDatabaseSchema = "main",
    resultsDatabaseSchema = "main",
    cdmSourceName = "GiBleed",
    cdmVersion = "5.3",
    numThreads = threads,
    writeToTable = FALSE,
    writeToCsv = TRUE,
    outputFolder = out,
    outputFile = "results.json",
    csvFile = "results.csv"
  )
})
run_opts_clean <- list(
  cdmDatabaseSchema = "main",
  resultsDatabaseSchema = "main",
  cdmSourceName = "GiBleed",
  cdmVersion = "5.3",
  numThreads = threads,
  writeToTable = FALSE,
  writeToCsv = TRUE,
  outputFolder = out,
  outputFile = "results.json",
  csvFile = "results.csv"
)

res_file <- file.path(out, "results.json")
if (file.exists(res_file)) {
  results <- jsonlite::fromJSON(res_file)
  
  check_results <- results$CheckResults
  
  timing <- list(
    r_elapsed_seconds = t[["elapsed"]],
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
      R = R.version.string
    )
  )
  json(timing, file.path(out, "timing.json"))
  
  # For options.json we only want the arguments to executeDqChecks, minus connectionDetails

  json(run_opts_clean, file.path(out, "options.json"))
} else {
  stop("results.json not found")
}
