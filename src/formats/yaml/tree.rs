//! Builds a span tree over the source from saphyr-parser events: where every value starts
//! and ends, plus the document model.
use super::scalar;
use crate::formats::{Entry, Node, ParseError, text};
use saphyr_parser::{Event, Parser, ScalarStyle, ScanError, Span, StrInput, Tag};
use std::{borrow::Cow, collections::HashSet};

const MAX_DEPTH: usize = 128;

/// A value of the source. Positions are byte offsets.
pub struct YNode {
    /// First byte of the value: the quote of quoted scalars, the header of block scalars,
    /// the bracket of flow collections, the first key / dash of block collections. Empty
    /// scalars start and end where their value would be written.
    pub start: usize,
    /// After the last byte of the value (excluding trailing comments).
    pub end: usize,
    pub kind: YKind,
}

pub enum YKind {
    Scalar {
        style: ScalarStyle,
        /// Has an explicit `!!type` tag before it.
        tagged: bool,
        /// Implicit empty value (`key:` with nothing after it).
        empty: bool,
    },
    Sequence {
        flow: bool,
        items: Vec<YItem>,
    },
    Mapping {
        flow: bool,
        entries: Vec<YEntry>,
    },
}

pub struct YItem {
    /// The `-` of block sequence items (the item start in flow sequences).
    pub dash: usize,
    pub value: YNode,
}

pub struct YEntry {
    pub key_start: usize,
    /// After the `:` (block mappings) where the value slot begins.
    pub slot: usize,
    pub value: YNode,
}

impl YNode {
    pub fn is_flow(&self) -> bool {
        matches!(
            self.kind,
            YKind::Sequence { flow: true, .. } | YKind::Mapping { flow: true, .. }
        )
    }
}

/// The root value and document of `source`; `None` for a file without a document.
pub fn build(source: &str) -> Result<Option<(YNode, Node)>, ParseError> {
    let mut builder = Builder {
        source,
        offsets: (!source.is_ascii()).then(|| {
            source
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(source.len()))
                .collect()
        }),
        events: Parser::new_from_str(source),
        cursor: 0,
    };

    match builder.next()? {
        (Event::StreamStart, _) => {}
        (_, span) => return Err(builder.error(span, "expected the start of the stream")),
    }
    match builder.next()? {
        (Event::StreamEnd, _) => return Ok(None),
        (Event::DocumentStart(explicit), span) => {
            builder.cursor = if explicit { builder.byte(span.end) } else { 0 };
        }
        (_, span) => return Err(builder.error(span, "expected a document")),
    }
    let (event, span) = builder.next()?;
    let root = builder.node(event, span, 0, false)?;
    match builder.next()? {
        (Event::DocumentEnd, _) => {}
        (_, span) => return Err(builder.error(span, "expected the end of the document")),
    }
    match builder.next()? {
        (Event::StreamEnd, _) => Ok(Some(root)),
        (_, span) => Err(builder.error(span, "files with several YAML documents are not supported")),
    }
}

/// Skips spaces, tabs, line breaks and comments.
pub fn skip_space(source: &str, mut at: usize) -> usize {
    let bytes = source.as_bytes();
    while let Some(byte) = bytes.get(at) {
        match byte {
            b' ' | b'\t' | b'\r' | b'\n' => at += 1,
            b'#' => at = text::line_end(source, at),
            _ => break,
        }
    }
    at
}

struct Builder<'a> {
    source: &'a str,
    /// Byte offset of every char index; `None` for ASCII sources.
    offsets: Option<Vec<usize>>,
    events: Parser<'a, StrInput<'a>>,
    /// After the last consumed token.
    cursor: usize,
}

impl<'a> Builder<'a> {
    fn next(&mut self) -> Result<(Event<'a>, Span), ParseError> {
        match self.events.next() {
            Some(Ok(event)) => Ok(event),
            Some(Err(err)) => Err(scan_error(&err)),
            None => Err(ParseError::new("unexpected end of the document")),
        }
    }

    /// Markers count characters; positions here are bytes.
    fn byte(&self, marker: saphyr_parser::Marker) -> usize {
        match &self.offsets {
            None => marker.index().min(self.source.len()),
            Some(offsets) => offsets[marker.index().min(offsets.len() - 1)],
        }
    }

    fn error(&self, span: Span, message: impl Into<String>) -> ParseError {
        ParseError::at(self.source, self.byte(span.start), message)
    }

    fn error_at(&self, at: usize, message: impl Into<String>) -> ParseError {
        ParseError::at(self.source, at, message)
    }

    fn node(
        &mut self,
        event: Event<'a>,
        span: Span,
        depth: usize,
        in_flow: bool,
    ) -> Result<(YNode, Node), ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.error(span, "the document is nested too deeply"));
        }
        match event {
            Event::Scalar(value, style, anchor, tag) => {
                self.check_anchor(anchor, span)?;
                let tag = self.core_tag(tag, span)?;
                self.scalar(&value, style, tag.as_deref(), span)
            }
            Event::SequenceStart(anchor, tag) => {
                self.check_anchor(anchor, span)?;
                self.collection_tag(tag, "seq", span)?;
                self.sequence(span, depth, in_flow)
            }
            Event::MappingStart(anchor, tag) => {
                self.check_anchor(anchor, span)?;
                self.collection_tag(tag, "map", span)?;
                self.mapping(span, depth, in_flow)
            }
            Event::Alias(_) => Err(self.error(span, "aliases (`*name`) are not supported")),
            _ => Err(self.error(span, "unexpected YAML structure")),
        }
    }

    fn check_anchor(&self, anchor: usize, span: Span) -> Result<(), ParseError> {
        if anchor == 0 {
            Ok(())
        } else {
            Err(self.error(span, "anchors (`&name`) are not supported"))
        }
    }

    /// The suffix of a core schema tag (`str`, `int`, ...).
    fn core_tag(
        &self,
        tag: Option<Cow<'a, Tag>>,
        span: Span,
    ) -> Result<Option<String>, ParseError> {
        let Some(tag) = tag else {
            return Ok(None);
        };
        if tag.handle == "tag:yaml.org,2002:"
            && ["str", "int", "float", "bool", "null", "seq", "map"].contains(&tag.suffix.as_str())
        {
            Ok(Some(tag.suffix.clone()))
        } else {
            Err(self.error(span, format!("the tag `{tag}` is not supported")))
        }
    }

    fn collection_tag(
        &self,
        tag: Option<Cow<'a, Tag>>,
        expected: &str,
        span: Span,
    ) -> Result<(), ParseError> {
        match self.core_tag(tag, span)? {
            Some(suffix) if suffix != expected => {
                Err(self.error(span, format!("`!!{suffix}` cannot tag a collection")))
            }
            _ => Ok(()),
        }
    }

    fn scalar(
        &mut self,
        value: &str,
        style: ScalarStyle,
        tag: Option<&str>,
        span: Span,
    ) -> Result<(YNode, Node), ParseError> {
        let empty = style == ScalarStyle::Plain && value.is_empty();
        let (start, end) = match style {
            _ if empty => (self.cursor, self.cursor),
            ScalarStyle::Plain => (self.byte(span.start), self.byte(span.end)),
            ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                let start = self.byte(span.start);
                let end = quoted_end(self.source, start)
                    .ok_or_else(|| self.error(span, "unterminated quoted text"))?;
                (start, end)
            }
            ScalarStyle::Literal | ScalarStyle::Folded => self.block_scalar(span)?,
        };
        self.cursor = end;

        let node = match tag {
            None if style == ScalarStyle::Plain => scalar::resolve(value),
            None | Some("str") => Node::string(value),
            Some(tag) => scalar::resolve_tagged(value, tag)
                .ok_or_else(|| self.error_at(start, format!("`{value}` is not a valid !!{tag}")))?,
        };
        let node_kind = YKind::Scalar {
            style,
            tagged: tag.is_some(),
            empty,
        };
        Ok((
            YNode {
                start,
                end,
                kind: node_kind,
            },
            node,
        ))
    }

    /// The header and content lines of a `|` / `>` scalar. The parser's span starts at the
    /// first content line and ends inside the following line, so both ends are located
    /// in the source.
    fn block_scalar(&self, span: Span) -> Result<(usize, usize), ParseError> {
        let bytes = self.source.as_bytes();
        let mut header = skip_space(self.source, self.cursor);
        while bytes.get(header) == Some(&b'!') {
            while bytes.get(header).is_some_and(|byte| !byte.is_ascii_whitespace()) {
                header += 1;
            }
            header = skip_space(self.source, header);
        }
        if !matches!(bytes.get(header), Some(b'|' | b'>')) {
            return Err(self.error(span, "unsupported block scalar layout"));
        }

        let content = text::line_end(self.source, header);
        let scanned = self.byte(span.end).max(content);
        let keep = self.source[header..content]
            .split(char::is_whitespace)
            .next()
            .is_some_and(|indicators| indicators.contains('+'));
        let next = text::line_start(self.source, scanned);
        let end = match self.source[content..scanned].rfind(|c: char| !c.is_whitespace()) {
            // `|+` keeps the empty lines after its content
            _ if keep && next > content && self.source[next..scanned].trim().is_empty() => {
                text::line_end(self.source, next - 1)
            }
            Some(last) => text::line_end(self.source, content + last),
            None => content,
        };
        Ok((header, end))
    }

    fn sequence(
        &mut self,
        span: Span,
        depth: usize,
        in_flow: bool,
    ) -> Result<(YNode, Node), ParseError> {
        let open = self.byte(span.start);
        let flow = self.source.as_bytes().get(open) == Some(&b'[');
        if in_flow && !flow {
            return Err(self.error(span, "unsupported flow sequence layout"));
        }
        if flow {
            self.cursor = open + 1;
        }

        let mut items = Vec::new();
        let mut nodes = Vec::new();
        let end = loop {
            let (event, span) = self.next()?;
            if let Event::SequenceEnd = event {
                break if flow {
                    self.byte(span.start) + 1
                } else {
                    items.last().map_or(open, |item: &YItem| item.value.end)
                };
            }
            let dash = if flow {
                self.byte(span.start)
            } else {
                let dash = skip_space(self.source, self.cursor);
                if self.source.as_bytes().get(dash) != Some(&b'-') {
                    return Err(self.error(span, "unsupported block sequence layout"));
                }
                self.cursor = dash + 1;
                dash
            };
            let (value, node) = self.node(event, span, depth + 1, flow)?;
            items.push(YItem { dash, value });
            nodes.push(node);
        };
        self.cursor = end;

        let start = if flow {
            open
        } else {
            items.first().map_or(open, |item| item.dash)
        };
        Ok((
            YNode {
                start,
                end,
                kind: YKind::Sequence { flow, items },
            },
            Node::Array { items: nodes },
        ))
    }

    fn mapping(
        &mut self,
        span: Span,
        depth: usize,
        in_flow: bool,
    ) -> Result<(YNode, Node), ParseError> {
        let open = self.byte(span.start);
        let flow = self.source.as_bytes().get(open) == Some(&b'{');
        if in_flow && !flow {
            return Err(self.error(span, "unsupported flow mapping layout"));
        }
        if flow {
            self.cursor = open + 1;
        }

        let mut entries = Vec::new();
        let mut nodes = Vec::new();
        let mut keys = HashSet::new();
        let end = loop {
            let (event, span) = self.next()?;
            let (key, key_style) = match event {
                Event::MappingEnd => {
                    break if flow {
                        self.byte(span.start) + 1
                    } else {
                        entries.last().map_or(open, |entry: &YEntry| entry.value.end)
                    };
                }
                Event::Scalar(key, style, anchor, tag) => {
                    self.check_anchor(anchor, span)?;
                    if self.core_tag(tag, span)?.is_some_and(|tag| tag != "str") {
                        return Err(self.error(span, "keys must be text"));
                    }
                    (key, style)
                }
                _ => return Err(self.error(span, "only text keys are supported")),
            };
            if key_style == ScalarStyle::Plain && key.is_empty() {
                return Err(self.error(span, "empty keys are not supported"));
            }
            let key_start = self.byte(span.start);
            let key_end = match key_style {
                ScalarStyle::Plain => self.byte(span.end),
                ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted => {
                    quoted_end(self.source, key_start)
                        .ok_or_else(|| self.error(span, "unterminated quoted key"))?
                }
                ScalarStyle::Literal | ScalarStyle::Folded => {
                    return Err(self.error(span, "block scalar keys are not supported"));
                }
            };
            if !keys.insert(key.to_string()) {
                return Err(self.error(span, format!("duplicate key `{key}`")));
            }

            let colon = skip_space(self.source, key_end);
            let slot = if self.source.as_bytes().get(colon) == Some(&b':') {
                colon + 1
            } else {
                key_end
            };
            self.cursor = slot;
            let (event, span) = self.next()?;
            let (value, node) = self.node(event, span, depth + 1, flow)?;

            let comment = if flow {
                None
            } else {
                text::comment_above(self.source, text::line_start(self.source, key_start), &["#"])
            };
            nodes.push(Entry::new(key.into_owned(), node, comment));
            entries.push(YEntry {
                key_start,
                slot,
                value,
            });
        };
        self.cursor = end;

        let start = if flow {
            open
        } else {
            entries.first().map_or(open, |entry| entry.key_start)
        };
        Ok((
            YNode {
                start,
                end,
                kind: YKind::Mapping { flow, entries },
            },
            Node::Object { entries: nodes },
        ))
    }
}

fn scan_error(err: &ScanError) -> ParseError {
    let marker = err.marker();
    ParseError {
        message: err.info().to_string(),
        line: Some(marker.line() as u32),
        column: Some(marker.col() as u32 + 1),
    }
}

/// After the closing quote of the quoted scalar starting at `start`.
fn quoted_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let quote = *bytes.get(start)?;
    let mut at = start + 1;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' if quote == b'"' => at += 2,
            b'\'' if quote == b'\'' && bytes.get(at + 1) == Some(&b'\'') => at += 2,
            byte if byte == quote => return Some(at + 1),
            _ => at += 1,
        }
    }
    None
}
