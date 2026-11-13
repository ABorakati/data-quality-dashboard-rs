//! Port of `org.ohdsi.sql.SqlSplit.splitSql`.

use crate::strings::{lower_same_len, tokenize_sql};
use std::fmt;

/// Java's `Stack.pop()` on an empty stack (an `end` token with no matching
/// `begin`/`case`) throws `EmptyStackException`; this is the same condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitError {
    /// Byte offset (in the input SQL) of the offending `end` token.
    pub position: usize,
}

impl fmt::Display for SplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cannot split SQL: 'end' at offset {} has no matching 'begin' or 'case' (java.util.EmptyStackException)",
            self.position
        )
    }
}

impl std::error::Error for SplitError {}

/// `SqlSplit.splitSql`, returning an error where Java throws.
pub fn try_split_sql(sql: &str) -> Result<Vec<String>, SplitError> {
    let mut parts: Vec<String> = Vec::new();
    let lower = lower_same_len(sql);
    let tokens = tokenize_sql(&lower);
    let mut nest_stack: Vec<&str> = Vec::new();
    let mut last_pop = "";
    let mut start = 0usize;
    let mut quote = false;
    let mut bracket = false;
    let mut quote_text = "";
    let mut cursor = 0usize;
    while cursor < tokens.len() {
        let token = &tokens[cursor];
        if quote {
            if token.text == quote_text {
                quote = false;
            }
        } else if bracket {
            if token.text == "]" {
                bracket = false;
            }
        } else if token.text == "'" || token.text == "\"" {
            quote = true;
            quote_text = token.text;
        } else if token.text == "[" {
            bracket = true;
        } else if token.text == "begin" || token.text == "case" {
            nest_stack.push(token.text);
        } else if token.text == "end"
            && (cursor == tokens.len() - 1 || tokens[cursor + 1].text != "if")
        {
            last_pop = nest_stack.pop().ok_or(SplitError {
                position: token.start,
            })?;
        } else if nest_stack.is_empty() && token.text == ";" {
            if cursor == 0 || (tokens[cursor - 1].text == "end" && last_pop == "begin") {
                // must have ; after end belonging to a begin
                parts.push(sql[tokens[start].start..token.end].to_owned());
            } else {
                // cannot have ; after anything but end
                parts.push(sql[tokens[start].start..token.end - 1].to_owned());
            }
            start = cursor + 1;
        }
        cursor += 1;
    }
    if start < cursor {
        parts.push(sql[tokens[start].start..tokens[cursor - 1].end].to_owned());
    }
    Ok(parts)
}

/// `SqlSplit.splitSql`: split a string with several SQL statements into
/// statements. Trailing `;` are removed except after `END` closing a `BEGIN`.
///
/// # Panics
/// Where Java throws (`end` token without matching `begin`/`case`), this
/// panics with the [`SplitError`] message. Use [`try_split_sql`] to get an error.
pub fn split_sql(sql: &str) -> Vec<String> {
    match try_split_sql(sql) {
        Ok(parts) => parts,
        Err(error) => panic!("{error}"),
    }
}
