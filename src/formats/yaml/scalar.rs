//! YAML 1.2 core schema resolution of scalars, and writing scalars back.
use crate::formats::Node;
use saphyr_parser::ScalarStyle;

/// The value of an untagged plain scalar.
pub fn resolve(text: &str) -> Node {
    match text {
        "" | "~" | "null" | "Null" | "NULL" => Node::Null,
        "true" | "True" | "TRUE" => Node::Boolean { value: true },
        "false" | "False" | "FALSE" => Node::Boolean { value: false },
        _ => match integer(text) {
            Some(value) => Node::Integer { value },
            None if is_float(text) => Node::Float {
                value: text.to_string(),
            },
            None => Node::string(text),
        },
    }
}

/// The value of a scalar with a core `!!type` tag (not `str`).
pub fn resolve_tagged(text: &str, tag: &str) -> Option<Node> {
    let resolved = resolve(text);
    match (tag, resolved) {
        ("null", node @ Node::Null)
        | ("bool", node @ Node::Boolean { .. })
        | ("int", node @ Node::Integer { .. })
        | ("float", node @ Node::Float { .. }) => Some(node),
        ("float", Node::Integer { .. }) => Some(Node::Float {
            value: text.to_string(),
        }),
        _ => None,
    }
}

/// Decimal text of a core schema integer (`[-+]?[0-9]+`, `0o[0-7]+`, `0x[0-9a-fA-F]+`).
fn integer(text: &str) -> Option<String> {
    if let Some(octal) = text.strip_prefix("0o") {
        return radix(octal, 8);
    }
    if let Some(hex) = text.strip_prefix("0x") {
        return radix(hex, 16);
    }
    let (negative, digits) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let digits = digits.trim_start_matches('0');
    Some(match (negative, digits.is_empty()) {
        (_, true) => "0".to_string(),
        (true, false) => format!("-{digits}"),
        (false, false) => digits.to_string(),
    })
}

fn radix(digits: &str, radix: u32) -> Option<String> {
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u128::from_str_radix(digits, radix)
        .ok()
        .map(|value| value.to_string())
}

/// Core schema floats that are not integers, including `.inf` / `.nan` spellings.
fn is_float(text: &str) -> bool {
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
        return true;
    }
    if matches!(text, ".nan" | ".NaN" | ".NAN") {
        return true;
    }

    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => (&unsigned[..index], Some(&unsigned[index + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let mantissa_ok = match fraction {
        Some(fraction) => digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty()),
        None => !whole.is_empty() && digits(whole),
    };
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    mantissa_ok && exponent_ok && (fraction.is_some() || exponent.is_some())
}

/// The float literal stored for a user-entered float.
pub fn float_text(value: &str) -> Option<String> {
    if is_float(value) {
        return Some(value.to_string());
    }
    let float: f64 = value.parse().ok()?;
    Some(if float.is_nan() {
        ".nan".to_string()
    } else if float.is_infinite() {
        if float < 0.0 { "-.inf" } else { ".inf" }.to_string()
    } else {
        format!("{float:?}")
    })
}

/// Where a scalar is written.
#[derive(Clone, Copy)]
pub enum Context {
    /// Block context; multi-line strings become literal blocks with content at `indent`.
    Block { indent: usize },
    Flow,
    Key,
}

/// The source text of a scalar (never a collection), preferring the original `style`.
pub fn render(node: &Node, style: Option<ScalarStyle>, context: Context) -> String {
    match node {
        Node::String { value } => string(value, style, context),
        Node::Integer { value } | Node::Float { value } | Node::Datetime { value } => {
            value.clone()
        }
        Node::Boolean { value } => value.to_string(),
        Node::Null => "null".to_string(),
        Node::Array { .. } | Node::Object { .. } => unreachable!("collections are not scalars"),
    }
}

fn string(value: &str, style: Option<ScalarStyle>, context: Context) -> String {
    if value.contains('\n') {
        if let Context::Block { indent } = context
            && let Some(block) = literal(value, indent)
        {
            return block;
        }
        return double_quoted(value);
    }
    let flow = !matches!(context, Context::Block { .. });
    match style {
        Some(ScalarStyle::DoubleQuoted) => double_quoted(value),
        Some(ScalarStyle::SingleQuoted) if single_ok(value) => single_quoted(value),
        Some(ScalarStyle::SingleQuoted) => double_quoted(value),
        _ if plain_ok(value, flow) => value.to_string(),
        _ if single_ok(value) => single_quoted(value),
        _ => double_quoted(value),
    }
}

/// A mapping key.
pub fn key(value: &str) -> String {
    string(value, None, Context::Key)
}

/// Whether `value` can be written unquoted and still reads back as the same string, also
/// for YAML 1.1 readers (SnakeYAML in Bukkit/Paper reads `yes`, `on`, `1:30` as non-text).
fn plain_ok(value: &str, flow: bool) -> bool {
    let Some(first) = value.chars().next() else {
        return false;
    };
    let second = value.chars().nth(1);
    let indicator_ok = match first {
        '-' | '?' | ':' => second.is_some_and(|c| !c.is_whitespace() && !(flow && is_flow(c))),
        ',' | '[' | ']' | '{' | '}' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%'
        | '@' | '`' => false,
        _ => true,
    };
    indicator_ok
        && value.trim() == value
        && !value.starts_with("---")
        && !value.starts_with("...")
        && !value.chars().any(|c| c.is_control() || c == '\u{feff}')
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && !(flow && value.chars().any(is_flow))
        && matches!(resolve(value), Node::String { .. })
        && !yaml11_special(value)
}

fn is_flow(c: char) -> bool {
    matches!(c, ',' | '[' | ']' | '{' | '}')
}

/// Text YAML 1.1 readers resolve to booleans, null or numbers.
fn yaml11_special(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if matches!(lower.as_str(), "y" | "n" | "yes" | "no" | "on" | "off" | "~" | "null") {
        return true;
    }
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let numeric = unsigned.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && unsigned
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '_' | ':' | '.' | 'e' | 'E' | '+' | '-'));
    numeric || matches!(lower.as_str(), ".inf" | "-.inf" | "+.inf" | ".nan")
}

fn single_ok(value: &str) -> bool {
    !value
        .chars()
        .any(|c| (c.is_control() && c != '\t') || c == '\u{feff}')
}

fn single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if c.is_control() || matches!(c, '\u{feff}' | '\u{fffe}' | '\u{ffff}') => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A `|` block for a multi-line string, content lines at `indent`; `None` when the text
/// cannot be written as one (leading spaces, control characters).
fn literal(value: &str, indent: usize) -> Option<String> {
    let body = value.trim_end_matches('\n');
    let trailing = value.len() - body.len();
    if body.is_empty()
        || body.starts_with([' ', '\t'])
        || value
            .chars()
            .any(|c| (c.is_control() && !matches!(c, '\n' | '\t')) || c == '\u{feff}')
    {
        return None;
    }

    let mut out = String::from(match trailing {
        0 => "|-",
        1 => "|",
        _ => "|+",
    });
    let padding = " ".repeat(indent);
    for line in body.split('\n') {
        out.push('\n');
        if !line.is_empty() {
            out.push_str(&padding);
            out.push_str(line);
        }
    }
    for _ in 1..trailing {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_core_schema() {
        assert_eq!(resolve("~"), Node::Null);
        assert_eq!(resolve("True"), Node::Boolean { value: true });
        assert_eq!(resolve("yes"), Node::string("yes"));
        assert_eq!(resolve("+0042"), Node::Integer { value: "42".into() });
        assert_eq!(resolve("-0"), Node::Integer { value: "0".into() });
        assert_eq!(resolve("0x1F"), Node::Integer { value: "31".into() });
        assert_eq!(resolve("0o17"), Node::Integer { value: "15".into() });
        assert_eq!(resolve("1.50"), Node::Float { value: "1.50".into() });
        assert_eq!(resolve("1e3"), Node::Float { value: "1e3".into() });
        assert_eq!(resolve("-.inf"), Node::Float { value: "-.inf".into() });
        assert_eq!(resolve("1.2.3"), Node::string("1.2.3"));
        assert_eq!(resolve("."), Node::string("."));
        assert_eq!(resolve("0x"), Node::string("0x"));
    }

    #[test]
    fn user_floats_become_core_floats() {
        assert_eq!(float_text("0.5").as_deref(), Some("0.5"));
        assert_eq!(float_text("5").as_deref(), Some("5.0"));
        assert_eq!(float_text("inf").as_deref(), Some(".inf"));
        assert_eq!(float_text("-Infinity").as_deref(), Some("-.inf"));
        assert_eq!(float_text("NaN").as_deref(), Some(".nan"));
        assert_eq!(float_text("abc"), None);
        for text in ["5", "1e400", "0.1", "-3"] {
            let stored = float_text(text).unwrap();
            assert!(matches!(resolve(&stored), Node::Float { .. }), "{text} -> {stored}");
        }
    }

    #[test]
    fn strings_are_quoted_only_when_needed() {
        let block = Context::Block { indent: 2 };
        let cases = [
            ("hello world", "hello world"),
            ("true", "'true'"),
            ("yes", "'yes'"),
            ("12:30", "'12:30'"),
            ("1.2.3", "'1.2.3'"),
            ("", "''"),
            ("a: b", "'a: b'"),
            ("#tag", "'#tag'"),
            ("-flag", "-flag"),
            ("- item", "'- item'"),
            (" padded", "' padded'"),
            ("it's", "it's"),
            ("&§ Färbung", "'&§ Färbung'"),
            ("tab\there", "'tab\there'"),
            ("bell\u{7}", "\"bell\\u0007\""),
        ];
        for (value, expected) in cases {
            assert_eq!(render(&Node::string(value), None, block), expected, "{value:?}");
        }
        assert_eq!(render(&Node::string("a,b"), None, Context::Flow), "'a,b'");
        assert_eq!(
            render(&Node::string("x"), Some(ScalarStyle::DoubleQuoted), block),
            "\"x\""
        );
        assert_eq!(
            render(&Node::string("it's"), Some(ScalarStyle::SingleQuoted), block),
            "'it''s'"
        );
    }

    #[test]
    fn multi_line_strings_become_literal_blocks() {
        let block = Context::Block { indent: 4 };
        assert_eq!(render(&Node::string("a\n\nb"), None, block), "|-\n    a\n\n    b");
        assert_eq!(render(&Node::string("a\n"), None, block), "|\n    a");
        assert_eq!(render(&Node::string("a\n\n"), None, block), "|+\n    a\n");
        assert_eq!(render(&Node::string(" a\nb"), None, block), "\" a\\nb\"");
        assert_eq!(render(&Node::string("a\nb"), None, Context::Flow), "\"a\\nb\"");
    }
}
