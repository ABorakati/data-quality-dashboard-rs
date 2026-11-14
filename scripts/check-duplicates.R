r_library <- Sys.getenv("DQD_R_LIBRARY")
if (nzchar(r_library)) .libPaths(c(r_library, .libPaths()))
library(DatabaseConnector)
# DQD_GIBLEED_SQLITE: the GiBleed CDM 5.3 SQLite file (Eunomia's GiBleed_5.3.sqlite).
sqlite_source <- Sys.getenv("DQD_GIBLEED_SQLITE")
if (!nzchar(sqlite_source)) stop("Set DQD_GIBLEED_SQLITE to the path of the GiBleed CDM 5.3 SQLite file.")
sqlite_details <- createConnectionDetails(dbms = "sqlite", server = sqlite_source)
sqlite_db <- connect(sqlite_details)

data <- querySql(sqlite_db, "SELECT drug_exposure_id FROM main.drug_exposure")
cat("Total rows:", nrow(data), "\n")
cat("Unique IDs:", length(unique(data[[1]])), "\n")
disconnect(sqlite_db)
