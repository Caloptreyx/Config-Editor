//! TOML through `toml_edit`, which keeps every byte of the parts of a document it does not
//! touch. Edits are applied to the parsed `DocumentMut` and the file is rendered again; only
//! line endings need restoring because `toml_edit` always writes `\n`.
//!
//! Inside a table, TOML writes every `key = value` line (scalars, arrays, inline tables, dotted
//! keys) before the `[sub-tables]` and `[[arrays of tables]]`, so objects list their key/value
//! entries first and their header entries after them, both in file order. [`arrange`] puts a
//! submitted document into the order the written file will have.
use std::collections::HashSet;
use std::fmt::Write as _;

use toml_edit::{
    Array, ArrayOfTables, DocumentMut, InlineTable, Item, KeyMut, RawString, Table, Value,
};

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
    let first = first_written(parsed.as_table());
    let mut writer = Writer::new(source);
    writer.merge_table(parsed.as_table_mut(), entries, true);
    if writer.failed {
        return source.to_owned();
    }
    writer.place_detached(&mut parsed);
    tidy(parsed.as_table_mut(), first);

    let (bom, body) = split_bom(source);
    format!("{bom}{}", restore_line_endings(body, &parsed.to_string()))
}

/// The whole document written from scratch; objects keep the form (inline or `[table]`) they
/// have in `source`.
pub fn emit(source: &str, document: &Node) -> String {
    let (bom, body) = split_bom(source);
    let file = body.parse::<DocumentMut>().ok();
    let entries = match document {
        Node::Object { entries } => entries.as_slice(),
        _ => &[],
    };
    let mut writer = Writer::new(body);
    let output =
        DocumentMut::from(writer.shaped_table(file.as_ref().map(DocumentMut::as_table), entries));
    format!(
        "{bom}{}",
        text::with_eol(&output.to_string(), text::eol(body))
    )
}

/// `document` in the order [`parse`] lists the file [`patch`] writes for it: in every table the
/// key/value entries first, then the header entries. Entries present in `source` keep their
/// form; new objects and arrays of objects become headers under a `[table]`, inline elsewhere.
pub fn arrange(source: &str, document: &Node) -> Node {
    let file = source.parse::<DocumentMut>().ok();
    arrange_table(file.as_ref().map(DocumentMut::as_table), document, true)
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
    let (mut lines, mut headers) = (Vec::new(), Vec::new());
    for (key, item) in table.iter() {
        let Some(value) = item_node(item) else {
            continue;
        };
        let entry = Entry::new(key, value, item_comment(table, key, item));
        if is_header(item) {
            headers.push(entry);
        } else {
            lines.push(entry);
        }
    }
    lines.extend(headers);
    Node::Object { entries: lines }
}

fn inline_node(table: &InlineTable) -> Node {
    let entries = table
        .iter()
        .map(|(key, value)| {
            let prefix = table.key(key).and_then(|key| key.leaf_decor().prefix());
            Entry::new(key, value_node(value), comment(prefix))
        })
        .collect();
    Node::Object { entries }
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

fn raw(text: Option<&RawString>) -> &str {
    text.and_then(RawString::as_str).unwrap_or("")
}

/// The whole lines of a decor prefix before the comment block that belongs to its entry
/// (the text that stays when the entry is removed).
fn detached(prefix: &str) -> &str {
    let mut end = prefix.rfind('\n').map_or(0, |at| at + 1);
    while end > 0 {
        let start = text::line_start(prefix, end - 1);
        if !prefix[start..end].trim_start().starts_with('#') {
            break;
        }
        end = start;
    }
    &prefix[..end]
}

/// `detached` text put in front of `prefix`, without doubling the blank line between them.
fn join_prefix(detached: &str, prefix: &str) -> String {
    let prefix = match prefix.find('\n') {
        Some(at) if !detached.is_empty() && prefix[..at].trim().is_empty() => &prefix[at + 1..],
        _ => prefix,
    };
    format!("{detached}{prefix}")
}

fn arrange_table(table: Option<&Table>, node: &Node, headers: bool) -> Node {
    let Node::Object { entries } = node else {
        return node.clone();
    };
    let (mut lines, mut tables) = (Vec::new(), Vec::new());
    for entry in entries {
        let existing = table.and_then(|table| table.get(&entry.key));
        let header = header_form(existing, &entry.value, headers);
        let value = match (existing, &entry.value) {
            (Some(Item::Table(child)), Node::Object { .. }) if header || child.is_dotted() => {
                arrange_table(Some(child), &entry.value, !child.is_dotted())
            }
            (_, Node::Object { .. }) if header => arrange_table(None, &entry.value, true),
            (_, Node::Array { items }) if header => Node::Array {
                items: partners(existing, items)
                    .into_iter()
                    .zip(items)
                    .map(|(partner, item)| arrange_table(partner, item, true))
                    .collect(),
            },
            (_, value) => value.clone(),
        };
        let entry = Entry::new(entry.key.clone(), value, entry.comment.clone());
        if header {
            tables.push(entry);
        } else {
            lines.push(entry);
        }
    }
    lines.extend(tables);
    Node::Object { entries: lines }
}

/// The table of the array of tables `existing` that each item of `items` is written into.
fn partners<'a>(existing: Option<&'a Item>, items: &[Node]) -> Vec<Option<&'a Table>> {
    let old: Vec<&Table> = existing
        .and_then(Item::as_array_of_tables)
        .map(|array| array.iter().collect())
        .unwrap_or_default();
    let old_nodes: Vec<Node> = old.iter().map(|table| table_node(table)).collect();
    let mut partners = vec![None; items.len()];
    for (from, to) in align(&old_nodes, items) {
        if let (Some(from), Some(to)) = (from, to) {
            partners[to] = Some(old[from]);
        }
    }
    partners
}

/// Whether `node` is written under its own `[header]` in a table, given the item it replaces;
/// `headers`: the table is a `[table]` (not a dotted key), so new objects become sub-tables.
fn header_form(existing: Option<&Item>, node: &Node, headers: bool) -> bool {
    match (existing, node) {
        (Some(Item::Table(table)), Node::Object { .. }) => !table.is_dotted(),
        (Some(Item::ArrayOfTables(_)), node) => is_table_like(node),
        (Some(Item::Value(value)), node) => headers && becomes_header(value, node),
        (_, node) => headers && is_table_like(node),
    }
}

/// Whether a `key = value` turns into a sub-table when it changes kind: objects and arrays of
/// objects do, unless the value already is an inline table / array.
fn becomes_header(value: &Value, node: &Node) -> bool {
    match node {
        Node::Object { .. } => !value.is_inline_table(),
        Node::Array { .. } => is_table_like(node) && !value.is_array(),
        _ => false,
    }
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

/// Whether the item is written under its own `[header]` / `[[header]]`.
fn is_header(item: &Item) -> bool {
    match item {
        Item::Table(table) => !table.is_dotted(),
        Item::ArrayOfTables(_) => true,
        Item::None | Item::Value(_) => false,
    }
}

/// The keys of a table in the order [`parse`] lists them.
fn listed_keys(table: &Table) -> Vec<&str> {
    let (headers, mut lines): (Vec<_>, Vec<_>) =
        table.iter().partition(|(_, item)| is_header(item));
    lines.extend(headers);
    lines.into_iter().map(|(key, _)| key).collect()
}

/// A table as `toml_edit` writes it.
struct Header {
    /// Tables are written sorted by this key; a table without a position takes the position
    /// of the table visited before it.
    key: (bool, isize),
    position: Option<isize>,
    /// Implicit tables without keys get no header of their own.
    shown: bool,
}

/// Every non-dotted table in visiting order (the root first).
fn collect_headers(table: &Table, member: bool, last: &mut isize, out: &mut Vec<Header>) {
    if !table.is_dotted() {
        if let Some(position) = table.position() {
            *last = position;
        }
        out.push(Header {
            key: (!out.is_empty(), *last),
            position: table.position(),
            shown: member || !table.is_implicit() || !table.get_values().is_empty(),
        });
    }
    for (_, item) in table.iter() {
        match item {
            Item::Table(child) => collect_headers(child, false, last, out),
            Item::ArrayOfTables(array) => {
                for child in array.iter() {
                    collect_headers(child, true, last, out);
                }
            }
            Item::None | Item::Value(_) => {}
        }
    }
}

/// The written tables (visiting index and position) in the order they are written; with
/// `renumbered` in visiting order.
fn written_order(root: &Table, renumbered: bool) -> Vec<(usize, Option<isize>)> {
    let mut headers = Vec::new();
    collect_headers(root, false, &mut 0, &mut headers);
    let mut order: Vec<usize> = (1..headers.len())
        .filter(|&index| headers[index].shown)
        .collect();
    if !renumbered {
        order.sort_by_key(|&index| headers[index].key);
    }
    order
        .into_iter()
        .map(|index| (index, headers[index].position))
        .collect()
}

/// Whether parsing the written document lists the header entries of every table in item
/// order. Parsing creates a table at its own header, or at the first header inside it when
/// it has none.
fn headers_in_order(root: &Table) -> bool {
    let mut headers = Vec::new();
    collect_headers(root, false, &mut 0, &mut headers);
    let mut order: Vec<usize> = (0..headers.len()).collect();
    order.sort_by_key(|&index| headers[index].key);
    let mut rank = vec![0; headers.len()];
    for (written, index) in order.into_iter().enumerate() {
        rank[index] = written;
    }
    let shown: Vec<Option<usize>> = headers
        .iter()
        .zip(rank)
        .map(|(header, rank)| header.shown.then_some(rank))
        .collect();
    let mut in_order = true;
    created_at(root, &shown, &mut 0, &mut in_order);
    in_order
}

/// When parsing creates `table` and when the first header inside it is written (as write
/// ranks); clears `in_order` when a table's header entries would be created out of order.
fn created_at(
    table: &Table,
    shown: &[Option<usize>],
    next: &mut usize,
    in_order: &mut bool,
) -> (Option<usize>, Option<usize>) {
    let own = if table.is_dotted() {
        None
    } else {
        *next += 1;
        shown[*next - 1]
    };
    let mut earliest = own;
    let mut headers = Vec::new();
    for (_, item) in table.iter() {
        match item {
            Item::Table(child) => {
                let (at, first) = created_at(child, shown, next, in_order);
                earliest = earlier(earliest, first);
                if !child.is_dotted() {
                    headers.extend(at);
                }
            }
            Item::ArrayOfTables(array) => {
                let mut members = Vec::new();
                for child in array.iter() {
                    let (at, first) = created_at(child, shown, next, in_order);
                    earliest = earlier(earliest, first);
                    members.extend(at);
                }
                *in_order &= members.is_sorted();
                headers.extend(members.first().copied());
            }
            Item::None | Item::Value(_) => {}
        }
    }
    *in_order &= headers.is_sorted();
    let at = if table.is_dotted() {
        None
    } else {
        own.or(earliest)
    };
    (at, earliest)
}

fn earlier(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    a.into_iter().chain(b).min()
}

/// The lowest position after `after` of a header table written in the file.
fn next_position(table: &Table, after: isize) -> Option<isize> {
    let own = table
        .position()
        .filter(|&position| !table.is_dotted() && position > after);
    table
        .iter()
        .filter_map(|(_, item)| match item {
            Item::Table(child) => next_position(child, after),
            Item::ArrayOfTables(array) => array
                .iter()
                .filter_map(|child| next_position(child, after))
                .min(),
            Item::None | Item::Value(_) => None,
        })
        .chain(own)
        .min()
}

fn table_at(table: &mut Table, position: isize) -> Option<&mut Table> {
    if !table.is_dotted() && table.position() == Some(position) {
        return Some(table);
    }
    for (_, item) in table.iter_mut() {
        let found = match item {
            Item::Table(child) => table_at(child, position),
            Item::ArrayOfTables(array) => {
                array.iter_mut().find_map(|child| table_at(child, position))
            }
            Item::None | Item::Value(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn table_at_index<'a>(
    table: &'a mut Table,
    index: usize,
    next: &mut usize,
) -> Option<&'a mut Table> {
    if !table.is_dotted() {
        if *next == index {
            return Some(table);
        }
        *next += 1;
    }
    for (_, item) in table.iter_mut() {
        let found = match item {
            Item::Table(child) => table_at_index(child, index, next),
            Item::ArrayOfTables(array) => array
                .iter_mut()
                .find_map(|child| table_at_index(child, index, next)),
            Item::None | Item::Value(_) => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The key carrying the line prefix of the `key = value` entry `name` (the first key written
/// for a dotted table).
fn line_key<'a>(table: &'a mut Table, name: &str) -> Option<KeyMut<'a>> {
    if table.get(name)?.is_value() {
        return table.key_mut(name);
    }
    let Item::Table(child) = table.get_mut(name)? else {
        return None;
    };
    if !child.is_dotted() {
        return None;
    }
    let first = child.iter().next()?.0.to_owned();
    line_key(child, &first)
}

/// Puts `text` in front of the `key = value` line(s) of the entry `name`; false for headers.
fn prepend_to_line(table: &mut Table, name: &str, text: &str) -> bool {
    let Some(mut key) = line_key(table, name) else {
        return false;
    };
    let prefix = join_prefix(text, raw(key.leaf_decor().prefix()));
    key.leaf_decor_mut().set_prefix(prefix);
    true
}

/// What the file starts with: its first `key = value` line or its first `[header]`.
#[derive(PartialEq)]
enum First {
    Line(String),
    Table(isize),
}

fn first_line(root: &Table) -> Option<String> {
    root.iter()
        .find(|(_, item)| !is_header(item))
        .map(|(key, _)| key.to_owned())
}

fn first_written(root: &Table) -> Option<First> {
    match first_line(root) {
        Some(name) => Some(First::Line(name)),
        None => written_order(root, false)
            .first()
            .and_then(|&(_, position)| position)
            .map(First::Table),
    }
}

/// Layout after the merge: the file's leading text stays on top, a header after a new table
/// is separated from it by a blank line, and tables written out of item order are renumbered.
fn tidy(root: &mut Table, first: Option<First>) {
    let renumbered = !headers_in_order(root);
    let order = written_order(root, renumbered);
    if let Some(first) = first {
        keep_file_header(root, &first, &order);
    }
    for pair in order.windows(2) {
        let [(_, None), (index, Some(_))] = pair else {
            continue;
        };
        if let Some(table) = table_at_index(root, *index, &mut 0) {
            let prefix = raw(table.decor().prefix());
            if !starts_blank(prefix) {
                let prefix = format!("\n{prefix}");
                table.decor_mut().set_prefix(prefix);
            }
        }
    }
    if renumbered {
        renumber(root, &mut 0);
    }
}

/// Moves the text at the top of the file (the leading part of the first entry's prefix) to
/// the entry written first now.
fn keep_file_header(root: &mut Table, first: &First, order: &[(usize, Option<isize>)]) {
    let new_line = first_line(root);
    let unchanged = match (first, &new_line, order.first()) {
        (First::Line(old), Some(new), _) => old == new,
        (First::Table(old), None, Some((_, position))) => *position == Some(*old),
        _ => false,
    };
    if unchanged {
        return;
    }

    let head = match first {
        First::Line(name) => {
            let Some(mut key) = line_key(root, name) else {
                return;
            };
            let prefix = raw(key.leaf_decor().prefix()).to_owned();
            let head = detached(&prefix).to_owned();
            key.leaf_decor_mut().set_prefix(&prefix[head.len()..]);
            head
        }
        First::Table(position) => {
            let Some(table) = table_at(root, *position) else {
                return;
            };
            let prefix = raw(table.decor().prefix()).to_owned();
            let head = detached(&prefix).to_owned();
            let rest = &prefix[head.len()..];
            let rest = if starts_blank(rest) {
                rest.to_owned()
            } else {
                format!("\n{rest}")
            };
            table.decor_mut().set_prefix(rest);
            head
        }
    };
    if head.is_empty() {
        return;
    }

    match new_line {
        Some(name) => {
            prepend_to_line(root, &name, &head);
        }
        None => {
            if let Some(&(index, _)) = order.first()
                && let Some(table) = table_at_index(root, index, &mut 0)
            {
                let prefix = join_prefix(&head, raw(table.decor().prefix()));
                table.decor_mut().set_prefix(prefix);
            }
        }
    }
}

/// Whether a prefix starts with a blank line.
fn starts_blank(prefix: &str) -> bool {
    prefix
        .find('\n')
        .is_some_and(|at| prefix[..at].trim().is_empty())
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
    /// Text that stood before removed entries, with the position of the table after which
    /// it is written again (in front of the next `[header]`).
    detached: Vec<(isize, String)>,
}

impl Writer {
    fn new(source: &str) -> Self {
        Self {
            crlf: text::eol(source) == "\r\n",
            failed: false,
            detached: Vec::new(),
        }
    }

    fn merge_table(&mut self, table: &mut Table, entries: &[Entry], root: bool) {
        let headers = !table.is_dotted();
        let keep: HashSet<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        let indent = body_indent(table);

        for entry in entries {
            let reshaped = match table.get_mut(&entry.key) {
                Some(item) => {
                    let was_header = is_header(item);
                    self.merge_item(item, &entry.value, headers);
                    was_header != is_header(item)
                }
                None => {
                    let header = header_form(None, &entry.value, headers);
                    let item = self.shaped_item(None, &entry.value, header);
                    table.insert(&entry.key, item);
                    if !header
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

        let anchor = if root {
            Some(isize::MIN)
        } else {
            table.position()
        };
        self.remove_missing(table, &keep, anchor);
        // An emptied implicit table is not written at all; keep its header.
        if table.is_empty() {
            table.set_implicit(false);
        }
        self.order_table(table, entries);
    }

    /// Removes the entries that are not kept. The text above a removed entry, up to its own
    /// comment block, stays: in front of the next `key = value` line, or else in front of the
    /// next `[header]` after `anchor`.
    fn remove_missing(&mut self, table: &mut Table, keep: &HashSet<&str>, anchor: Option<isize>) {
        let names: Vec<String> = table.iter().map(|(key, _)| key.to_owned()).collect();
        let mut carry = String::new();
        for name in names {
            if keep.contains(name.as_str()) {
                if !carry.is_empty() && prepend_to_line(table, &name, &carry) {
                    carry.clear();
                }
                continue;
            }
            let Some((key, item)) = table.remove_entry(&name) else {
                continue;
            };
            match &item {
                Item::Value(_) => carry.push_str(detached(raw(key.leaf_decor().prefix()))),
                Item::Table(child) if !child.is_dotted() => self.detach(child),
                Item::ArrayOfTables(array) => {
                    for child in array.iter() {
                        self.detach(child);
                    }
                }
                Item::None | Item::Table(_) => {}
            }
        }
        if !carry.is_empty()
            && let Some(anchor) = anchor
        {
            self.detached.push((anchor, carry));
        }
    }

    /// Keeps the text above a removed `[header]` for the next header.
    fn detach(&mut self, table: &Table) {
        let text = detached(raw(table.decor().prefix()));
        if !text.is_empty()
            && let Some(position) = table.position()
        {
            self.detached.push((position, text.to_owned()));
        }
    }

    /// Writes the text of removed entries in front of the next `[header]` (or at the end),
    /// keeping the order the texts had in the file.
    fn place_detached(&mut self, document: &mut DocumentMut) {
        let mut detached = std::mem::take(&mut self.detached);
        detached.sort_by_key(|(after, _)| *after);
        for (after, text) in detached.into_iter().rev() {
            let next = next_position(document.as_table(), after);
            match next.and_then(|position| table_at(document.as_table_mut(), position)) {
                Some(table) => {
                    let prefix = join_prefix(&text, raw(table.decor().prefix()));
                    table.decor_mut().set_prefix(prefix);
                }
                None if text.trim().is_empty() => {}
                None => {
                    let trailing = format!("{text}{}", raw(Some(document.trailing())));
                    document.set_trailing(trailing);
                }
            }
        }
    }

    fn order_table(&mut self, table: &mut Table, entries: &[Entry]) {
        let expected: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        if listed_keys(table) == expected {
            return;
        }
        for key in &expected {
            if let Some((key, item)) = table.remove_entry(key) {
                table.insert_formatted(&key, item);
            }
        }
    }

    /// `headers`: the item belongs to a `[table]`, so new objects become sub-tables.
    fn merge_item(&mut self, item: &mut Item, node: &Node, headers: bool) {
        if item_node(item).as_ref() == Some(node) {
            return;
        }
        let header = header_form(Some(&*item), node, headers);
        match (item, node) {
            (Item::Table(table), Node::Object { entries })
                if header || (table.is_dotted() && !entries.is_empty()) =>
            {
                self.merge_table(table, entries, false);
            }
            (Item::ArrayOfTables(array), Node::Array { items }) if header => {
                self.merge_tables(array, items);
            }
            (Item::Value(value), node) if !header => self.merge_value(value, node),
            (item, node) => *item = self.shaped_item(None, node, header),
        }
    }

    fn merge_tables(&mut self, array: &mut ArrayOfTables, items: &[Node]) {
        let mut old: Vec<Table> = array.iter().cloned().collect();
        let old_nodes: Vec<Node> = old.iter().map(table_node).collect();
        array.clear();
        for (from, to) in align(&old_nodes, items) {
            match (from, to.map(|to| &items[to])) {
                (Some(from), None) => self.detach(&old[from]),
                (Some(from), Some(Node::Object { entries })) => {
                    let mut table = std::mem::take(&mut old[from]);
                    self.merge_table(&mut table, entries, false);
                    array.push(table);
                }
                (None, Some(Node::Object { entries })) => {
                    let table = self.shaped_table(None, entries);
                    array.push(table);
                }
                _ => {}
            }
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

        let expected: Vec<&str> = entries.iter().map(|entry| entry.key.as_str()).collect();
        if !table
            .iter()
            .map(|(key, _)| key)
            .eq(expected.iter().copied())
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

    /// A new item in the form `header` decides; tables follow the shape of `existing` (the
    /// item of the file at the same place) so their entries keep their forms too.
    fn shaped_item(&mut self, existing: Option<&Item>, node: &Node, header: bool) -> Item {
        match node {
            Node::Object { entries } if header => {
                Item::Table(self.shaped_table(existing.and_then(Item::as_table), entries))
            }
            Node::Array { items } if header => Item::ArrayOfTables(
                partners(existing, items)
                    .into_iter()
                    .zip(items)
                    .filter_map(|(partner, item)| match item {
                        Node::Object { entries } => Some(self.shaped_table(partner, entries)),
                        _ => None,
                    })
                    .collect(),
            ),
            node => Item::Value(self.new_value(node)),
        }
    }

    fn shaped_table(&mut self, file: Option<&Table>, entries: &[Entry]) -> Table {
        let mut table = Table::new();
        for entry in entries {
            let existing = file.and_then(|file| file.get(&entry.key));
            let header = header_form(existing, &entry.value, true);
            let item = self.shaped_item(existing, &entry.value, header);
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
        assert_eq!(
            parse(&output).unwrap(),
            arrange(source, document),
            "{output}"
        );
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
    fn key_value_lines_are_listed_before_headers() {
        let source = "point = { x = 1 }\nname = \"a\"\n\n[server]\nport = 1\n";
        let mut document = parse(source).unwrap();
        assert_eq!(keys(&document), ["point", "name", "server"]);
        assert_eq!(arrange(source, &document), document);

        set(&mut document, &[], "name", Node::string("b"));
        set(&mut document, &["point"], "y", int("2"));
        assert_eq!(
            apply(source, &document),
            "point = { x = 1, y = 2 }\nname = \"b\"\n\n[server]\nport = 1\n"
        );
    }

    #[test]
    fn arrange_follows_the_form_entries_are_written_in() {
        let source = "point = { x = 1 }\n\n[server]\nport = 1\n";
        let document = object(vec![
            ("server", object(vec![("port", int("1"))])),
            ("limits", object(vec![("max", int("5"))])),
            (
                "point",
                object(vec![("x", int("1")), ("deep", object(vec![]))]),
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
        let arranged = arrange(source, &document);
        assert_eq!(
            keys(&arranged),
            ["point", "empty", "debug", "server", "limits", "rules"]
        );
        // Inside an inline table everything is a `key = value`, kept in the given order.
        assert_eq!(keys(&entry(&arranged, &["point"]).value), ["x", "deep"]);
        assert_eq!(
            apply(source, &document),
            "point = { x = 1, deep = {} }\nempty = []\ndebug = false\n\n[server]\nport = 1\n\n[limits]\nmax = 5\n\n[[rules]]\n"
        );

        let forge = parse(FORGE).unwrap();
        assert_eq!(arrange(FORGE, &forge), forge);
    }

    #[test]
    fn removing_entries_keeps_the_text_above_them() {
        // The file header stays; the removed key's own comment goes with it.
        let source = "# Config file\n\n# Port\nport = 1\nname = \"x\"\n";
        let mut document = parse(source).unwrap();
        remove(&mut document, &[], "port");
        assert_eq!(apply(source, &document), "# Config file\n\nname = \"x\"\n");

        // Without a key after it, the text goes in front of the next header.
        let source = "# Header\n\nport = 1\n\n[a]\nx = 1\n";
        let mut document = parse(source).unwrap();
        remove(&mut document, &[], "port");
        assert_eq!(apply(source, &document), "# Header\n\n[a]\nx = 1\n");

        let source = "# Config\n\n[a]\nx = 1\n\n# Section b\n[b]\ny = 2\n";
        let mut document = parse(source).unwrap();
        remove(&mut document, &[], "a");
        assert_eq!(
            apply(source, &document),
            "# Config\n\n# Section b\n[b]\ny = 2\n"
        );

        let source = "# Top\n\n[a]\nx = 1\n\n[[r]]\nn = 1\n\n[[r]]\nn = 2\n";
        let mut document = parse(source).unwrap();
        remove(&mut document, &[], "a");
        items_mut(&mut document, &[], "r").remove(0);
        assert_eq!(apply(source, &document), "# Top\n\n[[r]]\nn = 2\n");

        let mut document = parse(FORGE).unwrap();
        remove(&mut document, &["general"], "showFps");
        assert_eq!(
            apply(FORGE, &document),
            FORGE.replace("\t# Show the FPS counter\n\tshowFps = true\n", "")
        );
    }

    #[test]
    fn new_root_keys_go_below_the_file_header() {
        let mut document = parse(FORGE).unwrap();
        set(&mut document, &[], "version", int("3"));
        assert_eq!(
            apply(FORGE, &document),
            FORGE.replace(
                "# Forge client config\n\n",
                "# Forge client config\n\nversion = 3\n\n"
            )
        );

        let source = "[a]\nx = 1\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &[], "version", int("3"));
        assert_eq!(apply(source, &document), "version = 3\n\n[a]\nx = 1\n");
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
