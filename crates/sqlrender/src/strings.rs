//! Port of the parts of `org.ohdsi.sql.StringUtils` (plus a few Java string
//! semantics) needed by render, split and translate.
//!
//! Offsets are byte offsets into UTF-8 strings. Every lowercase conversion used
//! for position bookkeeping goes through [`lower_same_len`], which keeps byte
//! offsets of the original and lowercase strings identical (Java relies on the
//! same property, in UTF-16 units).

/// `Character.isWhitespace`.
pub(crate) fn is_java_whitespace(c: char) -> bool {
    match c {
        '\u{00A0}' | '\u{2007}' | '\u{202F}' => false,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{1C}'..='\u{1F}' => true,
        c => c.is_whitespace(),
    }
}

/// `Character.isLetterOrDigit`.
pub(crate) fn is_letter_or_digit(c: char) -> bool {
    c.is_alphanumeric()
}

/// Regex `\s` in Java (ASCII only): `[ \t\n\x0B\f\r]`.
pub(crate) fn is_regex_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

/// `String.trim()`: strips every char `<= ' '`.
pub(crate) fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `String.toLowerCase()` that never changes the byte length of any character,
/// so offsets computed on the result are valid on the input.
pub(crate) fn lower_same_len(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mut lower = c.to_lowercase();
        match (lower.next(), lower.next()) {
            (Some(l), None) if l.len_utf8() == c.len_utf8() => out.push(l),
            _ => out.push(c),
        }
    }
    out
}

/// Clamped `substring(start, end)`; out-of-range or non-boundary requests yield
/// an empty/clamped result instead of an exception.
pub(crate) fn sub(s: &str, start: isize, end: isize) -> &str {
    let len = s.len() as isize;
    let a = start.clamp(0, len) as usize;
    let b = end.clamp(0, len) as usize;
    if a >= b {
        return "";
    }
    s.get(a..b).unwrap_or("")
}

/// `StringUtils.replace(string, start, end, replacement)`.
pub(crate) fn replace_range(s: &str, start: isize, end: isize, replacement: &str) -> String {
    let len = s.len() as isize;
    let mut out = String::with_capacity(s.len() + replacement.len());
    out.push_str(sub(s, 0, start));
    out.push_str(replacement);
    if end <= len {
        out.push_str(sub(s, end, len));
    }
    out
}

/// `StringUtils.replaceAll(result, search, replace)`: literal, non-regex, the
/// replacement is not rescanned.
pub(crate) fn replace_all(result: &str, search: &str, replace: &str) -> String {
    if search.is_empty() {
        // Java: indexOf("", pos) matches at every position; that would loop
        // forever advancing by replace.length(). Patterns never have empty keys.
        return result.to_owned();
    }
    result.replace(search, replace)
}

/// `String.split(regex)` for a single literal char delimiter: trailing empty
/// strings are removed; no match returns the input unchanged.
pub(crate) fn java_split(s: &str, delimiter: char) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(delimiter).collect();
    if parts.len() == 1 {
        return parts;
    }
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
}

/// `StringUtils.Token`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Token<'a> {
    pub start: usize,
    pub end: usize,
    pub text: &'a str,
    pub in_quotes: bool,
}

const HINT_KEY_WORD: &[u8] = b"hint";

/// `StringUtils.tokenizeSql`: alphanumeric/underscore/`@` runs are tokens; every
/// other non-whitespace char is its own token; whitespace and comments are
/// dropped (except `--hint`).
pub(crate) fn tokenize_sql(sql: &str) -> Vec<Token<'_>> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut comment1 = false;
    let mut comment2 = false;
    let mut in_single = false;
    let mut in_double = false;
    for (cursor, ch) in sql.char_indices() {
        let ch_len = ch.len_utf8();
        if comment1 {
            if ch == '\n' {
                comment1 = false;
                start = cursor + 1;
            }
        } else if comment2 {
            if ch == '/' && cursor > 0 && bytes[cursor - 1] == b'*' {
                comment2 = false;
                start = cursor + 1;
            }
        } else if !is_letter_or_digit(ch) && ch != '_' && ch != '@' {
            if cursor > start {
                tokens.push(Token {
                    start,
                    end: cursor,
                    text: &sql[start..cursor],
                    in_quotes: in_single || in_double,
                });
            }
            let next = bytes.get(cursor + 1).copied();
            if ch == '-'
                && next == Some(b'-')
                && !in_single
                && !in_double
                && (sql.len() - cursor < 6 || &bytes[cursor + 2..cursor + 6] != HINT_KEY_WORD)
            {
                comment1 = true;
            } else if ch == '/' && next == Some(b'*') && !in_single && !in_double {
                comment2 = true;
            } else if !is_java_whitespace(ch) {
                tokens.push(Token {
                    start: cursor,
                    end: cursor + ch_len,
                    text: &sql[cursor..cursor + ch_len],
                    in_quotes: in_single || in_double,
                });
                if ch == '\'' && !in_double {
                    in_single = !in_single;
                }
                if ch == '"' && !in_single {
                    in_double = !in_double;
                }
            }
            start = cursor + ch_len;
        }
    }
    if sql.len() > start && !comment1 && !comment2 {
        tokens.push(Token {
            start,
            end: sql.len(),
            text: &sql[start..],
            in_quotes: in_single || in_double,
        });
    }
    tokens
}

/// `StringUtils.safeSplit`: split on `delimiter`, honouring `"` literals and
/// backslash escapes.
pub(crate) fn safe_split(string: &str, delimiter: char) -> Vec<&str> {
    if string.is_empty() {
        return vec![""];
    }
    let mut result = Vec::new();
    let mut literal = false;
    let mut escape = false;
    let mut start = 0usize;
    for (i, c) in string.char_indices() {
        if c == '"' && !escape {
            literal = !literal;
        }
        if !literal && c == delimiter && !escape {
            result.push(&string[start..i]);
            start = i + c.len_utf8();
        }
        escape = c == '\\' && !escape;
    }
    result.push(&string[start..]);
    result
}

/// `String.hashCode()`.
pub(crate) fn java_string_hash(s: &str) -> i32 {
    let mut h: i32 = 0;
    for unit in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    h
}

/// Iteration order of a default `java.util.HashMap<String, _>` (or `HashSet`)
/// filled by inserting the given *distinct* keys in order and never removing:
/// bucket index ascending, insertion order within a bucket. Returns indices
/// into `keys`.
pub(crate) fn java_hashmap_order<S: AsRef<str>>(keys: &[S]) -> Vec<usize> {
    let n = keys.len();
    let mut capacity = 16usize;
    while n > capacity * 3 / 4 {
        capacity *= 2;
    }
    let mut order: Vec<(usize, usize)> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let h = java_string_hash(key.as_ref()) as u32;
            (((h ^ (h >> 16)) as usize) & (capacity - 1), i)
        })
        .collect();
    order.sort();
    order.into_iter().map(|(_, i)| i).collect()
}
