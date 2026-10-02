//! JSON / JSONC: a span-keeping parser (tolerating `//` and `/* */` comments and trailing
//! commas) and a writer that applies document changes as minimal text edits.
use std::collections::{HashMap, HashSet};

use super::{Entry, Node, ParseError, rules, text};

const BOM: char = '\u{feff}';
/// Deeper documents are rejected instead of risking the stack.
const MAX_DEPTH: usize = 256;
/// Arrays whose changed middle part is larger than this (old × new items) skip the LCS
/// alignment and pair items positionally.
const LCS_LIMIT: usize = 1 << 20;

pub fn parse(source: &str) -> Result<Node, ParseError> {
    parse_tree(source).map(|(node, _)| node)
}

pub fn patch(source: &str, current: &Node, document: &Node) -> String {
    let Ok((_, root)) = parse_tree(source) else {
        return emit(source, document);
    };
    let mut patcher = Patcher {
        source,
        style: Style::of(source),
        edits: text::Edits::default(),
    };
    patcher.value(&root, current, document, true);
    patcher
        .edits
        .apply(source)
        .unwrap_or_else(|| emit(source, document))
}

pub fn emit(source: &str, document: &Node) -> String {
    let style = Style::of(source);
    let mut out = String::new();
    if source.starts_with(BOM) {
        out.push(BOM);
    }
    style.write(&mut out, document, "", true);
    out.push_str(style.eol);
    out
}

/// JSON number literals with a fraction or exponent are kept as written, whole numbers get
/// `.0`, other finite numbers Rust can read are written canonically; NaN and infinities have
/// no JSON literal.
pub fn float_text(value: &str) -> Option<String> {
    if scan_number(value.as_bytes(), 0) == Some((value.len(), false)) {
        return Some(value.to_string());
    }
    if rules::is_integer(value) {
        return Some(format!("{value}.0"));
    }
    let float = value
        .parse::<f64>()
        .ok()
        .filter(|float| float.is_finite())?;
    Some(format!("{float:?}"))
}

/// Byte positions of a parsed value.
struct Span {
    start: usize,
    end: usize,
    shape: Shape,
}

enum Shape {
    Scalar,
    Object(Vec<Member>),
    Array(Vec<Member>),
}

/// An object entry or array item.
struct Member {
    /// The key literal of an entry, the value of an item.
    start: usize,
    /// Start of the member's lines: the comment block above its key, else its own line.
    lines_from: usize,
    value: Span,
    comma: Option<usize>,
}

impl Member {
    /// End of the member including its comma.
    fn after(&self) -> usize {
        self.comma.map_or(self.value.end, |comma| comma + 1)
    }
}

fn parse_tree(source: &str) -> Result<(Node, Span), ParseError> {
    Parser {
        source,
        bytes: source.as_bytes(),
        pos: 0,
    }
    .document()
}

struct Parser<'s> {
    source: &'s str,
    bytes: &'s [u8],
    pos: usize,
}

impl Parser<'_> {
    fn document(mut self) -> Result<(Node, Span), ParseError> {
        if self.source.starts_with(BOM) {
            self.pos = BOM.len_utf8();
        }
        self.skip_trivia()?;
        match self.peek() {
            None => return Err(ParseError::at(self.source, 0, "the file is empty")),
            Some(b'{' | b'[') => {}
            Some(_) => return Err(self.error("the document must be an object or an array")),
        }
        let root = self.value(0)?;
        self.skip_trivia()?;
        if self.pos < self.bytes.len() {
            return Err(self.error("unexpected content after the document"));
        }
        Ok(root)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::at(self.source, self.pos, message)
    }

    /// Skips whitespace and comments.
    fn skip_trivia(&mut self) -> Result<(), ParseError> {
        loop {
            match (self.peek(), self.bytes.get(self.pos + 1).copied()) {
                (Some(b' ' | b'\t' | b'\n' | b'\r'), _) => self.pos += 1,
                (Some(b'/'), Some(b'/')) => {
                    self.pos = self.source[self.pos..]
                        .find('\n')
                        .map_or(self.bytes.len(), |index| self.pos + index);
                }
                (Some(b'/'), Some(b'*')) => match self.source[self.pos + 2..].find("*/") {
                    Some(index) => self.pos += index + 4,
                    None => return Err(self.error("unterminated comment")),
                },
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<(Node, Span), ParseError> {
        let start = self.pos;
        let (node, shape) = match self.peek() {
            Some(b'{') => self.object(depth)?,
            Some(b'[') => self.array(depth)?,
            Some(b'"') => (
                Node::String {
                    value: self.string()?,
                },
                Shape::Scalar,
            ),
            Some(b'-' | b'0'..=b'9') => (self.number()?, Shape::Scalar),
            Some(_) => (self.keyword()?, Shape::Scalar),
            None => return Err(self.error("unexpected end of file")),
        };
        let span = Span {
            start,
            end: self.pos,
            shape,
        };
        Ok((node, span))
    }

    fn object(&mut self, depth: usize) -> Result<(Node, Shape), ParseError> {
        self.open(depth)?;
        let mut entries = Vec::new();
        let mut members = Vec::new();
        let mut keys = HashSet::new();
        loop {
            self.skip_trivia()?;
            match self.peek() {
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                Some(b'"') => {}
                Some(_) => return Err(self.error("expected a key or `}`")),
                None => return Err(self.error("unexpected end of file")),
            }
            let start = self.pos;
            let key = self.string()?;
            if !keys.insert(key.clone()) {
                return Err(ParseError::at(
                    self.source,
                    start,
                    format!("duplicate key `{key}`"),
                ));
            }
            self.skip_trivia()?;
            if self.peek() != Some(b':') {
                return Err(self.error("expected `:` after the key"));
            }
            self.pos += 1;
            self.skip_trivia()?;
            let (value, span) = self.value(depth + 1)?;
            let (comma, closed) = self.separator(b'}')?;

            let line = text::line_start(self.source, start);
            let comment = if text::starts_line(self.source, start) {
                comment_above(self.source, line)
            } else {
                None
            };
            let lines_from = comment.as_ref().map_or(line, |(at, _)| *at);
            entries.push(Entry::new(key, value, comment.map(|(_, body)| body)));
            members.push(Member {
                start,
                lines_from,
                value: span,
                comma,
            });
            if closed {
                break;
            }
        }
        Ok((Node::Object { entries }, Shape::Object(members)))
    }

    fn array(&mut self, depth: usize) -> Result<(Node, Shape), ParseError> {
        self.open(depth)?;
        let mut items = Vec::new();
        let mut members = Vec::new();
        loop {
            self.skip_trivia()?;
            if self.peek() == Some(b']') {
                self.pos += 1;
                break;
            }
            let (value, span) = self.value(depth + 1)?;
            let (comma, closed) = self.separator(b']')?;
            items.push(value);
            members.push(Member {
                start: span.start,
                lines_from: text::line_start(self.source, span.start),
                value: span,
                comma,
            });
            if closed {
                break;
            }
        }
        Ok((Node::Array { items }, Shape::Array(members)))
    }

    /// Consumes the opening bracket of a container at `depth`.
    fn open(&mut self, depth: usize) -> Result<(), ParseError> {
        if depth >= MAX_DEPTH {
            return Err(self.error("the document is nested too deeply"));
        }
        self.pos += 1;
        Ok(())
    }

    /// After a member: its comma (position, not closed) or the consumed closing bracket.
    fn separator(&mut self, close: u8) -> Result<(Option<usize>, bool), ParseError> {
        self.skip_trivia()?;
        match self.peek() {
            Some(b',') => {
                self.pos += 1;
                Ok((Some(self.pos - 1), false))
            }
            Some(byte) if byte == close => {
                self.pos += 1;
                Ok((None, true))
            }
            _ => Err(self.error(format!("expected `,` or `{}`", close as char))),
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        self.pos += 1;
        let mut value = String::new();
        let mut run = self.pos;
        loop {
            match self.peek() {
                Some(b'"') => {
                    value.push_str(&self.source[run..self.pos]);
                    self.pos += 1;
                    return Ok(value);
                }
                Some(b'\\') => {
                    value.push_str(&self.source[run..self.pos]);
                    value.push(self.escape()?);
                    run = self.pos;
                }
                Some(b'\n') | None => {
                    return Err(ParseError::at(self.source, start, "unterminated string"));
                }
                Some(0x00..=0x1f) => {
                    return Err(self.error("control characters must be escaped in strings"));
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    /// Decodes the escape sequence at the backslash under the cursor.
    fn escape(&mut self) -> Result<char, ParseError> {
        let start = self.pos;
        let escaped = match self.bytes.get(start + 1).copied() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.pos += 2;
                return self
                    .unicode_escape()
                    .ok_or_else(|| ParseError::at(self.source, start, "invalid unicode escape"));
            }
            _ => return Err(self.error("invalid escape sequence")),
        };
        self.pos += 2;
        Ok(escaped)
    }

    /// The character of a `\u` escape whose hex digits start at the cursor, combining a
    /// UTF-16 surrogate pair.
    fn unicode_escape(&mut self) -> Option<char> {
        let first = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&first) {
            if !self.source[self.pos..].starts_with("\\u") {
                return None;
            }
            self.pos += 2;
            let low = self.hex4().filter(|low| (0xDC00..0xE000).contains(low))?;
            0x10000 + ((first - 0xD800) << 10) + (low - 0xDC00)
        } else {
            first
        };
        char::from_u32(code)
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.source.get(self.pos..self.pos + 4)?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let code = u32::from_str_radix(digits, 16).ok()?;
        self.pos += 4;
        Some(code)
    }

    fn number(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        let Some((end, integer)) = scan_number(self.bytes, start).filter(|(end, _)| {
            !self
                .bytes
                .get(*end)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'.')
        }) else {
            return Err(self.error("invalid number"));
        };
        self.pos = end;
        let value = self.source[start..end].to_string();
        Ok(if integer {
            Node::Integer { value }
        } else {
            Node::Float { value }
        })
    }

    fn keyword(&mut self) -> Result<Node, ParseError> {
        let rest = &self.source[self.pos..];
        let keywords = [
            ("true", Node::Boolean { value: true }),
            ("false", Node::Boolean { value: false }),
            ("null", Node::Null),
        ];
        let Some((word, node)) = keywords
            .into_iter()
            .find(|(word, _)| rest.starts_with(*word))
        else {
            let found = rest.chars().next().unwrap_or_default();
            return Err(self.error(format!("unexpected character `{found}`")));
        };
        self.pos += word.len();
        Ok(node)
    }
}

/// The end of the JSON number literal at `start` and whether it is an integer
/// (`-?(0|[1-9][0-9]*)` without fraction or exponent).
fn scan_number(bytes: &[u8], start: usize) -> Option<(usize, bool)> {
    let digits_end = |from: usize| {
        from + bytes[from..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let mut at = start;
    if bytes.get(at) == Some(&b'-') {
        at += 1;
    }
    match bytes.get(at).copied() {
        Some(b'0') => at += 1,
        Some(b'1'..=b'9') => at = digits_end(at + 1),
        _ => return None,
    }
    let mut integer = true;
    if bytes.get(at) == Some(&b'.') {
        let end = digits_end(at + 1);
        if end == at + 1 {
            return None;
        }
        at = end;
        integer = false;
    }
    if matches!(bytes.get(at).copied(), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at).copied(), Some(b'+' | b'-')) {
            at += 1;
        }
        let end = digits_end(at);
        if end == at {
            return None;
        }
        at = end;
        integer = false;
    }
    Some((at, integer))
}

/// The comment directly above the line starting at `line` (a `/* */` block or `//` lines):
/// the start of its first line and its text.
fn comment_above(source: &str, line: usize) -> Option<(usize, String)> {
    block_comment_above(source, line).or_else(|| {
        let body = text::comment_above(source, line, &["//"])?;
        let mut start = line;
        while start > 0 {
            let above = text::line_start(source, start - 1);
            if !source[above..start].trim_start().starts_with("//") {
                break;
            }
            start = above;
        }
        Some((start, body))
    })
}

/// A `/* */` comment that starts its line and ends the line above `line`. Lines lose their
/// indentation, a leading `*` and one space; blank first and last lines are dropped.
fn block_comment_above(source: &str, line: usize) -> Option<(usize, String)> {
    if line == 0 {
        return None;
    }
    let above = text::line_start(source, line - 1);
    let previous = source[above..line].trim_end();
    if !previous.ends_with("*/") {
        return None;
    }
    let close = above + previous.len() - 2;
    let open = source[..close].rfind("/*")?;
    let body = &source[open + 2..close];
    if !text::starts_line(source, open) || body.contains("*/") {
        return None;
    }
    let lines: Vec<&str> = body
        .lines()
        .map(|row| {
            let row = row.trim();
            let row = row.strip_prefix('*').unwrap_or(row);
            row.strip_prefix(' ').unwrap_or(row)
        })
        .collect();
    let first = lines.iter().position(|row| !row.is_empty())?;
    let last = lines.iter().rposition(|row| !row.is_empty())?;
    Some((
        text::line_start(source, open),
        lines[first..=last].join("\n"),
    ))
}

/// How new text is laid out: the file's indent unit and line ending.
struct Style {
    unit: String,
    eol: &'static str,
}

impl Style {
    fn of(source: &str) -> Self {
        Self {
            unit: indent_unit(source),
            eol: text::eol(source),
        }
    }

    /// Writes `node` starting at the current column of a line indented by `indent`.
    fn write(&self, out: &mut String, node: &Node, indent: &str, multiline: bool) {
        match node {
            Node::Object { entries } => self.container(
                out,
                ('{', '}'),
                entries
                    .iter()
                    .map(|entry| (Some(entry.key.as_str()), &entry.value)),
                indent,
                multiline,
            ),
            Node::Array { items } => self.container(
                out,
                ('[', ']'),
                items.iter().map(|item| (None::<&str>, item)),
                indent,
                multiline,
            ),
            Node::String { value } | Node::Datetime { value } => push_quoted(out, value),
            Node::Integer { value } | Node::Float { value } => out.push_str(value),
            Node::Boolean { value } => out.push_str(if *value { "true" } else { "false" }),
            Node::Null => out.push_str("null"),
        }
    }

    fn container<'n>(
        &self,
        out: &mut String,
        (open, close): (char, char),
        members: impl Iterator<Item = (Option<&'n str>, &'n Node)>,
        indent: &str,
        multiline: bool,
    ) {
        out.push(open);
        let inner = format!("{indent}{}", self.unit);
        let mut empty = true;
        for (key, value) in members {
            if !empty {
                out.push(',');
                if !multiline {
                    out.push(' ');
                }
            }
            if multiline {
                out.push_str(self.eol);
                out.push_str(&inner);
            }
            self.member(out, key, value, &inner, multiline);
            empty = false;
        }
        if multiline && !empty {
            out.push_str(self.eol);
            out.push_str(indent);
        }
        out.push(close);
    }

    /// An object entry (`"key": value`) or array item, on a line indented by `indent`.
    fn member(
        &self,
        out: &mut String,
        key: Option<&str>,
        value: &Node,
        indent: &str,
        multiline: bool,
    ) {
        if let Some(key) = key {
            push_quoted(out, key);
            out.push_str(": ");
        }
        self.write(out, value, indent, multiline);
    }
}

/// Tabs or the leading spaces of the first indented line that is not a comment; 2 spaces
/// when nothing is indented.
fn indent_unit(source: &str) -> String {
    source
        .lines()
        .find_map(|line| {
            let content = line.trim_start_matches([' ', '\t']);
            if content.len() == line.len()
                || content.trim().is_empty()
                || content.starts_with(['*', '/'])
            {
                return None;
            }
            Some(if line.starts_with('\t') {
                "\t".to_string()
            } else {
                " ".repeat(line.len() - line.trim_start_matches(' ').len())
            })
        })
        .unwrap_or_else(|| "  ".to_string())
}

fn push_quoted(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// One step of turning a container's old members into its new ones, in order.
#[derive(Clone, Copy)]
enum Step {
    /// Old member patched into the new member (both indices).
    Keep(usize, usize),
    Remove(usize),
    Insert(usize),
}

struct Patcher<'s> {
    source: &'s str,
    style: Style,
    edits: text::Edits,
}

impl Patcher<'_> {
    /// `parent_multiline`: whether the enclosing container spans lines, which decides the
    /// layout of values that had no layout of their own (scalars, empty containers).
    fn value(&mut self, span: &Span, current: &Node, document: &Node, parent_multiline: bool) {
        if current == document {
            return;
        }
        let patched = match (&span.shape, current, document) {
            (
                Shape::Object(members),
                Node::Object { entries: old },
                Node::Object { entries: new },
            ) => self.object(span, members, old, new),
            (Shape::Array(members), Node::Array { items: old }, Node::Array { items: new }) => {
                self.array(span, members, old, new)
            }
            _ => false,
        };
        if !patched {
            self.reemit(span, document, parent_multiline);
        }
    }

    fn reemit(&mut self, span: &Span, document: &Node, parent_multiline: bool) {
        let source = self.source;
        let multiline = match &span.shape {
            Shape::Object(members) | Shape::Array(members) if !members.is_empty() => {
                source[span.start..span.end].contains('\n')
            }
            _ => parent_multiline,
        };
        let mut out = String::new();
        self.style.write(
            &mut out,
            document,
            text::indentation(source, span.start),
            multiline,
        );
        self.edits.replace(span.start, span.end, out);
    }

    fn object(&mut self, span: &Span, members: &[Member], old: &[Entry], new: &[Entry]) -> bool {
        let Some(steps) = match_keys(old, new) else {
            return false;
        };
        let old: Vec<&Node> = old.iter().map(|entry| &entry.value).collect();
        let new: Vec<(Option<&str>, &Node)> = new
            .iter()
            .map(|entry| (Some(entry.key.as_str()), &entry.value))
            .collect();
        self.patch_members(span, members, &old, &new, &steps)
    }

    fn array(&mut self, span: &Span, members: &[Member], old: &[Node], new: &[Node]) -> bool {
        let steps = align(old, new);
        let old: Vec<&Node> = old.iter().collect();
        let new: Vec<(Option<&str>, &Node)> = new.iter().map(|item| (None, item)).collect();
        self.patch_members(span, members, &old, &new, &steps)
    }

    /// Applies `steps` to a container in place; `false` (with nothing edited) when the
    /// container has to be re-emitted instead.
    fn patch_members(
        &mut self,
        span: &Span,
        members: &[Member],
        old: &[&Node],
        new: &[(Option<&str>, &Node)],
        steps: &[Step],
    ) -> bool {
        let source = self.source;
        let structural = steps.iter().any(|step| !matches!(step, Step::Keep(..)));
        if members.len() != old.len()
            || (structural && (new.is_empty() || !line_based(source, span, members)))
        {
            return false;
        }
        let multiline = source[span.start..span.end].contains('\n');
        for step in steps {
            if let Step::Keep(from, to) = *step {
                self.value(&members[from].value, old[from], new[to].1, multiline);
            }
        }
        if !structural {
            return true;
        }

        let indent = text::indentation(source, members[0].start);
        let trailing = members[members.len() - 1].comma.is_some();
        let last = new.len() - 1;
        let mut anchor = members[0].lines_from;
        let mut lines = String::new();
        for step in steps {
            match *step {
                Step::Keep(from, to) => {
                    self.insert_lines(anchor, &mut lines);
                    let member = &members[from];
                    match (member.comma, to < last || trailing) {
                        (Some(comma), false) => self.edits.delete(comma, comma + 1),
                        (None, true) => self.edits.insert(member.value.end, ","),
                        _ => {}
                    }
                    anchor = text::next_line(source, member.after());
                }
                Step::Remove(from) => {
                    let member = &members[from];
                    let end = text::next_line(source, member.after());
                    self.edits.delete(member.lines_from, end);
                }
                Step::Insert(to) => {
                    let (key, value) = new[to];
                    lines.push_str(indent);
                    self.style.member(&mut lines, key, value, indent, true);
                    if to < last || trailing {
                        lines.push(',');
                    }
                    lines.push_str(self.style.eol);
                }
            }
        }
        self.insert_lines(anchor, &mut lines);
        true
    }

    fn insert_lines(&mut self, at: usize, lines: &mut String) {
        if !lines.is_empty() {
            self.edits.insert(at, std::mem::take(lines));
        }
    }
}

/// Every member (and the opening bracket) ends its line, apart from comments, and every
/// member starts its own line, so members can be removed and inserted as whole lines.
fn line_based(source: &str, span: &Span, members: &[Member]) -> bool {
    !members.is_empty()
        && trivia_until_eol(source, span.start + 1)
        && members.iter().all(|member| {
            text::starts_line(source, member.start) && trivia_until_eol(source, member.after())
        })
}

/// Whether only whitespace and comments that end on this line follow `at` on its line.
fn trivia_until_eol(source: &str, at: usize) -> bool {
    let rest = source[at..text::line_end(source, at)].trim();
    rest.is_empty()
        || rest.starts_with("//")
        || rest
            .strip_prefix("/*")
            .is_some_and(|body| body.ends_with("*/") && !body[..body.len() - 2].contains("*/"))
}

/// Old entries matched to new ones by key; `None` when kept keys changed their order.
fn match_keys(old: &[Entry], new: &[Entry]) -> Option<Vec<Step>> {
    let positions: HashMap<&str, usize> = old
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.key.as_str(), index))
        .collect();
    let mut steps = Vec::with_capacity(old.len() + new.len());
    let mut next = 0;
    for (index, entry) in new.iter().enumerate() {
        match positions.get(entry.key.as_str()) {
            Some(&found) if found >= next => {
                steps.extend((next..found).map(Step::Remove));
                steps.push(Step::Keep(found, index));
                next = found + 1;
            }
            Some(_) => return None,
            None => steps.push(Step::Insert(index)),
        }
    }
    steps.extend((next..old.len()).map(Step::Remove));
    Some(steps)
}

/// Equal items are matched (longest common subsequence); between matches, remaining items are
/// paired positionally and the rest removed or inserted.
fn align(old: &[Node], new: &[Node]) -> Vec<Step> {
    let mut steps = Vec::with_capacity(old.len().max(new.len()));
    let (mut from, mut to) = (0, 0);
    for (old_index, new_index) in equal_pairs(old, new) {
        pair_gap(&mut steps, from..old_index, to..new_index);
        steps.push(Step::Keep(old_index, new_index));
        (from, to) = (old_index + 1, new_index + 1);
    }
    pair_gap(&mut steps, from..old.len(), to..new.len());
    steps
}

fn pair_gap(steps: &mut Vec<Step>, old: std::ops::Range<usize>, new: std::ops::Range<usize>) {
    let paired = old.len().min(new.len());
    steps.extend(
        old.clone()
            .zip(new.clone())
            .map(|(from, to)| Step::Keep(from, to)),
    );
    steps.extend(old.skip(paired).map(Step::Remove));
    steps.extend(new.skip(paired).map(Step::Insert));
}

/// Index pairs of equal items: the common prefix and suffix plus the LCS of the middle.
fn equal_pairs(old: &[Node], new: &[Node]) -> Vec<(usize, usize)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    let mut pairs: Vec<(usize, usize)> = (0..prefix).map(|index| (index, index)).collect();
    pairs.extend(
        lcs(&old[prefix..old_end], &new[prefix..new_end])
            .into_iter()
            .map(|(from, to)| (from + prefix, to + prefix)),
    );
    pairs.extend((0..suffix).map(|index| (old_end + index, new_end + index)));
    pairs
}

fn lcs(old: &[Node], new: &[Node]) -> Vec<(usize, usize)> {
    if old.is_empty() || new.is_empty() || old.len().saturating_mul(new.len()) > LCS_LIMIT {
        return Vec::new();
    }
    // lengths[i * width + j]: LCS length of old[i..] and new[j..].
    let width = new.len() + 1;
    let mut lengths = vec![0u32; (old.len() + 1) * width];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lengths[i * width + j] = if old[i] == new[j] {
                lengths[(i + 1) * width + j + 1] + 1
            } else {
                lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
            };
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::*;

    const SETTINGS: &str = r#"// Editor settings
{
  // Font size in pixels
  "editor.fontSize": 14,
  /* Tab width
   * in spaces */
  "editor.tabSize": 4,
  "files.exclude": {
    "**/.git": true, // hide git
    "**/node_modules": true,
  },
  "window.title": "${activeEditorShort}",
}
"#;

    const PACKAGE: &str = r#"{
  "name": "my-mod",
  "version": "1.0.0",
  "scripts": {
    "build": "tsc",
    "test": "node --test"
  },
  "dependencies": {
    "left-pad": "^1.3.0"
  },
  "keywords": ["minecraft", "mod"],
  "private": true
}
"#;

    /// Indented with tabs, see `tabs`.
    const MOD_CONFIG: &str = r#"{
    // Mod configuration
    "enabled": true,
    "spawnRate": 0.25,
    "blockedItems": [
        "minecraft:tnt",
        "minecraft:bedrock", // never obtainable
    ],
    "limits": { "maxPlayers": 20 },
}
"#;

    fn tabs(source: &str) -> String {
        source.replace("    ", "\t")
    }

    /// `Format::apply`, asserting the round-trip invariant.
    fn apply(source: &str, document: &Node) -> String {
        let out = Format::Json.apply(source, document).unwrap();
        assert_eq!(parse(&out).as_ref(), Ok(document), "{out}");
        out
    }

    fn object(entries: Vec<(&str, Node)>) -> Node {
        Node::Object {
            entries: entries
                .into_iter()
                .map(|(key, value)| Entry::new(key, value, None))
                .collect(),
        }
    }

    fn int(value: &str) -> Node {
        Node::Integer {
            value: value.into(),
        }
    }

    fn entries(node: &mut Node) -> &mut Vec<Entry> {
        let Node::Object { entries } = node else {
            panic!("not an object");
        };
        entries
    }

    fn items(node: &mut Node) -> &mut Vec<Node> {
        let Node::Array { items } = node else {
            panic!("not an array");
        };
        items
    }

    fn entry<'a>(node: &'a mut Node, key: &str) -> &'a mut Node {
        &mut entries(node)
            .iter_mut()
            .find(|entry| entry.key == key)
            .unwrap()
            .value
    }

    fn comments(source: &str) -> Vec<(String, Option<String>)> {
        let mut document = parse(source).unwrap();
        entries(&mut document)
            .iter()
            .map(|entry| (entry.key.clone(), entry.comment.clone()))
            .collect()
    }

    #[test]
    fn parses_jsonc_with_comments_and_trailing_commas() {
        assert_eq!(
            comments(SETTINGS),
            vec![
                (
                    "editor.fontSize".to_string(),
                    Some("Font size in pixels".to_string())
                ),
                (
                    "editor.tabSize".to_string(),
                    Some("Tab width\nin spaces".to_string())
                ),
                ("files.exclude".to_string(), None),
                ("window.title".to_string(), None),
            ]
        );
        let mut document = parse(SETTINGS).unwrap();
        assert_eq!(*entry(&mut document, "editor.tabSize"), int("4"));
        assert_eq!(
            *entry(&mut document, "window.title"),
            Node::string("${activeEditorShort}")
        );
        assert_eq!(
            *entry(&mut document, "files.exclude"),
            object(vec![
                ("**/.git", Node::Boolean { value: true }),
                ("**/node_modules", Node::Boolean { value: true }),
            ])
        );
    }

    #[test]
    fn comment_blocks_must_touch_the_key() {
        let source = r#"{
  /**
   * Maximum players
   */
  "max": 10,

  // detached

  "motd": "hi", // about motd
  "pvp": true
}
"#;
        assert_eq!(
            comments(source),
            vec![
                ("max".to_string(), Some("Maximum players".to_string())),
                ("motd".to_string(), None),
                ("pvp".to_string(), None),
            ]
        );
        assert!(
            comments(PACKAGE)
                .iter()
                .all(|(_, comment)| comment.is_none())
        );
    }

    #[test]
    fn unchanged_document_keeps_every_byte() {
        for source in [SETTINGS, PACKAGE, MOD_CONFIG] {
            let document = parse(source).unwrap();
            assert_eq!(Format::Json.apply(source, &document).unwrap(), source);
            assert_eq!(patch(source, &document, &document), source);
        }
    }

    #[test]
    fn scalar_edits_keep_comments_and_layout() {
        let mut document = parse(SETTINGS).unwrap();
        *entry(&mut document, "editor.fontSize") = int("16");
        *entry(&mut document, "window.title") = Node::string("say \"hi\"\tnow");
        assert_eq!(
            apply(SETTINGS, &document),
            SETTINGS
                .replace("\"editor.fontSize\": 14", "\"editor.fontSize\": 16")
                .replace("\"${activeEditorShort}\"", r#""say \"hi\"\tnow""#)
        );
    }

    #[test]
    fn removing_a_key_removes_its_lines_and_its_comment() {
        let mut document = parse(SETTINGS).unwrap();
        entries(&mut document).remove(0);
        assert_eq!(
            apply(SETTINGS, &document),
            SETTINGS.replace("  // Font size in pixels\n  \"editor.fontSize\": 14,\n", "")
        );

        let mut document = parse(SETTINGS).unwrap();
        entries(&mut document).pop();
        assert_eq!(
            apply(SETTINGS, &document),
            SETTINGS.replace("  \"window.title\": \"${activeEditorShort}\",\n", "")
        );
    }

    #[test]
    fn keys_inserted_first_go_above_the_first_comment() {
        let mut document = parse(SETTINGS).unwrap();
        entries(&mut document).insert(0, Entry::new("editor.wordWrap", Node::string("on"), None));
        assert_eq!(
            apply(SETTINGS, &document),
            SETTINGS.replace(
                "{\n  // Font size",
                "{\n  \"editor.wordWrap\": \"on\",\n  // Font size"
            )
        );

        let source = "{\n  // Section\n\n  // about a\n  \"a\": 1\n}\n";
        let mut document = parse(source).unwrap();
        entries(&mut document).insert(0, Entry::new("z", int("2"), None));
        assert_eq!(
            apply(source, &document),
            "{\n  // Section\n\n  \"z\": 2,\n  // about a\n  \"a\": 1\n}\n"
        );
        entries(&mut document).remove(1);
        assert_eq!(
            apply(source, &document),
            "{\n  // Section\n\n  \"z\": 2\n}\n"
        );
    }

    #[test]
    fn nested_edits_follow_trailing_comma_style() {
        let mut document = parse(SETTINGS).unwrap();
        entries(&mut document).remove(1);
        entries(entry(&mut document, "files.exclude")).push(Entry::new(
            "**/dist",
            Node::Boolean { value: true },
            None,
        ));
        assert_eq!(
            apply(SETTINGS, &document),
            r#"// Editor settings
{
  // Font size in pixels
  "editor.fontSize": 14,
  "files.exclude": {
    "**/.git": true, // hide git
    "**/node_modules": true,
    "**/dist": true,
  },
  "window.title": "${activeEditorShort}",
}
"#
        );
    }

    #[test]
    fn commas_follow_entries_without_trailing_commas() {
        let mut document = parse(PACKAGE).unwrap();
        entries(&mut document).pop();
        entries(entry(&mut document, "dependencies")).clear();
        entries(entry(&mut document, "scripts")).push(Entry::new(
            "lint",
            Node::string("eslint ."),
            None,
        ));
        assert_eq!(
            apply(PACKAGE, &document),
            r#"{
  "name": "my-mod",
  "version": "1.0.0",
  "scripts": {
    "build": "tsc",
    "test": "node --test",
    "lint": "eslint ."
  },
  "dependencies": {},
  "keywords": ["minecraft", "mod"]
}
"#
        );

        let mut document = parse(PACKAGE).unwrap();
        *entry(&mut document, "version") = Node::string("1.1.0");
        items(entry(&mut document, "keywords")).remove(0);
        entries(&mut document).push(Entry::new("license", Node::string("MIT"), None));
        assert_eq!(
            apply(PACKAGE, &document),
            r#"{
  "name": "my-mod",
  "version": "1.1.0",
  "scripts": {
    "build": "tsc",
    "test": "node --test"
  },
  "dependencies": {
    "left-pad": "^1.3.0"
  },
  "keywords": ["mod"],
  "private": true,
  "license": "MIT"
}
"#
        );
    }

    #[test]
    fn single_line_containers_edit_in_place_or_reemit() {
        let source = "{\"list\": [1,2,  3], \"on\": true}\n";

        let mut document = parse(source).unwrap();
        items(entry(&mut document, "list"))[1] = int("20");
        assert_eq!(
            apply(source, &document),
            "{\"list\": [1,20,  3], \"on\": true}\n"
        );

        let mut document = parse(source).unwrap();
        items(entry(&mut document, "list")).remove(0);
        assert_eq!(
            apply(source, &document),
            "{\"list\": [2, 3], \"on\": true}\n"
        );

        let mut document = parse(source).unwrap();
        entries(&mut document).push(Entry::new("name", Node::string("x"), None));
        assert_eq!(
            apply(source, &document),
            "{\"list\": [1, 2, 3], \"on\": true, \"name\": \"x\"}\n"
        );
    }

    #[test]
    fn tab_indented_config_edits() {
        let source = tabs(MOD_CONFIG);
        let mut document = parse(&source).unwrap();
        let blocked = items(entry(&mut document, "blockedItems"));
        blocked.remove(0);
        blocked.push(Node::string("minecraft:barrier"));
        entries(entry(&mut document, "limits")).push(Entry::new("maxMobs", int("100"), None));
        *entry(&mut document, "spawnRate") = Node::Float { value: "1".into() };
        entries(&mut document).push(Entry::new(
            "spawn",
            object(vec![
                ("x", int("1")),
                (
                    "y",
                    Node::Array {
                        items: vec![int("64")],
                    },
                ),
            ]),
            None,
        ));

        let out = Format::Json.apply(&source, &document).unwrap();
        *entry(&mut document, "spawnRate") = Node::Float {
            value: "1.0".into(),
        };
        assert_eq!(parse(&out).unwrap(), document);
        assert_eq!(
            out,
            tabs(
                r#"{
    // Mod configuration
    "enabled": true,
    "spawnRate": 1.0,
    "blockedItems": [
        "minecraft:bedrock", // never obtainable
        "minecraft:barrier",
    ],
    "limits": {"maxPlayers": 20, "maxMobs": 100},
    "spawn": {
        "x": 1,
        "y": [
            64
        ]
    },
}
"#
            )
        );
    }

    #[test]
    fn removing_the_last_item_keeps_the_trailing_comma_style() {
        let source = tabs(MOD_CONFIG);
        let mut document = parse(&source).unwrap();
        items(entry(&mut document, "blockedItems")).pop();
        assert_eq!(
            apply(&source, &document),
            source.replace("\t\t\"minecraft:bedrock\", // never obtainable\n", "")
        );
    }

    #[test]
    fn multi_line_arrays_align_items() {
        let source = "[\n  1,\n  2,\n  3\n]\n";
        let document = Node::Array {
            items: vec![int("1"), int("5"), int("3"), int("4")],
        };
        assert_eq!(apply(source, &document), "[\n  1,\n  5,\n  3,\n  4\n]\n");

        let document = Node::Array {
            items: vec![int("1"), int("2")],
        };
        assert_eq!(apply(source, &document), "[\n  1,\n  2\n]\n");
    }

    #[test]
    fn crlf_is_kept_for_new_lines() {
        let source = "{\r\n  \"a\": 1,\r\n  \"b\": [\r\n    1\r\n  ]\r\n}\r\n";
        let mut document = parse(source).unwrap();
        *entry(&mut document, "a") = int("2");
        items(entry(&mut document, "b")).push(int("2"));
        entries(&mut document).push(Entry::new("c", Node::Boolean { value: true }, None));
        assert_eq!(
            apply(source, &document),
            "{\r\n  \"a\": 2,\r\n  \"b\": [\r\n    1,\r\n    2\r\n  ],\r\n  \"c\": true\r\n}\r\n"
        );
    }

    #[test]
    fn reemitted_values_use_the_file_style() {
        let source = "[\r\n\t1\r\n]\r\n";
        let document = object(vec![
            (
                "a",
                Node::Array {
                    items: vec![int("1"), object(vec![])],
                },
            ),
            ("b", Node::string("x")),
        ]);
        let expected = "{\r\n\t\"a\": [\r\n\t\t1,\r\n\t\t{}\r\n\t],\r\n\t\"b\": \"x\"\r\n}\r\n";
        assert_eq!(apply(source, &document), expected);
        assert_eq!(emit(source, &document), expected);
    }

    #[test]
    fn reordered_keys_reemit_only_their_object() {
        let source = r#"{
  // server
  "server": {
    "port": 8080, // listen port
    "host": "0.0.0.0"
  },
  "debug": false
}
"#;
        let mut document = parse(source).unwrap();
        entries(entry(&mut document, "server")).reverse();
        assert_eq!(
            apply(source, &document),
            r#"{
  // server
  "server": {
    "host": "0.0.0.0",
    "port": 8080
  },
  "debug": false
}
"#
        );
    }

    #[test]
    fn non_ascii_text_before_an_edit() {
        let source = "{\n  \"name\": \"Grüße 🌍\",\n  \"max\": 10\n}\n";
        let mut document = parse(source).unwrap();
        *entry(&mut document, "max") = int("20");
        assert_eq!(
            apply(source, &document),
            "{\n  \"name\": \"Grüße 🌍\",\n  \"max\": 20\n}\n"
        );
        *entry(&mut document, "name") = Node::string("Ünïcödé ✓");
        assert_eq!(
            apply(source, &document),
            "{\n  \"name\": \"Ünïcödé ✓\",\n  \"max\": 20\n}\n"
        );
    }

    #[test]
    fn duplicate_keys_are_errors_with_their_position() {
        let source = "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": 1,\n    \"c\": 2\n  }\n}\n";
        assert_eq!(
            parse(source),
            Err(ParseError {
                message: "duplicate key `c`".into(),
                line: Some(5),
                column: Some(5),
            })
        );
        let error = parse("{\"ä\": 1, \"ä\": 2}").unwrap_err();
        assert_eq!((error.line, error.column), (Some(1), Some(10)));
    }

    #[test]
    fn parse_errors_point_at_the_problem() {
        let cases = [
            ("", "the file is empty", 1, 1),
            ("  // nothing here\n", "the file is empty", 1, 1),
            ("42\n", "the document must be an object or an array", 1, 1),
            (
                "{\n  \"a\": 1\n  \"b\": 2\n}\n",
                "expected `,` or `}`",
                3,
                3,
            ),
            ("{\"a\": 01}", "invalid number", 1, 7),
            ("{\"a\": \"x}", "unterminated string", 1, 7),
            (r#"["\ud800"]"#, "invalid unicode escape", 1, 3),
            (
                "{\"a\": 1} x",
                "unexpected content after the document",
                1,
                10,
            ),
            ("[1, /* open", "unterminated comment", 1, 5),
            ("[1,\n  ,]", "unexpected character `,`", 2, 3),
        ];
        for (source, message, line, column) in cases {
            assert_eq!(
                parse(source),
                Err(ParseError {
                    message: message.into(),
                    line: Some(line),
                    column: Some(column),
                }),
                "{source:?}"
            );
        }
    }

    #[test]
    fn strings_and_numbers_decode_and_encode() {
        let source = r#"["a\"b\\c\/d\n\u00e9\ud83d\ude00", 1.5e3, -0, 0.5, null, false]"#;
        assert_eq!(
            parse(source).unwrap(),
            Node::Array {
                items: vec![
                    Node::string("a\"b\\c/d\né😀"),
                    Node::Float {
                        value: "1.5e3".into()
                    },
                    int("-0"),
                    Node::Float {
                        value: "0.5".into()
                    },
                    Node::Null,
                    Node::Boolean { value: false },
                ],
            }
        );

        let source = "{\"s\": \"x\"}\n";
        let document = object(vec![("s", Node::string("tab\there \"q\" \u{1}\\"))]);
        assert_eq!(
            apply(source, &document),
            "{\"s\": \"tab\\there \\\"q\\\" \\u0001\\\\\"}\n"
        );
    }

    #[test]
    fn float_text_produces_json_float_literals() {
        let cases = [
            ("0.5", Some("0.5")),
            ("1e-3", Some("1e-3")),
            ("2E+10", Some("2E+10")),
            ("1e400", Some("1e400")),
            ("5", Some("5.0")),
            ("-0", Some("-0.0")),
            ("+5", Some("5.0")),
            (".5", Some("0.5")),
            ("01.50", Some("1.5")),
            ("nan", None),
            ("inf", None),
            ("-infinity", None),
            ("abc", None),
        ];
        for (input, expected) in cases {
            let text = float_text(input);
            assert_eq!(text.as_deref(), expected, "{input}");
            if let Some(text) = text {
                assert_eq!(
                    parse(&format!("[{text}]")).unwrap(),
                    Node::Array {
                        items: vec![Node::Float { value: text }],
                    }
                );
            }
        }
    }

    #[test]
    fn byte_order_mark_is_kept() {
        let source = "\u{feff}{\"a\": 1}\n";
        let document = object(vec![("a", int("2"))]);
        assert_eq!(apply(source, &document), "\u{feff}{\"a\": 2}\n");
        assert_eq!(
            emit("\u{feff}{}\n", &object(vec![("a", int("1"))])),
            "\u{feff}{\n  \"a\": 1\n}\n"
        );
    }
}
