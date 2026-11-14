# Downloads the PostgreSQL JDBC driver that DatabaseConnector uses for the R oracle. Only the oracle needs it.
# DQD_R_LIBRARY: optional library holding the pinned DataQualityDashboard. DQD_JDBC_DIR: where to put the driver (required).
r_library <- Sys.getenv("DQD_R_LIBRARY")
if (nzchar(r_library)) .libPaths(c(r_library, .libPaths()))
jdbc_dir <- Sys.getenv("DQD_JDBC_DIR")
if (!nzchar(jdbc_dir)) stop("Set DQD_JDBC_DIR to the folder for the JDBC drivers.")
library(DatabaseConnector)
DatabaseConnector::downloadJdbcDrivers("postgresql", jdbc_dir)
