//! TOML through `toml_edit`, which keeps every byte of the parts of a document it does not
//! touch. Edits are applied to the parsed `DocumentMut` and the file is rendered again; only
//! line endings need restoring because `toml_edit` always writes `\n`.
//!
//! In a TOML table the plain keys are always written before its `[sub-tables]`, so objects list
//! their plain entries first and their object / array-of-object entries after them (both groups
//! in file order). [`arrange`] puts a submitted document into that order.
use std::collections::HashSet;
use std::fmt::Write as _;

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, RawString, Table, Value};

use super::{Entry, Node, ParseError, text};

const BOM: &str = "\u{feff}";

/// Above this many item comparisons array items are matched by index instead of by content.
const ALIGN_LIMIT: usize = 1_000_000;

pub fn parse(source: &str) -> Result<Node, ParseError> {
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| match error.span() {
            Some(span) => ParseError::at(source, span.start, error.message()),
            None => ParseError::new(error.message()),
        })?;
    Ok(table_node(document.as_table()))
}

pub fn patch(source: &str, _current: &Node, document: &Node) -> String {
    let Node::Object { entries } = document else {
        return source.to_owned();
    };
    let Ok(mut parsed) = source.parse::<DocumentMut>() else {
        return source.to_owned();
    };
    let mut writer = Writer::new(source);
    writer.merge_table(parsed.as_table_mut(), entries);
    if writer.failed {
        return source.to_owned();
    }
    if writer.renumber {
        renumber(parsed.as_table_mut(), &mut 0);
    }

    let (bom, body) = split_bom(source);
    format!("{bom}{}", restore_line_endings(body, &parsed.to_string()))
}

pub fn emit(source: &str, document: &Node) -> String {
    let (bom, body) = split_bom(source);
    let mut writer = Writer::new(body);
    let mut output = DocumentMut::new();
    if let Node::Object { entries } = document {
        for entry in entries {
            let item = writer.new_item(&entry.value, true);
            output.insert(&entry.key, item);
        }
    }
    format!(
        "{bom}{}",
        text::with_eol(&output.to_string(), text::eol(body))
    )
}

/// `document` with every object's entries in the order TOML lists them: plain entries first,
/// then objects and arrays of objects, each group keeping its relative order.
pub fn arrange(document: &Node) -> Node {
    match document {
        Node::Object { entries } => Node::Object {
            entries: arranged(
                entries
                    .iter()
                    .map(|entry| {
                        Entry::new(
                            entry.key.clone(),
                            arrange(&entry.value),
                            entry.comment.clone(),
                        )
                    })
                    .collect(),
            ),
        },
        Node::Array { items } => Node::Array {
            items: items.iter().map(arrange).collect(),
        },
        node => node.clone(),
    }
}

/// The literal stored for a user-entered float: TOML float literals are kept as written,
/// other numbers are written canonically.
pub fn float_text(value: &str) -> Option<String> {
    let text = value.trim();
    if float_literal(text).is_some() {
        return Some(text.to_owned());
    }

    let (negative, magnitude) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let candidate = match magnitude.to_ascii_lowercase().as_str() {
        "inf" | "infinity" if negative => "-inf".to_owned(),
        "inf" | "infinity" => "inf".to_owned(),
        "nan" => "nan".to_owned(),
        _ => {
            let decimal = format!("{text}.0");
            if magnitude.bytes().all(|byte| byte.is_ascii_digit())
                && float_literal(&decimal).is_some()
            {
                decimal
            } else {
                format!("{:?}", text.parse::<f64>().ok()?)
            }
        }
    };
    float_literal(&candidate).map(|_| candidate)
}

/// Whether `value` is exactly one TOML date/time literal.
pub fn datetime_ok(value: &str) -> bool {
    datetime_literal(value).is_some()
}

fn float_literal(text: &str) -> Option<Value> {
    match text.parse::<Value>().ok()? {
        Value::Float(float) if float.display_repr() == text => Some(Value::Float(float)),
        _ => None,
    }
}

fn datetime_literal(text: &str) -> Option<Value> {
    match text.parse::<Value>().ok()? {
        Value::Datetime(datetime) if datetime.display_repr() == text => {
            Some(Value::Datetime(datetime))
        }
        _ => None,
    }
}

fn table_node(table: &Table) -> Node {
    let entries = table
        .iter()
        .filter_map(|(key, item)| {
            let value = item_node(item)?;
            Some(Entry::new(key, value, item_comment(table, key, item)))
        })
        .collect();
    Node::Object {
        entries: arranged(entries),
    }
}

fn inline_node(table: &InlineTable) -> Node {
    let entries = table
        .iter()
        .map(|(key, value)| {
            let prefix = table.key(key).and_then(|key| key.leaf_decor().prefix());
            Entry::new(key, value_node(value), comment(prefix))
        })
        .collect();
    Node::Object {
        entries: arranged(entries),
    }
}

fn item_node(item: &Item) -> Option<Node> {
    match item {
        Item::None => None,
        Item::Value(value) => Some(value_node(value)),
        Item::Table(table) => Some(table_node(table)),
        Item::ArrayOfTables(array) => Some(Node::Array {
            items: array.iter().map(table_node).collect(),
        }),
    }
}

fn value_node(value: &Value) -> Node {
    match value {
        Value::String(string) => Node::String {
            value: string.value().clone(),
        },
        Value::Integer(integer) => Node::Integer {
            value: integer.value().to_string(),
        },
        Value::Float(float) => Node::Float {
            value: float.display_repr().trim().to_owned(),
        },
        Value::Boolean(boolean) => Node::Boolean {
            value: *boolean.value(),
        },
        Value::Datetime(datetime) => Node::Datetime {
            value: datetime.display_repr().trim().to_owned(),
        },
        Value::Array(array) => Node::Array {
            items: array.iter().map(value_node).collect(),
        },
        Value::InlineTable(table) => inline_node(table),
    }
}

/// Header tables carry the comments above their `[header]`; everything else on its key.
fn item_comment(table: &Table, key: &str, item: &Item) -> Option<String> {
    let prefix = match item {
        Item::Table(child) if !child.is_dotted() => child.decor().prefix(),
        Item::ArrayOfTables(array) => array.get(0)?.decor().prefix(),
        _ => table.key(key)?.leaf_decor().prefix(),
    };
    comment(prefix)
}

/// The `#` comment block at the end of a decor prefix (the lines right above the entry).
fn comment(prefix: Option<&RawString>) -> Option<String> {
    let prefix = prefix?.as_str()?;
    text::comment_above(prefix, text::line_start(prefix, prefix.len()), &["#"])
}

fn arranged(entries: Vec<Entry>) -> Vec<Entry> {
    let (tables, mut plain): (Vec<Entry>, Vec<Entry>) = entries
        .into_iter()
        .partition(|entry| is_table_like(&entry.value));
    plain.extend(tables);
    plain
}

fn arranged_keys<'a>(keys: impl Iterator<Item = (&'a str, bool)>) -> Vec<&'a str> {
    let (tables, mut plain): (Vec<_>, Vec<_>) = keys.partition(|(_, table_like)| *table_like);
    plain.extend(tables);
    plain.into_iter().map(|(key, _)| key).collect()
}

fn expected_keys(entries: &[Entry]) -> Vec<&str> {
    arranged_keys(
        entries
            .iter()
            .map(|entry| (entry.key.as_str(), is_table_like(&entry.value))),
    )
}

fn is_table_like(node: &Node) -> bool {
    match node {
        Node::Object { .. } => true,
        Node::Array { items } => {
            !items.is_empty() && items.iter().all(|item| matches!(item, Node::Object { .. }))
        }
        _ => false,
    }
}

fn item_is_table_like(item: &Item) -> bool {
    match item {
        Item::None => false,
        Item::Value(value) => value_is_table_like(value),
        Item::Table(_) => true,
        Item::ArrayOfTables(array) => !array.is_empty(),
    }
}

fn value_is_table_like(value: &Value) -> bool {
    match value {
        Value::InlineTable(_) => true,
        Value::Array(array) => !array.is_empty() && array.iter().all(Value::is_inline_table),
        _ => false,
    }
}

/// Whether the item is written under its own `[header]` / `[[header]]`.
fn is_header(item: &Item) -> bool {
    match item {
        Item::Table(table) => !table.is_dotted(),
        Item::ArrayOfTables(_) => true,
        Item::None | Item::Value(_) => false,
    }
}

fn header_position(item: &Item) -> Option<isize> {
    match item {
        Item::Table(table) if !table.is_dotted() => table.position(),
        Item::ArrayOfTables(array) => array.get(0).and_then(Table::position),
        _ => None,
    }
}

/// Numbers every header table in document order, so `toml_edit` writes them in item order.
fn renumber(table: &mut Table, next: &mut isize) {
    if !table.is_dotted() {
        table.set_position(Some(*next));
        *next += 1;
    }
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(child) => renumber(child, next),
            Item::ArrayOfTables(array) => {
                for child in array.iter_mut() {
                    renumber(child, next);
                }
            }
            Item::None | Item::Value(_) => {}
        }
    }
}

/// The indentation of the table's last plain key, used for keys added to it.
fn body_indent(table: &Table) -> Option<String> {
    let (key, _) = table.iter().filter(|(_, item)| item.is_value()).last()?;
    let prefix = table.key(key)?.leaf_decor().prefix()?.as_str()?;
    let indent = &prefix[prefix.rfind('\n').map_or(0, |at| at + 1)..];
    indent
        .bytes()
        .all(|byte| byte == b' ' || byte == b'\t')
        .then(|| indent.to_owned())
}

fn prefix_of(value: &Value) -> Option<String> {
    value
        .decor()
        .prefix()
        .and_then(RawString::as_str)
        .map(str::to_owned)
}

fn suffix_of(value: &Value) -> Option<String> {
    value
        .decor()
        .suffix()
        .and_then(RawString::as_str)
        .map(str::to_owned)
}

/// A TOML basic string with every line break and control character escaped.
fn basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push('\t'),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Matches old to new array items: `(Some(old), Some(new))` keeps (and updates) an item,
/// `(Some(old), None)` removes one, `(None, Some(new))` inserts one. Equal items are matched
/// by a longest common subsequence; the unequal items between them are paired up in order.
fn align(old: &[Node], new: &[Node]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (old.len(), new.len());
    let mut pairs = Vec::with_capacity(n.max(m));
    if n.saturating_mul(m) > ALIGN_LIMIT {
        for index in 0..n.max(m) {
            pairs.push(((index < n).then_some(index), (index < m).then_some(index)));
        }
        return pairs;
    }

    let width = m + 1;
    let mut common = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i * width + j] = if old[i] == new[j] {
                common[(i + 1) * width + j + 1] + 1
            } else {
                common[(i + 1) * width + j].max(common[i * width + j + 1])
            };
        }
    }

    let (mut removed, mut inserted) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            pair_up(&mut pairs, &mut removed, &mut inserted);
            pairs.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if j < m && (i == n || common[i * width + j + 1] >= common[(i + 1) * width + j]) {
            inserted.push(j);
            j += 1;
        } else {
            removed.push(i);
            i += 1;
        }
    }
    pair_up(&mut pairs, &mut removed, &mut inserted);
    pairs
}

fn pair_up(
    pairs: &mut Vec<(Option<usize>, Option<usize>)>,
    removed: &mut Vec<usize>,
    inserted: &mut Vec<usize>,
) {
    for index in 0..removed.len().max(inserted.len()) {
        pairs.push((removed.get(index).copied(), inserted.get(index).copied()));
    }
    removed.clear();
    inserted.clear();
}

fn split_bom(source: &str) -> (&'static str, &str) {
    match source.strip_prefix(BOM) {
        Some(body) => (BOM, body),
        None => ("", source),
    }
}

/// `toml_edit` writes `\n` everywhere. Keeps the file's own bytes before and after the
/// changed lines and writes the changed lines with the file's line ending.
fn restore_line_endings(source: &str, output: &str) -> String {
    if !source.contains('\r') {
        return output.to_owned();
    }
    let stripped = source.replace('\r', "");
    let (old, new) = (stripped.as_bytes(), output.as_bytes());

    let same = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let head = old[..same]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |at| at + 1);
    let same = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(old.len().min(new.len()) - head)
        .take_while(|(a, b)| a == b)
        .count();
    let mut tail = old.len() - same;
    if tail > 0 && old[tail - 1] != b'\n' {
        tail = old[tail..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(old.len(), |at| tail + at + 1);
    }

    let changed = &output[head..new.len() - (old.len() - tail)];
    let mut result = String::with_capacity(source.len() + changed.len());
    result.push_str(&source[..source_offset(source, head)]);
    result.push_str(&text::with_eol(changed, text::eol(source)));
    result.push_str(&source[source_offset(source, tail)..]);
    result
}

/// The offset in `source` after `stripped` of its bytes that are not `\r`.
fn source_offset(source: &str, stripped: usize) -> usize {
    let mut seen = 0;
    for (offset, byte) in source.bytes().enumerate() {
        if seen == stripped {
            return offset;
        }
        if byte != b'\r' {
            seen += 1;
        }
    }
    source.len()
}

/// Applies a document to a parsed file (or builds a new one), keeping the formatting of
/// everything that stays equal.
struct Writer {
    crlf: bool,
    /// A value of the document cannot be stored in TOML (an integer beyond 64 bits).
    failed: bool,
    /// Sub-tables changed order; table positions must follow the item order.
    renumber: bool,
}

impl Writer {
    fn new(source: &str) -> Self {
        Self {
            crlf: text::eol(source) == "\r\n",
            failed: false,
            renumber: false,
        }
    }

    fn merge_table(&mut self, table: &mut Table, entries: &[Entry]) {
        let headers = !table.is_dotted();
        let keep: HashSet<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        table.retain(|key, _| keep.contains(key));
        let indent = body_indent(table);

        for entry in entries {
            let reshaped = match table.get_mut(&entry.key) {
                Some(item) => {
                    let was_header = is_header(item);
                    self.merge_item(item, &entry.value, headers);
                    was_header != is_header(item)
                }
                None => {
                    let item = self.new_item(&entry.value, headers);
                    let plain = item.is_value();
                    table.insert(&entry.key, item);
                    if plain
                        && let Some(indent) = &indent
                        && let Some(mut key) = table.key_mut(&entry.key)
                    {
                        key.leaf_decor_mut().set_prefix(indent.as_str());
                    }
                    false
                }
            };
            // A key moving between `key = value` and `[header]` drops the other form's spacing.
            if reshaped && let Some(mut key) = table.key_mut(&entry.key) {
                key.leaf_decor_mut().clear();
            }
        }

        // Empty implicit / dotted tables are not written at all; keep the table in the file.
        if table.is_empty() {
            table.set_dotted(false);
            table.set_implicit(false);
        }
        self.order_table(table, entries);
    }

    fn order_table(&mut self, table: &mut Table, entries: &[Entry]) {
        let expected = expected_keys(entries);
        if arranged_keys(
            table
                .iter()
                .map(|(key, item)| (key, item_is_table_like(item))),
        ) == expected
        {
            return;
        }
        for key in &expected {
            if let Some((key, item)) = table.remove_entry(key) {
                table.insert_formatted(&key, item);
            }
        }
        let positions: Vec<isize> = table
            .iter()
            .filter_map(|(_, item)| header_position(item))
            .collect();
        if !positions.is_sorted() {
            self.renumber = true;
        }
    }

    /// `headers`: the item belongs to a `[table]`, so new objects become sub-tables.
    fn merge_item(&mut self, item: &mut Item, node: &Node, headers: bool) {
        if item_node(item).as_ref() == Some(node) {
            return;
        }
        match (item, node) {
            (Item::Table(table), Node::Object { entries }) => self.merge_table(table, entries),
            (Item::ArrayOfTables(array), Node::Array { items }) if is_table_like(node) => {
                self.merge_tables(array, items);
            }
            (Item::Value(value), node) if !(headers && becomes_header(value, node)) => {
                self.merge_value(value, node);
            }
            (item, node) => *item = self.new_item(node, headers),
        }
    }

    fn merge_tables(&mut self, array: &mut ArrayOfTables, items: &[Node]) {
        let mut old: Vec<Table> = array.iter().cloned().collect();
        let old_nodes: Vec<Node> = old.iter().map(table_node).collect();
        array.clear();
        for (from, to) in align(&old_nodes, items) {
            let Some(Node::Object { entries }) = to.map(|to| &items[to]) else {
                continue;
            };
            let table = match from {
                Some(from) => {
                    let mut table = std::mem::take(&mut old[from]);
                    self.merge_table(&mut table, entries);
                    table
                }
                None => self.new_table(entries),
            };
            array.push(table);
        }
    }

    fn merge_value(&mut self, value: &mut Value, node: &Node) {
        if value_node(value) == *node {
            return;
        }
        match node {
            Node::Object { entries } if value.is_inline_table() => {
                if let Some(table) = value.as_inline_table_mut() {
                    self.merge_inline(table, entries);
                }
            }
            Node::Array { items } if value.is_array() => {
                if let Some(array) = value.as_array_mut() {
                    self.merge_array(array, items);
                }
            }
            node => {
                let mut fresh = match node {
                    Node::String { value: text } => self.string_value(text, Some(&*value)),
                    node => self.new_value(node),
                };
                *fresh.decor_mut() = value.decor().clone();
                *value = fresh;
            }
        }
    }

    fn merge_inline(&mut self, table: &mut InlineTable, entries: &[Entry]) {
        let old_last = table
            .iter()
            .last()
            .map(|(key, value)| (key.to_owned(), suffix_of(value)));
        let keep: HashSet<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        table.retain(|key, _| keep.contains(key));

        for entry in entries {
            match table.get_mut(&entry.key) {
                Some(value) => self.merge_value(value, &entry.value),
                None => {
                    let value = self.new_value(&entry.value);
                    table.insert(entry.key.as_str(), value);
                }
            }
        }

        let expected = expected_keys(entries);
        if arranged_keys(
            table
                .iter()
                .map(|(key, value)| (key, value_is_table_like(value))),
        ) != expected
        {
            for key in &expected {
                if let Some((key, value)) = table.remove_entry(key) {
                    table.insert_formatted(&key, value);
                }
            }
        }

        // The spacing before `}` belongs to the last value.
        let Some((old_key, Some(suffix))) = old_last else {
            return;
        };
        let Some(new_key) = table.iter().last().map(|(key, _)| key.to_owned()) else {
            return;
        };
        if new_key != old_key {
            if let Some(value) = table.get_mut(&old_key) {
                value.decor_mut().set_suffix("");
            }
            if let Some(value) = table.get_mut(&new_key) {
                value.decor_mut().set_suffix(suffix);
            }
        }
    }

    fn merge_array(&mut self, array: &mut Array, items: &[Node]) {
        let mut old: Vec<Value> = array.iter().cloned().collect();
        let old_nodes: Vec<Node> = old.iter().map(value_node).collect();
        let first_prefix = old.first().and_then(prefix_of);
        let last_suffix = old.last().and_then(suffix_of);
        let last = old.len().checked_sub(1);
        // One item per line when the array is written that way.
        let line_prefix = old
            .last()
            .and_then(prefix_of)
            .and_then(|prefix| prefix.rfind('\n').map(|at| prefix[at..].to_owned()));
        let spacing = |index: usize| match &line_prefix {
            Some(line) => line.clone(),
            None if index == 0 => String::new(),
            None => " ".to_owned(),
        };

        let mut merged: Vec<(Option<usize>, Value)> = Vec::with_capacity(items.len());
        for (from, to) in align(&old_nodes, items) {
            let Some(to) = to else {
                continue;
            };
            let value = match from {
                Some(from) => {
                    let mut value = std::mem::replace(&mut old[from], Value::from(false));
                    self.merge_value(&mut value, &items[to]);
                    value
                }
                None => {
                    let mut value = self.new_value(&items[to]);
                    value.decor_mut().set_prefix(spacing(merged.len()));
                    value.decor_mut().set_suffix("");
                    value
                }
            };
            merged.push((from, value));
        }

        // The spacing after `[` and before `]` stays at the ends of the array.
        if let Some(prefix) = &first_prefix
            && merged.first().is_some_and(|(from, _)| *from != Some(0))
        {
            for (index, (from, value)) in merged.iter_mut().enumerate() {
                if index == 0 {
                    value.decor_mut().set_prefix(prefix.as_str());
                } else if *from == Some(0) {
                    value.decor_mut().set_prefix(spacing(index));
                }
            }
        }
        if let Some(suffix) = &last_suffix
            && merged.last().is_some_and(|(from, _)| *from != last)
        {
            let end = merged.len() - 1;
            for (index, (from, value)) in merged.iter_mut().enumerate() {
                if index == end {
                    value.decor_mut().set_suffix(suffix.as_str());
                } else if *from == last {
                    value.decor_mut().set_suffix("");
                }
            }
        }

        array.clear();
        for (_, value) in merged {
            array.push_formatted(value);
        }
    }

    fn new_item(&mut self, node: &Node, headers: bool) -> Item {
        match node {
            Node::Object { entries } if headers => Item::Table(self.new_table(entries)),
            Node::Array { items } if headers && is_table_like(node) => Item::ArrayOfTables(
                items
                    .iter()
                    .filter_map(|item| match item {
                        Node::Object { entries } => Some(self.new_table(entries)),
                        _ => None,
                    })
                    .collect(),
            ),
            node => Item::Value(self.new_value(node)),
        }
    }

    fn new_table(&mut self, entries: &[Entry]) -> Table {
        let mut table = Table::new();
        for entry in entries {
            let item = self.new_item(&entry.value, true);
            table.insert(&entry.key, item);
        }
        // A table holding only sub-tables needs no header of its own.
        let implicit = table.get_values().is_empty() && !table.is_empty();
        table.set_implicit(implicit);
        table
    }

    fn new_value(&mut self, node: &Node) -> Value {
        match node {
            Node::String { value } => self.string_value(value, None),
            Node::Integer { value } => match value.parse::<i64>() {
                Ok(integer) => Value::from(integer),
                Err(_) => self.unstorable(),
            },
            Node::Float { value } => float_literal(value).unwrap_or_else(|| self.unstorable()),
            Node::Boolean { value } => Value::from(*value),
            Node::Datetime { value } => {
                datetime_literal(value).unwrap_or_else(|| self.unstorable())
            }
            Node::Null => self.unstorable(),
            Node::Array { items } => {
                Value::Array(items.iter().map(|item| self.new_value(item)).collect())
            }
            Node::Object { entries } => {
                let mut table = InlineTable::new();
                for entry in entries {
                    let value = self.new_value(&entry.value);
                    table.insert(entry.key.as_str(), value);
                }
                Value::InlineTable(table)
            }
        }
    }

    /// A string value; replacing a `'literal'` string keeps that quoting when possible, and
    /// line breaks are escaped in CRLF files so the string's text does not depend on them.
    fn string_value(&self, text: &str, old: Option<&Value>) -> Value {
        let literal_quoted = old
            .and_then(|old| match old {
                Value::String(string) => string.as_repr()?.as_raw().as_str(),
                _ => None,
            })
            .is_some_and(|repr| repr.starts_with('\'') && !repr.starts_with("'''"));
        let fits_literal = !text
            .chars()
            .any(|c| c == '\'' || (c.is_control() && c != '\t'));

        let literal = if literal_quoted && fits_literal {
            Some(format!("'{text}'"))
        } else if text.contains('\r') || (self.crlf && text.contains('\n')) {
            Some(basic_string(text))
        } else {
            None
        };
        literal
            .and_then(|literal| literal.parse::<Value>().ok())
            .filter(|value| value.as_str() == Some(text))
            .unwrap_or_else(|| Value::from(text))
    }

    /// Marks the document as not storable and returns a placeholder for the value.
    fn unstorable(&mut self) -> Value {
        self.failed = true;
        Value::from(false)
    }
}

/// Whether a plain `key = value` in a `[table]` turns into a sub-table when it changes kind:
/// objects and arrays of objects do, unless the value already is an inline table / array.
fn becomes_header(value: &Value, node: &Node) -> bool {
    match node {
        Node::Object { .. } => !value.is_inline_table(),
        Node::Array { .. } => is_table_like(node) && !value.is_array(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::Format;
    use super::*;

    const FORGE: &str = "\
# Forge client config\n\
\n\
[general]\n\
\t# Show the FPS counter\n\
\tshowFps = true\n\
\t# Render distance in chunks\n\
\t# Range: 2 ~ 32\n\
\trenderDistance = 12 # chunks\n\
\tmotd = 'Welcome, Steve'\n\
\n\
[client.render]\n\
\t#Maximum particles\n\
\tmaxParticles = 4000\n\
\tscale = 1.5\n\
\ttags = [\"a\", \"b\"]\n\
\twindow = { width = 854, height = 480 }\n\
\n\
[[rules]]\n\
\tname = \"first\"\n\
\tenabled = true\n\
\n\
[[rules]]\n\
\tname = \"second\"\n\
\tenabled = false\n";

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

    fn float(value: &str) -> Node {
        Node::Float {
            value: value.into(),
        }
    }

    fn boolean(value: bool) -> Node {
        Node::Boolean { value }
    }

    fn datetime(value: &str) -> Node {
        Node::Datetime {
            value: value.into(),
        }
    }

    fn entry<'a>(node: &'a Node, path: &[&str]) -> &'a Entry {
        let Node::Object { entries } = node else {
            panic!("not an object");
        };
        let (key, rest) = path.split_first().expect("empty path");
        let found = entries
            .iter()
            .find(|entry| entry.key == *key)
            .unwrap_or_else(|| panic!("missing {key}"));
        if rest.is_empty() {
            found
        } else {
            entry(&found.value, rest)
        }
    }

    fn keys(node: &Node) -> Vec<&str> {
        let Node::Object { entries } = node else {
            panic!("not an object");
        };
        entries.iter().map(|entry| entry.key.as_str()).collect()
    }

    fn entries_mut<'a>(node: &'a mut Node, path: &[&str]) -> &'a mut Vec<Entry> {
        let Node::Object { entries } = node else {
            panic!("not an object");
        };
        match path.split_first() {
            None => entries,
            Some((key, rest)) => {
                let found = entries
                    .iter_mut()
                    .find(|entry| entry.key == *key)
                    .unwrap_or_else(|| panic!("missing {key}"));
                entries_mut(&mut found.value, rest)
            }
        }
    }

    /// Replaces the value of `key`, or appends it to the object like the editor does.
    fn set(document: &mut Node, path: &[&str], key: &str, value: Node) {
        let entries = entries_mut(document, path);
        match entries.iter_mut().find(|entry| entry.key == key) {
            Some(entry) => entry.value = value,
            None => entries.push(Entry::new(key, value, None)),
        }
    }

    fn remove(document: &mut Node, path: &[&str], key: &str) {
        entries_mut(document, path).retain(|entry| entry.key != key);
    }

    fn items_mut<'a>(document: &'a mut Node, path: &[&str], key: &str) -> &'a mut Vec<Node> {
        let found = entries_mut(document, path)
            .iter_mut()
            .find(|entry| entry.key == key)
            .unwrap_or_else(|| panic!("missing {key}"));
        let Node::Array { items } = &mut found.value else {
            panic!("{key}: not an array");
        };
        items
    }

    fn apply(source: &str, document: &Node) -> String {
        let output = Format::Toml.apply(source, document).unwrap();
        assert_eq!(parse(&output).unwrap(), arrange(document), "{output}");
        output
    }

    #[test]
    fn parses_tables_arrays_and_comments() {
        let document = parse(FORGE).unwrap();
        assert_eq!(keys(&document), ["general", "client", "rules"]);
        assert_eq!(
            keys(&entry(&document, &["general"]).value),
            ["showFps", "renderDistance", "motd"]
        );
        assert_eq!(
            keys(&entry(&document, &["client", "render"]).value),
            ["maxParticles", "scale", "tags", "window"]
        );

        // The file header is separated from `[general]` by a blank line.
        assert_eq!(entry(&document, &["general"]).comment, None);
        let comment = |path: &[&str]| entry(&document, path).comment.clone();
        assert_eq!(
            comment(&["general", "showFps"][..]).as_deref(),
            Some("Show the FPS counter")
        );
        assert_eq!(
            comment(&["general", "renderDistance"][..]).as_deref(),
            Some("Render distance in chunks\nRange: 2 ~ 32")
        );
        assert_eq!(comment(&["general", "motd"][..]), None);
        assert_eq!(
            comment(&["client", "render", "maxParticles"][..]).as_deref(),
            Some("Maximum particles")
        );

        assert_eq!(
            entry(&document, &["general", "renderDistance"]).value,
            int("12")
        );
        assert_eq!(
            entry(&document, &["general", "motd"]).value,
            Node::string("Welcome, Steve")
        );
        assert_eq!(
            entry(&document, &["client", "render", "scale"]).value,
            float("1.5")
        );
        assert_eq!(
            entry(&document, &["client", "render", "tags"]).value,
            Node::Array {
                items: vec![Node::string("a"), Node::string("b")]
            }
        );
        assert_eq!(
            entry(&document, &["client", "render", "window"]).value,
            object(vec![("width", int("854")), ("height", int("480"))])
        );
        assert_eq!(
            entry(&document, &["rules"]).value,
            Node::Array {
                items: vec![
                    object(vec![
                        ("name", Node::string("first")),
                        ("enabled", boolean(true))
                    ]),
                    object(vec![
                        ("name", Node::string("second")),
                        ("enabled", boolean(false))
                    ]),
                ]
            }
        );
    }

    #[test]
    fn header_comments_and_number_literals() {
        let document =
            parse("# Main settings\n[general]\n# Port\nport = 0x1F\nratio = 1_000.5\n").unwrap();
        assert_eq!(
            entry(&document, &["general"]).comment.as_deref(),
            Some("Main settings")
        );
        assert_eq!(
            entry(&document, &["general", "port"]).comment.as_deref(),
            Some("Port")
        );
        assert_eq!(entry(&document, &["general", "port"]).value, int("31"));
        assert_eq!(
            entry(&document, &["general", "ratio"]).value,
            float("1_000.5")
        );
    }

    #[test]
    fn plain_keys_are_listed_before_tables() {
        let source = "point = { x = 1 }\nname = \"a\"\n";
        let mut document = parse(source).unwrap();
        assert_eq!(keys(&document), ["name", "point"]);
        assert_eq!(arrange(&document), document);

        set(&mut document, &[], "name", Node::string("b"));
        assert_eq!(
            apply(source, &document),
            "point = { x = 1 }\nname = \"b\"\n"
        );
    }

    #[test]
    fn arrange_moves_objects_after_plain_entries() {
        let document = object(vec![
            (
                "server",
                object(vec![("motd", Node::string("x")), ("port", int("1"))]),
            ),
            (
                "rules",
                Node::Array {
                    items: vec![object(vec![])],
                },
            ),
            ("empty", Node::Array { items: vec![] }),
            ("debug", boolean(false)),
        ]);
        assert_eq!(
            keys(&arrange(&document)),
            ["empty", "debug", "server", "rules"]
        );
        let forge = parse(FORGE).unwrap();
        assert_eq!(arrange(&forge), forge);
    }

    #[test]
    fn unchanged_document_keeps_every_byte() {
        let crlf = FORGE.replace('\n', "\r\n");
        for source in [FORGE, crlf.as_str()] {
            let document = parse(source).unwrap();
            assert_eq!(patch(source, &document, &document), source);
            assert_eq!(Format::Toml.apply(source, &document).unwrap(), source);
        }
    }

    #[test]
    fn scalar_edits_keep_comments_and_quoting() {
        let current = parse(FORGE).unwrap();
        let mut document = current.clone();
        set(&mut document, &["general"], "renderDistance", int("16"));
        set(
            &mut document,
            &["general"],
            "motd",
            Node::string("Hello, Alex"),
        );
        set(&mut document, &["client", "render"], "scale", float("2.25"));
        let expected = FORGE
            .replace(
                "renderDistance = 12 # chunks",
                "renderDistance = 16 # chunks",
            )
            .replace("'Welcome, Steve'", "'Hello, Alex'")
            .replace("scale = 1.5", "scale = 2.25");
        assert_eq!(patch(FORGE, &current, &document), expected);
        assert_eq!(apply(FORGE, &document), expected);

        let source = "motd = 'hi' # greeting\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &[], "motd", Node::string("It's me"));
        assert_eq!(apply(source, &document), "motd = \"It's me\" # greeting\n");
    }

    #[test]
    fn adds_and_removes_keys_in_nested_tables() {
        let mut document = parse(FORGE).unwrap();
        remove(&mut document, &["client", "render"], "scale");
        set(&mut document, &["client", "render"], "fancy", boolean(true));
        set(
            &mut document,
            &["client"],
            "audio",
            object(vec![("volume", float("0.5"))]),
        );
        let expected = FORGE.replace("\tscale = 1.5\n", "").replace(
            "480 }\n",
            "480 }\n\tfancy = true\n\n[client.audio]\nvolume = 0.5\n",
        );
        assert_eq!(apply(FORGE, &document), expected);
    }

    #[test]
    fn adds_and_removes_array_of_tables_entries() {
        let third = object(vec![
            ("name", Node::string("third")),
            ("enabled", boolean(true)),
        ]);

        let mut document = parse(FORGE).unwrap();
        items_mut(&mut document, &[], "rules").push(third.clone());
        assert_eq!(
            apply(FORGE, &document),
            format!("{FORGE}\n[[rules]]\nname = \"third\"\nenabled = true\n")
        );

        let mut document = parse(FORGE).unwrap();
        items_mut(&mut document, &[], "rules").remove(0);
        assert_eq!(
            apply(FORGE, &document),
            FORGE.replace("\n[[rules]]\n\tname = \"first\"\n\tenabled = true\n", "")
        );

        // A changed entry is updated in place, keeping its header and indentation.
        let mut document = parse(FORGE).unwrap();
        items_mut(&mut document, &[], "rules")[1] = third;
        assert_eq!(
            apply(FORGE, &document),
            FORGE
                .replace("\"second\"", "\"third\"")
                .replace("enabled = false", "enabled = true")
        );
    }

    #[test]
    fn inline_arrays_and_tables_keep_their_layout() {
        let source = "mods = [\n\t\"jei\",\n\t\"jade\",\n]\ntags = [\"a\", \"b\"]\npoint = { x = 1, y = 2 }\n";
        let mut document = parse(source).unwrap();
        items_mut(&mut document, &[], "mods").push(Node::string("emi"));
        items_mut(&mut document, &[], "tags").remove(0);
        remove(&mut document, &["point"], "y");
        set(&mut document, &["point"], "z", int("3"));
        assert_eq!(
            apply(source, &document),
            "mods = [\n\t\"jei\",\n\t\"jade\",\n\t\"emi\",\n]\ntags = [\"b\"]\npoint = { x = 1, z = 3 }\n"
        );
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let source = "# Server\r\n[server]\r\nport = 25565 # default\r\nname = \"srv\"\r\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &["server"], "port", int("25566"));
        set(&mut document, &["server"], "motd", Node::string("hi"));
        assert_eq!(
            apply(source, &document),
            "# Server\r\n[server]\r\nport = 25566 # default\r\nname = \"srv\"\r\nmotd = \"hi\"\r\n"
        );

        // Line breaks inside strings are escaped so they do not depend on the file's line ending.
        let mut document = parse(source).unwrap();
        set(
            &mut document,
            &["server"],
            "name",
            Node::string("two\nlines"),
        );
        assert_eq!(
            apply(source, &document),
            "# Server\r\n[server]\r\nport = 25565 # default\r\nname = \"two\\nlines\"\r\n"
        );
    }

    #[test]
    fn non_ascii_text_before_edited_values() {
        let source =
            "title = \"Grüße aus Köln\" # 🎉\nport = 1\n\n[server]\nname = \"Ñandú\" # név\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &[], "port", int("2"));
        set(&mut document, &["server"], "name", Node::string("Ærø"));
        assert_eq!(
            apply(source, &document),
            "title = \"Grüße aus Köln\" # 🎉\nport = 2\n\n[server]\nname = \"Ærø\" # név\n"
        );
    }

    #[test]
    fn datetimes_round_trip() {
        let source = "released = 1979-05-27T07:32:00Z\nday = 1979-05-27\n";
        let mut document = parse(source).unwrap();
        assert_eq!(
            document,
            object(vec![
                ("released", datetime("1979-05-27T07:32:00Z")),
                ("day", datetime("1979-05-27")),
            ])
        );
        set(&mut document, &[], "day", datetime("2024-01-02"));
        set(&mut document, &[], "at", datetime("07:32:00"));
        assert_eq!(
            apply(source, &document),
            "released = 1979-05-27T07:32:00Z\nday = 2024-01-02\nat = 07:32:00\n"
        );

        for valid in ["1979-05-27", "07:32:00", "1979-05-27T07:32:00-08:00"] {
            assert!(datetime_ok(valid), "{valid}");
        }
        for invalid in ["tomorrow", " 1979-05-27", "1979-13-01", "12"] {
            assert!(!datetime_ok(invalid), "{invalid}");
        }
    }

    #[test]
    fn float_text_keeps_literals_and_normalizes_numbers() {
        let cases = [
            ("0.5", Some("0.5")),
            ("1e3", Some("1e3")),
            ("1_000.5", Some("1_000.5")),
            ("-inf", Some("-inf")),
            ("5", Some("5.0")),
            ("-3", Some("-3.0")),
            (".5", Some("0.5")),
            ("1.", Some("1.0")),
            ("Infinity", Some("inf")),
            ("-INF", Some("-inf")),
            ("NaN", Some("nan")),
            ("abc", None),
            ("", None),
        ];
        for (input, expected) in cases {
            assert_eq!(float_text(input).as_deref(), expected, "{input}");
        }
    }

    #[test]
    fn reordered_keys_are_rewritten_in_the_new_order() {
        let source = "a = 1 # first\nb = 2\n";
        let document = object(vec![("b", int("2")), ("a", int("1"))]);
        assert_eq!(apply(source, &document), "b = 2\na = 1 # first\n");
    }

    #[test]
    fn combined_edits_round_trip_and_keep_other_comments() {
        let mut document = parse(FORGE).unwrap();
        set(&mut document, &[], "version", int("3"));
        set(
            &mut document,
            &["general"],
            "showFps",
            object(vec![("enabled", boolean(true))]),
        );
        remove(&mut document, &["client", "render"], "window");
        set(
            &mut document,
            &["client", "render"],
            "tags",
            Node::Array { items: vec![] },
        );
        items_mut(&mut document, &[], "rules").reverse();
        let output = apply(FORGE, &document);
        assert!(
            output.contains("\t# Range: 2 ~ 32\n\trenderDistance = 12 # chunks\n"),
            "{output}"
        );
        assert!(
            output.contains("\t#Maximum particles\n\tmaxParticles = 4000\n"),
            "{output}"
        );
    }

    #[test]
    fn integers_beyond_64_bits_are_rejected() {
        let mut document = parse("port = 1\n").unwrap();
        set(&mut document, &[], "port", int("99999999999999999999"));
        assert!(Format::Toml.apply("port = 1\n", &document).is_err());
    }

    #[test]
    fn duplicate_keys_are_parse_errors_with_their_line() {
        let error = parse("[server]\nport = 1\nport = 2\n").unwrap_err();
        assert!(error.message.contains("duplicate key"), "{}", error.message);
        assert_eq!((error.line, error.column), (Some(3), Some(1)));

        let error = parse("[a]\nx = 1\n\n[a]\ny = 2\n").unwrap_err();
        assert_eq!(error.line, Some(4));
    }

    #[test]
    fn emit_writes_tables_and_arrays_of_tables() {
        let document = object(vec![
            ("title", Node::string("x")),
            ("owner", object(vec![("name", Node::string("a"))])),
            (
                "rules",
                Node::Array {
                    items: vec![object(vec![("n", int("1"))]), object(vec![("n", int("2"))])],
                },
            ),
        ]);
        let expected =
            "title = \"x\"\n\n[owner]\nname = \"a\"\n\n[[rules]]\nn = 1\n\n[[rules]]\nn = 2\n";
        assert_eq!(emit("a = 1\n", &document), expected);
        assert_eq!(emit("a = 1\r\n", &document), expected.replace('\n', "\r\n"));
        assert_eq!(parse(expected).unwrap(), document);
    }
}
