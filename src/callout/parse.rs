use std::collections::HashMap;

/// Position is a 1-based line number so error diagnostics and the eventual
/// rendered badge anchor can both refer to it directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Callout {
    pub line: usize,
    pub label: String,
    pub body: Option<String>,
    /// `--key=value` options written between the label and the body, e.g.
    /// `// CALLOUT: lbl --align=left Body text.` parses to
    /// `options = {"align" => "left"}`. Unknown keys round-trip but have
    /// no rendering effect today; that's how new per-callout options
    /// (alignment, width, theme) can land without a parser change.
    pub options: HashMap<String, String>,
}

/// Walks `content` line by line and returns every well-formed callout
/// marker. A marker is a line whose first non-whitespace content matches
/// `<comment_prefix> CALLOUT: <label>[ <body>]`. Malformed lines are
/// silently skipped — the splicer leaves them in the rendered listing
/// unchanged.
// CALLOUT: parse-entry The single entry point: walks lines, calls parse_line, collects every match.
pub fn parse_callouts(content: &str, comment_prefix: &str) -> Vec<Callout> {
    let mut out = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        if let Some(callout) = parse_line(raw_line, comment_prefix, idx + 1) {
            out.push(callout);
        }
    }
    out
}

pub(super) fn parse_line(raw_line: &str, comment_prefix: &str, line: usize) -> Option<Callout> {
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
    // Pull `--key=value` options off the front of `rest` while the
    // leading token matches the option shape; the rest becomes body.
    let (options, body_str) = parse_options(rest.map(|s| s.trim_start()));
    let body = body_str
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(Callout {
        line,
        label: label.to_string(),
        body,
        options,
    })
}

/// Parses a leading sequence of `--key=value` tokens, returning the
/// option map plus whatever's left (the body). Tokens that don't match
/// the `--key=value` shape end option parsing; everything from that
/// token onward is the body (verbatim, with the leading whitespace
/// preserved so callers can re-trim).
fn parse_options(rest: Option<&str>) -> (HashMap<String, String>, Option<&str>) {
    let mut options = HashMap::new();
    let mut cursor = match rest {
        Some(s) => s,
        None => return (options, None),
    };
    loop {
        let trimmed = cursor.trim_start();
        if !trimmed.starts_with("--") {
            return (options, Some(cursor));
        }
        // Token is the substring up to the next whitespace.
        let (token, after) = match trimmed.split_once(char::is_whitespace) {
            Some((t, a)) => (t, Some(a)),
            None => (trimmed, None),
        };
        // Must contain `=` to be a valid option; otherwise treat as body.
        let kv = token.strip_prefix("--").and_then(|s| s.split_once('='));
        let Some((key, value)) = kv else {
            return (options, Some(cursor));
        };
        if key.is_empty() {
            return (options, Some(cursor));
        }
        options.insert(key.to_string(), value.to_string());
        cursor = match after {
            Some(rest) => rest,
            None => return (options, None),
        };
    }
}

// CALLOUT: label-grammar Labels are deliberately narrow: alphanumerics, hyphens, underscores. Anything else is rejected so labels stay safe to use as HTML id attributes and URL fragments.
pub(super) fn is_valid_label(label: &str) -> bool {
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

/// Extensions whose highlight-language name differs from the extension
/// itself. Absent extensions pass through unchanged (`yaml`, `toml`,
/// `sql`, `go`, …), which is both the info string a highlighter wants and
/// the input [`comment_prefix_for_extension`] takes, so listing them
/// would be noise.
///
/// The pairs are unique in both columns. That is what lets
/// [`lang_for_extension`] and [`comment_prefix_for_language`] read the one
/// table in opposite directions instead of each keeping its own copy.
const EXTENSION_LANGUAGES: &[(&str, &str)] = &[
    ("rs", "rust"),
    ("py", "python"),
    ("js", "javascript"),
    ("ts", "typescript"),
    ("sh", "bash"),
    ("yml", "yaml"),
    ("md", "markdown"),
];

/// The fence info string for a listing with this file extension — what a
/// self-contained `{{#include}}` writes after its opening fence so the
/// block highlights and its callout markers parse.
pub fn lang_for_extension(ext: &str) -> &str {
    EXTENSION_LANGUAGES
        .iter()
        .find(|(candidate, _)| *candidate == ext)
        .map_or(ext, |(_, lang)| *lang)
}

/// Maps a fenced-code-block info string to the language's single-line
/// comment prefix. Accepts the language names authors typically write
/// after the opening fence (`rust`, `yaml`, `python`, etc.) and falls back
/// to [`comment_prefix_for_extension`] for any input that's already an
/// extension (`rs`, `yml`).
pub fn comment_prefix_for_language(language: &str) -> Option<&'static str> {
    let normalised = match language {
        // Spellings with no extension of their own, so they can't come
        // back out of the table.
        "shell" | "zsh" => "sh",
        "c++" => "cpp",
        other => extension_for_lang(other).unwrap_or(other),
    };
    comment_prefix_for_extension(normalised)
}

fn extension_for_lang(lang: &str) -> Option<&'static str> {
    EXTENSION_LANGUAGES
        .iter()
        .find(|(_, candidate)| *candidate == lang)
        .map(|(ext, _)| *ext)
}

/// Produce the callout list for a fenced block. `info` is the fence's info
/// string (`rust`, `yaml`, `diff`, …). Diff blocks are handled specially:
/// only added (`+`) lines are stripped of their diff indicator and parsed
/// against every known comment prefix. Context (` `) lines, removed (`-`)
/// lines, and diff metadata (`---`, `+++`, `@@`, `\`) are skipped — a diff's
/// badges are unique to what that diff changed, so only a new or edited
/// callout (an added marker line) carries a badge. An unchanged callout is
/// already badged wherever the listing is `{{#include}}`-d in full, and a
/// deleted one is gone in the post-diff state.
pub(crate) fn callouts_for_block(info: &str, block_text: &str) -> Vec<Callout> {
    if info == "diff" {
        return callouts_from_diff_block(block_text);
    }
    if let Some(prefix) = comment_prefix_for_language(info) {
        return parse_callouts(block_text, prefix);
    }
    Vec::new()
}

pub(super) const ALL_COMMENT_PREFIXES: &[&str] = &["//", "#", "--"];

fn callouts_from_diff_block(block_text: &str) -> Vec<Callout> {
    let mut out = Vec::new();
    for (idx, raw_line) in block_text.lines().enumerate() {
        if raw_line.starts_with("---")
            || raw_line.starts_with("+++")
            || raw_line.starts_with("@@")
            || raw_line.starts_with('\\')
        {
            continue;
        }
        let Some(stripped) = raw_line.strip_prefix('+') else {
            // Context (` `) and removed (`-`) lines carry no badge: an
            // unchanged callout is already badged wherever the listing is
            // `{{#include}}`-d in full, and a removed one is gone in the
            // new state. Only an added marker (a new or edited callout)
            // surfaces here.
            continue;
        };
        for prefix in ALL_COMMENT_PREFIXES {
            if let Some(callout) = parse_line(stripped, prefix, idx + 1) {
                out.push(callout);
                break;
            }
        }
    }
    out
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
                ..Default::default()
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
                ..Default::default()
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

    #[test]
    fn comment_prefix_for_language_normalises_common_fence_labels() {
        assert_eq!(comment_prefix_for_language("rust"), Some("//"));
        assert_eq!(comment_prefix_for_language("python"), Some("#"));
        assert_eq!(comment_prefix_for_language("javascript"), Some("//"));
        assert_eq!(comment_prefix_for_language("shell"), Some("#"));
        assert_eq!(comment_prefix_for_language("c++"), Some("//"));
        assert_eq!(comment_prefix_for_language("yaml"), Some("#"));
        assert_eq!(comment_prefix_for_language("rs"), Some("//"));
    }

    #[test]
    fn lang_for_extension_names_the_highlighter_language() {
        assert_eq!(lang_for_extension("rs"), "rust");
        assert_eq!(lang_for_extension("yml"), "yaml");
        assert_eq!(lang_for_extension("sh"), "bash");
    }

    #[test]
    fn lang_for_extension_passes_through_an_extension_that_is_already_a_language() {
        assert_eq!(lang_for_extension("yaml"), "yaml");
        assert_eq!(lang_for_extension("toml"), "toml");
        assert_eq!(lang_for_extension("sql"), "sql");
        // Unknown extensions pass through too, so an include of a file the
        // table has never heard of still gets an info string that
        // `comment_prefix_for_language` can act on.
        assert_eq!(lang_for_extension("ttl"), "ttl");
    }

    #[test]
    fn naming_a_listing_by_extension_or_by_language_finds_the_same_comment_prefix() {
        // THE invariant the shared table exists to hold. A self-contained
        // include picks its info string with `lang_for_extension`, and the
        // callout pass reads that info string back with
        // `comment_prefix_for_language`. If the two directions disagree,
        // badges silently vanish from listings that used to carry them.
        for (ext, lang) in EXTENSION_LANGUAGES {
            assert_eq!(
                lang_for_extension(ext),
                *lang,
                "ext {ext} lost its language"
            );
            assert_eq!(
                comment_prefix_for_language(lang),
                comment_prefix_for_extension(ext),
                "{ext} and {lang} disagree on the comment prefix",
            );
        }
    }

    #[test]
    fn the_extension_language_table_is_unique_in_both_columns() {
        // Reading one table in two directions is only sound while neither
        // column repeats.
        for (i, (ext, lang)) in EXTENSION_LANGUAGES.iter().enumerate() {
            for (other_ext, other_lang) in &EXTENSION_LANGUAGES[i + 1..] {
                assert_ne!(ext, other_ext, "duplicate extension {ext}");
                assert_ne!(lang, other_lang, "duplicate language {lang}");
            }
        }
    }

    // ---------------------------------------------------------------
    // ch.6 slice 4: per-callout `--align` (and other `--key=value`)
    // options after the label, before the body.
    // ---------------------------------------------------------------

    #[test]
    fn parses_align_option_only_no_body() {
        let s = "// CALLOUT: lbl --align=left\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: None,
                options,
            }]
        );
    }

    #[test]
    fn parses_align_option_followed_by_body() {
        let s = "// CALLOUT: lbl --align=left Body text here.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Body text here.".into()),
                options,
            }]
        );
    }

    #[test]
    fn parses_multiple_options_then_body() {
        let s = "// CALLOUT: lbl --align=left --width=20em Body text.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        options.insert("width".into(), "20em".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Body text.".into()),
                options,
            }]
        );
    }

    #[test]
    fn unknown_option_keys_are_preserved_in_options_map() {
        // Forward-compat: a marker that uses a key the renderer doesn't
        // recognise (here `--theme=dark`) is parsed normally; the unknown
        // key sits in `options` for future use and has no rendering effect
        // today. Bodies AFTER the unknown option still parse cleanly.
        let s = "// CALLOUT: lbl --theme=dark Body text.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("theme".into(), "dark".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Body text.".into()),
                options,
            }]
        );
    }

    #[test]
    fn malformed_option_without_equals_is_part_of_body() {
        // `--align` (no `=value`) doesn't match the `--key=value` shape,
        // so it's treated as the start of the body. The grammar stays
        // unambiguous: options are EXACTLY `--key=value` and body is
        // everything from the first non-matching token onward.
        let s = "// CALLOUT: lbl --align Body without an equals.\n";
        let got = parse_callouts(s, "//");
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("--align Body without an equals.".into()),
                options: HashMap::new(),
            }]
        );
    }

    #[test]
    fn double_dash_separator_inside_body_is_preserved() {
        // Once the body has started (first non-option token), any later
        // `--` is part of the body verbatim. Authors writing technical
        // prose like "--no-verify" stay safe.
        let s = "// CALLOUT: lbl --align=left Use --no-verify carefully.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Use --no-verify carefully.".into()),
                options,
            }]
        );
    }
}
