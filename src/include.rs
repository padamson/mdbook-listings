//! Intercepts `{{#include listings/...}}` and `{{#include snippets/...}}`
//! before mdbook's built-in `links` preprocessor expands them, so the
//! callout splicer downstream can find any `CALLOUT:` markers in the
//! included source and so frozen-listing includes get a locator anchor.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::callout::for_each_fenced_block_with_span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeDirective {
    pub tag: Option<String>,
    pub rel_path: String,
    pub span: Range<usize>,
    pub fence_close_end: Option<usize>,
}

// CALLOUT: parse-entry Two-pass scan: first collect fence body spans, then walk lines for `{{#include ...}}` directives. Skips backslash-escaped forms and directives inside inline code spans (chapter prose quotes the syntax verbatim).
pub fn parse_listing_includes(content: &str) -> Vec<IncludeDirective> {
    let mut fences: Vec<(usize, usize)> = Vec::new();
    for_each_fenced_block_with_span(content, |_info, _text, body_start, close_end| {
        fences.push((body_start, close_end));
    });

    const PREFIX: &[u8] = b"{{#include ";
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut line_start = 0;
    while line_start < bytes.len() {
        let line_end = match content[line_start..].find('\n') {
            Some(off) => line_start + off,
            None => bytes.len(),
        };
        let mut i = line_start;
        while i + PREFIX.len() <= line_end {
            if &bytes[i..i + PREFIX.len()] != PREFIX {
                i += 1;
                continue;
            }
            if i > 0 && bytes[i - 1] == b'\\' {
                i += PREFIX.len();
                continue;
            }
            let backticks_before = bytes[line_start..i].iter().filter(|&&b| b == b'`').count();
            if backticks_before % 2 == 1 {
                i += PREFIX.len();
                continue;
            }
            let inner_start = i + PREFIX.len();
            let Some(end_rel) = content[inner_start..].find("}}") else {
                break;
            };
            let directive_end = inner_start + end_rel + 2;
            let path = content[inner_start..inner_start + end_rel].trim();
            // CALLOUT: snippets-intercept Two prefixes are intercepted: `listings/` (frozen tags — emit anchor) and `snippets/` (no anchor; we expand to give the callout splicer a shot at any CALLOUT markers in the snippet source). Other forms fall through to mdbook's built-in `links` preprocessor.
            let intercepted = path.starts_with("listings/") || path.starts_with("snippets/");
            if !intercepted || path.contains(':') {
                i = directive_end;
                continue;
            }
            // CALLOUT: tag-from-stem Tag is the file stem of `listings/...` paths so `listings/sub/foo.rs` and `listings/foo.rs` produce the same anchor; subdirectory stem collisions would clash on the anchor, but the book has none today.
            let tag = if path.starts_with("listings/") {
                Some(
                    std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string(),
                )
            } else {
                None
            };
            let fence_close_end = fences
                .iter()
                .find(|(body_start, close_end)| i >= *body_start && i < *close_end)
                .map(|(_, close_end)| *close_end);
            out.push(IncludeDirective {
                tag,
                rel_path: path.to_string(),
                span: i..directive_end,
                fence_close_end,
            });
            i = directive_end;
        }
        if line_end == bytes.len() {
            break;
        }
        line_start = line_end + 1;
    }
    out
}

#[derive(Debug)]
pub enum SpliceError {
    ListingFileMissing {
        tag: String,
        path: PathBuf,
        source: std::io::Error,
        line: usize,
        chapter_path: Option<PathBuf>,
    },
    ListingIncludeOutsideFence {
        tag: String,
        line: usize,
        chapter_path: Option<PathBuf>,
    },
}

impl std::fmt::Display for SpliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpliceError::ListingFileMissing {
                tag,
                path,
                source,
                line,
                chapter_path,
            } => {
                write!(
                    f,
                    "{}:{line}: {{{{#include listings/{tag}.…}}}} references missing file {}: {source}",
                    chapter_path
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<chapter>".into()),
                    path.display(),
                )
            }
            SpliceError::ListingIncludeOutsideFence {
                tag,
                line,
                chapter_path,
            } => {
                write!(
                    f,
                    "{}:{line}: {{{{#include listings/{tag}.…}}}} appears outside any fenced code block; \
                     wrap it in ```<lang> ... ``` so the anchor has a <pre> sibling",
                    chapter_path
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<chapter>".into()),
                )
            }
        }
    }
}

impl std::error::Error for SpliceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SpliceError::ListingFileMissing { source, .. } => Some(source),
            SpliceError::ListingIncludeOutsideFence { .. } => None,
        }
    }
}

// CALLOUT: include-splice-entry The HTML splicer entry point. Walks every intercepted directive; replaces with file body and (for `listings/`) drops a `<div data-listing-tag>` locator anchor after the closing fence.
pub fn splice_chapter(
    content: &str,
    src_dir: &Path,
    chapter_path: Option<&Path>,
) -> Result<String, SpliceError> {
    let directives = parse_listing_includes(content);
    if directives.is_empty() {
        return Ok(content.to_string());
    }

    let mut out = String::with_capacity(content.len() * 2);
    let mut cursor = 0;
    for d in &directives {
        let Some(close_end) = d.fence_close_end else {
            return Err(SpliceError::ListingIncludeOutsideFence {
                tag: d.tag.clone().unwrap_or_else(|| d.rel_path.clone()),
                line: line_number(content, d.span.start),
                chapter_path: chapter_path.map(Path::to_path_buf),
            });
        };
        let abs_path = src_dir.join(&d.rel_path);
        let mut body = std::fs::read_to_string(&abs_path).map_err(|source| {
            SpliceError::ListingFileMissing {
                tag: d.tag.clone().unwrap_or_else(|| d.rel_path.clone()),
                path: abs_path.clone(),
                source,
                line: line_number(content, d.span.start),
                chapter_path: chapter_path.map(Path::to_path_buf),
            }
        })?;
        // Why: the chapter's newline-after-directive (preserved via
        // `content[d.span.end..]`) terminates the last content line; keeping
        // the file's own trailing newline produces a blank line before the
        // closing fence.
        while body.ends_with('\n') {
            body.pop();
        }
        out.push_str(&content[cursor..d.span.start]);
        out.push_str(&body);
        out.push_str(&content[d.span.end..close_end]);
        if let Some(tag) = &d.tag {
            // CALLOUT: include-anchor-emit One `<div data-listing-tag="...">` per `listings/` include, dropped just past the closing fence so the screenshot tool can find the rendered `<pre>` via `previousElementSibling`.
            out.push_str(&format!(
                "<div data-listing-tag=\"{tag}\" aria-hidden=\"true\"></div>\n",
            ));
        }
        cursor = close_end;
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_listing_includes_extracts_well_formed_directive() {
        let content = "Before.\n```rust\n{{#include listings/foo.rs}}\n```\nAfter.\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag.as_deref(), Some("foo"));
        assert_eq!(got[0].rel_path, "listings/foo.rs");
    }

    #[test]
    fn parse_listing_includes_extracts_tag_as_file_stem() {
        let content = "```rust\n{{#include listings/some-tag-v3.rs}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag.as_deref(), Some("some-tag-v3"));
    }

    #[test]
    fn parse_listing_includes_collects_snippets_with_no_tag() {
        let content = "```rust\n{{#include snippets/excerpt.rs}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag, None);
        assert_eq!(got[0].rel_path, "snippets/excerpt.rs");
    }

    #[test]
    fn parse_listing_includes_skips_escaped_form() {
        let content = "Inline example: \\{{#include listings/foo.rs}} should not match.\n";
        assert!(parse_listing_includes(content).is_empty());
    }

    #[test]
    fn parse_listing_includes_skips_directive_inside_inline_code_span() {
        let content = "Prose discussing `{{#include listings/foo.rs}}` syntax.\n";
        assert!(parse_listing_includes(content).is_empty());
    }

    #[test]
    fn parse_listing_includes_skips_unintercepted_paths_and_line_ranges() {
        let content = concat!(
            "```toml\n",
            "{{#include ../../Cargo.toml}}\n",
            "```\n\n",
            "```rust\n",
            "{{#include listings/foo.rs:5:20}}\n",
            "```\n\n",
            "```rust\n",
            "{{#include snippets/foo.rs:setup}}\n",
            "```\n",
        );
        assert!(parse_listing_includes(content).is_empty());
    }

    #[test]
    fn parse_listing_includes_handles_subdirectory_path() {
        let content = "```rust\n{{#include listings/sub/foo.rs}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag.as_deref(), Some("foo"));
        assert_eq!(got[0].rel_path, "listings/sub/foo.rs");
    }

    #[test]
    fn parse_listing_includes_records_fence_close_end_for_in_fence_directive() {
        let content = "```rust\n{{#include listings/foo.rs}}\n```\nafter\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1);
        assert!(got[0].fence_close_end.is_some());
    }

    #[test]
    fn parse_listing_includes_records_no_fence_close_end_for_out_of_fence_directive() {
        let content = "Inline mention: {{#include listings/foo.rs}} not in fence.\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].fence_close_end, None);
    }

    #[test]
    fn splice_chapter_replaces_directive_with_file_contents_and_emits_anchor() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body() {}\n").unwrap();
        let content = "```rust\n{{#include listings/foo.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(out.contains("fn body() {}"), "got:\n{out}");
        assert!(!out.contains("{{#include"), "got:\n{out}");
        assert!(out.contains("data-listing-tag=\"foo\""), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_emits_anchor_after_closing_fence_not_inside_block() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body() {}\n").unwrap();
        let content = "```rust\n{{#include listings/foo.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        let anchor_pos = out.find("data-listing-tag").expect("anchor present");
        let close_fence_pos = out
            .find("```\n")
            .map(|p| p + 4)
            .expect("close fence present");
        assert!(anchor_pos > close_fence_pos, "got:\n{out}");
    }

    #[test]
    fn splice_chapter_returns_listing_file_missing_with_chapter_line_for_absent_file() {
        let tmp = TempDir::new().unwrap();
        let chapter = std::path::Path::new("ch99-foo.md");
        let content = "intro\n\n```rust\n{{#include listings/missing-tag.rs}}\n```\n";
        let err = splice_chapter(content, tmp.path(), Some(chapter)).expect_err("should fail");
        match err {
            SpliceError::ListingFileMissing {
                tag,
                line,
                chapter_path,
                ..
            } => {
                assert_eq!(tag, "missing-tag");
                assert_eq!(line, 4);
                assert_eq!(chapter_path.as_deref(), Some(chapter));
            }
            SpliceError::ListingIncludeOutsideFence { .. } => panic!("wrong variant"),
        }
    }

    #[test]
    fn splice_chapter_returns_listing_include_outside_fence_when_directive_has_no_enclosing_fence()
    {
        let chapter = std::path::Path::new("ch99-foo.md");
        let content = "Mid-paragraph: {{#include listings/foo.rs}} bare directive.\n";
        let tmp = TempDir::new().unwrap();
        let err = splice_chapter(content, tmp.path(), Some(chapter)).expect_err("should fail");
        match err {
            SpliceError::ListingIncludeOutsideFence {
                tag,
                line,
                chapter_path,
            } => {
                assert_eq!(tag, "foo");
                assert_eq!(line, 1);
                assert_eq!(chapter_path.as_deref(), Some(chapter));
            }
            SpliceError::ListingFileMissing { .. } => panic!("wrong variant"),
        }
    }

    #[test]
    fn splice_chapter_passes_through_unintercepted_includes_untouched() {
        let tmp = TempDir::new().unwrap();
        let content = concat!(
            "```toml\n",
            "{{#include ../../Cargo.toml}}\n",
            "```\n\n",
            "```rust\n",
            "{{#include listings/foo.rs:5:20}}\n",
            "```\n",
        );
        let out = splice_chapter(content, tmp.path(), None).expect("splice");
        assert_eq!(out, content, "got:\n{out}");
    }

    #[test]
    fn splice_chapter_expands_snippet_include_without_emitting_anchor() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("snippets")).unwrap();
        std::fs::write(src.join("snippets/excerpt.rs"), "fn snippet_body() {}\n").unwrap();
        let content = "```rust\n{{#include snippets/excerpt.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(out.contains("fn snippet_body() {}"), "got:\n{out}");
        assert!(!out.contains("data-listing-tag"), "got:\n{out}");
        assert!(!out.contains("{{#include"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_handles_two_includes_in_one_chapter_with_independent_anchors() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body_one() {}\n").unwrap();
        std::fs::write(src.join("listings/bar.rs"), "fn body_two() {}\n").unwrap();
        let content = concat!(
            "```rust\n",
            "{{#include listings/foo.rs}}\n",
            "```\n\n",
            "```rust\n",
            "{{#include listings/bar.rs}}\n",
            "```\n",
        );
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(out.contains("fn body_one() {}"));
        assert!(out.contains("fn body_two() {}"));
        assert!(out.contains("data-listing-tag=\"foo\""));
        assert!(out.contains("data-listing-tag=\"bar\""));
    }

    #[test]
    fn splice_chapter_appends_trailing_newline_when_file_lacks_one() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body() {}").unwrap();
        let content = "```rust\n{{#include listings/foo.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(out.contains("fn body() {}\n```"), "got:\n{out}");
    }
}
