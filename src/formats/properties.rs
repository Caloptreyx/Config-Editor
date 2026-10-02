//! Java `.properties` files (also `eula.txt`), read like `java.util.Properties.load`.
use std::collections::HashSet;

use super::{
    Node, ParseError,
    flat::{self, Line, LineEdits, edit_container},
    text,
};

const COMMENT_MARKERS: &[&str] = &["#", "!"];

#[derive(Clone, Copy, PartialEq)]
enum Separator {
    /// `key` alone.
    Missing,
    /// Only whitespace: a value starting with `=` or `:` must escape it.
    Whitespace,
    /// Contains `=` or `:`.
    Explicit,
}

struct Pair {
    line: Line,
    separator: Separator,
    /// The raw value, through the end of the logical line (before its line break).
    value_start: usize,
    value_end: usize,
}

impl AsRef<Line> for Pair {
    fn as_ref(&self) -> &Line {
        &self.line
    }
}

impl Pair {
    fn rewrite(&self, value: &str, ascii: bool) -> (usize, usize, String) {
        let text = match self.separator {
            Separator::Missing => format!("={}", escape_value(value, ascii, false)),
            Separator::Whitespace => escape_value(value, ascii, true),
            Separator::Explicit => escape_value(value, ascii, false),
        };
        (self.value_start, self.value_end, text)
    }
}

pub fn parse(source: &str) -> Result<Node, ParseError> {
    let entries = scan(source)?
        .into_iter()
        .map(|pair| pair.line.into_entry(source, COMMENT_MARKERS))
        .collect();
    Ok(Node::Object { entries })
}

pub fn patch(source: &str, _current: &Node, document: &Node) -> String {
    let (Ok(pairs), Node::Object { entries }) = (scan(source), document) else {
        return emit(source, document);
    };
    let ascii = source.is_ascii();
    let mut writer = LineEdits::new(source, COMMENT_MARKERS);
    let edited = edit_container(
        &mut writer,
        &pairs,
        entries,
        source.len(),
        |pair, value| pair.rewrite(value, ascii),
        |key, value| pair_text(key, value, ascii),
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
    let ascii = source.is_ascii();
    entries
        .iter()
        .map(|entry| {
            let value = flat::scalar_text(&entry.value).unwrap_or_default();
            pair_text(&entry.key, &value, ascii) + eol
        })
        .collect()
}

fn pair_text(key: &str, value: &str, ascii: bool) -> String {
    format!(
        "{}={}",
        escape_key(key, ascii),
        escape_value(value, ascii, false)
    )
}

fn scan(source: &str) -> Result<Vec<Pair>, ParseError> {
    let mut pairs = Vec::new();
    let mut seen = HashSet::new();
    let mut at = 0;
    while at < source.len() {
        let content = skip_blank(source, at);
        if content == text::line_end(source, at)
            || matches!(source.as_bytes()[content], b'#' | b'!')
        {
            at = text::next_line(source, at);
            continue;
        }
        let pair = split(source, at, content)?;
        if !seen.insert(pair.line.key.clone()) {
            return Err(ParseError::on_line(
                text::line_col(source, at).0 as usize,
                format!("duplicate key `{}`", pair.line.key),
            ));
        }
        at = pair.line.end;
        pairs.push(pair);
    }
    Ok(pairs)
}

/// Java's whitespace between key and value.
fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\x0c')
}

fn skip_blank(source: &str, mut at: usize) -> usize {
    let bytes = source.as_bytes();
    while at < bytes.len() && is_blank(bytes[at] as char) {
        at += 1;
    }
    at
}

/// The key/value pair whose logical line starts at `start` (first non-blank at `content`).
fn split(source: &str, start: usize, content: usize) -> Result<Pair, ParseError> {
    let (chars, value_end, end) = logical_line(source, content);

    let mut index = 0;
    let mut escaped = false;
    while index < chars.len() {
        let c = chars[index].1;
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '=' || c == ':' || is_blank(c) {
            break;
        }
        index += 1;
    }
    let key_end = index;

    let mut separator = Separator::Missing;
    while let Some(&(_, c)) = chars.get(index) {
        if is_blank(c) && separator != Separator::Explicit {
            separator = Separator::Whitespace;
        } else if (c == '=' || c == ':') && separator != Separator::Explicit {
            separator = Separator::Explicit;
        } else if !is_blank(c) {
            break;
        }
        index += 1;
    }

    // An empty value starts after the last character, so a dropped trailing backslash is
    // inside the replaced range.
    let value_start = match chars.get(index) {
        Some(&(offset, _)) => offset,
        None => chars
            .last()
            .map_or(content, |&(offset, c)| offset + c.len_utf8()),
    };
    Ok(Pair {
        line: Line {
            key: unescape(source, &chars[..key_end])?,
            value: unescape(source, &chars[index..])?,
            start,
            end,
        },
        separator,
        value_start,
        value_end,
    })
}

/// The characters of the logical line starting at `content` with their offsets, without
/// continuation backslashes and the leading blanks of continuation lines; plus the end of its
/// last physical line (before the line break) and the start of the following line.
fn logical_line(source: &str, content: usize) -> (Vec<(usize, char)>, usize, usize) {
    let mut chars = Vec::new();
    let mut at = content;
    loop {
        let end = text::line_end(source, at);
        let segment = &source[at..end];
        let continued = (segment.len() - segment.trim_end_matches('\\').len()) % 2 == 1;
        let kept = if continued { end - 1 } else { end };
        chars.extend(
            source[at..kept]
                .char_indices()
                .map(|(index, c)| (at + index, c)),
        );
        let next = text::next_line(source, at);
        if !continued || next >= source.len() {
            return (chars, end, next);
        }
        at = skip_blank(source, next);
    }
}

/// Decodes `\t \n \r \f \uXXXX` (surrogate pairs combined) and `\X` → `X`.
fn unescape(source: &str, chars: &[(usize, char)]) -> Result<String, ParseError> {
    let mut out = String::with_capacity(chars.len());
    let mut index = 0;
    while let Some(&(offset, c)) = chars.get(index) {
        index += 1;
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(&(_, escaped)) = chars.get(index) else {
            break;
        };
        index += 1;
        match escaped {
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'f' => out.push('\x0c'),
            'u' => {
                let unit = hex_unit(chars, index)
                    .ok_or_else(|| ParseError::at(source, offset, "malformed \\uXXXX escape"))?;
                index += 4;
                let mut decoded = char::from_u32(unit);
                if (0xD800..0xDC00).contains(&unit)
                    && let Some(low) = low_surrogate(chars, index)
                {
                    index += 6;
                    decoded = char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00));
                }
                out.push(decoded.unwrap_or(char::REPLACEMENT_CHARACTER));
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// The four hex digits at `at`.
fn hex_unit(chars: &[(usize, char)], at: usize) -> Option<u32> {
    chars
        .get(at..at + 4)?
        .iter()
        .try_fold(0, |unit, &(_, c)| Some(unit * 16 + c.to_digit(16)?))
}

/// The low surrogate of a `\uXXXX` escape at `at`.
fn low_surrogate(chars: &[(usize, char)], at: usize) -> Option<u32> {
    let escape = chars.get(at..at + 2)?;
    if escape[0].1 != '\\' || escape[1].1 != 'u' {
        return None;
    }
    hex_unit(chars, at + 2).filter(|unit| (0xDC00..0xE000).contains(unit))
}

/// `value` written after the separator; `guard_separator` escapes a leading `=`/`:` that
/// would otherwise be read as the separator.
fn escape_value(value: &str, ascii: bool, guard_separator: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for (index, c) in value.chars().enumerate() {
        match c {
            ' ' if index == 0 => out.push_str("\\ "),
            '=' | ':' if index == 0 && guard_separator => {
                out.push('\\');
                out.push(c);
            }
            _ => push_escaped(&mut out, c, ascii),
        }
    }
    out
}

fn escape_key(key: &str, ascii: bool) -> String {
    let mut out = String::with_capacity(key.len());
    for (index, c) in key.chars().enumerate() {
        match c {
            '=' | ':' | ' ' => {
                out.push('\\');
                out.push(c);
            }
            '#' | '!' if index == 0 => {
                out.push('\\');
                out.push(c);
            }
            _ => push_escaped(&mut out, c, ascii),
        }
    }
    out
}

/// Backslashes and control characters escaped; non-ASCII as `\uXXXX` in `ascii` files
/// (Java reads those as ISO-8859-1).
fn push_escaped(out: &mut String, c: char, ascii: bool) {
    match c {
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\x0c' => out.push_str("\\f"),
        c if c.is_control() || (ascii && !c.is_ascii()) => {
            let mut units = [0; 2];
            for unit in c.encode_utf16(&mut units).iter() {
                out.push_str(&format!("\\u{unit:04X}"));
            }
        }
        c => out.push(c),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Entry, Format, Node, flat::infer};
    use super::parse;

    const SERVER: &str = "#Minecraft server properties\n\
        #Fri Sep 12 10:00:00 UTC 2026\n\
        accept-transfers=false\n\
        generator-settings={}\n\
        level-seed=\n\
        max-players=20\n\
        motd=A Minecraft Server\n\
        rcon.password=\n\
        resource-pack=https\\://example.com/pack.zip\n\
        server-port=25565\n";

    /// Applies `document` and checks it reads back unchanged.
    fn apply(source: &str, document: &Node) -> String {
        let out = Format::Properties.apply(source, document).unwrap();
        assert_eq!(parse(&out).unwrap(), *document, "{out:?}");
        out
    }

    fn entries(document: &mut Node) -> &mut Vec<Entry> {
        match document {
            Node::Object { entries } => entries,
            _ => panic!("not an object"),
        }
    }

    /// `source` parsed, with `key` set to `value` (appended when missing).
    fn with(source: &str, key: &str, value: &str) -> Node {
        set(parse(source).unwrap(), key, value)
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

    fn value<'a>(document: &'a Node, key: &str) -> &'a Node {
        match document {
            Node::Object { entries } => {
                &entries.iter().find(|entry| entry.key == key).unwrap().value
            }
            _ => panic!("not an object"),
        }
    }

    #[test]
    fn reads_server_properties() {
        let document = parse(SERVER).unwrap();
        let Node::Object { entries } = &document else {
            panic!()
        };
        assert_eq!(entries.len(), 8);
        assert_eq!(
            entries[0].comment.as_deref(),
            Some("Minecraft server properties\nFri Sep 12 10:00:00 UTC 2026")
        );
        assert_eq!(entries[1].comment, None);
        assert_eq!(
            *value(&document, "accept-transfers"),
            Node::Boolean { value: false }
        );
        assert_eq!(*value(&document, "generator-settings"), Node::string("{}"));
        assert_eq!(*value(&document, "level-seed"), Node::string(""));
        assert_eq!(
            *value(&document, "max-players"),
            Node::Integer { value: "20".into() }
        );
        assert_eq!(
            *value(&document, "motd"),
            Node::string("A Minecraft Server")
        );
        assert_eq!(
            *value(&document, "resource-pack"),
            Node::string("https://example.com/pack.zip")
        );
    }

    #[test]
    fn unchanged_document_keeps_the_file() {
        assert_eq!(apply(SERVER, &parse(SERVER).unwrap()), SERVER);
    }

    #[test]
    fn edits_only_the_changed_value() {
        let out = apply(SERVER, &with(SERVER, "max-players", "40"));
        assert_eq!(out, SERVER.replace("max-players=20", "max-players=40"));
        assert!(out.contains("resource-pack=https\\://example.com/pack.zip\n"));

        let out = apply(SERVER, &with(SERVER, "level-seed", "-123"));
        assert_eq!(out, SERVER.replace("level-seed=\n", "level-seed=-123\n"));
    }

    #[test]
    fn rewritten_values_use_minimal_escaping() {
        let out = apply(
            SERVER,
            &with(SERVER, "resource-pack", "https://example.com/new pack.zip"),
        );
        assert_eq!(
            out,
            SERVER.replace(
                "resource-pack=https\\://example.com/pack.zip",
                "resource-pack=https://example.com/new pack.zip"
            )
        );

        let source = "motd=x\n";
        assert_eq!(
            apply(source, &with(source, "motd", " two\nlines\\")),
            "motd=\\ two\\nlines\\\\\n"
        );
    }

    #[test]
    fn adds_and_removes_keys() {
        let mut document = with(SERVER, "enable-rcon", "true");
        entries(&mut document).retain(|entry| entry.key != "rcon.password");
        let position = entries(&mut document)
            .iter()
            .position(|entry| entry.key == "max-players")
            .unwrap();
        entries(&mut document).insert(position + 1, Entry::new("my key", infer("a=b"), None));
        assert_eq!(
            apply(SERVER, &document),
            "#Minecraft server properties\n\
            #Fri Sep 12 10:00:00 UTC 2026\n\
            accept-transfers=false\n\
            generator-settings={}\n\
            level-seed=\n\
            max-players=20\n\
            my\\ key=a=b\n\
            motd=A Minecraft Server\n\
            resource-pack=https\\://example.com/pack.zip\n\
            server-port=25565\n\
            enable-rcon=true\n"
        );
    }

    #[test]
    fn appending_terminates_the_last_line() {
        assert_eq!(apply("a=1", &with("a=1", "b", "2")), "a=1\nb=2\n");
        assert_eq!(apply("", &with("", "eula", "true")), "eula=true\n");
    }

    #[test]
    fn reordered_keys_rewrite_the_file() {
        let source = "b=1\n# about a\na=2\n";
        let mut document = parse(source).unwrap();
        entries(&mut document).reverse();
        assert_eq!(apply(source, &document), "a=2\nb=1\n");
    }

    #[test]
    fn continuation_lines_form_one_value() {
        let source = "# who\nplayers = alice,\\\n    bob,\\\n    carol\nother : 1\n";
        let document = parse(source).unwrap();
        assert_eq!(
            *value(&document, "players"),
            Node::string("alice,bob,carol")
        );
        let Node::Object { entries } = &document else {
            panic!()
        };
        assert_eq!(entries[0].comment.as_deref(), Some("who"));
        assert_eq!(
            apply(source, &with(source, "players", "dave")),
            "# who\nplayers = dave\nother : 1\n"
        );
    }

    #[test]
    fn unicode_escapes() {
        let source = "motd=\\u00A7aHello \\u00e9\\uD83D\\uDE00\n";
        assert_eq!(
            *value(&parse(source).unwrap(), "motd"),
            Node::string("\u{a7}aHello \u{e9}\u{1F600}")
        );
        // An ASCII file is read as ISO-8859-1 by Java, so non-ASCII text is escaped.
        assert_eq!(
            apply(source, &with(source, "motd", "\u{a7}bHi \u{1F600}")),
            "motd=\\u00A7bHi \\uD83D\\uDE00\n"
        );

        let source = "# Größe\nmotd=x\nmax=1\n";
        assert_eq!(
            apply(source, &with(source, "motd", "\u{a7}bGrüße")),
            "# Größe\nmotd=\u{a7}bGrüße\nmax=1\n"
        );
        assert!(parse("a=\\u12G4\n").is_err());
    }

    #[test]
    fn separators_stay_readable() {
        let source = "key value\nflag\n";
        let document = parse(source).unwrap();
        assert_eq!(*value(&document, "key"), Node::string("value"));
        assert_eq!(*value(&document, "flag"), Node::string(""));
        assert_eq!(
            apply(source, &with(source, "key", "=x")),
            "key \\=x\nflag\n"
        );
        assert_eq!(
            apply(source, &with(source, "flag", "on")),
            "key value\nflag=on\n"
        );
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let source = "#c\r\na=1\r\nb=2\r\n";
        let document = set(with(source, "b", "3"), "c", "x");
        assert_eq!(apply(source, &document), "#c\r\na=1\r\nb=3\r\nc=x\r\n");
    }

    #[test]
    fn eula() {
        let source = "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\n#Fri Sep 12 10:00:00 UTC 2026\neula=false\n";
        assert_eq!(
            apply(source, &with(source, "eula", "true")),
            source.replace("eula=false", "eula=true")
        );
    }

    #[test]
    fn duplicate_keys_are_errors() {
        let error = parse("a=1\n\n# again\na = 2\n").unwrap_err();
        assert_eq!(error.line, Some(4));
    }
}
