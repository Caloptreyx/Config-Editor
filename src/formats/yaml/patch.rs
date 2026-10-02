//! Applies a new document to the span tree as minimal text edits.
use super::{
    scalar::{self, Context},
    tree::{YEntry, YItem, YKind, YNode},
    write::{self, Style, column},
};
use crate::formats::{Entry, Node, text};
use std::collections::HashMap;

/// Largest `old × new` sequence alignment computed; longer lists are re-emitted.
const MAX_ALIGNMENT: usize = 1_000_000;

/// Where a value sits, which decides how it is re-emitted.
#[derive(Clone, Copy)]
pub enum Slot {
    Root,
    /// Mapping value after the `:` ending at `after`, key at `column`.
    Value { after: usize, column: usize },
    /// Sequence item after the `-` ending at `after`, dash at `column`.
    Item { after: usize, column: usize },
}

pub struct Patcher<'a> {
    source: &'a str,
    style: Style,
    eol: &'static str,
    edits: text::Edits,
}

impl<'a> Patcher<'a> {
    pub fn new(source: &'a str, style: Style) -> Self {
        Self {
            source,
            style,
            eol: text::eol(source),
            edits: text::Edits::default(),
        }
    }

    /// The edited source; `None` when the edits conflict.
    pub fn finish(self) -> Option<String> {
        self.edits.apply(self.source)
    }

    fn put(&mut self, start: usize, end: usize, text: &str) {
        self.edits
            .replace(start, end, text::with_eol(text, self.eol));
    }

    /// Writes `new` over `node` (whose document is `old`), in place where possible, else by
    /// re-emitting the slot. `false` only for an empty root, which the caller re-emits.
    pub fn update(&mut self, node: &YNode, old: &Node, new: &Node, slot: Slot) -> bool {
        old == new || self.in_place(node, old, new, slot) || self.reemit(node, old, new, slot)
    }

    /// Records edits only when it returns `true`.
    fn in_place(&mut self, node: &YNode, old: &Node, new: &Node, slot: Slot) -> bool {
        match (&node.kind, old, new) {
            (
                YKind::Scalar {
                    style,
                    tagged: false,
                    empty: false,
                },
                _,
                new,
            ) if new.is_scalar() => {
                let context = match slot {
                    // a literal block would swallow the comment after the value
                    _ if ends_with_block(new) && self.followed_on_line(node) => Context::Flow,
                    Slot::Root => Context::Block {
                        indent: column(self.source, node.start) + self.style.indent,
                    },
                    Slot::Value { column, .. } => Context::Block {
                        indent: column + self.style.indent,
                    },
                    Slot::Item { column, .. } => Context::Block { indent: column + 2 },
                };
                let text = scalar::render(new, Some(*style), context);
                self.put(node.start, node.end, &text);
                true
            }
            (
                YKind::Mapping {
                    flow: false,
                    entries,
                },
                Node::Object {
                    entries: old_entries,
                },
                Node::Object {
                    entries: new_entries,
                },
            ) if !new_entries.is_empty() => self.mapping(entries, old_entries, new_entries),
            (
                YKind::Sequence { flow: false, items },
                Node::Array { items: old_items },
                Node::Array { items: new_items },
            ) if !new_items.is_empty() => self.sequence(items, old_items, new_items),
            _ if node.is_flow() && flow_patchable(node, old, new) => {
                self.flow_values(node, old, new);
                true
            }
            _ => false,
        }
    }

    /// Replaces the whole slot with `new`. Flow collections that had entries stay flow.
    fn reemit(&mut self, node: &YNode, old: &Node, new: &Node, slot: Slot) -> bool {
        let keep_flow = node.is_flow() && !is_empty_collection(old) && !new.is_scalar();
        match slot {
            Slot::Root if matches!(node.kind, YKind::Scalar { empty: true, .. }) => false,
            Slot::Root => {
                let text = if keep_flow {
                    write::flow(new)
                } else {
                    write::root(new, column(self.source, node.start), self.style)
                };
                self.put(node.start, node.end, &text);
                true
            }
            Slot::Value { after, column } | Slot::Item { after, column } => {
                let text = if keep_flow {
                    format!(" {}", write::flow(new))
                } else {
                    let kind = match slot {
                        Slot::Value { .. } => write::Slot::Value,
                        _ => write::Slot::Item,
                    };
                    write::after_indicator(new, column, kind, self.style)
                };
                // a comment after a value written as lines would end up inside the last one
                let end = if ends_with_block(new) && self.followed_on_line(node) {
                    text::line_end(self.source, node.end)
                } else {
                    node.end
                };
                self.put(after, end, &text);
                true
            }
        }
    }

    fn mapping(&mut self, entries: &[YEntry], old: &[Entry], new: &[Entry]) -> bool {
        let source = self.source;
        let index: HashMap<&str, usize> = old
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.key.as_str(), index))
            .collect();

        let mut kept = Vec::new();
        let mut inserted = Vec::new();
        let mut previous: Option<usize> = None;
        for (position, entry) in new.iter().enumerate() {
            match index.get(entry.key.as_str()) {
                Some(&at) if previous.is_some_and(|previous| at < previous) => return false,
                Some(&at) => {
                    kept.push((at, position));
                    previous = Some(at);
                }
                None => inserted.push((previous, position)),
            }
        }
        let mut removed = vec![true; entries.len()];
        for &(at, _) in &kept {
            removed[at] = false;
        }

        let first = entries[0].key_start;
        let starts_line = |entry: &YEntry| text::starts_line(source, entry.key_start);
        if entries
            .iter()
            .zip(&removed)
            .any(|(entry, removed)| *removed && !starts_line(entry))
            || (inserted.iter().any(|(after, _)| after.is_none()) && !text::starts_line(source, first))
        {
            return false;
        }

        // nested edits first: an insertion at the end of a nested value has to come before
        // one after the entry at the same position
        let column = column(source, first);
        for (at, position) in kept {
            let entry = &entries[at];
            self.update(
                &entry.value,
                &old[at].value,
                &new[position].value,
                Slot::Value {
                    after: entry.slot,
                    column,
                },
            );
        }
        // an entry owns the comment block above it (its `comment`)
        let comment_start =
            |at: usize| text::comment_start(source, text::line_start(source, at), &["#"]);
        for (entry, _) in entries.iter().zip(&removed).filter(|(_, removed)| **removed) {
            self.edits.delete(
                comment_start(entry.key_start),
                text::next_line(source, entry.value.end),
            );
        }
        for (after, position) in inserted {
            let line = write::entry(&new[position], column, self.style);
            self.insert_line(after.map(|at| entries[at].value.end), comment_start(first), &line);
        }
        true
    }

    fn sequence(&mut self, items: &[YItem], old: &[Node], new: &[Node]) -> bool {
        let source = self.source;
        if old.len().saturating_mul(new.len()) > MAX_ALIGNMENT {
            return false;
        }

        let mut updated = Vec::new();
        let mut removed = Vec::new();
        let mut inserted = Vec::new();
        let (mut old_at, mut new_at) = (0, 0);
        let mut matches = common_items(old, new);
        // the end of both lists closes the last gap
        matches.push((old.len(), new.len()));
        for (old_match, new_match) in matches {
            let paired = (old_match - old_at).min(new_match - new_at);
            updated.extend((0..paired).map(|offset| (old_at + offset, new_at + offset)));
            removed.extend(old_at + paired..old_match);
            let after = (old_at + paired).checked_sub(1);
            inserted.extend((new_at + paired..new_match).map(|position| (after, position)));
            old_at = old_match + 1;
            new_at = new_match + 1;
        }

        let first = items[0].dash;
        if removed
            .iter()
            .any(|&at| !text::starts_line(source, items[at].dash))
            || (inserted.iter().any(|(after, _)| after.is_none()) && !text::starts_line(source, first))
        {
            return false;
        }

        let column = column(source, first);
        for (at, position) in updated {
            let item = &items[at];
            self.update(
                &item.value,
                &old[at],
                &new[position],
                Slot::Item {
                    after: item.dash + 1,
                    column,
                },
            );
        }
        for at in removed {
            self.edits.delete(
                text::line_start(source, items[at].dash),
                text::next_line(source, items[at].value.end),
            );
        }
        for (after, position) in inserted {
            let line = write::item(&new[position], column, self.style);
            let before = text::line_start(source, first);
            self.insert_line(after.map(|at| items[at].value.end), before, &line);
        }
        true
    }

    /// Whether something (a comment) follows the value on its last line.
    fn followed_on_line(&self, node: &YNode) -> bool {
        !self.source[node.end..text::line_end(self.source, node.end)]
            .trim()
            .is_empty()
    }

    /// Adds `line` (already indented) after the line holding `after`, or at the line start
    /// `before` when there is nothing before it.
    fn insert_line(&mut self, after: Option<usize>, before: usize, line: &str) {
        match after {
            Some(after) => {
                let at = text::line_end(self.source, after);
                self.put(at, at, &format!("\n{line}"));
            }
            None => self.put(before, before, &format!("{line}\n")),
        }
    }

    /// Rewrites the changed scalars of a flow collection checked by [`flow_patchable`].
    fn flow_values(&mut self, node: &YNode, old: &Node, new: &Node) {
        if old == new {
            return;
        }
        match (&node.kind, old, new) {
            (YKind::Scalar { style, .. }, _, new) => {
                let text = scalar::render(new, Some(*style), Context::Flow);
                self.put(node.start, node.end, &text);
            }
            (YKind::Sequence { items, .. }, Node::Array { items: old }, Node::Array { items: new }) => {
                for ((item, old), new) in items.iter().zip(old).zip(new) {
                    self.flow_values(&item.value, old, new);
                }
            }
            (
                YKind::Mapping { entries, .. },
                Node::Object { entries: old },
                Node::Object { entries: new },
            ) => {
                for ((entry, old), new) in entries.iter().zip(old).zip(new) {
                    self.flow_values(&entry.value, &old.value, &new.value);
                }
            }
            _ => {}
        }
    }
}

/// Whether every difference inside a flow collection is a scalar replaced by a scalar.
fn flow_patchable(node: &YNode, old: &Node, new: &Node) -> bool {
    if old == new {
        return true;
    }
    match (&node.kind, old, new) {
        (
            YKind::Scalar {
                tagged: false,
                empty: false,
                ..
            },
            _,
            new,
        ) => new.is_scalar(),
        (YKind::Sequence { items, .. }, Node::Array { items: old }, Node::Array { items: new }) => {
            old.len() == new.len()
                && items
                    .iter()
                    .zip(old)
                    .zip(new)
                    .all(|((item, old), new)| flow_patchable(&item.value, old, new))
        }
        (
            YKind::Mapping { entries, .. },
            Node::Object { entries: old },
            Node::Object { entries: new },
        ) => {
            old.len() == new.len()
                && entries.iter().zip(old).zip(new).all(|((entry, old), new)| {
                    old.key == new.key && flow_patchable(&entry.value, &old.value, &new.value)
                })
        }
        _ => false,
    }
}

/// Whether the last line written for `node` belongs to a literal block (multi-line string).
fn ends_with_block(node: &Node) -> bool {
    match node {
        Node::String { value } => value.contains('\n'),
        Node::Array { items } => items.last().is_some_and(ends_with_block),
        Node::Object { entries } => entries.last().is_some_and(|entry| ends_with_block(&entry.value)),
        _ => false,
    }
}

fn is_empty_collection(node: &Node) -> bool {
    match node {
        Node::Array { items } => items.is_empty(),
        Node::Object { entries } => entries.is_empty(),
        _ => false,
    }
}

/// Index pairs of a longest common subsequence of equal items.
fn common_items(old: &[Node], new: &[Node]) -> Vec<(usize, usize)> {
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
