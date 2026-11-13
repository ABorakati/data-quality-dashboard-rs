//! Port of `org.ohdsi.sql.SqlRender.renderSql`.

use crate::strings::{
    is_java_whitespace, is_regex_space, java_hashmap_order, java_split, java_trim, replace_range,
    sub,
};
use std::fmt;

/// Java throws a `RuntimeException` for an unparseable `{if}` condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError(pub String);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RenderError {}

#[derive(Debug, Clone)]
struct Span {
    start: isize,
    end: isize,
    valid: bool,
}

impl Span {
    fn new(start: isize, end: isize) -> Self {
        Span {
            start,
            end,
            valid: true,
        }
    }
}

/// Spans are referenced by index because `replace` mutates them in place.
struct IfThenElse {
    condition: usize,
    if_true: usize,
    if_false: Option<usize>,
}

fn find_curly_bracket_spans(s: &str) -> Vec<Span> {
    find_spans(s, b'{', b'}')
}

fn find_parentheses(s: &str) -> Vec<Span> {
    find_spans(s, b'(', b')')
}

/// Spans in order of their closing bracket.
fn find_spans(s: &str, open: u8, close: u8) -> Vec<Span> {
    let mut starts: Vec<usize> = Vec::new();
    let mut spans = Vec::new();
    for (i, b) in s.bytes().enumerate() {
        if b == open {
            starts.push(i);
        } else if b == close {
            if let Some(start) = starts.pop() {
                spans.push(Span::new(start as isize, i as isize + 1));
            }
        }
    }
    spans
}

fn link_if_then_elses(s: &str, spans: &[Span]) -> Vec<IfThenElse> {
    let mut result = Vec::new();
    if spans.len() > 1 {
        for i in 0..spans.len() - 1 {
            for j in i + 1..spans.len() {
                if spans[j].start > spans[i].end {
                    let in_between = java_trim(sub(s, spans[i].end, spans[j].start));
                    if in_between == "?" {
                        let mut ite = IfThenElse {
                            condition: i,
                            if_true: j,
                            if_false: None,
                        };
                        for k in j + 1..spans.len() {
                            if spans[k].start > spans[j].end {
                                let in_between = java_trim(sub(s, spans[j].end, spans[k].start));
                                if in_between == ":" {
                                    ite.if_false = Some(k);
                                }
                            }
                        }
                        result.push(ite);
                    }
                }
            }
        }
    }
    result
}

fn evaluate_condition(s: &str) -> Result<bool, RenderError> {
    let mut s = java_trim(s).to_owned();
    let mut spans = find_parentheses(&s);
    // Spans are in order of closing parenthesis, so nested ones are processed first.
    for i in 0..spans.len() {
        let (start, end) = (spans[i].start, spans[i].end);
        if !preceded_by_in(start, &s) {
            let evaluation = evaluate_boolean_condition(sub(&s, start + 1, end - 1))?;
            let marker = if evaluation { "1" } else { "0" };
            s = format!(
                "{}{}{}",
                sub(&s, 0, start),
                marker,
                sub(&s, start + 1, s.len() as isize)
            );
            s = replace(&s, &mut spans, start, end, start, start);
        }
    }
    evaluate_boolean_condition(&s)
}

fn evaluate_boolean_condition(s: &str) -> Result<bool, RenderError> {
    let s = java_trim(s);
    if s.contains('&') {
        for part in java_split(s, '&') {
            if !evaluate_primitive_condition(part)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    if s.contains('|') {
        for part in java_split(s, '|') {
            if evaluate_primitive_condition(part)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    evaluate_primitive_condition(s)
}

fn preceded_by_in(start: isize, s: &str) -> bool {
    let end = start.clamp(0, s.len() as isize) as usize;
    let Some(prefix) = s.get(..end) else {
        return false;
    };
    let mut matched = 0;
    for c in prefix.chars().rev() {
        let c = c.to_ascii_lowercase();
        if !is_java_whitespace(c) {
            if matched == 0 && c == 'n' {
                matched += 1;
            } else if matched == 1 && c == 'i' {
                matched += 1;
            } else {
                return false;
            }
        } else if matched == 2 {
            return true;
        }
    }
    false
}

fn remove_parentheses(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() > 1
        && ((b[0] == b'\'' && b[b.len() - 1] == b'\'') || (b[0] == b'"' && b[b.len() - 1] == b'"'))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn evaluate_primitive_condition(s: &str) -> Result<bool, RenderError> {
    let s = java_trim(s);
    let lc = s.to_ascii_lowercase();
    if lc == "false" || lc == "0" || lc == "!true" || lc == "!1" {
        return Ok(false);
    }
    if lc == "true" || lc == "1" || lc == "!false" || lc == "!0" {
        return Ok(true);
    }

    if let Some(found) = s.find("==") {
        let left = remove_parentheses(java_trim(&s[..found]));
        let right = remove_parentheses(java_trim(&s[found + 2..]));
        return Ok(left == right);
    }
    if let Some(found) = s.find("!=").or_else(|| s.find("<>")) {
        let left = remove_parentheses(java_trim(&s[..found]));
        let right = remove_parentheses(java_trim(&s[found + 2..]));
        return Ok(left != right);
    }
    if let Some(found) = lc.find(" in ") {
        let left = remove_parentheses(java_trim(&s[..found]));
        let right = java_trim(&s[found + 4..]);
        if right.len() > 2 && right.starts_with('(') && right.ends_with(')') {
            let inner = &right[1..right.len() - 1];
            for part in java_split(inner, ',') {
                if left == remove_parentheses(part) {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    }
    Err(RenderError(format!(
        "Error parsing boolean condition: \"{s}\""
    )))
}

/// `SqlRender.replace`: replace `[to_replace_start, to_replace_end)` with the
/// substring `[replace_with_start, replace_with_end]` (end inclusive) and shift
/// the tracked spans.
fn replace(
    s: &str,
    spans: &mut [Span],
    to_replace_start: isize,
    to_replace_end: isize,
    replace_with_start: isize,
    replace_with_end: isize,
) -> String {
    let replace_with = sub(s, replace_with_start, replace_with_end + 1).to_owned();
    let result = replace_range(s, to_replace_start, to_replace_end, &replace_with);
    for span in spans.iter_mut() {
        if span.valid {
            if span.start > to_replace_start {
                if span.start >= replace_with_start && span.start < replace_with_end {
                    let delta = to_replace_start - replace_with_start;
                    span.start += delta;
                    span.end += delta;
                } else if span.start > to_replace_end {
                    let delta = to_replace_start - to_replace_end + replace_with.len() as isize;
                    span.start += delta;
                    span.end += delta;
                } else {
                    span.valid = false;
                }
            } else if span.end > to_replace_end {
                let delta = to_replace_start - to_replace_end + replace_with.len() as isize;
                span.end += delta;
            }
        }
    }
    result
}

/// Ordered `java.util.HashMap<String, String>` emulation: remembers insertion
/// order so that Java's iteration order can be reproduced.
#[derive(Default)]
struct JavaMap {
    entries: Vec<(String, String)>,
}

impl JavaMap {
    fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    fn put(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_owned(),
            None => self.entries.push((key.to_owned(), value.to_owned())),
        }
    }

    /// Entries in `HashMap.entrySet()` iteration order.
    fn iter_java_order(&self) -> Vec<&(String, String)> {
        let keys: Vec<&str> = self.entries.iter().map(|(k, _)| k.as_str()).collect();
        java_hashmap_order(&keys)
            .into_iter()
            .map(|i| &self.entries[i])
            .collect()
    }
}

fn extract_defaults(s: &str) -> JavaMap {
    let mut defaults = JavaMap::default();
    let pre = "{DEFAULT ";
    let post = "}";
    let mut default_end = 0usize;
    loop {
        let Some(default_start) = s[default_end..].find(pre).map(|i| i + default_end) else {
            break;
        };
        let Some(end) = s[default_start + pre.len()..]
            .find(post)
            .map(|i| i + default_start + pre.len())
        else {
            break;
        };
        default_end = end;
        let span = &s[default_start + pre.len()..default_end];
        if let Some(found) = span.find('=') {
            let mut parameter = java_trim(&span[..found]);
            if parameter.starts_with('@') {
                parameter = &parameter[1..];
            }
            // Java: span.substring(found + 2) (skips '=' and one more char).
            let rest_start = span[found + 1..]
                .chars()
                .next()
                .map_or(span.len(), |c| found + 1 + c.len_utf8());
            let default_value = remove_parentheses(java_trim(&span[rest_start..]));
            defaults.put(parameter, default_value);
        }
    }
    defaults
}

/// `string.replaceAll("\\{DEFAULT[^}]*\\}\\s*\n?", "")`.
fn remove_defaults(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pos = 0usize;
    while let Some(found) = s[pos..].find("{DEFAULT") {
        let start = pos + found;
        let Some(close) = s[start..].find('}') else {
            break;
        };
        out.push_str(&s[pos..start]);
        let mut end = start + close + 1;
        end += s[end..]
            .chars()
            .take_while(|c| is_regex_space(*c))
            .map(char::len_utf8)
            .sum::<usize>();
        pos = end;
    }
    out.push_str(&s[pos..]);
    out
}

fn substitute_parameters(string: &str, parameter_to_value: &mut JavaMap) -> String {
    let defaults = extract_defaults(string);
    let mut string = remove_defaults(string);
    for (key, value) in defaults.iter_java_order() {
        if !parameter_to_value.contains_key(key) {
            parameter_to_value.put(key, value);
        }
    }
    // Longest parameter name first (stable w.r.t. HashMap iteration order).
    let mut sorted = parameter_to_value.iter_java_order();
    sorted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (key, value) in sorted {
        // Java: replaceAll("@" + key, escaped value); the backslash/dollar
        // escaping makes the replacement a literal.
        string = string.replace(&format!("@{key}"), value);
    }
    string
}

fn parse_if_then_else(s: &str) -> Result<String, RenderError> {
    let mut spans = find_curly_bracket_spans(s);
    let if_then_elses = link_if_then_elses(s, &spans);

    let mut result = s.to_owned();
    for ite in &if_then_elses {
        if spans[ite.condition].valid {
            let (cs, ce) = (spans[ite.condition].start, spans[ite.condition].end);
            let start = cs;
            let end = match ite.if_false {
                Some(f) => spans[f].end,
                None => spans[ite.if_true].end,
            };
            if evaluate_condition(sub(&result, cs + 1, ce - 1))? {
                let (ts, te) = (spans[ite.if_true].start, spans[ite.if_true].end);
                result = replace(&result, &mut spans, start, end, ts + 1, te - 2);
            } else if let Some(f) = ite.if_false {
                let (fs, fe) = (spans[f].start, spans[f].end);
                result = replace(&result, &mut spans, start, end, fs + 1, fe - 2);
            } else {
                result = replace(&result, &mut spans, start, end, 0, -1);
            }
        }
    }
    Ok(result)
}

/// `SqlRender.renderSql(sql, names, values)`, returning an error where Java
/// throws (unparseable `{if}` condition).
pub fn try_render_sql(
    sql: &str,
    parameter_names: &[&str],
    parameter_values: &[&str],
) -> Result<String, RenderError> {
    let mut map = JavaMap::default();
    for (name, value) in parameter_names.iter().zip(parameter_values) {
        map.put(name, value);
    }
    let result = substitute_parameters(sql, &mut map);
    parse_if_then_else(&result)
}

/// `SqlRender.renderSql(sql, names, values)`: substitute `@parameters`, apply
/// `{DEFAULT @x = v}` defaults and evaluate `{cond}?{then}:{else}` blocks.
///
/// # Panics
/// Where Java throws (unparseable `{if}` condition) this panics with the Java
/// message. Use [`try_render_sql`] to get an error instead.
pub fn render_sql(sql: &str, parameter_names: &[&str], parameter_values: &[&str]) -> String {
    match try_render_sql(sql, parameter_names, parameter_values) {
        Ok(s) => s,
        Err(error) => panic!("{error}"),
    }
}
