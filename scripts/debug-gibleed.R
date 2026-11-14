library(DatabaseConnector)
# DQD_GIBLEED_SQLITE: the GiBleed CDM 5.3 SQLite file (Eunomia's GiBleed_5.3.sqlite).
sqlite_source <- Sys.getenv("DQD_GIBLEED_SQLITE")
if (!nzchar(sqlite_source)) stop("Set DQD_GIBLEED_SQLITE to the path of the GiBleed CDM 5.3 SQLite file.")
sqlite_details <- createConnectionDetails(dbms = "sqlite", server = sqlite_source)
sqlite_db <- connect(sqlite_details)

data <- querySql(sqlite_db, "SELECT * FROM main.drug_exposure")
print(sapply(data, function(x) if(is.numeric(x)) max(x, na.rm=TRUE) else NA))
disconnect(sqlite_db)
