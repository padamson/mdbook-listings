//! Parses inline `CALLOUT:` markers out of a frozen listing's source.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::directive::{FencePolicy, scan_directives};
use crate::fence::FencedBlocks;

mod parse;
mod render_html;
mod render_pdf;
mod sidecar;
mod strip;

pub use parse::{
    Callout, comment_prefix_for_extension, comment_prefix_for_language, parse_callouts,
};
pub(crate) use render_html::html_escape;
pub use sidecar::{SidecarCallouts, SidecarLoadError};

use parse::{callouts_for_block, is_valid_label};
use render_html::splice_callout_lists_html;
use render_pdf::splice_callout_lists_pdf;
use strip::listing_tag_after_fence;

/// Errors raised by the callout splicer.
#[derive(Debug)]
pub enum SpliceError {
    /// A `{{#callout <label>}}` directive named a label that no callout
    /// marker in the chapter defines.
    UnknownLabel { label: String },
    /// The same label appears as BOTH an inline `// CALLOUT:` marker in
    /// the frozen listing AND as a `[[callout]]` entry in the sidecar
    /// TOML file. Cross-source collisions silently hide one of the two
    /// rendered badges, so the build fails loudly and names the
    /// duplicate label plus both source paths.
    LabelCollision {
        label: String,
        listing_tag: String,
        sidecar_path: PathBuf,
    },
    /// A sidecar `[[callout]]` entry's `line` value points at a source
    /// line that the strip pass removes (because the source line itself
    /// is an inline `// CALLOUT:` marker). The badge would have nowhere
    /// to land in the rendered listing; fail loudly so the author
    /// either re-points the sidecar entry or removes the inline marker.
    SidecarLineOnStrippedMarker {
        label: String,
        listing_tag: String,
        source_line: usize,
        sidecar_path: PathBuf,
    },
}

impl std::fmt::Display for SpliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpliceError::UnknownLabel { label } => write!(
                f,
                "{{{{#callout {label}}}}} references a label that no callout marker defines \
                 in this chapter",
            ),
            SpliceError::LabelCollision {
                label,
                listing_tag,
                sidecar_path,
            } => write!(
                f,
                "label `{label}` is defined by both an inline `// CALLOUT:` marker in \
                 listing `{listing_tag}` and a `[[callout]]` entry in sidecar {}",
                sidecar_path.display(),
            ),
            SpliceError::SidecarLineOnStrippedMarker {
                label,
                listing_tag,
                source_line,
                sidecar_path,
            } => write!(
                f,
                "sidecar entry `{label}` (in {}) points at line {source_line} of listing \
                 `{listing_tag}`, but that line is an inline `// CALLOUT:` marker that gets \
                 stripped from the rendered listing — re-point the sidecar entry at a \
                 non-marker line or remove the inline marker",
                sidecar_path.display(),
            ),
        }
    }
}

impl std::error::Error for SpliceError {}

/// Which renderer the splicer is producing output for. The HTML emitter
/// uses raw `<dl>` tags so the rendered DOM carries stable
/// `data-callout-badge` and `dt[id]` attributes for cross-refs and e2e
/// assertions; the PDF emitter falls back to a markdown blockquote so
/// the typst-pdf backend renders the callouts as a styled note block
/// without relying on raw HTML passthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedRenderer {
    Html,
    TypstPdf,
}

impl SupportedRenderer {
    pub fn from_renderer_name(name: &str) -> Option<Self> {
        match name {
            "html" => Some(Self::Html),
            "typst-pdf" => Some(Self::TypstPdf),
            _ => None,
        }
    }
}

/// Replace each fenced code block in `content` with the original block plus
/// (when the block contains callout markers) a trailing per-renderer
/// callout list (`<dl class="callouts">` for HTML, markdown blockquote for
/// PDF), and replace each `{{#callout <label>}}` directive in chapter
/// prose with an inline anchor (HTML) or marker badge (PDF) that links
/// back to the listing badge.
// CALLOUT: splice-entry The HTML splicer entry point. The diff splicer in src/diff.rs has a sister function with the same shape — fence-aware walk + per-block emit.
pub fn splice_chapter(
    content: &str,
    renderer: SupportedRenderer,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    let label_to_ordinal = collect_first_occurrence_ordinals(content, sidecars)?;
    let with_lists = splice_callout_lists(content, &label_to_ordinal, renderer, sidecars)?;
    replace_callout_refs(&with_lists, &label_to_ordinal, renderer)
}

/// A label's canonical badge: its within-listing ordinal and the number of
/// the listing it first appears in (`None` when numbering is off). The prose
/// cross-ref scopes its displayed token to `listing_number.ordinal`.
#[derive(Clone)]
struct CalloutRef {
    ordinal: usize,
    listing_number: Option<String>,
}

/// Render a badge's visible token, scoped to its listing when numbered:
/// `5.3.1` with a listing number, bare `1` without.
fn scoped_badge(listing_number: Option<&str>, ordinal: usize) -> String {
    match listing_number {
        Some(n) => format!("{n}.{ordinal}"),
        None => ordinal.to_string(),
    }
}

/// Records each label's ordinal at its FIRST occurrence in the chapter.
/// Subsequent occurrences (e.g. when the same source file is shown via
/// `{{#diff}}` after being `{{#include}}`'d) are ignored: the first dt
/// gets `id="callout-<label>"` and acts as the canonical anchor target;
/// later dts render the badge but no `id` so the HTML stays valid.
fn collect_first_occurrence_ordinals(
    content: &str,
    sidecars: &SidecarCallouts,
) -> Result<HashMap<String, CalloutRef>, SpliceError> {
    let mut map: HashMap<String, CalloutRef> = HashMap::new();
    for block in FencedBlocks::new(content) {
        let (inline, sidecar) =
            split_callouts_for_block(block.info, block.body, content, block.close_end, sidecars)?;
        let listing_number = listing_number_after_fence(content, block.close_end);
        // Ordinal pass uses block-encounter order: inline by
        // source position (already sorted), then sidecar by
        // source line (sorted). Stable across render + ordinal
        // because the render path sorts by post-strip line,
        // which preserves source order when the source-line→
        // post-strip translation is monotone (which it is —
        // shift count only ever grows).
        let mut merged = inline;
        merged.extend(sidecar);
        merged.sort_by_key(|c| c.line);
        for (idx, c) in merged.iter().enumerate() {
            map.entry(c.label.clone()).or_insert_with(|| CalloutRef {
                ordinal: idx + 1,
                listing_number: listing_number.clone(),
            });
        }
    }
    Ok(map)
}

fn splice_callout_lists(
    content: &str,
    label_to_ordinal: &HashMap<String, CalloutRef>,
    renderer: SupportedRenderer,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    match renderer {
        SupportedRenderer::Html => splice_callout_lists_html(content, sidecars),
        SupportedRenderer::TypstPdf => {
            splice_callout_lists_pdf(content, label_to_ordinal, sidecars)
        }
    }
}

/// Replace `{{#callout <label>}}` directives that sit outside fenced code
/// blocks. Directives inside fenced blocks, inline code spans, or behind a
/// backslash escape (e.g. literal documentation examples) pass through
/// untouched so authors can show the syntax.
// CALLOUT: cross-ref-replace Cross-ref entry: the shared scanner skips directives inside fenced blocks; this pass errors on labels not in the chapter's collected map.
fn replace_callout_refs(
    content: &str,
    label_to_ordinal: &HashMap<String, CalloutRef>,
    renderer: SupportedRenderer,
) -> Result<String, SpliceError> {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for occ in scan_directives(content, "{{#callout ", FencePolicy::SkipInside) {
        let label = occ.args.trim();
        if !is_valid_label(label) {
            // Malformed label: leave the directive literal (copied through
            // with the surrounding prose on the next push).
            continue;
        }
        let cref = label_to_ordinal
            .get(label)
            .ok_or_else(|| SpliceError::UnknownLabel {
                label: label.to_string(),
            })?;
        out.push_str(&content[cursor..occ.span.start]);
        out.push_str(&render_callout_ref(label, cref, renderer));
        cursor = occ.span.end;
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

// CALLOUT: cross-ref-emit Renders the prose-side anchor for HTML; falls back to a bracketed badge for typst-pdf where raw HTML doesn't carry through.
fn render_callout_ref(label: &str, cref: &CalloutRef, renderer: SupportedRenderer) -> String {
    // The visible token scopes to the listing (`5.3.1`); `data-callout-ordinal`
    // stays the bare within-listing ordinal the JS and matching tests key on.
    let badge = scoped_badge(cref.listing_number.as_deref(), cref.ordinal);
    let ordinal = cref.ordinal;
    match renderer {
        SupportedRenderer::Html => {
            let label_esc = html_escape(label);
            format!(
                "<a class=\"callout-badge callout-ref\" href=\"#callout-{label_esc}\" \
                 data-callout-ref=\"{label_esc}\" data-callout-ordinal=\"{ordinal}\">{badge}</a>",
            )
        }
        SupportedRenderer::TypstPdf => format!("**[{badge}]**"),
    }
}

/// Read the `data-listing-number` the numbering pass stamps onto a listing's
/// locator anchor — present on both include (`data-listing-tag`) and diff
/// (`data-listing-diff-left`) anchors. `None` when numbering is off or the
/// block has no anchor. Tolerates one newline between fence and anchor.
fn listing_number_after_fence(content: &str, close_end: usize) -> Option<String> {
    let tail = &content[close_end..];
    let after_newline = tail.strip_prefix('\n').unwrap_or(tail);
    let div_open = after_newline.find("<div ")?;
    if div_open > crate::anchor::SCAN_TOLERANCE {
        return None;
    }
    let div_end = after_newline[div_open..].find('>')? + div_open;
    let div_text = &after_newline[div_open..div_end];
    crate::anchor::attr_value(div_text, "data-listing-number")
}

/// Split inline-marker callouts from sidecar callouts for a given block.
/// Returns `(inline, sidecar)` so the render path can keep their
/// post-strip line bookkeeping separate (inline have their post-strip
/// line in [`StripResult::post_strip_lines`]; sidecar lines need
/// translation from source-line to post-strip via
/// [`StripResult::stripped_source_lines`]).
///
/// Errors on cross-source label collisions (same label both inline AND
/// sidecar) — silently shadowing one would hide a rendered badge.
fn split_callouts_for_block(
    info: &str,
    block_text: &str,
    content: &str,
    close_end: usize,
    sidecars: &SidecarCallouts,
) -> Result<(Vec<Callout>, Vec<Callout>), SpliceError> {
    let inline = callouts_for_block(info, block_text);
    let Some(tag) = listing_tag_after_fence(content, close_end) else {
        return Ok((inline, Vec::new()));
    };
    let sidecar = sidecars.for_tag(tag);
    if sidecar.is_empty() {
        return Ok((inline, Vec::new()));
    }
    let inline_labels: HashSet<&str> = inline.iter().map(|c| c.label.as_str()).collect();
    for entry in sidecar {
        if inline_labels.contains(entry.label.as_str()) {
            return Err(SpliceError::LabelCollision {
                label: entry.label.clone(),
                listing_tag: tag.to_string(),
                sidecar_path: sidecars
                    .path_for_tag(tag)
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
            });
        }
    }
    Ok((inline, sidecar.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::sidecar::write_sidecar;
    use super::*;

    #[test]
    fn splice_chapter_html_cross_ref_to_context_only_diff_label_resolves_to_include() {
        // A callout that appears only on a context line in a diff (no badge
        // there) but also in a full listing still resolves a prose
        // `{{#callout LABEL}}` — to the include occurrence, which carries
        // the canonical `id="callout-LABEL"` anchor.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,2 @@\n",
            " // CALLOUT: shared-ref Unchanged in the diff.\n",
            " fn carried() {}\n",
            "```\n\n",
            "```rust\n",
            "// CALLOUT: shared-ref The full listing where it lives.\n",
            "fn body() {}\n",
            "```\n\n",
            "See callout {{#callout shared-ref}} for details.\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("cross-ref to a context-only diff label should resolve, not error");
        assert_eq!(
            out.matches("data-callout-badge=\"shared-ref\"").count(),
            1,
            "exactly one listing-side badge (the include's) should render; got:\n{out}",
        );
        assert_eq!(
            out.matches("id=\"callout-shared-ref\"").count(),
            1,
            "exactly one canonical anchor should exist; got:\n{out}",
        );
        assert!(
            out.contains("href=\"#callout-shared-ref\"")
                && out.contains("data-callout-ref=\"shared-ref\""),
            "the prose cross-ref should resolve to the canonical anchor; got:\n{out}",
        );
    }

    #[test]
    fn replace_callout_refs_skips_directives_inside_inline_backticks_in_prose() {
        // A chapter that documents the cross-ref syntax in prose like
        // "use `{{#callout LABEL}}` to ..." must not have the example
        // text resolve as a real cross-ref — the inline backticks mark
        // it as a documentation example, mirroring how the diff parser
        // skips directives between `…` on the same line.
        let content =
            "```rust\n// CALLOUT: greeting Hello.\n```\n\nUse `{{#callout LABEL}}` to refer.\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("`{{#callout LABEL}}`"),
            "literal example syntax in inline backticks must survive verbatim; got:\n{out}",
        );
    }

    #[test]
    fn replace_callout_refs_leaves_backslash_escaped_directive_literal() {
        // The backslash escape works for cross-refs the same way it does
        // for the include and diff directives: the example stays literal
        // even when the label would resolve.
        let content =
            "```rust\n// CALLOUT: greeting Hello.\n```\n\nEscaped: \\{{#callout greeting}}.\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("\\{{#callout greeting}}"),
            "escaped directive must survive verbatim; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-ref=\"greeting\""),
            "escaped directive must not render a cross-ref anchor; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_replaces_callout_directive_with_anchor_to_listing_badge() {
        let content = concat!(
            "Prose mentions {{#callout greeting}} the marker.\n\n",
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("href=\"#callout-greeting\""),
            "expected anchor href pointing at listing badge id; got:\n{out}",
        );
        assert!(
            out.contains("data-callout-ref=\"greeting\""),
            "expected ref-side data attribute; got:\n{out}",
        );
        assert!(
            !out.contains("{{#callout greeting}}"),
            "directive should be replaced; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_resolves_forward_reference_to_callout_defined_below() {
        let content = concat!(
            "See {{#callout later}} below.\n\n",
            "```rust\n",
            "// CALLOUT: later Defined after the reference.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(out.contains("href=\"#callout-later\""));
    }

    #[test]
    fn splice_chapter_callout_ref_carries_per_listing_ordinal() {
        let content = concat!(
            "Reference {{#callout two}} here.\n\n",
            "```rust\n",
            "// CALLOUT: one First.\n",
            "// CALLOUT: two Second.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let segment = out.split("data-callout-ref=\"two\"").nth(1).unwrap_or("");
        assert!(
            segment.contains("data-callout-ordinal=\"2\""),
            "ref to `two` should carry ordinal 2; got segment:\n{segment}",
        );
    }

    #[test]
    fn splice_chapter_unknown_callout_label_returns_error() {
        let content = "Unknown ref {{#callout missing}} here.\n";
        let err = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect_err("expected unknown-label error");
        match err {
            SpliceError::UnknownLabel { label } => assert_eq!(label, "missing"),
            other => panic!("expected UnknownLabel, got {other:?}"),
        }
    }

    #[test]
    fn splice_chapter_does_not_replace_callout_directive_inside_code_block() {
        // Authors show literal directive syntax inside fenced examples; the
        // splicer must not rewrite them into anchors.
        let content = concat!(
            "```text\n",
            "See {{#callout greeting}} for details.\n",
            "```\n\n",
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("{{#callout greeting}}"),
            "literal directive inside code block should pass through; got:\n{out}",
        );
        assert!(
            !out.contains("href=\"#callout-greeting\""),
            "should not have rendered anchor for the in-code-block reference; got:\n{out}",
        );
    }

    /// A listing block whose locator anchor carries `data-listing-number`,
    /// as the numbering pass leaves it for the callout pass.
    fn numbered_listing(number: Option<&str>) -> String {
        let num = number
            .map(|n| format!(" data-listing-number=\"{n}\""))
            .unwrap_or_default();
        format!(
            "```rust\n// CALLOUT: foo A note.\nfn x() {{}}\n```\n<div data-listing-tag=\"x\"{num} aria-hidden=\"true\"></div>\n"
        )
    }

    #[test]
    fn in_listing_badge_scopes_to_listing_number_when_numbered() {
        let out = splice_chapter(
            &numbered_listing(Some("5.3")),
            SupportedRenderer::Html,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(
            out.contains(">5.3.1</button>"),
            "badge text scopes; got:\n{out}"
        );
        // The bare ordinal stays on the attribute the JS and tests key on.
        assert!(
            out.contains(r#"data-callout-ordinal="1""#),
            "data-callout-ordinal stays bare; got:\n{out}",
        );
    }

    #[test]
    fn in_listing_badge_stays_bare_when_unnumbered() {
        let out = splice_chapter(
            &numbered_listing(None),
            SupportedRenderer::Html,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(out.contains(">1</button>"), "badge stays bare; got:\n{out}");
        assert!(!out.contains("5.3"), "got:\n{out}");
    }

    #[test]
    fn prose_cross_ref_scopes_to_first_occurrence_listing_number() {
        let content = format!(
            "See {{{{#callout foo}}}}.\n\n{}",
            numbered_listing(Some("5.3"))
        );
        let out = splice_chapter(&content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains(">5.3.1</a>"),
            "cross-ref text scopes; got:\n{out}"
        );
        assert!(
            out.contains(r#"data-callout-ordinal="1""#),
            "cross-ref ordinal stays bare; got:\n{out}",
        );
    }

    #[test]
    fn pdf_badges_scope_to_listing_number() {
        let content = format!(
            "See {{{{#callout foo}}}}.\n\n{}",
            numbered_listing(Some("5.3"))
        );
        let out = splice_chapter(
            &content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(
            out.contains("**[5.3.1] foo**"),
            "in-listing PDF badge scopes; got:\n{out}"
        );
        assert!(
            out.contains("**[5.3.1]**"),
            "PDF cross-ref scopes; got:\n{out}"
        );
    }

    #[test]
    fn scoped_badge_joins_number_and_ordinal() {
        assert_eq!(scoped_badge(Some("5.3"), 2), "5.3.2");
        assert_eq!(scoped_badge(None, 2), "2");
    }

    #[test]
    fn listing_number_after_fence_reads_number_off_either_anchor() {
        let inc = "```rust\nx\n```\n<div data-listing-tag=\"a\" data-listing-number=\"5.1\" aria-hidden=\"true\"></div>\n";
        let close_end = inc.find("```\n<div").map(|i| i + 4).unwrap();
        assert_eq!(
            listing_number_after_fence(inc, close_end).as_deref(),
            Some("5.1")
        );

        let diff = "```diff\n+x\n```\n<div data-listing-diff-left=\"a\" data-listing-diff-right=\"b\" data-listing-number=\"5.2\" aria-hidden=\"true\"></div>";
        let close_end = diff.find("```\n<div").map(|i| i + 4).unwrap();
        assert_eq!(
            listing_number_after_fence(diff, close_end).as_deref(),
            Some("5.2")
        );

        let bare = "```rust\nx\n```\n<div data-listing-tag=\"a\" aria-hidden=\"true\"></div>\n";
        let close_end = bare.find("```\n<div").map(|i| i + 4).unwrap();
        assert_eq!(listing_number_after_fence(bare, close_end), None);
    }

    #[test]
    fn listing_number_after_fence_accepts_anchor_at_64_byte_offset() {
        let pad: String = "x".repeat(64);
        let content = format!(
            "```rust\nx\n```\n{pad}<div data-listing-tag=\"a\" data-listing-number=\"5.1\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.rfind("```\n").map(|i| i + 4).unwrap();
        assert_eq!(
            listing_number_after_fence(&content, close_end).as_deref(),
            Some("5.1"),
        );
    }

    #[test]
    fn listing_number_after_fence_rejects_anchor_at_65_byte_offset() {
        let pad: String = "x".repeat(65);
        let content = format!(
            "```rust\nx\n```\n{pad}<div data-listing-tag=\"a\" data-listing-number=\"5.1\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.rfind("```\n").map(|i| i + 4).unwrap();
        assert_eq!(listing_number_after_fence(&content, close_end), None);
    }

    /// `SpliceError`'s `Display` impl must actually format the variant's
    /// fields — without this assertion the whole body could be replaced
    /// with `Ok(Default::default())` (empty string) and no test would
    /// notice.
    #[test]
    fn splice_error_display_includes_label_and_paths_for_label_collision() {
        let err = SpliceError::LabelCollision {
            label: "duplicate".to_string(),
            listing_tag: "demo-v1".to_string(),
            sidecar_path: std::path::PathBuf::from("/tmp/demo.callouts.toml"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("duplicate"),
            "label missing from display; got: {msg}"
        );
        assert!(
            msg.contains("demo-v1"),
            "listing tag missing from display; got: {msg}"
        );
        assert!(
            msg.contains("/tmp/demo.callouts.toml"),
            "sidecar path missing from display; got: {msg}",
        );
    }

    #[test]
    fn splice_chapter_merges_sidecar_callouts_into_overlay_for_matching_tag() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 2
label = "sidecar-marker"
body = "Attached without modifying the listing bytes."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        // Listing has no inline markers; the language (css) has no recognised
        // single-line comment syntax in the table, so inline parsing yields
        // nothing — the sidecar is the only source.
        let content = concat!(
            "```css\n",
            ".callout-body { background: white; }\n",
            ".callout-body::after { content: ''; }\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap();
        assert!(
            out.contains(r#"data-callout-badge="sidecar-marker""#),
            "overlay should carry the sidecar badge; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_errors_on_label_collision_between_inline_and_sidecar() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 2
label = "duplicate"
body = "Sidecar definition."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = concat!(
            "```rust\n",
            "// CALLOUT: duplicate Inline definition.\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let err = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap_err();
        match err {
            SpliceError::LabelCollision {
                label,
                listing_tag,
                sidecar_path,
            } => {
                assert_eq!(label, "duplicate");
                assert_eq!(listing_tag, "demo-v1");
                assert!(sidecar_path.ends_with("demo-v1.callouts.toml"));
            }
            other => panic!("expected LabelCollision, got {other:?}"),
        }
    }

    #[test]
    fn splice_chapter_errors_when_sidecar_line_points_at_inline_marker_line() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Inline marker is on source line 1 of the block body.
        // Sidecar points at source line 1 too — would render onto
        // a line the strip pass removes.
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 1
label = "lands-on-stripped"
body = "Boom."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = concat!(
            "```rust\n",
            "// CALLOUT: inline Body.\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let err = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap_err();
        match err {
            SpliceError::SidecarLineOnStrippedMarker {
                label,
                source_line,
                listing_tag,
                ..
            } => {
                assert_eq!(label, "lands-on-stripped");
                assert_eq!(source_line, 1);
                assert_eq!(listing_tag, "demo-v1");
            }
            other => panic!("expected SidecarLineOnStrippedMarker, got {other:?}"),
        }
    }

    #[test]
    fn splice_chapter_inline_and_sidecar_callouts_with_distinct_labels_compose_in_line_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 3
label = "from-sidecar"
body = "Attached via sidecar."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = concat!(
            "```rust\n",
            "// CALLOUT: from-inline Inline marker.\n",
            "let x = 1;\n",
            "let y = 2;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap();
        // Both badges must render.
        assert!(
            out.contains(r#"data-callout-badge="from-inline""#),
            "got:\n{out}"
        );
        assert!(
            out.contains(r#"data-callout-badge="from-sidecar""#),
            "got:\n{out}"
        );
        // Ordinal 1 is the inline (line 1 before strip; line 1 after); ordinal
        // 2 is the sidecar (line 3). The HTML emits buttons in order;
        // ordinal "1" appears before ordinal "2" textually.
        let inline_pos = out.find(r#"data-callout-badge="from-inline""#).unwrap();
        let sidecar_pos = out.find(r#"data-callout-badge="from-sidecar""#).unwrap();
        assert!(
            inline_pos < sidecar_pos,
            "inline badge (line 1) should render before sidecar badge (line 3); got:\n{out}",
        );
    }
}
