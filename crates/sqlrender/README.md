# dqd-sqlrender

A native Rust port of the parts of [OHDSI SqlRender](https://github.com/OHDSI/SqlRender) 1.19.7 that the
DataQualityDashboard port needs:

* parameter rendering (`render_sql`, `SqlRender.renderSql`), including `@parameter` substitution and
  `{condition} ? {then} : {else}` blocks;
* statement splitting (`split_sql`, `SqlSplit.splitSql`);
* translation from the SQL Server dialect to SQLite, PostgreSQL and DuckDB (`Translator`, `SqlTranslate`).

```rust
use dqd_sqlrender::{Translator, render_sql, split_sql};

let sql = render_sql("SELECT * FROM @schema.person", &["schema"], &["cdm"]);
let translator = Translator::new();
let sqlite = translator.translate(&sql, "sqlite", None)?;
let statements = split_sql(&sqlite);
```

The output is compared byte for byte with R SqlRender 1.19.7 in the repository's parity tests. The unmodified
`replacementPatterns.csv` and `supportedDialects.csv` from that release live in `data/`, with their
`NOTICE` and `provenance.json`; the crate is a derivative work of SqlRender's Java sources, which are
licensed under the Apache License 2.0.

Part of the [data-quality-dashboard-rs](https://github.com/ABorakati/data-quality-dashboard-rs) workspace.
