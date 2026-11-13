//! pgjdbc runs every `Statement` SQL text through `Parser.replaceProcessing` before it reaches the server;
//! the only observable effect for plain SQL is the "Unterminated ..." syntax errors. JDBC escape
//! sequences (`{fn ...}`, `{d ...}`, ...) are not rewritten.

use crate::javacompat::DbError;

fn is_space(c: u16) -> bool {
    matches!(c, 0x20 | 0x09 | 0x0A | 0x0D | 0x0C)
}

fn is_operator_char(c: u16) -> bool {
    matches!(
        char::from_u32(c as u32),
        Some(
            '+' | '-'
                | '*'
                | '/'
                | '<'
                | '>'
                | '='
                | '~'
                | '!'
                | '@'
                | '#'
                | '%'
                | '^'
                | '&'
                | '|'
                | '`'
                | '?'
        )
    )
}

fn terminates_identifier(c: u16) -> bool {
    c == b'"' as u16 || is_space(c) || is_operator_char(c)
}

/// `Character.isJavaIdentifierPart` (approximated by Unicode alphanumerics, `_`, `$` and currency symbols).
fn ident_cont(c: u16) -> bool {
    match char::from_u32(c as u32) {
        Some(ch) => {
            ch.is_alphanumeric()
                || ch == '_'
                || ch == '$'
                || matches!(ch, '\u{a2}'..='\u{a5}' | '\u{20a0}'..='\u{20c0}')
        }
        None => true,
    }
}

fn ident_start(c: u16) -> bool {
    match char::from_u32(c as u32) {
        Some(ch) => {
            ch.is_alphabetic()
                || ch == '_'
                || ch == '$'
                || matches!(ch, '\u{a2}'..='\u{a5}' | '\u{20a0}'..='\u{20c0}')
        }
        None => true,
    }
}

fn parse_single_quotes(query: &[u16], mut offset: usize) -> usize {
    let mut standard = true;
    if offset >= 2
        && (query[offset - 1] == b'e' as u16 || query[offset - 1] == b'E' as u16)
        && terminates_identifier(query[offset - 2])
    {
        standard = false;
    }
    offset += 1;
    while offset < query.len() {
        if query[offset] == b'\'' as u16 {
            return offset;
        }
        if !standard && query[offset] == b'\\' as u16 {
            offset += 1;
        }
        offset += 1;
    }
    query.len()
}

fn parse_double_quotes(query: &[u16], mut offset: usize) -> usize {
    offset += 1;
    while offset < query.len() && query[offset] != b'"' as u16 {
        offset += 1;
    }
    offset
}

fn parse_dollar_quotes(query: &[u16], mut offset: usize) -> usize {
    if offset + 1 < query.len() && (offset == 0 || !ident_cont(query[offset - 1])) {
        let mut end: Option<usize> = None;
        if query[offset + 1] == b'$' as u16 {
            end = Some(offset + 1);
        } else if query[offset + 1] != b'$' as u16 && ident_start(query[offset + 1]) {
            for d in offset + 2..query.len() {
                if query[d] == b'$' as u16 {
                    end = Some(d);
                    break;
                } else if query[d] == b'$' as u16 || !ident_cont(query[d]) {
                    break;
                }
            }
        }
        if let Some(end) = end.filter(|&e| e > 0) {
            let tag_start = offset;
            let tag_len = end - offset + 1;
            offset = end;
            offset += 1;
            while offset < query.len() {
                if query[offset] == b'$' as u16
                    && offset + tag_len <= query.len()
                    && query[tag_start..tag_start + tag_len] == query[offset..offset + tag_len]
                {
                    offset += tag_len - 1;
                    break;
                }
                offset += 1;
            }
        }
    }
    offset
}

fn parse_line_comment(query: &[u16], mut offset: usize) -> usize {
    if offset + 1 < query.len() && query[offset + 1] == b'-' as u16 {
        while offset + 1 < query.len() {
            offset += 1;
            if query[offset] == b'\r' as u16 || query[offset] == b'\n' as u16 {
                break;
            }
        }
    }
    offset
}

fn parse_block_comment(query: &[u16], mut offset: usize) -> usize {
    if offset + 1 < query.len() && query[offset + 1] == b'*' as u16 {
        let mut level = 1;
        offset += 2;
        while offset < query.len() {
            match query[offset - 1] {
                c if c == b'*' as u16 => {
                    if query[offset] == b'/' as u16 {
                        level -= 1;
                        offset += 1;
                    }
                }
                c if c == b'/' as u16 => {
                    if query[offset] == b'*' as u16 {
                        level += 1;
                        offset += 1;
                    }
                }
                _ => {}
            }
            if level == 0 {
                offset -= 1;
                break;
            }
            offset += 1;
        }
    }
    offset
}

/// `java.text.NumberFormat` grouping for the `{0}` message argument.
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Reproduce the errors `Parser.replaceProcessing` raises for unterminated quotes and comments.
pub fn precheck(sql: &str) -> Result<(), DbError> {
    let chars: Vec<u16> = sql.encode_utf16().collect();
    let len = chars.len();
    let fail = |start: usize, what: &str| {
        DbError::new(
            "org.postgresql.util.PSQLException",
            format!("{what} started at position {} in SQL {sql}", grouped(start)),
            "42601",
            0,
        )
    };
    let mut i = 0;
    while i < len {
        let c = chars[i];
        let start = i;
        if c == b'$' as u16 {
            i = parse_dollar_quotes(&chars, i);
            if i >= len {
                return Err(fail(start, "Unterminated dollar quote")
                    .with_suffix(". Expected terminating $$"));
            }
        } else if c == b'\'' as u16 {
            i = parse_single_quotes(&chars, i);
            if i >= len {
                // MessageFormat swallows the apostrophe of "Expected ' char".
                return Err(
                    fail(start, "Unterminated string literal").with_suffix(". Expected  char")
                );
            }
        } else if c == b'"' as u16 {
            i = parse_double_quotes(&chars, i);
            if i >= len {
                return Err(
                    fail(start, "Unterminated identifier").with_suffix(". Expected \" char")
                );
            }
        } else if c == b'/' as u16 {
            i = parse_block_comment(&chars, i);
            if i >= len {
                return Err(
                    fail(start, "Unterminated block comment").with_suffix(". Expected */ sequence")
                );
            }
        } else if c == b'-' as u16 {
            i = parse_line_comment(&chars, i);
        }
        i += 1;
    }
    Ok(())
}

trait WithSuffix {
    fn with_suffix(self, suffix: &str) -> Self;
}

impl WithSuffix for DbError {
    fn with_suffix(mut self, suffix: &str) -> Self {
        self.message.push_str(suffix);
        self
    }
}
