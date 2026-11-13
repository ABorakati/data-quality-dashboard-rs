//! Port of `org.ohdsi.sql.SqlTranslate` for the `sqlite`, `postgresql` and
//! `sql server` (identity) targets.

use crate::strings::{
    is_regex_space, java_hashmap_order, java_trim, lower_same_len, replace_all, safe_split, sub,
    tokenize_sql,
};
use regex::Regex;
use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

const PATTERNS_CSV: &str = include_str!("../../../resources/sqlrender/replacementPatterns.csv");

/// Targets this crate can translate to (patterns are loaded only for these).
const TARGET_DIALECTS: [&str; 2] = ["sqlite", "postgresql"];

const SESSION_ID_LENGTH: usize = 8;
const MAX_TABLE_NAME_LENGTH: usize = 63;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateError {
    /// The target dialect is not `sql server`, `sqlite` or `postgresql`.
    UnsupportedDialect(String),
}

impl fmt::Display for TranslateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TranslateError::UnsupportedDialect(dialect) => write!(
                f,
                "Don't know how to translate to {dialect}. Valid target dialects are sql server, sqlite, postgresql"
            ),
        }
    }
}

impl std::error::Error for TranslateError {}

/// A parsed element of a search pattern (`SqlTranslate.Block`).
#[derive(Debug)]
struct Block {
    text: String,
    is_variable: bool,
    /// Regex source of an `@@(regex)name` variable.
    reg_ex: Option<String>,
    /// `\A(?:regex)` with Java's CASE_INSENSITIVE | DOTALL | MULTILINE flags.
    re_prefix: Option<Regex>,
    /// `\A(?:regex)\z`, i.e. `Matcher.matches()`.
    re_full: Option<Regex>,
    /// Unanchored, for `Matcher.find()` loops.
    re_find: Option<Regex>,
}

/// One `pattern -> replacement` rule of the CSV.
#[derive(Debug)]
struct Rule {
    blocks: Vec<Block>,
    replacement: String,
}

/// SQL Server -> SQLite/PostgreSQL translator (SqlRender 1.19.7 semantics).
/// Patterns are parsed once; the value is `Send + Sync` and may be shared.
#[derive(Debug)]
pub struct Translator {
    rules: HashMap<&'static str, Vec<Rule>>,
}

impl Default for Translator {
    fn default() -> Self {
        Self::new()
    }
}

fn compile(regex: &str, wrap: impl Fn(&str) -> String) -> Regex {
    Regex::new(&format!("(?ims){}", wrap(regex)))
        .unwrap_or_else(|e| panic!("unsupported regex in replacement pattern `{regex}`: {e}"))
}

/// `SqlTranslate.parseSearchPattern`.
fn parse_search_pattern(pattern: &str) -> Vec<Block> {
    let lower = lower_same_len(pattern);
    let tokens = tokenize_sql(&lower);
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let token = &tokens[i];
        let mut text = token.text.to_owned();
        let mut is_variable = text.chars().count() > 2 && text.starts_with('@');
        let mut reg_ex = None;
        if token.text == "@@" && i + 2 < tokens.len() && tokens[i + 1].text == "(" {
            // A regex variable: rejoin the subsequent tokens.
            let mut escape = false;
            let mut nesting = 0;
            for j in i + 2..tokens.len() {
                if escape {
                    escape = false;
                } else if tokens[j].text == "\\" {
                    escape = true;
                } else if tokens[j].text == "(" {
                    nesting += 1;
                } else if tokens[j].text == ")" {
                    if nesting == 0 {
                        let name = tokens
                            .get(j + 1)
                            .unwrap_or_else(|| panic!("Error in search pattern: {pattern}"))
                            .text;
                        text = format!("@@{name}");
                        reg_ex = Some(
                            sub(
                                pattern,
                                tokens[i + 1].end as isize,
                                tokens[j].start as isize,
                            )
                            .to_owned(),
                        );
                        is_variable = true;
                        i = j + 1;
                        break;
                    }
                    nesting -= 1;
                }
            }
        }
        let (re_prefix, re_full, re_find) = match &reg_ex {
            Some(re) => (
                Some(compile(re, |r| format!(r"\A(?:{r})"))),
                Some(compile(re, |r| format!(r"\A(?:{r})\z"))),
                Some(compile(re, |r| r.to_owned())),
            ),
            None => (None, None, None),
        };
        blocks.push(Block {
            text,
            is_variable,
            reg_ex,
            re_prefix,
            re_full,
            re_find,
        });
        i += 1;
    }
    let bad = |b: &Block| b.is_variable && b.reg_ex.is_none();
    if blocks.is_empty() || bad(&blocks[0]) || bad(&blocks[blocks.len() - 1]) {
        panic!(
            "Error in search pattern: pattern cannot start or end with a non-regex variable: {pattern}"
        );
    }
    blocks
}

#[derive(Debug, Default)]
struct MatchedPattern {
    start: usize,
    end: usize,
    start_token: usize,
    /// Insertion-ordered `variableToValue` (a Java `HashMap`).
    variables: Vec<(String, String)>,
}

impl MatchedPattern {
    fn put(&mut self, key: &str, value: &str) {
        match self.variables.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_owned(),
            None => self.variables.push((key.to_owned(), value.to_owned())),
        }
    }
}

/// Java `Matcher.matches()` of the block's regex against the whole string.
fn regex_matches(block: &Block, s: &str) -> bool {
    block.re_full.as_ref().is_some_and(|re| re.is_match(s))
}

/// `string.replaceAll("\\s$", "")`: `$` also matches before a final line
/// terminator, so up to two trailing characters can be removed.
fn strip_trailing_space(s: &str) -> String {
    fn is_line_terminator(rest: &str) -> bool {
        matches!(
            rest,
            "\n" | "\r\n" | "\r" | "\u{85}" | "\u{2028}" | "\u{2029}"
        )
    }
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        let after = &s[i + c.len_utf8()..];
        if is_regex_space(c) && (after.is_empty() || is_line_terminator(after)) {
            continue;
        }
        out.push(c);
    }
    out
}

/// `SqlTranslate.matchesEnd`: start of the last match of `regex` that ends at
/// the end of `s`, or -1.
fn matches_end(block: &Block, s: &str) -> isize {
    let s = strip_trailing_space(s);
    let mut start = -1;
    if let Some(re) = &block.re_find {
        for m in re.find_iter(&s) {
            if m.end() == s.len() {
                start = m.start() as isize;
            }
        }
    }
    start
}

/// `SqlTranslate.search`: find the first occurrence of the pattern at or after
/// token `start_token`. Faithfully keeps Java's quirks (state such as the nest
/// stack and partial variable values survive an aborted attempt).
fn search(sql: &str, pattern: &[Block], start_token: usize) -> Option<MatchedPattern> {
    let lowercase_sql = lower_same_len(sql);
    let tokens = tokenize_sql(&lowercase_sql);
    let n = tokens.len() as isize;
    let size = pattern.len();
    let mut match_count = 0usize;
    let mut var_start: isize = 0;
    let mut nest_stack: Vec<&str> = Vec::new();
    let mut in_pattern_quote = false;
    let mut mp = MatchedPattern::default();
    let mut cursor = start_token as isize;
    while cursor < n {
        let token = tokens[cursor as usize];
        let current = &pattern[match_count];
        if current.is_variable {
            if current.reg_ex.is_some()
                && (match_count == size - 1 || pattern[match_count + 1].is_variable)
            {
                // Regex variable at end of pattern, or followed by another variable.
                let found = current
                    .re_prefix
                    .as_ref()
                    .and_then(|re| re.find(&sql[token.start..]));
                if let Some(m) = found {
                    if match_count == 0 {
                        mp.start = token.start;
                        mp.start_token = cursor as usize;
                    }
                    let matched_end = token.start + m.end();
                    mp.put(&current.text, &sql[token.start..matched_end]);
                    match_count += 1;
                    if match_count == size {
                        mp.end = matched_end;
                        return Some(mp);
                    } else if pattern[match_count].is_variable {
                        var_start = matched_end as isize;
                    }
                    // Fast forward cursor to after the matched text.
                    while cursor < n && (tokens[cursor as usize].start) < matched_end {
                        cursor += 1;
                    }
                    cursor -= 1;
                } else {
                    match_count = 0;
                }
            } else if nest_stack.is_empty()
                && match_count + 1 < size
                && token.text == pattern[match_count + 1].text
            {
                // Found the token after the variable.
                if current.reg_ex.is_some() && match_count == 0 {
                    // First element is a regex: find the last part before this token that matches.
                    let segment = sub(sql, var_start, token.start as isize);
                    let start = matches_end(current, segment);
                    if start != -1 {
                        let abs = start + var_start;
                        mp.put(&current.text, sub(sql, abs, token.start as isize));
                        mp.start = abs.max(0) as usize;
                        mp.start_token = cursor as usize;
                        match_count += 2;
                        if match_count == size {
                            mp.end = token.end;
                            return Some(mp);
                        } else if pattern[match_count].is_variable {
                            var_start = if cursor < n - 1 {
                                tokens[cursor as usize + 1].start as isize
                            } else {
                                -1
                            };
                        }
                        if token.text == "'" {
                            in_pattern_quote = !in_pattern_quote;
                        }
                    }
                } else if current.reg_ex.is_some()
                    && !regex_matches(current, sub(sql, var_start, token.start as isize))
                {
                    // Content didn't match regex.
                    match_count = 0;
                    cursor = mp.start_token as isize;
                } else {
                    // No regex or matched regex.
                    mp.put(&current.text, sub(sql, var_start, token.start as isize));
                    match_count += 2;
                    if match_count == size {
                        mp.end = token.end;
                        return Some(mp);
                    } else if pattern[match_count].is_variable {
                        var_start = if cursor < n - 1 {
                            tokens[cursor as usize + 1].start as isize
                        } else {
                            -1
                        };
                    }
                    if token.text == "'" {
                        in_pattern_quote = !in_pattern_quote;
                    }
                }
            } else if match_count != 0
                && nest_stack.is_empty()
                && !in_pattern_quote
                && (token.text == ";" || token.text == ")")
            {
                // Not allowed to span multiple statements or outside of nesting.
                match_count = 0;
                cursor = mp.start_token as isize;
            } else if let Some(&top) = nest_stack.last().filter(|t| **t == "\"" || **t == "'") {
                // Inside a quoted string.
                if token.text == top {
                    nest_stack.pop();
                }
            } else if token.text == "\"" || token.text == "'" {
                nest_stack.push(token.text);
            } else if !in_pattern_quote && token.text == "(" {
                nest_stack.push(token.text);
            } else if !in_pattern_quote
                && token.text == ")"
                && nest_stack.last().is_some_and(|t| *t == "(")
            {
                nest_stack.pop();
            }
        } else if token.text == current.text && (match_count != 0 || !token.in_quotes) {
            // The first part of the pattern cannot be within quotes.
            if match_count == 0 {
                mp.start = token.start;
                mp.start_token = cursor as usize;
            }
            match_count += 1;
            if match_count == size {
                mp.end = token.end;
                return Some(mp);
            } else if pattern[match_count].is_variable {
                var_start = if cursor < n - 1 {
                    tokens[cursor as usize + 1].start as isize
                } else {
                    -1
                };
            }
            if token.text == "'" || token.text == "\"" {
                in_pattern_quote = !in_pattern_quote;
            }
        } else if match_count != 0 {
            match_count = 0;
            cursor = mp.start_token as isize;
        }
        if match_count != 0 && cursor == n - 1 {
            // At the end of the SQL with the pattern unfinished: it cannot finish.
            match_count = 0;
            cursor = mp.start_token as isize;
        }
        cursor += 1;
    }
    None
}

/// `SqlTranslate.searchAndReplace`.
fn search_and_replace(mut sql: String, pattern: &[Block], replace_pattern: &str) -> String {
    let mut matched = search(&sql, pattern, 0);
    while let Some(mp) = matched {
        let mut replacement = replace_pattern.to_owned();
        let keys: Vec<&str> = mp.variables.iter().map(|(k, _)| k.as_str()).collect();
        for i in java_hashmap_order(&keys) {
            let (key, value) = &mp.variables[i];
            replacement = replace_all(&replacement, key, value);
        }
        sql = format!("{}{}{}", &sql[..mp.start], replacement, &sql[mp.end..]);
        let mut delta = 1;
        if tokenize_sql(&replacement).is_empty() {
            delta = 0;
        }
        // Special situation: the replacement starts with a variable whose content
        // starts like the search pattern; don't skip the first token.
        if delta > 0
            && replace_pattern.starts_with("@@")
            && java_trim(&lower_same_len(&replacement)).starts_with(pattern[0].text.as_str())
        {
            delta = 0;
        }
        matched = search(&sql, pattern, mp.start_token + delta);
    }
    sql
}

/// `replaceAll("(?m)^[ \t]*\r?\n", "")`: removes blank lines.
fn remove_blank_lines(s: &str) -> String {
    if !s.contains('\n') {
        return s.to_owned();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut copied = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if s.is_char_boundary(i) && is_line_start(s, i) {
            let mut j = i;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                j += 1;
            }
            if j < b.len() && b[j] == b'\r' {
                j += 1;
            }
            if j < b.len() && b[j] == b'\n' {
                out.push_str(&s[copied..i]);
                copied = j + 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[copied..]);
    out
}

/// Java `^` in MULTILINE mode: start of input, or after a line terminator that
/// is not at the end of input (and not between `\r` and `\n`).
fn is_line_start(s: &str, i: usize) -> bool {
    if i >= s.len() {
        return false;
    }
    if i == 0 {
        return true;
    }
    match s[..i].chars().next_back() {
        Some('\n' | '\u{85}' | '\u{2028}' | '\u{2029}') => true,
        Some('\r') => s.as_bytes()[i] != b'\n',
        _ => false,
    }
}

static GLOBAL_SESSION_ID: LazyLock<String> = LazyLock::new(|| {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut x = (nanos as u64) ^ (u64::from(std::process::id()) << 32) | 1;
    let chars: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut next = |modulus: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % modulus as u64) as usize
    };
    let mut id = String::new();
    id.push(chars[next(26)] as char); // first character has to be a letter
    for _ in 1..SESSION_ID_LENGTH {
        id.push(chars[next(chars.len())] as char);
    }
    id
});

fn global_session_id() -> &'static str {
    &GLOBAL_SESSION_ID
}

/// `SqlTranslate.line2columns`.
fn line_to_columns(line: &str) -> Vec<String> {
    safe_split(line, ',')
        .into_iter()
        .map(|column| {
            let column = if column.len() > 1 && column.starts_with('"') && column.ends_with('"') {
                &column[1..column.len() - 1]
            } else {
                column
            };
            column.replace("\\\"", "\"").replace("\\n", "\n")
        })
        .collect()
}

/// `BufferedReader.readLine` line splitting (`\n`, `\r\n`, `\r`).
fn read_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let b = text.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                start = i + 1;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                if b.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < b.len() {
        lines.push(&text[start..]);
    }
    lines
}

impl Translator {
    /// Parses the embedded `replacementPatterns.csv` (SqlRender 1.19.7) for the
    /// supported targets.
    ///
    /// # Panics
    /// If the embedded resource contains a pattern this port cannot handle
    /// (never the case for the shipped file; covered by tests).
    pub fn new() -> Translator {
        let mut rules: HashMap<&'static str, Vec<Rule>> = HashMap::new();
        for target in TARGET_DIALECTS {
            rules.insert(target, Vec::new());
        }
        for line in read_lines(PATTERNS_CSV).into_iter().skip(1) {
            if line.is_empty() {
                continue;
            }
            let row = line_to_columns(line);
            let Some(target) = TARGET_DIALECTS.iter().find(|t| **t == row[0]) else {
                continue;
            };
            let pattern = row[1].replace('@', "@@");
            let replacement = row[2].replace('@', "@@");
            rules
                .get_mut(target)
                .expect("target registered")
                .push(Rule {
                    blocks: parse_search_pattern(&pattern),
                    replacement,
                });
        }
        Translator { rules }
    }

    /// `SqlTranslate.translateSqlWithPath(sql, dialect, null, tempEmulationSchema, null)`.
    ///
    /// `sql server` returns `sql` unchanged. Only `sqlite` and `postgresql` are
    /// otherwise supported.
    pub fn translate(
        &self,
        sql: &str,
        target_dialect: &str,
        temp_emulation_schema: Option<&str>,
    ) -> Result<String, TranslateError> {
        if target_dialect == "sql server" {
            return Ok(sql.to_owned());
        }
        let rules = self
            .rules
            .get(target_dialect)
            .ok_or_else(|| TranslateError::UnsupportedDialect(target_dialect.to_owned()))?;
        let temp_prefix = match temp_emulation_schema {
            None => String::new(),
            Some(schema) => format!("{schema}."),
        };
        let mut sql = sql.to_owned();
        for rule in rules {
            let mut replacement = rule.replacement.clone();
            if replacement.contains("%session_id%") {
                replacement = replacement.replace("%session_id%", global_session_id());
            }
            if replacement.contains("%temp_prefix%") {
                replacement = replacement.replace("%temp_prefix%", &temp_prefix);
            }
            sql = remove_blank_lines(&search_and_replace(sql, &rule.blocks, &replacement));
        }
        Ok(remove_blank_lines(&sql))
    }

    /// `SqlTranslate.check(sql, dialect)`: warnings about over-long table names.
    pub fn check(&self, sql: &str, _target_dialect: &str) -> Vec<String> {
        static TEMP_NAME: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("#[0-9a-zA-Z_]+").expect("valid regex"));
        static TABLE_NAME: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(create|drop|truncate)[ \t\n\x0B\f\r]+table +[0-9a-zA-Z_]+")
                .expect("valid regex")
        });
        let (temp_name, table_name) = (&*TEMP_NAME, &*TABLE_NAME);

        let mut warnings = Vec::new();

        // Temp table names (Java: HashSet iteration order).
        let mut long_temp_names: Vec<String> = Vec::new();
        for m in temp_name.find_iter(sql) {
            if m.as_str().len() > MAX_TABLE_NAME_LENGTH - SESSION_ID_LENGTH - 1
                && !long_temp_names.iter().any(|n| n == m.as_str())
            {
                long_temp_names.push(m.as_str().to_owned());
            }
        }
        for i in java_hashmap_order(&long_temp_names) {
            warnings.push(format!(
                "Temp table name '{}' is too long. Temp table names should be shorter than {} characters to prevent some DMBSs from throwing an error.",
                long_temp_names[i],
                MAX_TABLE_NAME_LENGTH - SESSION_ID_LENGTH
            ));
        }

        // Normal table names.
        let lowercase = lower_same_len(sql);
        let mut long_names: Vec<String> = Vec::new();
        for m in table_name.find_iter(&lowercase) {
            let last_space = m.as_str().rfind(' ').unwrap_or(0);
            let name = &sql[m.start() + last_space..m.end()];
            if name.len() > MAX_TABLE_NAME_LENGTH
                && !long_temp_names.iter().any(|n| *n == format!("#{name}"))
                && !long_names.iter().any(|n| n == name)
            {
                long_names.push(name.to_owned());
            }
        }
        for i in java_hashmap_order(&long_names) {
            warnings.push(format!(
                "Table name '{}' is too long. Table names should be shorter than {} characters to prevent some DMBSs from throwing an error.",
                long_names[i], MAX_TABLE_NAME_LENGTH
            ));
        }
        warnings
    }
}
