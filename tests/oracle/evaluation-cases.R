# Input construction only. Expected values come exclusively from pinned R functions.
result_row <- function(name = "edge", level = "FIELD", table = "PERSON", field = "VALUE", numerator = 0, denominator = 10, pct = numerator / denominator, error = NA_character_, concept = NA_character_, unit = NA_character_) {
  data.frame(numViolatedRows = numerator, pctViolatedRows = pct, numDenominatorRows = denominator, executionTime = NA_character_, queryText = "SELECT 'quoted', @literal;\n\t", checkName = name, checkLevel = level, checkDescription = "Description with \"quote\", comma and\nline", cdmTableName = table, cdmFieldName = field, conceptId = concept, unitConceptId = unit, sqlFile = "edge.sql", category = "Conformance", subcategory = "Relational", context = "Verification", warning = NA_character_, error = error, checkId = paste(name, table, field, sep = "_"), stringsAsFactors = FALSE)
}
controls_for <- function(rows, threshold = 10, notes = "edge notes") {
  tables <- unique(rows[rows$checkLevel == "TABLE", "cdmTableName", drop = FALSE])
  fields <- unique(rows[rows$checkLevel == "FIELD", c("cdmTableName", "cdmFieldName"), drop = FALSE])
  concepts <- unique(rows[rows$checkLevel == "CONCEPT", c("cdmTableName", "cdmFieldName", "conceptId", "unitConceptId"), drop = FALSE])
  names <- unique(rows$checkName)
  for (name in names) for (kind in c("tables", "fields", "concepts")) {
    df <- get(kind)
    df[[paste0(name, "Threshold")]] <- rep(threshold, nrow(df))
    df[[paste0(name, "Notes")]] <- rep(notes, nrow(df))
    assign(kind, df)
  }
  list(descriptions = data.frame(), tables = tables, fields = fields, concepts = concepts)
}
evaluation_cases <- list()
add_evaluation <- function(name, rows, controls = controls_for(rows), operation = "evaluate") {
  before <- rows
  result <- outcome(function() {
    if (operation == "evaluate") get(".evaluateThresholds", ns)(rows, controls$tables, controls$fields, controls$concepts) else get(".calculateNotApplicableStatus", ns)(rows)
  })
  summary <- if (result$ok) get(".summarizeResults", ns)(result$value) else NULL
  evaluation_cases[[length(evaluation_cases) + 1L]] <<- list(name = name, operation = operation, rows = before, catalog = list(tables = controls$tables, fields = controls$fields, concepts = controls$concepts, headers = list(tables = names(controls$tables), fields = names(controls$fields), concepts = names(controls$concepts))), expected = result, summary = summary)
}
for (threshold in c(NA_real_, 0, -1, 10, 100)) for (pct in c(0, 0.1 - .Machine$double.eps, 0.1, 0.1 + .Machine$double.eps, 1)) {
  rows <- result_row(numerator = if (pct == 0) 0 else 1, pct = pct)
  add_evaluation(paste("threshold", threshold, sprintf("%.17g", pct)), rows, controls_for(rows, threshold))
}
rows <- result_row(numerator = 1)
controls <- controls_for(rows)
controls$fields <- controls$fields[FALSE, , drop = FALSE]
add_evaluation("threshold-zero-matches", rows, controls)
controls$fields$edgeThreshold <- NULL
add_evaluation("threshold-no-header-empty", rows, controls)
for (name in c("cdmTable", "cdmField", "edge")) for (error in c("no such table: PERSON", "no such column: VALUE", "Invalid object name PERSON", "ORA-00942: table or view does not exist", "syntax error", "warning: not found")) {
  rows <- result_row(name, if (name == "cdmTable") "TABLE" else "FIELD", error = error)
  add_evaluation(paste(name, error), rows)
}
prerequisites <- rbind(result_row("cdmTable", "TABLE", field = NA_character_), result_row("cdmField"), result_row("measureValueCompleteness"))
for (mask in 0:7) {
  included <- as.logical(intToBits(mask)[1:3])
  rows <- rbind(prerequisites[included, , drop = FALSE], result_row(numerator = 1))
  add_evaluation(paste0("gating-", mask), rows)
}
for (state in c("clean", "missing-table", "missing-field", "empty-table", "unpopulated-field", "query-error", "error-and-missing-table", "error-and-empty-field")) {
  rows <- rbind(prerequisites, result_row("isRequired", numerator = 1), result_row("measurePersonCompleteness", "TABLE", field = NA_character_, numerator = 1), result_row("edge", "CONCEPT", concept = "123", denominator = 0, pct = 0), result_row("edge", "CONCEPT", concept = "123", unit = "456", denominator = 0, pct = 0))
  if (state %in% c("missing-table", "error-and-missing-table")) rows$error[1] <- "no such table: PERSON"
  if (state == "missing-field") rows$error[2] <- "no such column: VALUE"
  if (state == "empty-table") { rows$numDenominatorRows[3] <- 0; rows$numViolatedRows[3] <- 0; rows$pctViolatedRows[3] <- 0 }
  if (state %in% c("unpopulated-field", "error-and-empty-field")) { rows$numViolatedRows[3] <- 10; rows$pctViolatedRows[3] <- 1 }
  if (grepl("error", state)) rows$error[4] <- "syntax error"
  add_evaluation(state, rows)
}
# Scalar assignment failures, absence of Notes, and unguarded 1:nrow behavior.
rows <- result_row(numerator = 1)
c <- controls_for(rows); c$fields <- c$fields[FALSE, , drop = FALSE]
c <- controls_for(rows); c$fields <- rbind(c$fields, c$fields)
add_evaluation("threshold-two-matches", rows, c)
c <- controls_for(rows); c$fields$edgeNotes <- NULL
add_evaluation("missing-notes-column", rows, c)
c <- controls_for(rows); c$fields$edgeThreshold <- NULL
add_evaluation("missing-threshold-column", rows, c)
add_evaluation("evaluate-empty-rows", rows[FALSE, , drop = FALSE], controls_for(rows))
# Concept selectors: scalar, comma-list string, and concept+unit.
for (selector in c("123", "123,456")) {
  rows <- result_row("edge", "CONCEPT", concept = selector, numerator = 1, pct = 0.1)
  add_evaluation(paste0("concept-selector-", selector), rows)
}
rows <- result_row("edge", "CONCEPT", concept = "123", unit = "456", numerator = 2, pct = 0.2)
add_evaluation("concept-unit-selector", rows)
# Direct applicability receives already evaluated rows, so duplicate join behavior
# is independent of threshold assignment's separate scalar cardinality constraint.
base <- get(".evaluateThresholds", ns)(prerequisites, controls_for(prerequisites)$tables, controls_for(prerequisites)$fields, controls_for(prerequisites)$concepts)
add_evaluation("duplicate-missing-field", rbind(base, base[2, ]), operation = "applicability")
add_evaluation("duplicate-empty-field", rbind(base, base[3, ]), operation = "applicability")
add_evaluation("duplicate-field-cross-product", rbind(base, base[2:3, ]), operation = "applicability")
add_evaluation("duplicate-table-error", rbind(base, base[1, ]), operation = "applicability")
different <- base[3, ]; different$numDenominatorRows <- 0
add_evaluation("conflicting-table-denominator-error", rbind(base, different), operation = "applicability")
null_keys <- base; null_keys$cdmFieldName[2:3] <- NA_character_
null_edge <- null_keys[2, ]; null_edge$checkName <- "isRequired"; null_edge$failed <- 1
add_evaluation("null-field-keys-match", rbind(null_keys, null_edge), operation = "applicability")
for (missing in c(FALSE, TRUE)) for (empty in c(FALSE, TRUE)) {
  era <- result_row("measureConditionEraCompleteness", "TABLE", table = "CONDITION_ERA", field = NA_character_)
  rows <- rbind(prerequisites, era)
  add_evaluation(paste("era-absent-both", missing, empty), rows)
  # One prerequisite exists, the other yields logical(0). TRUE || logical(0)
  # short-circuits, FALSE || logical(0) raises an upstream error.
  condition <- result_row("cdmTable", "TABLE", table = "CONDITION_OCCURRENCE", field = NA_character_, error = if (missing) "no such table" else NA_character_)
  add_evaluation(paste0("era-only-table-", missing), rbind(rows, condition))
  condition <- result_row("measureValueCompleteness", table = "CONDITION_OCCURRENCE", denominator = if (empty) 0 else 10, pct = 0)
  add_evaluation(paste0("era-only-completeness-", empty), rbind(rows, condition))
  condition_table <- result_row("cdmTable", "TABLE", table = "CONDITION_OCCURRENCE", field = NA_character_, error = if (missing) "no such table" else NA_character_)
  add_evaluation(paste("era-present-both", missing, empty), rbind(rows, condition_table, condition))
}
json(evaluation_cases, file.path(out, "evaluation.json"))
summary_cases <- list()
add_summary <- function(name, rows) {
  summary_cases[[length(summary_cases) + 1L]] <<- list(name = name, rows = rows, expected = get(".summarizeResults", ns)(rows))
}
add_summary("empty", base[FALSE, ])
for (category in c("Conformance", "Completeness", "Plausibility", NA_character_)) for (failed in c(0L, 1L, NA_integer_)) for (passed in c(0L, 1L, NA_integer_)) {
  row <- base[1, ]; row$category <- category; row$failed <- failed; row$passed <- passed
  add_summary(paste("phantom-na", category, failed, passed), row)
}
for (n in c(32L, 64L, 128L)) for (failures in c(1L, 3L, 5L, 7L)) {
  rows <- base[rep(1L, n), ]; rows$failed <- c(rep(1L, failures), rep(0L, n - failures)); rows$passed <- 1L - rows$failed
  add_summary(paste("ties-to-even", n, failures), rows)
}
row <- base[1, ]; row$isError <- 1L; row$error <- "syntax error"; row$failed <- 0L; row$passed <- 0L
add_summary("error-counts-overall-passed", row)
row$notApplicable <- 1L
add_summary("error-and-na-counts-overall-passed", row)
json(summary_cases, file.path(out, "summary.json"))
