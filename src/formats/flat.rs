//! Kind inference and line-edit helpers shared by the text-valued formats (properties, env,
//! ini).
use std::{borrow::Borrow, collections::HashMap};

use super::{
    Entry, Node,
    rules::is_integer,
    text::{self, Edits},
};

/// The kind a value written as `text` is shown as.
pub fn infer(text: &str) -> Node {
    match text {
        "true" => Node::Boolean { value: true },
        "false" => Node::Boolean { value: false },
        _ if is_integer(text) => Node::Integer {
            value: text.to_string(),
        },
        _ if is_decimal(text) => Node::Float {
            value: text.to_string(),
        },
        _ => Node::string(text),
    }
}

/// The text a scalar is written as; `None` for collections, null and dates.
pub fn scalar_text(node: &Node) -> Option<String> {
    match node {
        Node::String { value } | Node::Integer { value } | Node::Float { value } => {
            Some(value.clone())
        }
        Node::Boolean { value } => Some(value.to_string()),
        _ => None,
    }
}

/// Floats are stored as written; anything Rust reads as a number is accepted.
pub fn float_text(value: &str) -> Option<String> {
    value.parse::<f64>().ok().map(|_| value.to_string())
}

/// `^-?[0-9]+\.[0-9]+$`
fn is_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let Some((whole, fraction)) = digits.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && !fraction.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

/// A key/value entry of a line-based file.
pub struct Line {
    pub key: String,
    /// The decoded value text.
    pub value: String,
    /// Start of the entry's first line.
    pub start: usize,
    /// End of the entry's last line, after its line break.
    pub end: usize,
}

impl Line {
    pub fn into_entry(self, source: &str, markers: &[&str]) -> Entry {
        let comment = text::comment_above(source, self.start, markers);
        Entry::new(self.key, infer(&self.value), comment)
    }
}

/// Edits of a line-based file; inserted lines use the file's line ending.
pub struct LineEdits<'a> {
    source: &'a str,
    markers: &'a [&'a str],
    eol: &'static str,
    edits: Edits,
    /// Whether the text before the end of the file ends with a line break.
    terminated: bool,
}

impl<'a> LineEdits<'a> {
    pub fn new(source: &'a str, markers: &'a [&'a str]) -> Self {
        Self {
            source,
            markers,
            eol: text::eol(source),
            edits: Edits::default(),
            terminated: source.is_empty() || source.ends_with('\n'),
        }
    }

    pub fn eol(&self) -> &'static str {
        self.eol
    }

    pub fn replace(&mut self, start: usize, end: usize, text: String) {
        self.edits.replace(start, end, text);
    }

    pub fn delete(&mut self, start: usize, end: usize) {
        self.edits.delete(start, end);
    }

    /// Inserts `lines` at the line start `at`; at the end of a file whose last line has no
    /// line break, one is added first.
    pub fn insert_lines(&mut self, at: usize, lines: &[String]) {
        let mut inserted = String::new();
        if at == self.source.len() && !self.terminated {
            inserted.push_str(self.eol);
            self.terminated = true;
        }
        for line in lines {
            inserted.push_str(line);
            inserted.push_str(self.eol);
        }
        self.edits.insert(at, inserted);
    }

    /// Start of the comment block directly above the line starting at `line` (`line` itself
    /// when there is none).
    pub fn comment_start(&self, line: usize) -> usize {
        let mut start = line;
        while start > 0 {
            let above = text::line_start(self.source, start - 1);
            let content = self.source[above..start].trim_start();
            if !self
                .markers
                .iter()
                .any(|marker| content.starts_with(marker))
            {
                break;
            }
            start = above;
        }
        start
    }

    /// The edited file; `None` when edits overlap.
    pub fn finish(self) -> Option<String> {
        self.edits.apply(self.source)
    }
}

/// The index in `file` of each document key (`None` for new keys); `None` overall when keys
/// kept from the file appear in a different relative order.
pub fn match_keys<'k>(
    file: &[&str],
    document: impl IntoIterator<Item = &'k str>,
) -> Option<Vec<Option<usize>>> {
    let positions: HashMap<&str, usize> = file
        .iter()
        .enumerate()
        .map(|(index, key)| (*key, index))
        .collect();
    let mut last = None;
    document
        .into_iter()
        .map(|key| {
            let index = positions.get(key).copied();
            if let Some(index) = index {
                if last.is_some_and(|last| index < last) {
                    return None;
                }
                last = Some(index);
            }
            Some(index)
        })
        .collect()
}

/// Applies `document` to one container of flat entries: drops the lines of removed keys,
/// replaces changed values with the edit `rewrite` returns (start, end, text) and inserts new
/// keys rendered by `render` after the preceding kept key (before the first kept key and its
/// comment block when none precedes, at `end` when nothing is kept).
/// Returns `false` without editing when kept keys changed their relative order.
pub fn edit_container<T: AsRef<Line>, E: Borrow<Entry>>(
    writer: &mut LineEdits<'_>,
    lines: &[T],
    document: &[E],
    end: usize,
    rewrite: impl Fn(&T, &str) -> (usize, usize, String),
    render: impl Fn(&str, &str) -> String,
) -> bool {
    let keys: Vec<&str> = lines
        .iter()
        .map(|line| line.as_ref().key.as_str())
        .collect();
    let Some(matches) = match_keys(
        &keys,
        document.iter().map(|entry| entry.borrow().key.as_str()),
    ) else {
        return false;
    };

    let mut kept = vec![false; lines.len()];
    for &index in matches.iter().flatten() {
        kept[index] = true;
    }
    for (line, kept) in lines.iter().zip(kept) {
        if !kept {
            writer.delete(line.as_ref().start, line.as_ref().end);
        }
    }

    let mut at = matches.iter().flatten().next().map_or(end, |&index| {
        writer.comment_start(lines[index].as_ref().start)
    });
    for (entry, target) in document.iter().zip(matches) {
        let entry = entry.borrow();
        let value = scalar_text(&entry.value).unwrap_or_default();
        match target {
            Some(index) => {
                let line = &lines[index];
                if line.as_ref().value != value {
                    let (start, stop, text) = rewrite(line, &value);
                    writer.replace(start, stop, text);
                }
                at = line.as_ref().end;
            }
            None => writer.insert_lines(at, &[render(&entry.key, &value)]),
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_kinds_from_text() {
        assert_eq!(infer("true"), Node::Boolean { value: true });
        assert_eq!(infer("True"), Node::string("True"));
        assert_eq!(
            infer("-12"),
            Node::Integer {
                value: "-12".into()
            }
        );
        assert_eq!(infer("012"), Node::string("012"));
        assert_eq!(
            infer("0.75"),
            Node::Float {
                value: "0.75".into()
            }
        );
        assert_eq!(infer("1."), Node::string("1."));
        assert_eq!(infer("1e3"), Node::string("1e3"));
        assert_eq!(infer(""), Node::string(""));
    }
}
