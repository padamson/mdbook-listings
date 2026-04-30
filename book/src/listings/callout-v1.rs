//! Parses inline `CALLOUT:` markers out of a frozen listing's source.

/// Position is a 1-based line number so error diagnostics and the eventual
/// rendered badge anchor can both refer to it directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callout {
    pub line: usize,
    pub label: String,
    pub body: Option<String>,
}

/// Walks `content` line by line and returns every well-formed callout
/// marker. A marker is a line whose first non-whitespace content matches
/// `<comment_prefix> CALLOUT: <label>[ <body>]`. Malformed lines are
/// silently skipped — the splicer leaves them in the rendered listing
/// unchanged.
pub fn parse_callouts(content: &str, comment_prefix: &str) -> Vec<Callout> {
    let mut out = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        if let Some(callout) = parse_line(raw_line, comment_prefix, idx + 1) {
            out.push(callout);
        }
    }
    out
}

fn parse_line(raw_line: &str, comment_prefix: &str, line: usize) -> Option<Callout> {
    let after_prefix = raw_line.trim_start().strip_prefix(comment_prefix)?;
    let after_keyword = after_prefix.strip_prefix(' ')?.strip_prefix("CALLOUT:")?;
    let payload = after_keyword.strip_prefix(' ')?;
    let (label, rest) = match payload.split_once(char::is_whitespace) {
        Some((l, r)) => (l, Some(r)),
        None => (payload, None),
    };
    if label.is_empty() || !is_valid_label(label) {
        return None;
    }
    let body = rest.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Some(Callout {
        line,
        label: label.to_string(),
        body,
    })
}

fn is_valid_label(label: &str) -> bool {
    label
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Maps a listing's file extension to the language's single-line comment
/// prefix. Returns `None` for languages without a recognised inline-marker
/// syntax (block-comment-only languages take callouts via the sidecar form
/// instead).
pub fn comment_prefix_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "yaml" | "yml" | "toml" | "py" | "sh" | "bash" | "tf" | "hcl" => Some("#"),
        "rs" | "c" | "h" | "cpp" | "hpp" | "js" | "ts" | "jsx" | "tsx" => Some("//"),
        "sql" => Some("--"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_label_with_body_for_hash_prefix() {
        let s = "key: value\n# CALLOUT: greeting Says hello to the user.\nfoo: bar\n";
        let got = parse_callouts(s, "#");
        assert_eq!(
            got,
            vec![Callout {
                line: 2,
                label: "greeting".into(),
                body: Some("Says hello to the user.".into()),
            }]
        );
    }

    #[test]
    fn parses_label_only_form_for_hash_prefix() {
        let s = "# CALLOUT: anchor-only\n";
        let got = parse_callouts(s, "#");
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "anchor-only".into(),
                body: None,
            }]
        );
    }

    #[test]
    fn parses_double_slash_prefix() {
        let s = "fn main() {\n    // CALLOUT: entry The program starts here.\n}\n";
        let got = parse_callouts(s, "//");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].line, 2);
        assert_eq!(got[0].label, "entry");
        assert_eq!(got[0].body.as_deref(), Some("The program starts here."));
    }

    #[test]
    fn parses_double_dash_prefix_for_sql() {
        let s = "SELECT *\n-- CALLOUT: filter Limits to active rows.\nFROM users;\n";
        let got = parse_callouts(s, "--");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "filter");
    }

    #[test]
    fn skips_marker_with_wrong_prefix() {
        let s = "# CALLOUT: hash-marker\n";
        assert!(parse_callouts(s, "//").is_empty());
    }

    #[test]
    fn skips_missing_space_between_prefix_and_keyword() {
        let s = "#CALLOUT: nope\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn skips_missing_space_after_keyword() {
        let s = "# CALLOUT:nope\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn skips_empty_label() {
        let s = "# CALLOUT:  body-without-label\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn skips_label_with_invalid_characters() {
        let s = "# CALLOUT: bad/label has body\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn returns_none_body_when_label_alone_with_trailing_whitespace() {
        let s = "# CALLOUT: alone   \n";
        let got = parse_callouts(s, "#");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, None);
    }

    #[test]
    fn collects_multiple_callouts_in_one_listing() {
        let s = "\
            # first comment\n\
            # CALLOUT: one Body of one.\n\
            key: value\n\
            # CALLOUT: two\n\
            other: thing\n\
            # CALLOUT: three Body of three.\n\
        ";
        let got = parse_callouts(s, "#");
        assert_eq!(got.len(), 3);
        assert_eq!((got[0].line, &got[0].label[..]), (2, "one"));
        assert_eq!((got[1].line, &got[1].label[..]), (4, "two"));
        assert_eq!((got[2].line, &got[2].label[..]), (6, "three"));
        assert_eq!(got[1].body, None);
    }

    #[test]
    fn tolerates_indented_marker() {
        let s = "    # CALLOUT: indented Body text.\n";
        let got = parse_callouts(s, "#");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "indented");
    }

    #[test]
    fn comment_prefix_for_extension_covers_initial_table() {
        for ext in ["yaml", "yml", "toml", "py", "sh", "bash", "tf", "hcl"] {
            assert_eq!(comment_prefix_for_extension(ext), Some("#"), "ext: {ext}");
        }
        for ext in ["rs", "c", "h", "cpp", "hpp", "js", "ts", "jsx", "tsx"] {
            assert_eq!(comment_prefix_for_extension(ext), Some("//"), "ext: {ext}");
        }
        assert_eq!(comment_prefix_for_extension("sql"), Some("--"));
    }

    #[test]
    fn comment_prefix_for_extension_returns_none_for_unknown_languages() {
        assert_eq!(comment_prefix_for_extension("css"), None);
        assert_eq!(comment_prefix_for_extension(""), None);
        assert_eq!(comment_prefix_for_extension("md"), None);
    }
}
