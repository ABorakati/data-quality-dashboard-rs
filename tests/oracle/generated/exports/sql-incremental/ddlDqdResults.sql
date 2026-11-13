--DDL to create dqdashboard_results table.
DROP TABLE IF EXISTS main.dqdashboard_results;
CREATE TABLE main.dqdashboard_results
(
  num_violated_rows     BIGINT,
  pct_violated_rows     REAL,
  num_denominator_rows  BIGINT,
  execution_time        TEXT,
  query_text            TEXT,
  check_name            TEXT,
  check_level           TEXT,
  check_description     TEXT,
  cdm_table_name        TEXT,
  cdm_field_name        TEXT,
  concept_id            TEXT,
  unit_concept_id       TEXT,
  sql_file              TEXT,
  category              TEXT,
  subcategory           TEXT,
  context               TEXT,
  warning               TEXT,
  error                 TEXT,
  checkid               TEXT,
  is_error              INTEGER,
  not_applicable        INTEGER,
  failed                INTEGER,
  passed                INTEGER,
  not_applicable_reason TEXT,
  threshold_value       INTEGER,
  notes_value           TEXT
);

