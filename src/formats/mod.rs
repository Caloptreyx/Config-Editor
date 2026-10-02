//! Config file formats: detection, the shared document model, and the parse/apply
//! entry points that dispatch to the per-format modules.
//!
//! Every format module provides:
//! - `parse(source) -> Result<Node, ParseError>`
//! - `patch(source, current, document) -> String`: applies `document` to the file as minimal
//!   text edits (`current` is the parsed file and differs from `document`)
//! - `emit(source, document) -> String`: writes the whole document from scratch, keeping only
//!   the file's line endings and indentation style
//! - `float_text(value) -> Option<String>`: the float literal for this format, `None` when the
//!   value cannot be written as a float
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod env;
mod flat;
mod ini;
mod json;
mod properties;
mod rules;
mod text;
mod toml;
mod yaml;

#[derive(ToSchema, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Yaml,
    Json,
    Toml,
    Properties,
    Env,
    Ini,
}

/// A parsed config document.
#[derive(ToSchema, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schema(no_recursion)]
pub enum Node {
    String { value: String },
    /// Decimal text, optional leading `-`.
    Integer { value: String },
    /// The literal as written in the file or entered by the user.
    Float { value: String },
    Boolean { value: bool },
    Null,
    /// TOML date/time literal text.
    Datetime { value: String },
    Array { items: Vec<Node> },
    Object { entries: Vec<Entry> },
}

#[derive(ToSchema, Serialize, Deserialize, Clone, Debug)]
#[schema(no_recursion)]
pub struct Entry {
    pub key: String,
    pub value: Node,
    /// The comment block written directly above the key; ignored on requests.
    #[serde(default)]
    pub comment: Option<String>,
}

/// Entries compare by key and value; comments are presentation only.
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}

impl Entry {
    pub fn new(key: impl Into<String>, value: Node, comment: Option<String>) -> Self {
        Self {
            key: key.into(),
            value,
            comment,
        }
    }
}

impl Node {
    pub fn string(value: impl Into<String>) -> Self {
        Self::String {
            value: value.into(),
        }
    }

    pub fn is_scalar(&self) -> bool {
        !matches!(self, Self::Array { .. } | Self::Object { .. })
    }
}

#[derive(ToSchema, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// 1-based.
    pub line: Option<u32>,
    /// 1-based, in characters.
    pub column: Option<u32>,
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
        }
    }

    /// An error at the byte offset `at` of `source`.
    pub fn at(source: &str, at: usize, message: impl Into<String>) -> Self {
        let (line, column) = text::line_col(source, at);
        Self {
            message: message.into(),
            line: Some(line),
            column: Some(column),
        }
    }

    /// An error on the 1-based `line`.
    pub fn on_line(line: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: Some(line as u32),
            column: None,
        }
    }
}

impl Format {
    /// The format of a file, by its name.
    pub fn detect(path: &str) -> Option<Self> {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        if name == "eula.txt" {
            return Some(Self::Properties);
        }
        if name == ".env" || name.starts_with(".env.") || name.ends_with(".env") {
            return Some(Self::Env);
        }
        let (stem, extension) = name.rsplit_once('.')?;
        if stem.is_empty() {
            return None;
        }
        match extension {
            "yml" | "yaml" => Some(Self::Yaml),
            "json" | "jsonc" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            "properties" => Some(Self::Properties),
            "ini" | "cfg" => Some(Self::Ini),
            _ => None,
        }
    }

    pub fn parse(self, source: &str) -> Result<Node, ParseError> {
        match self {
            Self::Yaml => yaml::parse(source),
            Self::Json => json::parse(source),
            Self::Toml => toml::parse(source),
            Self::Properties => properties::parse(source),
            Self::Env => env::parse(source),
            Self::Ini => ini::parse(source),
        }
    }

    /// Writes `document` into `original`, keeping everything that did not change.
    /// Errors are user-facing messages (invalid document, or a file that does not parse).
    pub fn apply(self, original: &str, document: &Node) -> Result<String, Vec<String>> {
        let checked = rules::check(self, document)?;
        let document = match self {
            // TOML lists a table's plain keys before its sub-tables, INI its global keys
            // before the sections, whatever order they are sent in
            Self::Toml => toml::arrange(original, &checked),
            Self::Ini => globals_first(checked),
            _ => checked,
        };
        let current = self.parse(original).map_err(|err| {
            vec![format!(
                "the file does not parse, use the raw editor: {}",
                err.message
            )]
        })?;
        if current == document {
            return Ok(original.to_string());
        }

        let patched = self.patch(original, &current, &document);
        if self.parse(&patched).as_ref() == Ok(&document) {
            return Ok(patched);
        }
        tracing::debug!(format = ?self, "patched document did not round-trip, re-emitting it");

        let emitted = self.emit(original, &document);
        if self.parse(&emitted).as_ref() == Ok(&document) {
            return Ok(emitted);
        }
        Err(vec![
            "the document cannot be written in this format without changing it".to_string(),
        ])
    }

    fn patch(self, source: &str, current: &Node, document: &Node) -> String {
        match self {
            Self::Yaml => yaml::patch(source, current, document),
            Self::Json => json::patch(source, current, document),
            Self::Toml => toml::patch(source, current, document),
            Self::Properties => properties::patch(source, current, document),
            Self::Env => env::patch(source, current, document),
            Self::Ini => ini::patch(source, current, document),
        }
    }

    fn emit(self, source: &str, document: &Node) -> String {
        match self {
            Self::Yaml => yaml::emit(source, document),
            Self::Json => json::emit(source, document),
            Self::Toml => toml::emit(source, document),
            Self::Properties => properties::emit(source, document),
            Self::Env => env::emit(source, document),
            Self::Ini => ini::emit(source, document),
        }
    }

    fn float_text(self, value: &str) -> Option<String> {
        match self {
            Self::Yaml => yaml::float_text(value),
            Self::Json => json::float_text(value),
            Self::Toml => toml::float_text(value),
            Self::Properties | Self::Env | Self::Ini => flat::float_text(value),
        }
    }
}

/// The root entries of an INI document with scalars (global keys) before sections.
fn globals_first(document: Node) -> Node {
    match document {
        Node::Object { entries } => {
            let (mut globals, sections): (Vec<Entry>, Vec<Entry>) =
                entries.into_iter().partition(|entry| entry.value.is_scalar());
            globals.extend(sections);
            Node::Object { entries: globals }
        }
        node => node,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats_by_name() {
        let cases = [
            ("/plugins/Essentials/config.yml", Some(Format::Yaml)),
            ("/a/B.YAML", Some(Format::Yaml)),
            ("/tsconfig.jsonc", Some(Format::Json)),
            ("/config/forge-common.toml", Some(Format::Toml)),
            ("/server.properties", Some(Format::Properties)),
            ("/eula.txt", Some(Format::Properties)),
            ("/notes.txt", None),
            ("/.env", Some(Format::Env)),
            ("/.env.local", Some(Format::Env)),
            ("/app/prod.env", Some(Format::Env)),
            ("/php.ini", Some(Format::Ini)),
            ("/settings.CFG", Some(Format::Ini)),
            ("/.yml", None),
            ("/Makefile", None),
        ];
        for (path, format) in cases {
            assert_eq!(Format::detect(path), format, "{path}");
        }
    }

    #[test]
    fn node_wire_format_is_tagged_snake_case() {
        let node = Node::Object {
            entries: vec![
                Entry::new("port", Node::Integer { value: "25565".into() }, Some("c".into())),
                Entry::new("motd", Node::Null, None),
            ],
        };
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "object",
                "entries": [
                    { "key": "port", "value": { "kind": "integer", "value": "25565" }, "comment": "c" },
                    { "key": "motd", "value": { "kind": "null" }, "comment": null },
                ],
            })
        );
        let request: Node = serde_json::from_value(serde_json::json!({
            "kind": "object",
            "entries": [{ "key": "port", "value": { "kind": "integer", "value": "25565" } }],
        }))
        .unwrap();
        assert_eq!(request, Node::Object {
            entries: vec![Entry::new("port", Node::Integer { value: "25565".into() }, None)],
        });
    }
}
