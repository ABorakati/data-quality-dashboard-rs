args <- commandArgs(TRUE)
.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
x <- list(startTimestamp = as.POSIXct("2026-10-07 12:34:56.123456", tz = "UTC"), executionTime = "1 secs", executionTimeSeconds = 1.23456789,
  CheckResults = data.frame(checkId = c("a", "b"), numViolatedRows = c(1, NA), pctViolatedRows = c(0.123456789, NA), allNull = c(NA_real_, NA_real_), mixed = c("NA", NA_character_), logicalFlag = c(TRUE, NA), tiny = c(0.0000123456789, 123456789.123456), error = c(NA_character_, "quoted \"error\"\nline")),
  Metadata = data.frame(cdmSourceName = "Oracle", empty = NA_character_),
  Overview = list(countTotal = 2L, percentPassed = 50.123456, unknown = NA_real_, nan = NaN, infinity = Inf),
  scalarNull = NULL, scalarMissing = NA_character_, emptyNumeric = numeric())
cat("DEFAULT\n", jsonlite::toJSON(x), "\n", sep = "")
cat("ROUNDTRIP\n", jsonlite::toJSON(jsonlite::fromJSON(jsonlite::toJSON(x))), "\n", sep = "")
if (length(args)) {
  e <- new.env(parent = globalenv())
  sys.source(file.path(args[[1]], "R", "writeResultsTo.R"), e)
  sys.source(file.path(args[[1]], "R", "convertResultsCase.R"), e)
  folder <- tempfile("json-observation-"); dir.create(folder)
  input <- file.path(folder, "input.json"); writeLines(jsonlite::toJSON(x), input)
  snake <- e$convertJsonResultsFileCase(input, TRUE, folder, "snake.json", "snake")
  cat("SNAKE\n", readLines(file.path(folder, "snake.json")), "\n", sep = "")
  camel <- e$convertJsonResultsFileCase(file.path(folder, "snake.json"), TRUE, folder, "camel.json", "camel")
  cat("CAMEL\n", readLines(file.path(folder, "camel.json")), "\n", sep = "")
  cat("TYPES\n"); str(jsonlite::fromJSON(jsonlite::toJSON(x)))
  unlink(folder, recursive = TRUE)
}
