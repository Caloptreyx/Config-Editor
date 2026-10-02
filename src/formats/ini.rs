//! INI files: `[section]` headers, `key=value` / `key: value` lines and `;`/`#` comments.
//! Keys before the first section are global (root scalars).
use std::collections::HashSet;

use super::{
    Entry, Node, ParseError,
    flat::{self, Line, LineEdits, edit_container, match_keys},
    text,
};

const COMMENT_MARKERS: &[&str] = &[";", "#"];

struct Key {
    line: Line,
    /// Offset of the `=` or `:`.
    separator: usize,
    /// Whether whitespace surrounds the separator.
    spaced: bool,
    /// The value text (at the end of the line when empty).
    value_start: usize,
    value_end: usize,
}

impl AsRef<Line> for Key {
    fn as_ref(&self) -> &Line {
        &self.line
    }
}

impl Key {
    fn rewrite(&self, value: &str) -> (usize, usize, String) {
        if !self.line.value.is_empty() {
            return (self.value_start, self.value_end, value.to_string());
        }
        let space = if self.spaced { " " } else { "" };
        (
            self.separator + 1,
            self.value_end,
            format!("{space}{value}"),
        )
    }

    /// The text between key and value, used for new keys.
    fn separator_text(&self, source: &str) -> String {
        let key_end = self.line.start + source[self.line.start..self.separator].trim_end().len();
        if self.line.value.is_empty() {
            let space = if self.spaced { " " } else { "" };
            format!("{}{space}", &source[key_end..=self.separator])
        } else {
            source[key_end..self.value_start].to_string()
        }
    }
}

struct Section {
    name: String,
    /// Start of the header line.
    start: usize,
    /// End of the header line, after its line break.
    header_end: usize,
    keys: Vec<Key>,
}

impl Section {
    /// End of the last key line (of the header when there are no keys).
    fn end(&self) -> usize {
        self.keys.last().map_or(self.header_end, |key| key.line.end)
    }
}

struct File {
    globals: Vec<Key>,
    sections: Vec<Section>,
}

impl File {
    /// The separator written for new keys: the one of the file's first key line.
    fn separator_text(&self, source: &str) -> String {
        self.globals
            .iter()
            .chain(self.sections.iter().flat_map(|section| &section.keys))
            .next()
            .map_or_else(|| "=".to_string(), |key| key.separator_text(source))
    }
}

pub fn parse(source: &str) -> Result<Node, ParseError> {
    let file = scan(source)?;
    let mut entries: Vec<Entry> = file
        .globals
        .into_iter()
        .map(|key| key.line.into_entry(source, COMMENT_MARKERS))
        .collect();
    entries.extend(file.sections.into_iter().map(|section| {
        let comment = text::comment_above(source, section.start, COMMENT_MARKERS);
        let keys = section
            .keys
            .into_iter()
            .map(|key| key.line.into_entry(source, COMMENT_MARKERS))
            .collect();
        Entry::new(section.name, Node::Object { entries: keys }, comment)
    }));
    Ok(Node::Object { entries })
}

pub fn patch(source: &str, _current: &Node, document: &Node) -> String {
    let (Ok(file), Node::Object { entries }) = (scan(source), document) else {
        return emit(source, document);
    };
    edit(source, &file, entries).unwrap_or_else(|| emit(source, document))
}

pub fn emit(source: &str, document: &Node) -> String {
    let Node::Object { entries } = document else {
        return String::new();
    };
    let eol = text::eol(source);
    let separator =
        scan(source).map_or_else(|_| "=".to_string(), |file| file.separator_text(source));
    let mut out = String::new();
    for entry in entries {
        match &entry.value {
            Node::Object { entries: keys } => {
                if !out.is_empty() {
                    out.push_str(eol);
                }
                out.push_str(&format!("[{}]{eol}", entry.key));
                for key in keys {
                    out.push_str(&key_text(key, &separator));
                    out.push_str(eol);
                }
            }
            _ => {
                out.push_str(&key_text(entry, &separator));
                out.push_str(eol);
            }
        }
    }
    out
}

fn key_text(entry: &Entry, separator: &str) -> String {
    let value = flat::scalar_text(&entry.value).unwrap_or_default();
    format!("{}{separator}{value}", entry.key)
}

/// The minimal edits; `None` when the root must be re-emitted (reordered global keys or
/// sections, overlapping edits).
fn edit(source: &str, file: &File, entries: &[Entry]) -> Option<String> {
    let mut globals = Vec::new();
    let mut sections = Vec::new();
    for entry in entries {
        match &entry.value {
            Node::Object { entries: keys } => sections.push((entry.key.as_str(), keys.as_slice())),
            _ => globals.push(entry),
        }
    }

    let separator = file.separator_text(source);
    let render = |key: &str, value: &str| format!("{key}{separator}{value}");
    let mut writer = LineEdits::new(source, COMMENT_MARKERS);

    let globals_end = match (file.globals.last(), file.sections.first()) {
        (Some(key), _) => key.line.end,
        (None, Some(section)) => writer.comment_start(section.start),
        (None, None) => source.len(),
    };
    if !edit_container(
        &mut writer,
        &file.globals,
        &globals,
        globals_end,
        Key::rewrite,
        render,
    ) {
        return None;
    }

    let names: Vec<&str> = file
        .sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    let matches = match_keys(&names, sections.iter().map(|(name, _)| *name))?;
    let mut kept = vec![false; file.sections.len()];
    for &index in matches.iter().flatten() {
        kept[index] = true;
    }
    for (section, kept) in file.sections.iter().zip(kept) {
        if kept {
            continue;
        }
        // A section ending the file takes the blank lines above it along.
        let start = if section.end() == source.len() {
            blank_lines_start(source, section.start)
        } else {
            section.start
        };
        writer.delete(start, section.end());
    }

    let first_kept = matches.iter().flatten().next().copied();
    let last_kept = matches.iter().flatten().last().copied();
    let mut previous = None;
    for (&(name, keys), target) in sections.iter().zip(matches) {
        if let Some(index) = target {
            edit_section(&mut writer, &file.sections[index], keys, &separator, render);
            previous = Some(index);
            continue;
        }
        let mut lines = vec![format!("[{name}]")];
        lines.extend(keys.iter().map(|key| key_text(key, &separator)));
        match (previous, first_kept) {
            (None, Some(first)) => {
                lines.push(String::new());
                let at = writer.comment_start(file.sections[first].start);
                writer.insert_lines(at, &lines);
            }
            (Some(index), _) if previous != last_kept => {
                lines.insert(0, String::new());
                writer.insert_lines(file.sections[index].end(), &lines);
            }
            _ => {
                if !source.is_empty() || !globals.is_empty() {
                    lines.insert(0, String::new());
                }
                writer.insert_lines(source.len(), &lines);
            }
        }
    }
    writer.finish()
}

/// Edits the keys of a kept section; reordered keys re-emit the section's key lines.
fn edit_section(
    writer: &mut LineEdits<'_>,
    section: &Section,
    keys: &[Entry],
    separator: &str,
    render: impl Fn(&str, &str) -> String,
) {
    if edit_container(
        writer,
        &section.keys,
        keys,
        section.end(),
        Key::rewrite,
        render,
    ) {
        return;
    }
    let eol = writer.eol();
    let lines: String = keys
        .iter()
        .map(|key| key_text(key, separator) + eol)
        .collect();
    writer.replace(section.header_end, section.end(), lines);
}

/// Start of the blank lines directly above the line starting at `line`.
fn blank_lines_start(source: &str, line: usize) -> usize {
    let mut start = line;
    while start > 0 {
        let above = text::line_start(source, start - 1);
        if !source[above..start].trim().is_empty() {
            break;
        }
        start = above;
    }
    start
}

fn scan(source: &str) -> Result<File, ParseError> {
    let mut file = File {
        globals: Vec::new(),
        sections: Vec::new(),
    };
    let mut names = HashSet::new();
    let mut keys = HashSet::new();
    let mut at = 0;
    while at < source.len() {
        let line_number = || text::line_col(source, at).0 as usize;
        let end = text::next_line(source, at);
        let line_end = text::line_end(source, at);
        let content = source[at..line_end].trim();
        if content.is_empty() || content.starts_with([';', '#']) {
            at = end;
            continue;
        }

        if content.starts_with('[') {
            let name = content
                .strip_suffix(']')
                .map(|inner| inner[1..].trim())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| ParseError::on_line(line_number(), "expected `[section]`"))?;
            if !names.insert(name.to_string()) {
                return Err(ParseError::on_line(
                    line_number(),
                    format!("duplicate section `{name}`"),
                ));
            }
            keys.clear();
            file.sections.push(Section {
                name: name.to_string(),
                start: at,
                header_end: end,
                keys: Vec::new(),
            });
        } else {
            let key = key_line(source, at, line_end, end).ok_or_else(|| {
                ParseError::on_line(
                    line_number(),
                    "expected `key=value`, `[section]` or a comment",
                )
            })?;
            if !keys.insert(key.line.key.clone()) {
                return Err(ParseError::on_line(
                    line_number(),
                    format!("duplicate key `{}`", key.line.key),
                ));
            }
            match file.sections.last_mut() {
                Some(section) => section.keys.push(key),
                None => file.globals.push(key),
            }
        }
        at = end;
    }
    Ok(file)
}

/// The `key=value` / `key: value` line from `start` to `line_end` (line break excluded).
fn key_line(source: &str, start: usize, line_end: usize, end: usize) -> Option<Key> {
    let line = &source[start..line_end];
    let separator = line.find(['=', ':'])?;
    let key = line[..separator].trim();
    if key.is_empty() {
        return None;
    }
    let after = &line[separator + 1..];
    let value = after.trim();
    let value_start = line_end - after.trim_start().len();
    Some(Key {
        line: Line {
            key: key.to_string(),
            value: value.to_string(),
            start,
            end,
        },
        separator: start + separator,
        spaced: line[..separator].ends_with(char::is_whitespace)
            || after.starts_with(char::is_whitespace),
        value_start,
        value_end: value_start + value.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Entry, Format, Node, flat::infer};
    use super::parse;

    const GAME: &str = "; global settings\n\
        name = demo\n\
        debug=false\n\
        \n\
        ; database section\n\
        [database]\n\
        host = localhost\n\
        port = 5432\n\
        user =\n\
        \n\
        [cache]\n\
        enabled: true\n";

    /// Applies `document` and checks it reads back unchanged.
    fn apply(source: &str, document: &Node) -> String {
        let out = Format::Ini.apply(source, document).unwrap();
        assert_eq!(parse(&out).unwrap(), *document, "{out:?}");
        out
    }

    fn entries(document: &mut Node) -> &mut Vec<Entry> {
        match document {
            Node::Object { entries } => entries,
            _ => panic!("not an object"),
        }
    }

    /// The entries of `section` (the root for `None`).
    fn container<'a>(document: &'a mut Node, section: Option<&str>) -> &'a mut Vec<Entry> {
        let root = entries(document);
        match section {
            None => root,
            Some(name) => entries(
                &mut root
                    .iter_mut()
                    .find(|entry| entry.key == name)
                    .unwrap()
                    .value,
            ),
        }
    }

    /// `document` with `key` of `section` set to `value` (appended when missing).
    fn set(mut document: Node, section: Option<&str>, key: &str, value: &str) -> Node {
        let entries = container(&mut document, section);
        match entries.iter_mut().find(|entry| entry.key == key) {
            Some(entry) => entry.value = infer(value),
            None => entries.push(Entry::new(key, infer(value), None)),
        }
        document
    }

    fn object(entries: &[(&str, &str)]) -> Node {
        Node::Object {
            entries: entries
                .iter()
                .map(|(key, value)| Entry::new(*key, infer(value), None))
                .collect(),
        }
    }

    #[test]
    fn reads_globals_sections_and_comments() {
        let document = parse(GAME).unwrap();
        assert_eq!(
            document,
            Node::Object {
                entries: vec![
                    Entry::new("name", Node::string("demo"), None),
                    Entry::new("debug", Node::Boolean { value: false }, None),
                    Entry::new(
                        "database",
                        object(&[("host", "localhost"), ("port", "5432"), ("user", "")]),
                        None
                    ),
                    Entry::new("cache", object(&[("enabled", "true")]), None),
                ],
            }
        );
        let Node::Object { entries } = &document else {
            panic!()
        };
        assert_eq!(entries[0].comment.as_deref(), Some("global settings"));
        assert_eq!(entries[1].comment, None);
        assert_eq!(entries[2].comment.as_deref(), Some("database section"));
        assert_eq!(entries[3].comment, None);
    }

    #[test]
    fn unchanged_document_keeps_the_file() {
        assert_eq!(apply(GAME, &parse(GAME).unwrap()), GAME);
    }

    #[test]
    fn edits_only_the_changed_value() {
        let document = parse(GAME).unwrap();
        assert_eq!(
            apply(
                GAME,
                &set(document.clone(), Some("database"), "port", "5433")
            ),
            GAME.replace("port = 5432", "port = 5433")
        );
        assert_eq!(
            apply(
                GAME,
                &set(document.clone(), Some("database"), "user", "admin")
            ),
            GAME.replace("user =\n", "user = admin\n")
        );
        assert_eq!(
            apply(GAME, &set(document, None, "debug", "true")),
            GAME.replace("debug=false", "debug=true")
        );
        let source = "a=\nb: \n";
        let document = set(set(parse(source).unwrap(), None, "a", "1"), None, "b", "2");
        assert_eq!(apply(source, &document), "a=1\nb: 2\n");
    }

    #[test]
    fn adds_and_removes_keys_and_sections() {
        let mut document = set(parse(GAME).unwrap(), Some("database"), "timeout", "30");
        container(&mut document, None).retain(|entry| entry.key != "debug" && entry.key != "cache");
        container(&mut document, None).insert(1, Entry::new("mode", infer("fast"), None));
        container(&mut document, None).push(Entry::new("logs", object(&[("level", "info")]), None));
        assert_eq!(
            apply(GAME, &document),
            "; global settings\n\
            name = demo\n\
            mode = fast\n\
            \n\
            ; database section\n\
            [database]\n\
            host = localhost\n\
            port = 5432\n\
            user =\n\
            timeout = 30\n\
            \n\
            [logs]\n\
            level = info\n"
        );
    }

    #[test]
    fn new_globals_go_above_the_first_section_and_its_comment() {
        let source = "; header\n\n; db\n[db]\nhost=x\n";
        let mut document = parse(source).unwrap();
        container(&mut document, None).insert(0, Entry::new("name", infer("v"), None));
        assert_eq!(
            apply(source, &document),
            "; header\n\nname=v\n; db\n[db]\nhost=x\n"
        );
    }

    #[test]
    fn new_sections_in_new_files() {
        let document = Node::Object {
            entries: vec![
                Entry::new("top", infer("1"), None),
                Entry::new("s", object(&[("a", "1")]), None),
            ],
        };
        assert_eq!(apply("", &document), "top=1\n\n[s]\na=1\n");
        assert_eq!(
            apply("[t]", &set(parse("[t]").unwrap(), Some("t"), "k", "v")),
            "[t]\nk=v\n"
        );
    }

    #[test]
    fn reordered_keys_rewrite_their_section() {
        let source = "[a]\nx = 1\n; about y\ny = 2\n\n; b\n[b]\nz=3\n";
        let mut document = parse(source).unwrap();
        container(&mut document, Some("a")).reverse();
        assert_eq!(
            apply(source, &document),
            "[a]\ny = 2\nx = 1\n\n; b\n[b]\nz=3\n"
        );
    }

    #[test]
    fn reordered_sections_rewrite_the_file() {
        let source = "top=1\n[a]\nx=1\n; b\n[b]\ny=2\n";
        let mut document = parse(source).unwrap();
        container(&mut document, None).swap(1, 2);
        assert_eq!(apply(source, &document), "top=1\n\n[b]\ny=2\n\n[a]\nx=1\n");
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let source = "# Größe\r\n[s]\r\na=1\r\n";
        let mut document = set(
            set(parse(source).unwrap(), Some("s"), "a", "ü"),
            Some("s"),
            "b",
            "x",
        );
        container(&mut document, None).push(Entry::new("t", object(&[("c", "y")]), None));
        assert_eq!(
            apply(source, &document),
            "# Größe\r\n[s]\r\na=ü\r\nb=x\r\n\r\n[t]\r\nc=y\r\n"
        );
    }

    #[test]
    fn invalid_files_report_the_line() {
        assert_eq!(parse("[a]\nx=1\n\nx = 2\n").unwrap_err().line, Some(4));
        assert_eq!(parse("[a]\n[b]\n[ a ]\n").unwrap_err().line, Some(3));
        assert_eq!(parse("a=1\njust text\n").unwrap_err().line, Some(2));
        assert_eq!(parse("[open\n").unwrap_err().line, Some(1));
        assert!(parse("x=1\n[a]\nx=2\n[b]\nx=3\n").is_ok());
    }
}
