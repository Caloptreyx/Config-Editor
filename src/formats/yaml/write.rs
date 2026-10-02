//! Writes documents as YAML text (with `\n` line breaks).
use super::scalar::{self, Context};
use crate::formats::{Entry, Node, text};
use super::tree::{YKind, YNode};

/// Indentation habits of a file.
#[derive(Clone, Copy)]
pub struct Style {
    /// Extra indentation of a mapping nested under a key.
    pub indent: usize,
    /// Extra indentation of the `-` of a sequence under a key (0 when not indented).
    pub sequence_indent: usize,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            indent: 2,
            sequence_indent: 2,
        }
    }
}

impl Style {
    /// The first nested mapping and the first sequence under a key decide.
    pub fn detect(source: &str, root: &YNode) -> Self {
        let mut indent = None;
        let mut sequence_indent = None;
        visit(source, root, &mut indent, &mut sequence_indent);
        let default = Self::default();
        Self {
            indent: indent.unwrap_or(default.indent),
            sequence_indent: sequence_indent.unwrap_or(default.sequence_indent),
        }
    }
}

fn visit(
    source: &str,
    node: &YNode,
    indent: &mut Option<usize>,
    sequence_indent: &mut Option<usize>,
) {
    if indent.is_some() && sequence_indent.is_some() {
        return;
    }
    match &node.kind {
        YKind::Mapping { flow: false, entries } => {
            for entry in entries {
                let key = column(source, entry.key_start);
                match &entry.value.kind {
                    YKind::Mapping { flow: false, entries } if indent.is_none() => {
                        if let Some(first) = entries.first() {
                            let nested = column(source, first.key_start);
                            if nested > key && text::starts_line(source, first.key_start) {
                                *indent = Some(nested - key);
                            }
                        }
                    }
                    YKind::Sequence { flow: false, items } if sequence_indent.is_none() => {
                        if let Some(first) = items.first() {
                            let dash = column(source, first.dash);
                            if dash >= key && text::starts_line(source, first.dash) {
                                *sequence_indent = Some(dash - key);
                            }
                        }
                    }
                    _ => {}
                }
                visit(source, &entry.value, indent, sequence_indent);
            }
        }
        YKind::Sequence { flow: false, items } => {
            for item in items {
                visit(source, &item.value, indent, sequence_indent);
            }
        }
        _ => {}
    }
}

/// Byte column of `at` (indentation is ASCII).
pub fn column(source: &str, at: usize) -> usize {
    at - text::line_start(source, at)
}

#[derive(Clone, Copy)]
pub enum Slot {
    /// After `key:`.
    Value,
    /// After `-`.
    Item,
}

/// The text written right after the `:` / `-` at column `column`: a space and a scalar or
/// compact collection, or a line break and an indented block.
pub fn after_indicator(node: &Node, column: usize, slot: Slot, style: Style) -> String {
    let nested = match slot {
        Slot::Value => column + style.indent,
        Slot::Item => column + 2,
    };
    match (node, slot) {
        (Node::Object { entries }, Slot::Value) if !entries.is_empty() => {
            format!("\n{}", mapping(entries, nested, style))
        }
        (Node::Array { items }, Slot::Value) if !items.is_empty() => {
            format!("\n{}", sequence(items, column + style.sequence_indent, style))
        }
        (Node::Object { entries }, Slot::Item) if !entries.is_empty() => {
            format!(" {}", mapping(entries, nested, style).trim_start_matches(' '))
        }
        (Node::Array { items }, Slot::Item) if !items.is_empty() => {
            format!(" {}", sequence(items, nested, style).trim_start_matches(' '))
        }
        (node, _) => format!(" {}", inline(node, nested)),
    }
}

/// A whole document (or root value) whose first line starts at `column`.
pub fn root(node: &Node, column: usize, style: Style) -> String {
    match node {
        Node::Object { entries } if !entries.is_empty() => {
            mapping(entries, column, style).trim_start_matches(' ').to_string()
        }
        Node::Array { items } if !items.is_empty() => {
            sequence(items, column, style).trim_start_matches(' ').to_string()
        }
        node => inline(node, column + style.indent),
    }
}

/// An entry line (and its nested lines) of a block mapping at `column`.
pub fn entry(entry: &Entry, column: usize, style: Style) -> String {
    format!(
        "{}{}:{}",
        " ".repeat(column),
        scalar::key(&entry.key),
        after_indicator(&entry.value, column, Slot::Value, style)
    )
}

/// An item line (and its nested lines) of a block sequence at `column`.
pub fn item(item: &Node, column: usize, style: Style) -> String {
    format!(
        "{}-{}",
        " ".repeat(column),
        after_indicator(item, column, Slot::Item, style)
    )
}

fn mapping(entries: &[Entry], column: usize, style: Style) -> String {
    entries
        .iter()
        .map(|value| entry(value, column, style))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sequence(items: &[Node], column: usize, style: Style) -> String {
    items
        .iter()
        .map(|value| item(value, column, style))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Scalars and empty collections; multi-line strings indent their lines at `indent`.
fn inline(node: &Node, indent: usize) -> String {
    match node {
        Node::Object { .. } | Node::Array { .. } => flow(node),
        scalar => scalar::render(scalar, None, Context::Block { indent }),
    }
}

/// A flow collection or scalar (`{a: 1, b: [x, y]}`).
pub fn flow(node: &Node) -> String {
    match node {
        Node::Object { entries } => {
            let entries: Vec<String> = entries
                .iter()
                .map(|entry| format!("{}: {}", scalar::key(&entry.key), flow(&entry.value)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        Node::Array { items } => {
            let items: Vec<String> = items.iter().map(flow).collect();
            format!("[{}]", items.join(", "))
        }
        scalar => scalar::render(scalar, None, Context::Flow),
    }
}
