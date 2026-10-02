//! YAML (1.2 core schema) on top of saphyr-parser events. Anchors, aliases, custom tags and
//! multiple documents are rejected so every value has exactly one place in the file.
use super::{Node, ParseError, text};

mod patch;
mod scalar;
mod tree;
mod write;

pub use scalar::float_text;

pub fn parse(source: &str) -> Result<Node, ParseError> {
    Ok(tree::build(source)?.map_or(Node::Object { entries: Vec::new() }, |(_, node)| node))
}

pub fn patch(source: &str, current: &Node, document: &Node) -> String {
    let root = match tree::build(source) {
        Ok(Some((root, _))) => root,
        // a file with only comments keeps them above the new document
        Ok(None) => {
            let mut out = source.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(text::eol(source));
            }
            out.push_str(&emit(source, document));
            return out;
        }
        Err(_) => return emit(source, document),
    };

    let mut patcher = patch::Patcher::new(source, write::Style::detect(source, &root));
    if !patcher.update(&root, current, document, patch::Slot::Root) {
        return emit(source, document);
    }
    patcher.finish().unwrap_or_else(|| emit(source, document))
}

pub fn emit(source: &str, document: &Node) -> String {
    let style = match tree::build(source) {
        Ok(Some((root, _))) => write::Style::detect(source, &root),
        _ => write::Style::default(),
    };
    text::with_eol(&format!("{}\n", write::root(document, 0, style)), text::eol(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{Entry, Format};

    fn apply(source: &str, document: &Node) -> String {
        let written = Format::Yaml.apply(source, document).unwrap();
        assert_eq!(parse(&written).unwrap(), *document, "round trip of:\n{written}");
        written
    }

    /// Replaces the value at `path` (keys, or indexes for arrays).
    fn set(node: &mut Node, path: &[&str], value: Node) {
        *get(node, path) = value;
    }

    fn get<'a>(node: &'a mut Node, path: &[&str]) -> &'a mut Node {
        path.iter().fold(node, |node, part| match node {
            Node::Object { entries } => {
                &mut entries.iter_mut().find(|entry| entry.key == *part).unwrap().value
            }
            Node::Array { items } => &mut items[part.parse::<usize>().unwrap()],
            _ => panic!("not a collection"),
        })
    }

    fn entries<'a>(node: &'a mut Node, path: &[&str]) -> &'a mut Vec<Entry> {
        match get(node, path) {
            Node::Object { entries } => entries,
            _ => panic!("not an object"),
        }
    }

    fn items<'a>(node: &'a mut Node, path: &[&str]) -> &'a mut Vec<Node> {
        match get(node, path) {
            Node::Array { items } => items,
            _ => panic!("not an array"),
        }
    }

    fn int(value: &str) -> Node {
        Node::Integer {
            value: value.into(),
        }
    }

    const PAPER: &str = "\
# This is the main configuration file for Bukkit.
# As you can see, there's actually not that much to configure without any plugins.

settings:
  # Whether the end is enabled
  allow-end: true
  warn-on-overload: true
  permissions-file: permissions.yml
  update-folder: update
  connection-throttle: 4000
  query-plugins: true
  shutdown-message: 'Server closed'
  minimum-api: none
spawn-limits:
  monsters: 70
  animals: 10
chunk-gc:
  period-in-ticks: 600
ticks-per:
  animal-spawns: 400
  autosave: 6000
aliases: now-in-commands.yml
worlds:
- world
- world_nether   # the nether
- world_the_end
motd: |
  Welcome!
  Have fun
";

    #[test]
    fn reads_values_and_comments() {
        let document = parse(PAPER).unwrap();
        let Node::Object { entries } = &document else {
            panic!("root is not an object");
        };
        assert_eq!(entries[0].key, "settings");
        assert_eq!(entries[0].comment, None);
        let Node::Object { entries: settings } = &entries[0].value else {
            panic!("settings is not an object");
        };
        assert_eq!(settings[0].comment.as_deref(), Some("Whether the end is enabled"));
        assert_eq!(settings[4].value, int("4000"));
        assert_eq!(settings[6].value, Node::string("Server closed"));
        assert_eq!(entries[5].value, Node::Array {
            items: vec![
                Node::string("world"),
                Node::string("world_nether"),
                Node::string("world_the_end"),
            ],
        });
        assert_eq!(entries[6].value, Node::string("Welcome!\nHave fun\n"));
    }

    #[test]
    fn unchanged_document_is_byte_identical() {
        let document = parse(PAPER).unwrap();
        assert_eq!(apply(PAPER, &document), PAPER);
    }

    #[test]
    fn scalar_edits_keep_style_and_comments() {
        let mut document = parse(PAPER).unwrap();
        set(&mut document, &["settings", "allow-end"], Node::Boolean { value: false });
        set(&mut document, &["settings", "shutdown-message"], Node::string("Bye, it's late"));
        set(&mut document, &["worlds", "1"], Node::string("nether: hot"));
        set(&mut document, &["spawn-limits", "monsters"], Node::string("yes"));
        let expected = PAPER
            .replace("allow-end: true", "allow-end: false")
            .replace("'Server closed'", "'Bye, it''s late'")
            .replace("- world_nether   #", "- 'nether: hot'   #")
            .replace("monsters: 70", "monsters: 'yes'");
        assert_eq!(apply(PAPER, &document), expected);
    }

    #[test]
    fn multi_line_strings_use_literal_blocks() {
        let mut document = parse(PAPER).unwrap();
        set(&mut document, &["motd"], Node::string("Hello\nWorld"));
        set(&mut document, &["aliases"], Node::string("a\nb\n"));
        let expected = PAPER
            .replace("aliases: now-in-commands.yml", "aliases: |\n  a\n  b")
            .replace("motd: |\n  Welcome!\n  Have fun", "motd: |-\n  Hello\n  World");
        assert_eq!(apply(PAPER, &document), expected);
    }

    #[test]
    fn adds_and_removes_entries_in_nested_containers() {
        let mut document = parse(PAPER).unwrap();
        entries(&mut document, &["settings"]).retain(|entry| entry.key != "update-folder");
        entries(&mut document, &["spawn-limits"]).push(Entry::new("water-animals", int("5"), None));
        items(&mut document, &["worlds"]).remove(0);
        items(&mut document, &["worlds"]).push(Node::string("creative"));
        entries(&mut document, &["chunk-gc"]).push(Entry::new(
            "limits",
            Node::Object {
                entries: vec![Entry::new("max", int("8"), None)],
            },
            None,
        ));
        entries(&mut document, &[]).push(Entry::new(
            "groups",
            Node::Array {
                items: vec![Node::string("admin"), Node::string("true")],
            },
            None,
        ));
        let expected = PAPER
            .replace("  update-folder: update\n", "")
            .replace("  animals: 10\n", "  animals: 10\n  water-animals: 5\n")
            .replace("- world\n- world_nether", "- world_nether")
            .replace("- world_the_end\n", "- world_the_end\n- creative\n")
            .replace(
                "  period-in-ticks: 600\n",
                "  period-in-ticks: 600\n  limits:\n    max: 8\n",
            )
            .replace("  Have fun\n", "  Have fun\ngroups:\n- admin\n- 'true'\n");
        assert_eq!(apply(PAPER, &document), expected);
    }

    #[test]
    fn removing_a_key_takes_its_own_comment() {
        let mut document = parse(PAPER).unwrap();
        entries(&mut document, &["settings"]).remove(0);
        let expected = PAPER.replace("  # Whether the end is enabled\n  allow-end: true\n", "");
        assert_eq!(apply(PAPER, &document), expected);

        // the file header is separated by a blank line, so it is not the key's comment
        let mut document = parse(PAPER).unwrap();
        entries(&mut document, &[]).remove(0);
        let start = PAPER.find("settings:").unwrap();
        let end = PAPER.find("spawn-limits:").unwrap();
        assert_eq!(apply(PAPER, &document), format!("{}{}", &PAPER[..start], &PAPER[end..]));
    }

    #[test]
    fn indented_sequences_and_flow_collections() {
        let source = "\
plugins:
    - name: Essentials   # core
      enabled: true
    - name: WorldEdit
      enabled: false
ports: [25565, 25566]
tags: {a: 1, b: two}
";
        let mut document = parse(source).unwrap();
        set(&mut document, &["plugins", "1", "enabled"], Node::Boolean { value: true });
        items(&mut document, &["plugins"]).push(Node::Object {
            entries: vec![
                Entry::new("name", Node::string("LuckPerms"), None),
                Entry::new("enabled", Node::Boolean { value: true }, None),
            ],
        });
        set(&mut document, &["ports", "1"], int("25570"));
        entries(&mut document, &["tags"]).push(Entry::new("c", Node::Null, None));
        let expected = "\
plugins:
    - name: Essentials   # core
      enabled: true
    - name: WorldEdit
      enabled: true
    - name: LuckPerms
      enabled: true
ports: [25565, 25570]
tags: {a: 1, b: two, c: null}
";
        assert_eq!(apply(source, &document), expected);
    }

    #[test]
    fn replaces_values_of_another_kind() {
        let source = "a: 1  # one\nb:\n  c: 2\nd:\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &["a"], Node::Array {
            items: vec![int("1"), int("2")],
        });
        set(&mut document, &["b"], Node::string("flat"));
        set(&mut document, &["d"], Node::Object {
            entries: vec![Entry::new("e", Node::Array { items: vec![] }, None)],
        });
        assert_eq!(
            apply(source, &document),
            "a:\n  - 1\n  - 2  # one\nb: flat\nd:\n  e: []\n"
        );
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let source = "# server\r\nname: Lobby\r\nlimits:\r\n  players: 20\r\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &["limits", "players"], int("50"));
        entries(&mut document, &["limits"]).push(Entry::new("view", int("10"), None));
        assert_eq!(
            apply(source, &document),
            "# server\r\nname: Lobby\r\nlimits:\r\n  players: 50\r\n  view: 10\r\n"
        );
    }

    #[test]
    fn non_ascii_text_before_an_edit_keeps_offsets() {
        let source = "motd: \"§aWillkommen — 欢迎\" # grüß\nmax: 10\nnote: ok\n";
        let mut document = parse(source).unwrap();
        set(&mut document, &["max"], int("20"));
        set(&mut document, &["motd"], Node::string("§bHallo"));
        assert_eq!(
            apply(source, &document),
            "motd: \"§bHallo\" # grüß\nmax: 20\nnote: ok\n"
        );
    }

    #[test]
    fn empty_and_comment_only_files_get_a_document() {
        assert_eq!(parse("").unwrap(), Node::Object { entries: vec![] });
        let document = Node::Object {
            entries: vec![Entry::new("enabled", Node::Boolean { value: true }, None)],
        };
        assert_eq!(apply("", &document), "enabled: true\n");
        assert_eq!(apply("# notes", &document), "# notes\nenabled: true\n");
    }

    #[test]
    fn unsupported_yaml_is_a_parse_error_with_a_line() {
        let cases = [
            ("a: 1\na: 2\n", 2),
            ("base: &b 1\nother: *b\n", 1),
            ("a: 1\n---\nb: 2\n", 2),
            ("x: !custom 1\n", 1),
            ("? [a, b]\n: 1\n", 1),
        ];
        for (source, line) in cases {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, Some(line), "{source:?}: {}", error.message);
        }
    }

    #[test]
    fn core_tags_resolve_and_are_dropped_on_change() {
        let source = "port: !!str 25565\nid: !!int 7\n";
        let mut document = parse(source).unwrap();
        assert_eq!(document, Node::Object {
            entries: vec![
                Entry::new("port", Node::string("25565"), None),
                Entry::new("id", int("7"), None),
            ],
        });
        set(&mut document, &["port"], Node::string("25566"));
        assert_eq!(apply(source, &document), "port: '25566'\nid: !!int 7\n");
    }

    #[test]
    fn keep_chomping_blocks_own_their_empty_lines() {
        let source = "keep: |+\n  kept\n\nafter: 1\n";
        let mut document = parse(source).unwrap();
        entries(&mut document, &[]).insert(1, Entry::new("new", int("1"), None));
        assert_eq!(apply(source, &document), "keep: |+\n  kept\n\nnew: 1\nafter: 1\n");

        let mut document = parse(source).unwrap();
        entries(&mut document, &[]).remove(0);
        assert_eq!(apply(source, &document), "after: 1\n");
    }

    #[test]
    fn reordered_keys_reemit_the_mapping() {
        let source = "top: 1\nsection:\n  # first\n  a: 1\n  b: 2\n";
        let mut document = parse(source).unwrap();
        entries(&mut document, &["section"]).reverse();
        assert_eq!(apply(source, &document), "top: 1\nsection:\n  b: 2\n  a: 1\n");
    }
}
