//! Native Rust port of OHDSI SqlRender 1.19.7: parameter rendering
//! (`SqlRender.renderSql`), statement splitting (`SqlSplit.splitSql`) and
//! SQL Server -> SQLite/PostgreSQL translation (`SqlTranslate`).

mod render;
mod split;
mod strings;
mod translate;

pub use render::{RenderError, render_sql, try_render_sql};
pub use split::{SplitError, split_sql, try_split_sql};
pub use translate::{TranslateError, Translator};

/// SqlRender 1.19.7's unmodified `supportedDialects.csv`; the canonical copy of the file for the
/// whole workspace (the `dqd dialects` command reads it from here).
pub const SUPPORTED_DIALECTS_CSV: &str = include_str!("../data/supportedDialects.csv");
