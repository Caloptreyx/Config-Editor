//! Line and edit helpers shared by the format writers. Positions are byte offsets.

/// 1-based line and character column of the byte offset `at`.
pub fn line_col(source: &str, at: usize) -> (u32, u32) {
    let at = floor_char_boundary(source, at.min(source.len()));
    let before = &source[..at];
    let line = before.matches('\n').count() + 1;
    let column = before[line_start(source, at)..].chars().count() + 1;
    (line as u32, column as u32)
}

fn floor_char_boundary(source: &str, mut at: usize) -> usize {
    while !source.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Start of the line containing `at`.
pub fn line_start(source: &str, at: usize) -> usize {
    source[..at].rfind('\n').map_or(0, |index| index + 1)
}

/// End of the line containing `at`, before its `\n` or `\r\n`.
pub fn line_end(source: &str, at: usize) -> usize {
    match source[at..].find('\n') {
        Some(index) => {
            let end = at + index;
            if end > 0 && source.as_bytes()[end - 1] == b'\r' {
                end - 1
            } else {
                end
            }
        }
        None => source.len(),
    }
}

/// Start of the line after the one containing `at` (the end of the source on the last line).
pub fn next_line(source: &str, at: usize) -> usize {
    source[at..]
        .find('\n')
        .map_or(source.len(), |index| at + index + 1)
}

/// Whether only spaces and tabs precede `at` on its line.
pub fn starts_line(source: &str, at: usize) -> bool {
    source[line_start(source, at)..at]
        .bytes()
        .all(|byte| byte == b' ' || byte == b'\t')
}

/// The leading spaces and tabs of the line containing `at`.
pub fn indentation(source: &str, at: usize) -> &str {
    let start = line_start(source, at);
    let line = &source[start..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The line ending the file uses (`\r\n` when its first line break is one).
pub fn eol(source: &str) -> &'static str {
    match source.find('\n') {
        Some(index) if index > 0 && source.as_bytes()[index - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// `text` (written with `\n`) using the line ending `eol`.
pub fn with_eol(text: &str, eol: &str) -> String {
    if eol == "\n" {
        text.to_string()
    } else {
        text.replace('\n', eol)
    }
}

/// The comment block directly above the line starting at `line`: consecutive lines whose
/// first non-blank characters are one of `markers`, without the markers and one following
/// space, joined with `\n`. A blank line ends the block.
pub fn comment_above(source: &str, line: usize, markers: &[&str]) -> Option<String> {
    let mut lines = Vec::new();
    let mut end = line;
    while end > 0 {
        let start = line_start(source, end - 1);
        let text = source[start..end].trim_end_matches(['\n', '\r']).trim_start();
        let Some(comment) = markers.iter().find_map(|marker| text.strip_prefix(marker)) else {
            break;
        };
        lines.push(comment.strip_prefix(' ').unwrap_or(comment).trim_end());
        end = start;
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join("\n"))
}

/// Start of the comment block [`comment_above`] reads for the line starting at `line`
/// (`line` itself when there is none).
pub fn comment_start(source: &str, line: usize, markers: &[&str]) -> usize {
    let mut start = line;
    while start > 0 {
        let above = line_start(source, start - 1);
        let text = source[above..start].trim_start();
        if !markers.iter().any(|marker| text.starts_with(marker)) {
            break;
        }
        start = above;
    }
    start
}

/// A set of non-overlapping replacements applied in one pass.
#[derive(Default)]
pub struct Edits {
    edits: Vec<(usize, usize, String)>,
}

impl Edits {
    pub fn replace(&mut self, start: usize, end: usize, text: impl Into<String>) {
        self.edits.push((start, end, text.into()));
    }

    pub fn insert(&mut self, at: usize, text: impl Into<String>) {
        self.replace(at, at, text);
    }

    pub fn delete(&mut self, start: usize, end: usize) {
        self.replace(start, end, String::new());
    }

    /// The edited source; `None` when edits overlap. Insertions at the same position keep
    /// the order they were added in.
    pub fn apply(mut self, source: &str) -> Option<String> {
        self.edits.sort_by_key(|(start, end, _)| (*start, *end));
        let mut out = String::with_capacity(source.len());
        let mut at = 0;
        for (start, end, text) in &self.edits {
            if *start < at || *end > source.len() {
                return None;
            }
            out.push_str(&source[at..*start]);
            out.push_str(text);
            at = *end;
        }
        out.push_str(&source[at..]);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_handle_crlf() {
        let source = "a: 1\r\nbb: 2\r\nc";
        assert_eq!(line_end(source, 0), 4);
        assert_eq!(next_line(source, 0), 6);
        assert_eq!(line_start(source, 8), 6);
        assert_eq!(line_end(source, 13), 14);
        assert_eq!(eol(source), "\r\n");
        assert_eq!(eol("a\nb\r\n"), "\n");
    }

    #[test]
    fn columns_count_characters() {
        assert_eq!(line_col("ä: ö\nkey: x", 4), (1, 4));
        assert_eq!(line_col("ä: ö\nkey: x", 11), (2, 5));
    }

    #[test]
    fn comment_block_stops_at_blank_lines() {
        let source = "# header\n\n# first\n  #second line\nkey: 1\n";
        let key = source.find("key").unwrap();
        assert_eq!(
            comment_above(source, key, &["#"]).as_deref(),
            Some("first\nsecond line")
        );
        assert_eq!(comment_above(source, 0, &["#"]), None);
    }

    #[test]
    fn edits_apply_in_position_order_and_reject_overlaps() {
        let mut edits = Edits::default();
        edits.replace(3, 4, "9");
        edits.insert(0, "x");
        edits.insert(0, "y");
        assert_eq!(edits.apply("a: 1\n").as_deref(), Some("xya: 9\n"));

        let mut edits = Edits::default();
        edits.replace(0, 3, "");
        edits.replace(2, 4, "");
        assert_eq!(edits.apply("abcdef"), None);
    }
}
