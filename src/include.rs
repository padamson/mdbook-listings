//! Intercepts `{{#include listings/...}}` and `{{#include snippets/...}}`
//! before mdbook's built-in `links` preprocessor expands them, so the
//! callout splicer downstream can find any `CALLOUT:` markers in the
//! included source and so frozen-listing includes get a locator anchor.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::diff::{LineRange, parse_line_range};
use crate::directive::{FencePolicy, line_number, scan_directives};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeDirective {
    pub tag: Option<String>,
    /// Path part of the directive — never carries a `:start:end` suffix.
    pub rel_path: String,
    /// Optional line range parsed off the trailing `:start:end` suffix,
    /// matching mdBook's built-in `{{#include path:start:end}}` form.
    pub range: Option<LineRange>,
    pub caption: Option<String>,
    /// Stable name assigned via `label="..."`, resolved by
    /// `{{#listing-ref <label>}}` to the listing's current number.
    pub label: Option<String>,
    /// Highlight language from `lang="..."`, overriding the one the file
    /// extension implies. Only consulted for the self-contained form —
    /// inside an author's fence, the fence's own info string wins.
    pub lang: Option<String>,
    pub span: Range<usize>,
    pub fence_close_end: Option<usize>,
}

impl IncludeDirective {
    /// The info string for a self-contained expansion: the explicit
    /// `lang="..."` when the author set one, otherwise the language named
    /// by the file's extension. Empty for an extensionless path, which
    /// yields an unlabelled fence rather than a guess.
    fn info_string(&self) -> &str {
        if let Some(lang) = &self.lang {
            return lang;
        }
        Path::new(&self.rel_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map_or("", crate::callout::lang_for_extension)
    }
}

/// One past the newline that ends the directive's line, when nothing but
/// blanks follow it there. `None` when the rest of the line has content.
fn trailing_newline_end(content: &str, from: usize) -> Option<usize> {
    let rest = &content[from..];
    let blanks = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    rest[blanks..]
        .starts_with('\n')
        .then_some(from + blanks + 1)
}

/// Whether the directive at `at` is the only thing on its line, give or
/// take CommonMark's 3 spaces of leading indent. A fence emitted for a
/// directive that shares its line would open mid-line and never be read
/// as a fence at all, so the splicer rejects that case instead.
fn starts_its_own_line(content: &str, at: usize) -> bool {
    let line_start = content[..at].rfind('\n').map_or(0, |nl| nl + 1);
    let indent = &content[line_start..at];
    indent.len() <= 3 && indent.chars().all(|c| c == ' ')
}

// CALLOUT: include-parse-entry The shared directive scanner finds every unescaped `{{#include ...}}` outside inline code (chapter prose quotes the syntax verbatim); this pass keeps only the intercepted path prefixes and parses their range suffix. (Renamed from `parse-entry`, which collided with the marker of the same name in callout.rs.)
pub fn parse_listing_includes(content: &str) -> Vec<IncludeDirective> {
    let mut out = Vec::new();
    for occ in scan_directives(content, "{{#include ", FencePolicy::Annotate) {
        let (args, caption) = crate::directive::split_caption(occ.args);
        let (args, label) = crate::directive::split_label(&args);
        let (args, lang) = crate::directive::split_lang(&args);
        let raw = args.trim();
        // CALLOUT: snippets-intercept Two prefixes are intercepted: `listings/` (frozen tags — emit anchor) and `snippets/` (no anchor; we expand to give the callout splicer a shot at any CALLOUT markers in the snippet source). Other forms fall through to mdbook's built-in `links` preprocessor.
        let intercepted = raw.starts_with("listings/") || raw.starts_with("snippets/");
        if !intercepted {
            continue;
        }
        // Split on the first `:` to separate the path from an optional
        // `:start:end` suffix (mdBook's built-in include slicing form).
        // We accept the suffix here so listings/snippets includes can
        // address a fragment of the file the same way mdBook's `links`
        // preprocessor would for any other path. Other forms (anchor
        // names, `=anchor`) fall through to `links`.
        let (path, range) = match raw.split_once(':') {
            Some((p, suffix)) => match parse_line_range(suffix) {
                Some(r) => (p, Some(r)),
                None => continue,
            },
            None => (raw, None),
        };
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
        out.push(IncludeDirective {
            tag,
            rel_path: path.to_string(),
            range,
            caption,
            label,
            lang,
            span: occ.span,
            fence_close_end: occ.fence_close_end,
        });
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
    ListingIncludeMidLine {
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
            SpliceError::ListingIncludeMidLine {
                tag,
                line,
                chapter_path,
            } => {
                write!(
                    f,
                    "{}:{line}: {{{{#include listings/{tag}.…}}}} shares its line with other text; \
                     put it on a line of its own so it can render as a code block",
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
            SpliceError::ListingIncludeMidLine { .. } => None,
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
        if d.fence_close_end.is_none() && !starts_its_own_line(content, d.span.start) {
            return Err(SpliceError::ListingIncludeMidLine {
                tag: d.tag.clone().unwrap_or_else(|| d.rel_path.clone()),
                line: line_number(content, d.span.start),
                chapter_path: chapter_path.map(Path::to_path_buf),
            });
        }
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
        if let Some(range) = &d.range {
            // Prepend a two-line header that mirrors a unified-diff's
            // `--- left-tag\n@@ -A,B +C,D @@` shape: filename basename on
            // line 1 (analogous to `--- TAG`), `@@ start,end @@` on
            // line 2 (analogous to the hunk header). Both lines are
            // comment-prefixed when the file extension maps to a known
            // single-line syntax, so syntax highlighters render them as
            // metadata rather than invalid code.
            let header = crate::anchor::ranged_include_header(&d.rel_path, range);
            let sliced = range.slice(&body);
            body = format!("{header}\n{sliced}");
        }
        out.push_str(&content[cursor..d.span.start]);
        match d.fence_close_end {
            // The author wrote the fence, so only the bytes between its
            // lines are ours to replace.
            Some(close_end) => {
                // Why: the chapter's newline-after-directive (preserved via
                // `content[d.span.end..close_end]`) terminates the last
                // content line; keeping the file's own trailing newline
                // produces a blank line before the closing fence.
                while body.ends_with('\n') {
                    body.pop();
                }
                // Escape `{{` so mdbook's downstream links preprocessor
                // doesn't try to resolve literal directive-shaped strings
                // in the substituted bytes. Safe for frozen listings
                // (source code, not Markdown). Caveat: `snippets/` accepts
                // any extension, so a `.md` snippet would have its own
                // directives escaped too — acceptable, since rendering
                // them as literal text inside a code fence is what an
                // author quoting markdown wants anyway.
                out.push_str(&body.replace("{{", "\\{{"));
                out.push_str(&content[d.span.end..close_end]);
                cursor = close_end;
            }
            // No enclosing fence, so the directive renders the whole block
            // itself. The info string is load-bearing twice over: it picks
            // the highlighting, and the callout pass reads it back to learn
            // which comment prefix marks a `CALLOUT:` line.
            None => {
                out.push_str(&crate::fence::render_block(d.info_string(), &body));
                // The emitted block already ends in a newline, so the
                // directive's own line terminator would leave a stray blank
                // line — and the two forms would stop rendering identically.
                // Anything else trailing the directive is prose; leave it.
                cursor = trailing_newline_end(content, d.span.end).unwrap_or(d.span.end);
            }
        }
        if let Some(tag) = &d.tag {
            // CALLOUT: include-anchor-emit One `<div data-listing-tag="...">` per `listings/` include, dropped just past the closing fence so the screenshot tool can find the rendered `<pre>` via `previousElementSibling`.
            out.push_str(&crate::anchor::include_anchor(
                tag,
                d.range.as_ref(),
                d.caption.as_deref(),
                d.label.as_deref(),
            ));
        }
    }
    out.push_str(&content[cursor..]);
    Ok(out)
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
    fn parse_listing_includes_skips_unintercepted_path_prefixes_and_anchor_name_includes() {
        // - `../../Cargo.toml` lacks the `listings/` or `snippets/` prefix
        //   so it falls through to mdbook's built-in `links` preprocessor.
        // - `snippets/foo.rs:setup` uses mdbook's anchor-name include form
        //   (not a line-range) — also defer to `links`.
        let content = concat!(
            "```toml\n",
            "{{#include ../../Cargo.toml}}\n",
            "```\n\n",
            "```rust\n",
            "{{#include snippets/foo.rs:setup}}\n",
            "```\n",
        );
        assert!(
            parse_listing_includes(content).is_empty(),
            "expected non-listing includes and anchor-name forms to be skipped",
        );
    }

    #[test]
    fn parse_listing_includes_picks_up_listings_include_with_line_range() {
        let content = "```rust\n{{#include listings/foo.rs:5:20}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].rel_path, "listings/foo.rs");
        assert_eq!(got[0].tag.as_deref(), Some("foo"));
        assert_eq!(
            got[0].range,
            Some(LineRange {
                start: Some(5),
                end: Some(20)
            })
        );
    }

    #[test]
    fn parse_listing_includes_extracts_caption_and_keeps_path_clean() {
        let content = "```rust\n{{#include listings/foo.rs caption=\"The claim layer\"}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].rel_path, "listings/foo.rs");
        assert_eq!(got[0].tag.as_deref(), Some("foo"));
        assert_eq!(got[0].caption.as_deref(), Some("The claim layer"));
    }

    #[test]
    fn parse_listing_includes_caption_coexists_with_range() {
        let content = "```rust\n{{#include listings/foo.rs:5:20 caption=\"Slice\"}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].rel_path, "listings/foo.rs");
        assert_eq!(
            got[0].range,
            Some(LineRange {
                start: Some(5),
                end: Some(20)
            })
        );
        assert_eq!(got[0].caption.as_deref(), Some("Slice"));
    }

    #[test]
    fn parse_listing_includes_picks_up_snippets_include_with_line_range() {
        let content = "```rust\n{{#include snippets/foo.rs:5:20}}\n```\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].rel_path, "snippets/foo.rs");
        assert_eq!(got[0].tag, None, "snippets do not get a locator anchor");
        assert_eq!(
            got[0].range,
            Some(LineRange {
                start: Some(5),
                end: Some(20)
            })
        );
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
            SpliceError::ListingIncludeMidLine { .. } => panic!("wrong variant"),
        }
    }

    #[test]
    fn splice_chapter_rejects_a_directive_that_shares_its_line_with_prose() {
        // A fence has to start a line, so there is no sensible block to
        // render here — say so rather than emit broken Markdown.
        let chapter = std::path::Path::new("ch99-foo.md");
        let content = "Mid-paragraph: {{#include listings/foo.rs}} bare directive.\n";
        let tmp = TempDir::new().unwrap();
        let err = splice_chapter(content, tmp.path(), Some(chapter)).expect_err("should fail");
        match err {
            SpliceError::ListingIncludeMidLine {
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

    /// A listing on disk plus the chapter text that includes it, spliced.
    fn splice_one(file: &str, bytes: &str, chapter: &str) -> String {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        let path = src.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        splice_chapter(chapter, src, None).expect("splice")
    }

    #[test]
    fn splice_chapter_renders_a_bare_include_as_its_own_fenced_block() {
        // The directive alone on a line, no fence around it: the splicer
        // supplies the block that the author used to have to write.
        let out = splice_one(
            "listings/foo.rs",
            "fn body() {}\n",
            "Before.\n\n{{#include listings/foo.rs}}\n\nAfter.\n",
        );
        assert!(
            out.contains("```rust\nfn body() {}\n```\n"),
            "expected a self-contained block; got:\n{out}",
        );
        assert!(out.starts_with("Before.\n"), "got:\n{out}");
        assert!(out.ends_with("After.\n"), "got:\n{out}");
        assert!(!out.contains("{{#include"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_names_the_fence_language_from_the_file_extension() {
        let out = splice_one(
            "listings/conf.yml",
            "key: value\n",
            "{{#include listings/conf.yml}}\n",
        );
        assert!(
            out.contains("```yaml\nkey: value\n```\n"),
            "`.yml` should open a `yaml` fence; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_prefers_an_explicit_lang_over_the_extension() {
        let out = splice_one(
            "listings/schema.txt",
            "@prefix ex: <http://example.org/> .\n",
            "{{#include listings/schema.txt lang=\"turtle\"}}\n",
        );
        assert!(out.contains("```turtle\n"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_leaves_the_fence_unlabelled_for_an_extensionless_path() {
        // Better an unhighlighted block than a guessed language.
        let out = splice_one(
            "listings/Makefile",
            "all:\n",
            "{{#include listings/Makefile}}\n",
        );
        assert!(out.contains("```\nall:\n```\n"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_emits_the_anchor_after_a_bare_includes_own_closing_fence() {
        let out = splice_one(
            "listings/foo.rs",
            "fn body() {}\n",
            "{{#include listings/foo.rs}}\n",
        );
        let anchor = out.find("data-listing-tag").expect("anchor present");
        let close = out.rfind("```\n").expect("closing fence") + 4;
        assert!(anchor > close, "anchor must follow the fence; got:\n{out}");
    }

    #[test]
    fn splice_chapter_widens_a_bare_includes_fence_when_the_listing_contains_one() {
        let out = splice_one(
            "listings/readme.md",
            "Example:\n\n```rust\nfn main() {}\n```\n",
            "{{#include listings/readme.md}}\n",
        );
        assert!(
            out.contains("````markdown\n"),
            "wrapper must outgrow the listing's own fence; got:\n{out}",
        );
        assert!(out.trim_end().ends_with("</div>"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_carries_range_caption_and_label_through_the_bare_form() {
        // Everything the fenced form supports keeps working without a fence.
        let out = splice_one(
            "listings/sample.rs",
            "line1\nline2\nline3\nline4\n",
            "{{#include listings/sample.rs:2:3 caption=\"Cap\" label=\"lbl\"}}\n",
        );
        assert!(out.contains("line2\nline3"), "sliced body; got:\n{out}");
        assert!(!out.contains("line4"), "range respected; got:\n{out}");
        assert!(out.contains("// sample.rs"), "ranged header; got:\n{out}");
        assert!(
            out.contains(r#"data-listing-tag-range="2:3""#),
            "got:\n{out}"
        );
        assert!(out.contains(r#"data-listing-caption="Cap""#), "got:\n{out}");
        assert!(out.contains(r#"data-listing-label="lbl""#), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_escapes_double_braces_in_a_bare_include() {
        let out = splice_one(
            "listings/foo.rs",
            "let example = \"{{#include listings/bar.rs}}\";\n",
            "{{#include listings/foo.rs}}\n",
        );
        assert!(
            out.contains("\"\\{{#include listings/bar.rs}}\""),
            "got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_accepts_up_to_three_spaces_of_indent_before_a_bare_include() {
        // CommonMark's own threshold for a fence opener.
        let out = splice_one(
            "listings/foo.rs",
            "fn body() {}\n",
            "   {{#include listings/foo.rs}}\n",
        );
        assert!(out.contains("```rust\n"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_consumes_trailing_blanks_after_a_bare_include() {
        // The directive's line terminator is redundant once the block is
        // emitted, and so is any whitespace sitting before it.
        let padded = splice_one(
            "listings/foo.rs",
            "fn body() {}\n",
            "{{#include listings/foo.rs}}   \nAfter.\n",
        );
        let clean = splice_one(
            "listings/foo.rs",
            "fn body() {}\n",
            "{{#include listings/foo.rs}}\nAfter.\n",
        );
        assert_eq!(padded, clean, "padded:\n{padded}\nclean:\n{clean}");
        assert!(padded.ends_with("</div>\nAfter.\n"), "got:\n{padded}");
    }

    #[test]
    fn splice_chapter_keeps_prose_that_follows_a_bare_include_on_its_line() {
        let out = splice_one(
            "listings/foo.rs",
            "fn body() {}\n",
            "{{#include listings/foo.rs}} trailing prose\n",
        );
        assert!(out.contains(" trailing prose\n"), "got:\n{out}");
    }

    #[test]
    fn splice_chapter_rejects_a_bare_include_indented_past_a_fence_opener() {
        // Four spaces would make the emitted fence an indented code block
        // rather than a fence, so this is not a rendering we can produce.
        let tmp = TempDir::new().unwrap();
        let err = splice_chapter("    {{#include listings/foo.rs}}\n", tmp.path(), None)
            .expect_err("four spaces of indent should be rejected");
        assert!(matches!(err, SpliceError::ListingIncludeMidLine { .. }));
    }

    #[test]
    fn splice_chapter_rejects_a_bare_include_behind_a_short_run_of_prose() {
        // Short enough to pass an indent-length check on its own; the
        // characters still have to be blanks.
        let tmp = TempDir::new().unwrap();
        let err = splice_chapter("ab {{#include listings/foo.rs}}\n", tmp.path(), None)
            .expect_err("prose before the directive should be rejected");
        assert!(matches!(err, SpliceError::ListingIncludeMidLine { .. }));
    }

    #[test]
    fn parse_listing_includes_extracts_lang_and_keeps_path_clean() {
        let content = "{{#include listings/foo.txt lang=\"turtle\"}}\n";
        let got = parse_listing_includes(content);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].rel_path, "listings/foo.txt");
        assert_eq!(got[0].lang.as_deref(), Some("turtle"));
    }

    #[test]
    fn splice_chapter_passes_through_unintercepted_path_prefixes_untouched() {
        // Non-listing/non-snippet paths (e.g. `../../Cargo.toml`) and
        // anchor-name includes (e.g. `:setup`) are left alone for mdbook's
        // built-in `links` preprocessor to expand downstream.
        let tmp = TempDir::new().unwrap();
        let content = concat!(
            "```toml\n",
            "{{#include ../../Cargo.toml}}\n",
            "```\n\n",
            "```rust\n",
            "{{#include snippets/foo.rs:anchor-name}}\n",
            "```\n",
        );
        let out = splice_chapter(content, tmp.path(), None).expect("splice");
        assert_eq!(out, content, "got:\n{out}");
    }

    #[test]
    fn splice_chapter_slices_listings_include_with_line_range_and_emits_range_anchor() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(
            src.join("listings/sample.rs"),
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .unwrap();
        let content = "```rust\n{{#include listings/sample.rs:2:4}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(
            out.contains("line2\nline3\nline4"),
            "expected sliced lines 2-4 to be inlined; got:\n{out}",
        );
        assert!(
            !out.contains("line1") && !out.contains("line5"),
            "lines outside the range should be excluded; got:\n{out}",
        );
        assert!(
            out.contains(r#"data-listing-tag="sample""#),
            "expected listing-tag anchor; got:\n{out}",
        );
        assert!(
            out.contains(r#"data-listing-tag-range="2:4""#),
            "expected range data attribute; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_omits_range_anchor_attribute_for_whole_file_listings_include() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/sample.rs"), "fn body() {}\n").unwrap();
        let content = "```rust\n{{#include listings/sample.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(
            !out.contains("data-listing-tag-range"),
            "no range attr expected without :start:end suffix; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_emits_caption_attribute_on_anchor_when_present() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body() {}\n").unwrap();
        let content = "```rust\n{{#include listings/foo.rs caption=\"The claim layer\"}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(
            out.contains(r#"data-listing-caption="The claim layer""#),
            "expected caption attribute on anchor; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_omits_caption_attribute_when_absent() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body() {}\n").unwrap();
        let content = "```rust\n{{#include listings/foo.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(
            !out.contains("data-listing-caption"),
            "no caption attr expected without caption=; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_escapes_caption_attribute() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(src.join("listings/foo.rs"), "fn body() {}\n").unwrap();
        // `&` and `<` must be entity-escaped so the attribute stays
        // well-formed HTML.
        let content = "```rust\n{{#include listings/foo.rs caption=\"A & B <tag>\"}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(
            out.contains(r#"data-listing-caption="A &amp; B &lt;tag&gt;""#),
            "caption attribute should be HTML-escaped; got:\n{out}",
        );
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

    /// Included-body content containing literal `{{...}}` (test fixtures
    /// quoting example directives, etc.) must NOT be interpreted by
    /// mdbook's built-in `links` preprocessor downstream. The splicer
    /// escapes `{{` to `\{{` so the resolver leaves the literal alone;
    /// the rendered output still shows `{{...}}` visually.
    #[test]
    fn splice_chapter_escapes_double_braces_in_included_body() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path();
        std::fs::create_dir_all(src.join("listings")).unwrap();
        std::fs::write(
            src.join("listings/foo.rs"),
            "let example = \"{{#include listings/bar.rs}}\";\n",
        )
        .unwrap();
        let content = "```rust\n{{#include listings/foo.rs}}\n```\n";
        let out = splice_chapter(content, src, None).expect("splice");
        assert!(
            out.contains("\"\\{{#include listings/bar.rs}}\""),
            "expected `{{` in included body to be escaped to `\\{{`; got:\n{out}",
        );
    }
}
