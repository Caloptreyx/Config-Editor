//! `.env` files: `[export ]KEY=VALUE` lines with optional quotes and inline comments.
use std::collections::HashSet;

use super::{
    Node, ParseError,
    flat::{self, Line, LineEdits, edit_container},
    text,
};

const COMMENT_MARKERS: &[&str] = &["#"];

#[derive(Clone, Copy, PartialEq)]
enum Quote {
    None,
    Single,
    Double,
}

struct Assignment {
    line: Line,
    quote: Quote,
    /// The raw value, quotes included.
    value_start: usize,
    value_end: usize,
    /// An empty unquoted value directly followed by its inline comment (`KEY= # note`).
    comment_follows: bool,
}

impl AsRef<Line> for Assignment {
    fn as_ref(&self) -> &Line {
        &self.line
    }
}

impl Assignment {
    fn rewrite(&self, value: &str) -> (usize, usize, String) {
        let mut text = value_text(self.quote, value);
        if self.comment_follows {
            text.push(' ');
        }
        (self.value_start, self.value_end, text)
    }
}

pub fn parse(source: &str) -> Result<Node, ParseError> {
    let entries = scan(source)?
        .into_iter()
        .map(|assignment| assignment.line.into_entry(source, COMMENT_MARKERS))
        .collect();
    Ok(Node::Object { entries })
}

pub fn patch(source: &str, _current: &Node, document: &Node) -> String {
    let (Ok(assignments), Node::Object { entries }) = (scan(source), document) else {
        return emit(source, document);
    };
    let mut writer = LineEdits::new(source, COMMENT_MARKERS);
    let edited = edit_container(
        &mut writer,
        &assignments,
        entries,
        source.len(),
        Assignment::rewrite,
        assignment_text,
    );
    if !edited {
        return emit(source, document);
    }
    writer.finish().unwrap_or_else(|| emit(source, document))
}

pub fn emit(source: &str, document: &Node) -> String {
    let Node::Object { entries } = document else {
        return String::new();
    };
    let eol = text::eol(source);
    entries
        .iter()
        .map(|entry| {
            let value = flat::scalar_text(&entry.value).unwrap_or_default();
            assignment_text(&entry.key, &value) + eol
        })
        .collect()
}

fn assignment_text(key: &str, value: &str) -> String {
    format!("{key}={}", value_text(Quote::None, value))
}

/// `value` written in the `quote` style when it can be, double-quoted otherwise.
fn value_text(quote: Quote, value: &str) -> String {
    match quote {
        Quote::None if is_plain(value) => value.to_string(),
        Quote::Single if !value.contains(['\'', '\n', '\r']) => format!("'{value}'"),
        _ => double_quoted(value),
    }
}

fn is_plain(value: &str) -> bool {
    value.chars().all(|c| {
        !c.is_whitespace() && !c.is_control() && !matches!(c, '#' | '"' | '\'' | '\\' | '`')
    })
}

fn double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn scan(source: &str) -> Result<Vec<Assignment>, ParseError> {
    let mut assignments = Vec::new();
    let mut seen = HashSet::new();
    let mut at = 0;
    while at < source.len() {
        let content = skip_spaces(source, at);
        if content == text::line_end(source, at) || source[content..].starts_with('#') {
            at = text::next_line(source, at);
            continue;
        }
        let assignment = assignment(source, at, content)?;
        if !seen.insert(assignment.line.key.clone()) {
            return Err(ParseError::on_line(
                text::line_col(source, at).0 as usize,
                format!("duplicate key `{}`", assignment.line.key),
            ));
        }
        at = assignment.line.end;
        assignments.push(assignment);
    }
    Ok(assignments)
}

fn skip_spaces(source: &str, mut at: usize) -> usize {
    let bytes = source.as_bytes();
    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t') {
        at += 1;
    }
    at
}

/// Length of the key `[A-Za-z_][A-Za-z0-9_.-]*` at the start of `text`.
fn key_len(text: &str) -> usize {
    text.bytes()
        .enumerate()
        .take_while(|&(index, byte)| {
            byte.is_ascii_alphabetic()
                || byte == b'_'
                || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'.' | b'-')))
        })
        .count()
}

/// Start of the key after an `export ` prefix at `content`.
fn after_export(source: &str, content: usize) -> Option<usize> {
    let after = content + "export".len();
    if !source[content..].starts_with("export") {
        return None;
    }
    let key = skip_spaces(source, after);
    (key > after && key_len(&source[key..]) > 0).then_some(key)
}

/// The assignment on the line starting at `start` (first non-blank at `content`).
fn assignment(source: &str, start: usize, content: usize) -> Result<Assignment, ParseError> {
    let key_start = after_export(source, content).unwrap_or(content);
    let key_end = key_start + key_len(&source[key_start..]);
    if key_end == key_start {
        return Err(ParseError::at(source, key_start, "expected `KEY=VALUE`"));
    }
    let equals = skip_spaces(source, key_end);
    if source.as_bytes().get(equals) != Some(&b'=') {
        return Err(ParseError::at(source, equals, "expected `=` after the key"));
    }
    let line_end = text::line_end(source, equals);
    let value_start = skip_spaces(source, equals + 1);

    let (value, quote, value_end) = match source.as_bytes().get(value_start) {
        Some(b'\'') => {
            let (value, end) = single_quoted(source, value_start, line_end)?;
            (value, Quote::Single, end)
        }
        Some(b'"') => {
            let (value, end) = double_quoted_value(source, value_start)?;
            (value, Quote::Double, end)
        }
        _ => {
            let spaced = value_start > equals + 1;
            let (value, end, comment_follows) = unquoted(source, value_start, line_end, spaced);
            return Ok(Assignment {
                line: Line {
                    key: source[key_start..key_end].to_string(),
                    value,
                    start,
                    end: text::next_line(source, start),
                },
                quote: Quote::None,
                value_start,
                value_end: end,
                comment_follows,
            });
        }
    };

    let rest_end = text::line_end(source, value_end);
    let rest = source[value_end..rest_end].trim_start_matches([' ', '\t']);
    if !rest.is_empty() && !rest.starts_with('#') {
        return Err(ParseError::at(
            source,
            rest_end - rest.len(),
            "unexpected text after the quoted value",
        ));
    }
    Ok(Assignment {
        line: Line {
            key: source[key_start..key_end].to_string(),
            value,
            start,
            end: text::next_line(source, value_end),
        },
        quote,
        value_start,
        value_end,
        comment_follows: false,
    })
}

/// The unquoted value at `start` (up to an inline comment, trailing whitespace trimmed), its
/// end, and whether it is empty with the comment right at `start`.
fn unquoted(source: &str, start: usize, line_end: usize, spaced: bool) -> (String, usize, bool) {
    let region = &source[start..line_end];
    let bytes = region.as_bytes();
    let comment = (0..bytes.len()).find(|&index| {
        bytes[index] == b'#'
            && if index == 0 {
                spaced
            } else {
                matches!(bytes[index - 1], b' ' | b'\t')
            }
    });
    let value = region[..comment.unwrap_or(region.len())].trim_end();
    (value.to_string(), start + value.len(), comment == Some(0))
}

/// The value of the single-quoted string opening at `open` and the offset after it.
fn single_quoted(
    source: &str,
    open: usize,
    line_end: usize,
) -> Result<(String, usize), ParseError> {
    let close = source[open + 1..line_end]
        .find('\'')
        .ok_or_else(|| ParseError::at(source, open, "unterminated single-quoted value"))?;
    let close = open + 1 + close;
    Ok((source[open + 1..close].to_string(), close + 1))
}

/// The decoded value of the double-quoted string opening at `open` (it may span lines) and
/// the offset after it.
fn double_quoted_value(source: &str, open: usize) -> Result<(String, usize), ParseError> {
    let mut value = String::new();
    let mut chars = source[open + 1..].char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => return Ok((value, open + 1 + index + 1)),
            '\\' => match chars.next() {
                Some((_, 'n')) => value.push('\n'),
                Some((_, 'r')) => value.push('\r'),
                Some((_, 't')) => value.push('\t'),
                Some((_, escaped @ ('"' | '\\' | '$'))) => value.push(escaped),
                Some((_, other)) => {
                    value.push('\\');
                    value.push(other);
                }
                None => break,
            },
            c => value.push(c),
        }
    }
    Err(ParseError::at(
        source,
        open,
        "unterminated double-quoted value",
    ))
}

#[cfg(test)]
mod tests {
    use super::super::{Entry, Format, Node, flat::infer};
    use super::parse;

    const APP: &str = "# Database\n\
        export DB_HOST=localhost # local only\n\
        DB_PASS='s3cr#t'\n\
        GREETING = \"Hello\\n\\\"World\\\"\"\n\
        \n\
        # Multi-line key\n\
        CERT=\"-----BEGIN-----\n\
        abc\n\
        -----END-----\"\n\
        EMPTY=\n\
        PORT=8080\n";

    /// Applies `document` and checks it reads back unchanged.
    fn apply(source: &str, document: &Node) -> String {
        let out = Format::Env.apply(source, document).unwrap();
        assert_eq!(parse(&out).unwrap(), *document, "{out:?}");
        out
    }

    fn entries(document: &mut Node) -> &mut Vec<Entry> {
        match document {
            Node::Object { entries } => entries,
            _ => panic!("not an object"),
        }
    }

    /// `document` with `key` set to `value` (appended when missing).
    fn set(mut document: Node, key: &str, value: &str) -> Node {
        let entries = entries(&mut document);
        match entries.iter_mut().find(|entry| entry.key == key) {
            Some(entry) => entry.value = infer(value),
            None => entries.push(Entry::new(key, infer(value), None)),
        }
        document
    }

    fn with(source: &str, key: &str, value: &str) -> Node {
        set(parse(source).unwrap(), key, value)
    }

    #[test]
    fn reads_values_and_comments() {
        let document = parse(APP).unwrap();
        let Node::Object { entries } = &document else {
            panic!()
        };
        let read: Vec<(&str, Node, Option<&str>)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.key.as_str(),
                    entry.value.clone(),
                    entry.comment.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            read,
            vec![
                ("DB_HOST", Node::string("localhost"), Some("Database")),
                ("DB_PASS", Node::string("s3cr#t"), None),
                ("GREETING", Node::string("Hello\n\"World\""), None),
                (
                    "CERT",
                    Node::string("-----BEGIN-----\nabc\n-----END-----"),
                    Some("Multi-line key")
                ),
                ("EMPTY", Node::string(""), None),
                (
                    "PORT",
                    Node::Integer {
                        value: "8080".into()
                    },
                    None
                ),
            ]
        );
    }

    #[test]
    fn unchanged_document_keeps_the_file() {
        assert_eq!(apply(APP, &parse(APP).unwrap()), APP);
    }

    #[test]
    fn edits_keep_export_spacing_quotes_and_comments() {
        assert_eq!(
            apply(APP, &with(APP, "DB_HOST", "db.internal")),
            APP.replace("DB_HOST=localhost #", "DB_HOST=db.internal #")
        );
        assert_eq!(
            apply(APP, &with(APP, "DB_PASS", "new pass")),
            APP.replace("'s3cr#t'", "'new pass'")
        );
        assert_eq!(
            apply(APP, &with(APP, "GREETING", "Hi \"you\"")),
            APP.replace("\"Hello\\n\\\"World\\\"\"", "\"Hi \\\"you\\\"\"")
        );
        assert_eq!(
            apply(APP, &with(APP, "CERT", "none")),
            APP.replace("\"-----BEGIN-----\nabc\n-----END-----\"", "\"none\"")
        );
        assert_eq!(
            apply(APP, &with(APP, "EMPTY", "1.5")),
            APP.replace("EMPTY=\n", "EMPTY=1.5\n")
        );
    }

    #[test]
    fn values_that_need_quotes_get_double_quotes() {
        assert_eq!(
            apply(APP, &with(APP, "PORT", "80 # not a comment")),
            APP.replace("PORT=8080", "PORT=\"80 # not a comment\"")
        );
        assert_eq!(
            apply(APP, &with(APP, "DB_PASS", "it's\nmine")),
            APP.replace("'s3cr#t'", "\"it's\\nmine\"")
        );
        let source = "KEY= # set me\n";
        assert_eq!(
            apply(source, &with(source, "KEY", "v")),
            "KEY= v # set me\n"
        );
    }

    #[test]
    fn adds_and_removes_keys() {
        let mut document = set(parse(APP).unwrap(), "NEW_KEY", "a b");
        entries(&mut document).retain(|entry| entry.key != "CERT" && entry.key != "DB_HOST");
        entries(&mut document).insert(0, Entry::new("FIRST", infer("1"), None));
        assert_eq!(
            apply(APP, &document),
            "# Database\n\
            FIRST=1\n\
            DB_PASS='s3cr#t'\n\
            GREETING = \"Hello\\n\\\"World\\\"\"\n\
            \n\
            # Multi-line key\n\
            EMPTY=\n\
            PORT=8080\n\
            NEW_KEY=\"a b\"\n"
        );
    }

    #[test]
    fn reordered_keys_rewrite_the_file() {
        let source = "A=1\nB='x y'\n";
        let mut document = parse(source).unwrap();
        entries(&mut document).reverse();
        assert_eq!(apply(source, &document), "B=\"x y\"\nA=1\n");
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let source = "# Größe\r\nNAME=\"Jürgen\"\r\nMODE=dev\r\n";
        let document = set(with(source, "MODE", "prod"), "EXTRA", "ü");
        assert_eq!(
            apply(source, &document),
            "# Größe\r\nNAME=\"Jürgen\"\r\nMODE=prod\r\nEXTRA=ü\r\n"
        );
    }

    #[test]
    fn invalid_lines_report_their_line() {
        assert_eq!(parse("A=1\nnot a pair\n").unwrap_err().line, Some(2));
        assert_eq!(parse("A=1\n\nB='open\n").unwrap_err().line, Some(3));
        assert_eq!(parse("A=\"open\nstill\n").unwrap_err().line, Some(1));
        assert_eq!(parse("A=\"x\" y\n").unwrap_err().line, Some(1));
        assert_eq!(parse("A=1\n# c\nexport A=2\n").unwrap_err().line, Some(3));
    }
}
