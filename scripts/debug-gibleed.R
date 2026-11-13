library(DatabaseConnector)
sqlite_source <- "C:/Users/abora/tmp/dqd-bench/gibleed-source.sqlite"
sqlite_details <- createConnectionDetails(dbms = "sqlite", server = sqlite_source)
sqlite_db <- connect(sqlite_details)

data <- querySql(sqlite_db, "SELECT * FROM main.drug_exposure")
print(sapply(data, function(x) if(is.numeric(x)) max(x, na.rm=TRUE) else NA))
disconnect(sqlite_db)
