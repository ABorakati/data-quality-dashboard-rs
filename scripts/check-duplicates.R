.libPaths(c(Sys.getenv("DQD_R_LIBRARY", "C:/Users/abora/tmp/dqd-r-oracle-library"), .libPaths()))
library(DatabaseConnector)
sqlite_source <- "C:/Users/abora/tmp/dqd-bench/gibleed-source.sqlite"
sqlite_details <- createConnectionDetails(dbms = "sqlite", server = sqlite_source)
sqlite_db <- connect(sqlite_details)

data <- querySql(sqlite_db, "SELECT drug_exposure_id FROM main.drug_exposure")
cat("Total rows:", nrow(data), "\n")
cat("Unique IDs:", length(unique(data[[1]])), "\n")
disconnect(sqlite_db)
