//! Validates a submitted document against the format's rules and normalizes its scalars
//! to what the format will store, so `parse(apply(file, doc)) == check(doc)`.
use super::{Entry, Format, Node, flat};

/// The validated document, or one message per problem (prefixed with the entry path).
pub fn check(format: Format, document: &Node) -> Result<Node, Vec<String>> {
    let mut checker = Checker {
        format,
        errors: Vec::new(),
    };
    let root_ok = match (format, document) {
        (Format::Yaml, _) => true,
        (Format::Json, node) => matches!(node, Node::Object { .. } | Node::Array { .. }),
        (_, node) => matches!(node, Node::Object { .. }),
    };
    if !root_ok {
        return Err(vec![match format {
            Format::Json => "the document must be an object or an array".to_string(),
            _ => "the document must be an object".to_string(),
        }]);
    }

    let checked = checker.node(document, &mut String::new(), 0);
    if checker.errors.is_empty() {
        Ok(checked)
    } else {
        Err(checker.errors)
    }
}

pub fn is_integer(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    match digits.as_bytes() {
        [b'0'] => true,
        [first, rest @ ..] => {
            (b'1'..=b'9').contains(first) && rest.iter().all(u8::is_ascii_digit)
        }
        [] => false,
    }
}

struct Checker {
    format: Format,
    errors: Vec<String>,
}

impl Checker {
    fn error(&mut self, path: &str, message: impl std::fmt::Display) {
        if path.is_empty() {
            self.errors.push(message.to_string());
        } else {
            self.errors.push(format!("{path}: {message}"));
        }
    }

    /// `depth` counts the objects above the node (0 for the root).
    fn node(&mut self, node: &Node, path: &mut String, depth: usize) -> Node {
        match node {
            Node::Object { entries } => self.object(entries, path, depth),
            Node::Array { items } => {
                if matches!(self.format, Format::Properties | Format::Env | Format::Ini) {
                    self.error(path, "lists are not supported in this format");
                    return node.clone();
                }
                let items = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let len = path.len();
                        path.push_str(&format!("[{index}]"));
                        let item = self.node(item, path, depth);
                        path.truncate(len);
                        item
                    })
                    .collect();
                Node::Array { items }
            }
            scalar => self.scalar(scalar, path),
        }
    }

    fn object(&mut self, entries: &[Entry], path: &mut String, depth: usize) -> Node {
        let nested_allowed = match self.format {
            Format::Properties | Format::Env => depth == 0,
            Format::Ini => depth <= 1,
            _ => true,
        };
        if !nested_allowed {
            self.error(
                path,
                match self.format {
                    Format::Ini => "sections cannot contain other sections",
                    _ => "nested objects are not supported in this format",
                },
            );
            return Node::Object {
                entries: entries.to_vec(),
            };
        }

        let mut seen = std::collections::HashSet::with_capacity(entries.len());
        let mut checked = Vec::with_capacity(entries.len());
        for entry in entries {
            let len = path.len();
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(&entry.key);
            if !seen.insert(entry.key.as_str()) {
                self.error(path, "duplicate key");
            }
            if !self.key_ok(&entry.key, depth) {
                self.error(path, "this key cannot be written in this format");
            }
            let value = self.node(&entry.value, path, depth + 1);
            path.truncate(len);
            checked.push(Entry::new(entry.key.clone(), value, None));
        }
        Node::Object { entries: checked }
    }

    fn key_ok(&self, key: &str, depth: usize) -> bool {
        match self.format {
            Format::Env => {
                let mut chars = key.chars();
                chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
            }
            Format::Ini => {
                !key.is_empty()
                    && key.trim() == key
                    && !key.contains(['\n', '\r', '=', ':'])
                    && !key.starts_with(['[', ';', '#'])
                    && (depth > 0 || !key.contains(']'))
            }
            Format::Properties => !key.is_empty(),
            _ => true,
        }
    }

    fn scalar(&mut self, node: &Node, path: &str) -> Node {
        let flat = matches!(self.format, Format::Properties | Format::Env | Format::Ini);
        match node {
            Node::Integer { value } if !is_integer(value) => {
                self.error(path, format!("`{value}` is not a whole number"));
                node.clone()
            }
            Node::Float { value } => match self.format.float_text(value) {
                Some(text) if flat => flat::infer(&text),
                Some(text) => Node::Float { value: text },
                None => {
                    self.error(path, format!("`{value}` is not a number this format can store"));
                    node.clone()
                }
            },
            Node::Null if matches!(self.format, Format::Toml) || flat => {
                self.error(path, "empty (null) values are not supported in this format");
                node.clone()
            }
            Node::Datetime { value } if !matches!(self.format, Format::Toml) => {
                self.error(path, format!("`{value}`: dates are only supported in TOML"));
                node.clone()
            }
            Node::Datetime { value } if !super::toml::datetime_ok(value) => {
                self.error(path, format!("`{value}` is not a TOML date or time"));
                node.clone()
            }
            node if flat => flat::scalar_text(node).map_or_else(|| node.clone(), |text| flat::infer(&text)),
            node => node.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn integers_must_be_canonical_decimal() {
        for valid in ["0", "-0", "7", "-25565", "123456789012345678901234567890"] {
            assert!(is_integer(valid), "{valid}");
        }
        for invalid in ["", "-", "01", "+1", "1.0", "0x1F", " 1", "1e3"] {
            assert!(!is_integer(invalid), "{invalid}");
        }
    }

    #[test]
    fn reports_every_problem_with_its_path() {
        let document = object(vec![
            ("server", object(vec![("port", int("08")), ("port", int("1"))])),
            ("list", Node::Array { items: vec![int("x")] }),
        ]);
        let errors = check(Format::Yaml, &document).unwrap_err();
        assert_eq!(errors, vec![
            "server.port: `08` is not a whole number",
            "server.port: duplicate key",
            "list[0]: `x` is not a whole number",
        ]);
    }

    #[test]
    fn flat_formats_store_text_and_reinfer_kinds() {
        let document = object(vec![
            ("ratio", Node::Float { value: "1e5".into() }),
            ("flag", Node::string("true")),
            ("port", Node::string("25565")),
        ]);
        let checked = check(Format::Properties, &document).unwrap();
        assert_eq!(checked, object(vec![
            ("ratio", Node::string("1e5")),
            ("flag", Node::Boolean { value: true }),
            ("port", int("25565")),
        ]));
    }

    #[test]
    fn format_shapes_are_enforced() {
        let nested = object(vec![("a", object(vec![("b", object(vec![]))]))]);
        assert!(check(Format::Ini, &nested).is_err());
        assert!(check(Format::Properties, &object(vec![("a", object(vec![]))])).is_err());
        assert!(check(Format::Toml, &object(vec![("a", Node::Null)])).is_err());
        assert!(check(Format::Json, &Node::string("x")).is_err());
        assert!(check(Format::Yaml, &Node::string("x")).is_ok());
        assert!(check(Format::Env, &object(vec![("1BAD", Node::string("x"))])).is_err());
        assert!(check(Format::Yaml, &object(vec![("when", Node::Datetime { value: "1979-05-27".into() })])).is_err());
        assert!(check(Format::Toml, &object(vec![("when", Node::Datetime { value: "1979-05-27".into() })])).is_ok());
    }
}
