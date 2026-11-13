# Calls the actual upstream export functions and records both return values and bytes.
export_cases <- list()
source(file.path(upstream, "R", "writeResultsTo.R"), local = TRUE)
source(file.path(upstream, "R", "writeJsonResultsTo.R"), local = TRUE)
source(file.path(upstream, "R", "convertResultsCase.R"), local = TRUE)
source(file.path(upstream, "R", "reEvaluateThresholds.R"), local = TRUE)
source(file.path(upstream, "R", "writeDBResultsTo.R"), local = TRUE)
.readThresholdFile <- get(".readThresholdFile", ns)
.evaluateThresholds <- get(".evaluateThresholds", ns)
.summarizeResults <- get(".summarizeResults", ns)
export_rows <- rbind(result_row("edge", "FIELD", "PERSON", "person_id", 1, 10, 0.1), result_row("cdmTable", "TABLE", "PERSON", NA, 0, 10, 0, error = NA_character_))
export_rows$failed <- c(1L, 0L); export_rows$passed <- c(0L, 1L); export_rows$isError <- 0L; export_rows$notApplicable <- c(0L, 1L)
export_rows$notApplicableReason <- c(NA_character_, "Table PERSON is empty."); export_rows$thresholdValue <- c(0, NA_real_); export_rows$notesValue <- c("comma, \"quote\"", NA_character_)
json(list(Metadata = data.frame(CdmSourceName = "Oracle", cdmSourceAbbreviation = "ORA"), CheckResults = export_rows, Overview = .summarizeResults(export_rows)), file.path(out, "exports", "rows.json"))
folder <- tempfile("exports-"); dir.create(folder)
for (target in c("snake", "camel")) {
  name <- paste0("convert-", target)
  result <- outcome(function() convertJsonResultsFileCase(file.path(out, "exports", "rows.json"), TRUE, folder, paste0(name, ".json"), target))
  export_cases[[length(export_cases) + 1L]] <- list(name = name, expected = result, bytes = if (file.exists(file.path(folder, paste0(name, ".json")))) readLines(file.path(folder, paste0(name, ".json")), warn = FALSE) else NULL)
}
for (name in c("default-csv", "selected-columns", "semicolon-csv")) {
  path <- file.path(folder, paste0(name, ".csv"))
  cols <- if (name == "selected-columns") c("queryText", "error", "failed", "missingColumn") else NULL
  delim <- if (name == "semicolon-csv") ";" else ","
  value <- outcome(function() {
    json_path <- file.path(out, "exports", "rows.json")
    if (is.null(cols)) DataQualityDashboard::writeJsonResultsToCsv(json_path, path, delimiter = delim)
    else DataQualityDashboard::writeJsonResultsToCsv(json_path, path, columns = cols, delimiter = delim)
  })
  export_cases[[length(export_cases) + 1L]] <- list(name = name, expected = value, lines = if (file.exists(path)) readLines(path, warn = FALSE) else NULL)
}
source_json <- list(Metadata = data.frame(CdmSourceName = "Oracle", cdmSourceAbbreviation = "ORA"), CheckResults = export_rows, Overview = list(countTotal = 2L))
re_eval_file <- file.path(out, "exports", "re-evaluate.json"); writeLines(jsonlite::toJSON(source_json), re_eval_file)
re_eval_out <- file.path(folder, "re-evaluate-output"); dir.create(re_eval_out)
re_eval <- outcome(function() reEvaluateThresholds(re_eval_file, re_eval_out, "re-evaluate.json", tableCheckThresholdLoc = "default", fieldCheckThresholdLoc = "default", conceptCheckThresholdLoc = "default", cdmVersion = "5.3"))
re_eval_path <- file.path(re_eval_out, "re-evaluate.json")
re_eval_written <- if (file.exists(re_eval_path)) rawToChar(readBin(re_eval_path, "raw", n = file.info(re_eval_path)$size)) else NULL
re_eval_value_json <- if (isTRUE(re_eval$ok)) as.character(jsonlite::toJSON(re_eval$value, digits = NA)) else NULL
export_cases[[length(export_cases) + 1L]] <- list(name = "re-evaluate-thresholds", expected = re_eval, expectedValue = re_eval_value_json, expectedWritten = re_eval_written)
for (mode in c("sql-simple", "sql-incremental")) {
  sql_folder <- file.path(out, "exports", mode)
  if (dir.exists(sql_folder)) unlink(sql_folder, recursive = TRUE)
  dir.create(sql_folder, recursive = TRUE, showWarnings = FALSE)
  options <- list(cdmVersion = "5.3", checkNames = c("cdmTable", "isRequired"), checkLevels = c("TABLE", "FIELD"), tablesToExclude = character(), sqlOnly = TRUE, sqlOnlyUnionCount = 2, sqlOnlyIncrementalInsert = identical(mode, "sql-incremental"), outputFolder = sql_folder, outputFile = "unused.json", writeToTable = FALSE)
  value <- outcome(function() execute(options, sql_folder))
  files <- list.files(sql_folder, pattern = "\\.sql$", recursive = TRUE, full.names = TRUE)
  contents <- setNames(lapply(files, readLines, warn = FALSE), substring(files, nchar(sql_folder) + 2L))
  export_cases[[length(export_cases) + 1L]] <- list(name = mode, expected = value, files = contents)
}
unlink(folder, recursive = TRUE)
json(export_cases, file.path(out, "exports", "expected.json"))
