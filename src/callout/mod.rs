//! Parses inline `CALLOUT:` markers out of a frozen listing's source.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::directive::{FencePolicy, scan_directives};
use crate::fence::FencedBlocks;

mod parse;
mod render_html;
mod render_pdf;
mod sidecar;
mod strip;

pub(crate) use parse::{
    Callout, callouts_for_block, comment_prefix_for_extension, lang_for_extension, parse_callouts,
};
pub(crate) use render_html::{html_escape, html_unescape, render_caption_markdown};
use sidecar::DiffSide;
pub(crate) use sidecar::SidecarCallouts;

use parse::is_valid_label;
use render_html::splice_callout_lists_html;
use render_pdf::splice_callout_lists_pdf;
use strip::{
    DiffLineMap, diff_operands_after_fence, listing_anchor_after_fence, source_line_to_block_line,
    strip_marker_lines,
};

/// The lines of a listing with extension `ext` that the renderer strips as
/// callout markers when an include shows it.
pub(crate) fn marker_lines(content: &str, ext: &str) -> Vec<usize> {
    strip_marker_lines(content, lang_for_extension(ext)).stripped_source_lines
}

/// Errors raised by the callout splicer.
#[derive(Debug)]
pub enum SpliceError {
    /// A `{{#callout <label>}}` directive named a label that no callout
    /// marker in the chapter defines.
    UnknownLabel { label: String },
    /// A `{{#callout <label>}}` directive named a label whose marker sits
    /// only on unchanged context lines of the chapter's diffs. A diff
    /// badges only what it changed, so the marker renders nothing to link
    /// to; the generic unknown-label message would send the author looking
    /// for a typo.
    DiffContextLabel { label: String },
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
    /// A diff sidecar entry on a line that no `{{#diff}}` of its pair in
    /// the book shows. The sidecar belongs to that diff, so an entry it
    /// can't place is a mistake, not a slice leaving it out.
    DiffSidecarLineNotShown {
        label: String,
        side: DiffSide,
        line: usize,
        old: String,
        new: String,
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
            SpliceError::DiffContextLabel { label } => write!(
                f,
                "{{{{#callout {label}}}}} references a marker that appears in this chapter \
                 only on an unchanged context line of a {{{{#diff}}}}, and a diff badges only \
                 added lines; cite it from an {{{{#include}}}} of the listing instead",
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
            SpliceError::DiffSidecarLineNotShown {
                label,
                side,
                line,
                old,
                new,
                sidecar_path,
            } => write!(
                f,
                "sidecar entry `{label}` (in {}) points at line {line} of the {} file, which no \
                 {{{{#diff {old} {new}}}}} in the book shows — point it at a line inside a hunk, \
                 or widen the diff",
                sidecar_path.display(),
                side.as_str(),
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
    replace_callout_refs(&with_lists, &label_to_ordinal, renderer).map_err(|err| match err {
        SpliceError::UnknownLabel { label } if on_diff_context_line(content, &label) => {
            SpliceError::DiffContextLabel { label }
        }
        other => other,
    })
}

/// Whether `label` has a marker on a context line of one of the chapter's
/// diffs. Read from the unspliced chapter, since splicing strips markers.
fn on_diff_context_line(content: &str, label: &str) -> bool {
    FencedBlocks::new(content)
        .filter(|block| block.info == "diff")
        .any(|block| {
            parse::callouts_on_diff_lines(&block.unprefixed_body(), ' ')
                .iter()
                .any(|c| c.label == label)
        })
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
        let callouts = split_callouts_for_block(
            block.info,
            &block.unprefixed_body(),
            content,
            block.close_end,
            sidecars,
        )?;
        let listing_number = listing_number_after_fence(content, block.close_end);
        // Inline and sidecar callouts both carry block lines here, so sorting
        // by line orders them as the render path does by post-strip line:
        // the translation shifts by a count that only grows down the block.
        let mut merged = callouts.inline;
        merged.extend(callouts.sidecar.into_iter().map(|p| p.callout));
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

/// Replace `{{#callout <label>}}` directives that sit outside code blocks.
/// Directives inside code blocks, inline code spans, or behind a backslash
/// escape (e.g. literal documentation examples) pass through untouched so
/// authors can show the syntax.
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
    crate::anchor::attr_value(
        crate::anchor::anchor_after_fence(content, close_end)?,
        "data-listing-number",
    )
}

/// The labels of each diff sidecar that some `{{#diff}}` of its pair placed,
/// gathered across the book's chapters. A pair diffed in slices, even in
/// different chapters, may place each entry in any one of them.
#[derive(Debug, Default)]
pub(crate) struct DiffSidecarPlacements(BTreeMap<(String, String), HashSet<String>>);

impl DiffSidecarPlacements {
    /// Record the entries the chapter's diffs place, through the placement
    /// the render path uses.
    pub(crate) fn record(
        &mut self,
        content: &str,
        sidecars: &SidecarCallouts,
    ) -> Result<(), SpliceError> {
        for block in FencedBlocks::new(content).filter(|b| b.info == "diff") {
            let callouts = split_callouts_for_block(
                block.info,
                &block.unprefixed_body(),
                content,
                block.close_end,
                sidecars,
            )?;
            if let Some(pair) = callouts.diff_pair {
                self.0
                    .entry(pair)
                    .or_default()
                    .extend(callouts.sidecar.into_iter().map(|p| p.callout.label));
            }
        }
        Ok(())
    }

    /// Fail on the first entry of a diffed pair's sidecar that no diff placed:
    /// the sidecar belongs to that diff, so an entry it can't place is a
    /// mistake, not a slice leaving it out. A pair the book never diffs is
    /// not checked, as a listing's sidecar isn't for a book that never
    /// includes it.
    pub(crate) fn check(&self, sidecars: &SidecarCallouts) -> Result<(), SpliceError> {
        for ((old, new), placed) in &self.0 {
            if let Some(e) = sidecars
                .for_diff(old, new)
                .iter()
                .find(|e| !placed.contains(&e.callout.label))
            {
                return Err(SpliceError::DiffSidecarLineNotShown {
                    label: e.callout.label.clone(),
                    side: e.side,
                    line: e.callout.line,
                    old: old.clone(),
                    new: new.clone(),
                    sidecar_path: sidecars
                        .path_for_diff(old, new)
                        .map(Path::to_path_buf)
                        .unwrap_or_default(),
                });
            }
        }
        Ok(())
    }
}

/// The callouts one fenced block shows: its inline markers, and the entries
/// of the sidecar that annotates it (a listing's sidecar for an include, a
/// diff sidecar for a diff) that land on a line the block shows.
pub(super) struct BlockCallouts<'s> {
    pub(super) inline: Vec<Callout>,
    pub(super) sidecar: Vec<PlacedSidecar>,
    pub(super) sidecar_path: Option<&'s Path>,
    /// The (old, new) operands of a diff whose pair has a sidecar.
    pub(super) diff_pair: Option<(String, String)>,
}

/// A sidecar entry placed on its block. `callout.line` is the block line it
/// lands on, before marker lines are stripped; `listing` and `written_line`
/// are the frozen file and line the author wrote, for diagnostics.
pub(super) struct PlacedSidecar {
    pub(super) callout: Callout,
    pub(super) listing: String,
    pub(super) written_line: usize,
}

/// Split a block's inline-marker callouts from its sidecar callouts, so the
/// render path can keep their post-strip bookkeeping separate (inline have
/// their post-strip line in [`StripResult::post_strip_lines`]; sidecar block
/// lines need translating via [`StripResult::stripped_source_lines`]).
///
/// The sidecar half holds only entries for lines the block shows, so a
/// ranged include leaves out the rest and a `{{#callout}}` naming one finds
/// nothing here.
///
/// Errors on cross-source label collisions (same label both inline AND
/// sidecar) — silently shadowing one would hide a rendered badge. The check
/// covers the whole sidecar, not just the shown entries, since a label names
/// one line of the listing however much of it a block shows.
fn split_callouts_for_block<'s>(
    info: &str,
    block_text: &str,
    content: &str,
    close_end: usize,
    sidecars: &'s SidecarCallouts,
) -> Result<BlockCallouts<'s>, SpliceError> {
    let inline = callouts_for_block(info, block_text);
    let mut out = BlockCallouts {
        inline,
        sidecar: Vec::new(),
        sidecar_path: None,
        diff_pair: None,
    };
    if info == "diff" {
        let Some((old, new)) = diff_operands_after_fence(content, close_end) else {
            return Ok(out);
        };
        let entries = sidecars.for_diff(&old, &new);
        if entries.is_empty() {
            return Ok(out);
        }
        out.sidecar_path = sidecars.path_for_diff(&old, &new);
        // A diff badges inline markers on added lines only, so a colliding
        // marker is in the new file.
        check_collisions(
            &out.inline,
            entries.iter().map(|e| &e.callout),
            &new,
            out.sidecar_path,
        )?;
        let rows = DiffLineMap::new(block_text);
        out.sidecar = entries
            .iter()
            .filter_map(|e| {
                let row = rows.block_line(e.side, e.callout.line)?;
                let listing = match e.side {
                    DiffSide::Old => &old,
                    DiffSide::New => &new,
                };
                Some(PlacedSidecar {
                    callout: Callout {
                        line: row,
                        ..e.callout.clone()
                    },
                    listing: listing.clone(),
                    written_line: e.callout.line,
                })
            })
            .collect();
        out.diff_pair = Some((old, new));
        return Ok(out);
    }
    let Some(anchor) = listing_anchor_after_fence(content, close_end) else {
        return Ok(out);
    };
    let entries = sidecars.for_tag(anchor.tag);
    if entries.is_empty() {
        return Ok(out);
    }
    out.sidecar_path = sidecars.path_for_tag(anchor.tag);
    check_collisions(&out.inline, entries.iter(), anchor.tag, out.sidecar_path)?;
    let shown = anchor.shown_source_lines(block_text);
    out.sidecar = entries
        .iter()
        .filter(|c| shown.contains(&c.line))
        .map(|c| PlacedSidecar {
            callout: Callout {
                line: source_line_to_block_line(c.line, &anchor),
                ..c.clone()
            },
            listing: anchor.tag.to_string(),
            written_line: c.line,
        })
        .collect();
    Ok(out)
}

fn check_collisions<'a>(
    inline: &[Callout],
    mut sidecar: impl Iterator<Item = &'a Callout>,
    listing: &str,
    sidecar_path: Option<&Path>,
) -> Result<(), SpliceError> {
    let inline_labels: HashSet<&str> = inline.iter().map(|c| c.label.as_str()).collect();
    match sidecar.find(|c| inline_labels.contains(c.label.as_str())) {
        Some(c) => Err(SpliceError::LabelCollision {
            label: c.label.clone(),
            listing_tag: listing.to_string(),
            sidecar_path: sidecar_path.map(Path::to_path_buf).unwrap_or_default(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::sidecar::write_sidecar;
    use super::*;

    #[test]
    fn from_renderer_name_maps_both_supported_renderers_and_rejects_others() {
        assert_eq!(
            SupportedRenderer::from_renderer_name("html"),
            Some(SupportedRenderer::Html)
        );
        assert_eq!(
            SupportedRenderer::from_renderer_name("typst-pdf"),
            Some(SupportedRenderer::TypstPdf)
        );
        assert_eq!(SupportedRenderer::from_renderer_name("epub"), None);
    }

    #[test]
    fn align_option_renders_data_attr_only_for_left_and_right() {
        let content = concat!(
            "```rust\n",
            "// CALLOUT: l --align=left L.\n",
            "// CALLOUT: r --align=right R.\n",
            "// CALLOUT: c --align=center C.\n",
            "fn x() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(out.contains(r#"data-callout-align="left""#), "got:\n{out}");
        assert!(out.contains(r#"data-callout-align="right""#), "got:\n{out}");
        assert_eq!(
            out.matches("data-callout-align").count(),
            2,
            "unrecognised align values must render no attribute; got:\n{out}",
        );
    }

    #[test]
    fn pdf_callout_list_separates_entries_only_between_them() {
        let one = splice_chapter(
            "```rust\n// CALLOUT: a A.\nfn x() {}\n```\n",
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(
            !one.contains("> \n"),
            "single callout has no separator line; got:\n{one}",
        );
        let two = splice_chapter(
            "```rust\n// CALLOUT: a A.\n// CALLOUT: b B.\nfn x() {}\n```\n",
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert_eq!(
            two.matches("> \n").count(),
            1,
            "exactly one separator, between the two entries; got:\n{two}",
        );
    }

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
    fn a_cite_of_a_marker_only_on_a_diff_context_line_names_that_cause() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,3 @@\n",
            " # CALLOUT: carried An unchanged callout.\n",
            " carried: true\n",
            "+added: true\n",
            "```\n\n",
            "See {{#callout carried}}.\n",
        );
        let err = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect_err("a cite of an unbadged context marker cannot resolve");
        match err {
            SpliceError::DiffContextLabel { label } => assert_eq!(label, "carried"),
            other => panic!("expected DiffContextLabel, got {other:?}"),
        }
    }

    #[test]
    fn a_cite_of_a_marker_only_on_a_removed_diff_line_is_an_unknown_label() {
        // The callout is gone in the new state, so the generic message is
        // the right one; only context lines get the diff-specific cause.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,1 @@\n",
            "-# CALLOUT: gone A removed callout.\n",
            " kept: true\n",
            "```\n\n",
            "See {{#callout gone}}.\n",
        );
        let err = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect_err("a cite of a removed marker cannot resolve");
        match err {
            SpliceError::UnknownLabel { label } => assert_eq!(label, "gone"),
            other => panic!("expected UnknownLabel, got {other:?}"),
        }
    }

    #[test]
    fn the_diff_context_label_message_names_the_label_and_the_include_remedy() {
        let msg = SpliceError::DiffContextLabel {
            label: "carried".to_string(),
        }
        .to_string();
        for token in ["{{#callout carried}}", "{{#diff}}", "{{#include}}"] {
            assert!(
                msg.contains(token),
                "message should name {token}; got: {msg}"
            );
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
    fn a_listing_in_a_list_item_keeps_its_indent_through_the_callout_pass() {
        // The body, the overlay and the anchor all stay in the item; the
        // marker line goes and its badge lands on the line that follows.
        let content = "- Step\n\n  ```rust\n  // CALLOUT: foo A note.\n  fn x() {}\n  ```\n  \
                       <div data-listing-tag=\"x\" aria-hidden=\"true\"></div>\n\n  \
                       See {{#callout foo}}.\n\n- Next\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("  ```rust\n  fn x() {}\n  ```\n  <div class=\"callout-overlay\""),
            "got:\n{out}"
        );
        assert!(out.contains(r#"data-callout-line="1""#), "got:\n{out}");
        assert!(
            out.contains("See <a class=\"callout-badge callout-ref\""),
            "got:\n{out}"
        );
        let inserted: Vec<_> = out.lines().skip(2).take_while(|l| *l != "- Next").collect();
        assert!(
            inserted.iter().all(|l| l.is_empty() || l.starts_with("  ")),
            "every line in the item must keep its indent; got:\n{out}"
        );
    }

    #[test]
    fn a_diff_in_a_list_item_badges_its_added_marker() {
        // The item's indent sits before each line's diff indicator, so the
        // `+`, the metadata lines and the marker have to be read past it.
        let content = "- Step\n\n  ```diff\n  --- a\n  +++ b\n  @@ -1,1 +1,2 @@\n  \
                       +// CALLOUT: added New.\n   fn f() {}\n  ```\n  \
                       <div data-listing-diff-left=\"a\" data-listing-diff-right=\"b\" aria-hidden=\"true\"></div>\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains(
                "  @@ -1,1 +1,2 @@\n   fn f() {}\n  ```\n  <div class=\"callout-overlay\""
            ),
            "the marker line goes and the rest keeps its indent; got:\n{out}"
        );
        assert!(out.contains(r#"data-callout-badge="added""#), "got:\n{out}");
        assert!(out.contains(r#"data-callout-line="4""#), "got:\n{out}");
    }

    #[test]
    fn pdf_callout_list_in_a_list_item_takes_the_items_indent() {
        let content = "- Step\n\n  ```rust\n  // CALLOUT: foo A note.\n  fn x() {}\n  ```\n  \
                       <div data-listing-tag=\"x\" aria-hidden=\"true\"></div>\n";
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(
            out.contains("  ```\n\n  > **[1] foo** — A note.\n"),
            "got:\n{out}"
        );
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

    fn sidecars_with(entries: &[(usize, &str)]) -> (tempfile::TempDir, SidecarCallouts) {
        let tmp = tempfile::TempDir::new().unwrap();
        let toml: String = entries
            .iter()
            .map(|(line, label)| format!("[[callout]]\nline = {line}\nlabel = \"{label}\"\n\n"))
            .collect();
        write_sidecar(tmp.path(), "demo-v1", &toml);
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        (tmp, sidecars)
    }

    /// The source line whose badge carries `label`, read from the overlay
    /// entry that wraps it.
    fn badge_line<'a>(out: &'a str, label: &str) -> Option<&'a str> {
        let badge = out.find(&format!(r#"data-callout-badge="{label}""#))?;
        let key = r#"data-callout-line=""#;
        let at = out[..badge].rfind(key)? + key.len();
        Some(&out[at..at + out[at..].find('"')?])
    }

    // `demo-v1.rs` is nine lines, `let lN = N;`. `RANGED_DEMO` is an include
    // of `demo-v1.rs:5:7` as the include splicer emits it: two header lines,
    // then source lines 5 to 7.
    const RANGED_DEMO: &str = concat!(
        "```rust\n",
        "// demo.rs\n",
        "// @@ 5,7 @@\n",
        "let l5 = 5;\n",
        "let l6 = 6;\n",
        "let l7 = 7;\n",
        "```\n",
        "<div data-listing-tag=\"demo-v1\" data-listing-tag-range=\"5:7\" aria-hidden=\"true\"></div>\n",
    );

    const FULL_DEMO: &str = concat!(
        "```rust\n",
        "let l1 = 1;\nlet l2 = 2;\nlet l3 = 3;\nlet l4 = 4;\nlet l5 = 5;\n",
        "let l6 = 6;\nlet l7 = 7;\nlet l8 = 8;\nlet l9 = 9;\n",
        "```\n",
        "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
    );

    const AROUND_SLICE: &[(usize, &str)] = &[
        (4, "before-slice"),
        (5, "first-line"),
        (6, "mid-slice"),
        (7, "last-line"),
        (8, "after-slice"),
    ];

    #[test]
    fn ranged_include_badges_only_the_sidecar_entries_inside_its_slice() {
        let (_tmp, sidecars) = sidecars_with(AROUND_SLICE);
        for renderer in [SupportedRenderer::Html, SupportedRenderer::TypstPdf] {
            let out = splice_chapter(RANGED_DEMO, renderer, &sidecars).unwrap();
            for inside in ["first-line", "mid-slice", "last-line"] {
                assert!(
                    out.contains(inside),
                    "{renderer:?}: `{inside}`; got:\n{out}"
                );
            }
            for outside in ["before-slice", "after-slice"] {
                assert!(
                    !out.contains(outside),
                    "{renderer:?}: `{outside}` annotates a line the slice leaves out; got:\n{out}",
                );
            }
        }
    }

    #[test]
    fn ranged_include_lands_each_sidecar_badge_on_its_source_line() {
        let (_tmp, sidecars) = sidecars_with(AROUND_SLICE);
        let out = splice_chapter(RANGED_DEMO, SupportedRenderer::Html, &sidecars).unwrap();
        // Source line N of a slice from 5 is block line N - 5 + 1, after
        // the two header lines.
        assert_eq!(badge_line(&out, "first-line"), Some("3"), "got:\n{out}");
        assert_eq!(badge_line(&out, "mid-slice"), Some("4"), "got:\n{out}");
        assert_eq!(badge_line(&out, "last-line"), Some("5"), "got:\n{out}");
    }

    #[test]
    fn open_start_slice_counts_from_line_one_under_its_header() {
        let (_tmp, sidecars) = sidecars_with(&[(2, "in-slice"), (4, "after-slice")]);
        // `demo-v1.rs::3`: the header, then source lines 1 to 3.
        let content = concat!(
            "```rust\n",
            "// demo.rs\n",
            "// @@ 1,3 @@\n",
            "let l1 = 1;\nlet l2 = 2;\nlet l3 = 3;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" data-listing-tag-range=\":3\" aria-hidden=\"true\"></div>\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap();
        assert_eq!(badge_line(&out, "in-slice"), Some("4"), "got:\n{out}");
        assert!(!out.contains("after-slice"), "got:\n{out}");
    }

    #[test]
    fn full_include_leaves_out_a_sidecar_entry_past_the_last_line() {
        let (_tmp, sidecars) = sidecars_with(&[(9, "last-line"), (10, "past-end")]);
        let out = splice_chapter(FULL_DEMO, SupportedRenderer::Html, &sidecars).unwrap();
        assert_eq!(badge_line(&out, "last-line"), Some("9"), "got:\n{out}");
        assert!(!out.contains("past-end"), "got:\n{out}");
    }

    #[test]
    fn a_cite_of_a_sidecar_entry_outside_the_slice_is_an_unknown_label() {
        let (_tmp, sidecars) = sidecars_with(AROUND_SLICE);
        let content = format!("{RANGED_DEMO}\nSee {{{{#callout before-slice}}}}.\n");
        match splice_chapter(&content, SupportedRenderer::Html, &sidecars) {
            Err(SpliceError::UnknownLabel { label }) => assert_eq!(label, "before-slice"),
            other => panic!("expected UnknownLabel, got {other:?}"),
        }
    }

    #[test]
    fn a_cite_of_a_sidecar_entry_resolves_where_another_include_shows_its_line() {
        let (_tmp, sidecars) = sidecars_with(AROUND_SLICE);
        let content = format!("{FULL_DEMO}\n{RANGED_DEMO}\nSee {{{{#callout before-slice}}}}.\n");
        let out = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        assert!(
            out.contains(r#"data-callout-ref="before-slice""#),
            "got:\n{out}"
        );
    }

    #[test]
    fn a_slice_numbers_its_sidecar_badges_from_the_first_entry_it_shows() {
        let (_tmp, sidecars) = sidecars_with(AROUND_SLICE);
        let content = format!("{RANGED_DEMO}\nSee {{{{#callout first-line}}}}.\n");
        let out = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        assert!(
            out.contains(r#"data-callout-ref="first-line" data-callout-ordinal="1""#),
            "got:\n{out}"
        );
    }

    #[test]
    fn a_slice_numbers_inline_and_sidecar_callouts_together_in_row_order() {
        // Sidecar source line 5 is block row 3, above the inline marker on
        // row 4, though 5 is the larger number.
        let (_tmp, sidecars) = sidecars_with(&[(5, "first-line")]);
        let content = format!(
            "{}\nSee {{{{#callout first-line}}}} and {{{{#callout inline}}}}.\n",
            RANGED_DEMO.replace("let l6 = 6;\n", "// CALLOUT: inline X.\n")
        );
        let html = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        for (label, ordinal) in [("first-line", 1), ("inline", 2)] {
            assert!(
                html.contains(&format!(
                    r#"data-callout-ref="{label}" data-callout-ordinal="{ordinal}""#
                )),
                "{label} must be {ordinal}; got:\n{html}"
            );
        }
        let pdf = splice_chapter(&content, SupportedRenderer::TypstPdf, &sidecars).unwrap();
        assert!(pdf.contains("See **[1]** and **[2]**."), "got:\n{pdf}");
    }

    #[test]
    fn an_inline_marker_collides_with_a_sidecar_label_the_slice_leaves_out() {
        let (_tmp, sidecars) = sidecars_with(AROUND_SLICE);
        let content = RANGED_DEMO.replace("let l6 = 6;\n", "// CALLOUT: before-slice Inline.\n");
        match splice_chapter(&content, SupportedRenderer::Html, &sidecars) {
            Err(SpliceError::LabelCollision { label, .. }) => assert_eq!(label, "before-slice"),
            other => panic!("expected LabelCollision, got {other:?}"),
        }
    }

    fn diff_sidecar(toml: &str) -> (tempfile::TempDir, SidecarCallouts) {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(tmp.path(), "a-v1..a-v2", toml);
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        (tmp, sidecars)
    }

    const DIFF_ANCHOR: &str = "<div data-listing-diff-left=\"a-v1\" data-listing-diff-right=\"a-v2\" aria-hidden=\"true\"></div>\n";

    /// A diff of `a-v1` against `a-v2` as the diff splicer emits it. Block
    /// rows: 4 is context (old 1, new 1), 5 removed (old 2), 6 added
    /// (new 2), 7 context (old 3, new 3).
    fn small_diff() -> String {
        format!(
            "```diff\n--- a-v1\n+++ a-v2\n@@ -1,3 +1,3 @@\n fn f() {{\n-    old();\n+    new();\n }}\n```\n{DIFF_ANCHOR}"
        )
    }

    #[test]
    fn a_diff_sidecar_badges_an_added_a_removed_and_a_context_line() {
        let (_tmp, sidecars) = diff_sidecar(concat!(
            "[[callout]]\nline = 2\nlabel = \"added\"\n\n",
            "[[callout]]\nside = \"old\"\nline = 2\nlabel = \"removed\"\n\n",
            "[[callout]]\nside = \"old\"\nline = 3\nlabel = \"context\"\n",
        ));
        let out = splice_chapter(&small_diff(), SupportedRenderer::Html, &sidecars).unwrap();
        for (label, row) in [("added", "6"), ("removed", "5"), ("context", "7")] {
            assert_eq!(badge_line(&out, label), Some(row), "{label}; got:\n{out}");
        }
    }

    #[test]
    fn diff_sidecar_and_inline_callouts_number_in_row_order() {
        // The sidecar entry is old line 1 but sits on row 5, below the
        // inline marker on row 4.
        let (_tmp, sidecars) =
            diff_sidecar("[[callout]]\nside = \"old\"\nline = 1\nlabel = \"ctx\"\n");
        let content = format!(
            "```diff\n--- a-v1\n+++ a-v2\n@@ -1,1 +1,2 @@\n+// CALLOUT: inline X.\n fn f() {{\n```\n{DIFF_ANCHOR}\n\
             See {{{{#callout inline}}}} and {{{{#callout ctx}}}}.\n"
        );
        let out = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        for (label, ordinal) in [("inline", 1), ("ctx", 2)] {
            for attr in ["data-callout-badge", "data-callout-ref"] {
                assert!(
                    out.contains(&format!(
                        r#"{attr}="{label}" data-callout-ordinal="{ordinal}""#
                    )),
                    "{attr} {label} must be {ordinal}; got:\n{out}"
                );
            }
        }
    }

    #[test]
    fn a_diff_sidecar_line_shifts_past_a_stripped_context_marker() {
        let (_tmp, sidecars) = diff_sidecar("[[callout]]\nline = 2\nlabel = \"added\"\n");
        let content = format!(
            "```diff\n--- a-v1\n+++ a-v2\n@@ -1,3 +1,3 @@\n // CALLOUT: ctx-note Unchanged.\n-    old();\n+    new();\n }}\n```\n{DIFF_ANCHOR}"
        );
        let out = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        assert_eq!(badge_line(&out, "added"), Some("5"), "got:\n{out}");
    }

    #[test]
    fn a_diff_sidecar_entry_on_a_stripped_marker_line_fails() {
        let (_tmp, sidecars) = diff_sidecar("[[callout]]\nline = 1\nlabel = \"on-marker\"\n");
        let content = format!(
            "```diff\n--- a-v1\n+++ a-v2\n@@ -1,2 +1,2 @@\n // CALLOUT: ctx-note Unchanged.\n-    old();\n+    new();\n```\n{DIFF_ANCHOR}"
        );
        match splice_chapter(&content, SupportedRenderer::Html, &sidecars) {
            Err(SpliceError::SidecarLineOnStrippedMarker {
                label,
                listing_tag,
                source_line,
                ..
            }) => assert_eq!(
                (label.as_str(), listing_tag.as_str(), source_line),
                ("on-marker", "a-v2", 1),
                "the error names the line the author wrote, not its row"
            ),
            other => panic!("expected SidecarLineOnStrippedMarker, got {other:?}"),
        }
    }

    fn check_placements(chapters: &[&str], sidecars: &SidecarCallouts) -> Result<(), SpliceError> {
        let mut placements = DiffSidecarPlacements::default();
        for chapter in chapters {
            placements.record(chapter, sidecars)?;
        }
        placements.check(sidecars)
    }

    #[test]
    fn a_diff_sidecar_entry_that_no_diff_of_its_pair_shows_fails() {
        let (_tmp, sidecars) =
            diff_sidecar("[[callout]]\nside = \"old\"\nline = 40\nlabel = \"far\"\n");
        match check_placements(&[&small_diff()], &sidecars) {
            Err(SpliceError::DiffSidecarLineNotShown {
                label, side, line, ..
            }) => assert_eq!((label.as_str(), side, line), ("far", DiffSide::Old, 40)),
            other => panic!("expected DiffSidecarLineNotShown, got {other:?}"),
        }
    }

    #[test]
    fn the_not_shown_error_names_the_line_its_side_and_the_sidecar() {
        let (_tmp, sidecars) =
            diff_sidecar("[[callout]]\nside = \"old\"\nline = 40\nlabel = \"far\"\n");
        let msg = check_placements(&[&small_diff()], &sidecars)
            .unwrap_err()
            .to_string();
        for token in ["`far`", "40", "old", "a-v1..a-v2.callouts.toml"] {
            assert!(msg.contains(token), "missing {token}; got: {msg}");
        }
    }

    const FIRST_SLICE: &str = "```diff\n--- a-v1\n+++ a-v2\n@@ -2 +2 @@\n-x\n+y\n```\n";
    const SECOND_SLICE: &str = "```diff\n--- a-v1\n+++ a-v2\n@@ -20 +20 @@\n-p\n+q\n```\n";
    const TWO_ENTRIES: &str = concat!(
        "[[callout]]\nline = 2\nlabel = \"first\"\n\n",
        "[[callout]]\nline = 20\nlabel = \"second\"\n",
    );

    #[test]
    fn a_diff_sidecar_entry_shown_by_one_of_two_slices_of_its_pair_renders_there() {
        let (_tmp, sidecars) = diff_sidecar(TWO_ENTRIES);
        let content = format!("{FIRST_SLICE}{DIFF_ANCHOR}\n{SECOND_SLICE}{DIFF_ANCHOR}");
        let out = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        for label in ["first", "second"] {
            assert_eq!(
                out.matches(&format!(r#"data-callout-badge="{label}""#))
                    .count(),
                1,
                "{label} must badge only the slice that shows it; got:\n{out}"
            );
        }
    }

    #[test]
    fn slices_of_a_pair_in_different_chapters_place_its_entries_between_them() {
        let (_tmp, sidecars) = diff_sidecar(TWO_ENTRIES);
        let first = format!("{FIRST_SLICE}{DIFF_ANCHOR}");
        let second = format!("{SECOND_SLICE}{DIFF_ANCHOR}");
        check_placements(&[&first, &second], &sidecars)
            .expect("each entry is placed by one chapter's slice");
    }

    #[test]
    fn a_book_that_never_diffs_the_pair_leaves_its_sidecar_unchecked() {
        let (_tmp, sidecars) = diff_sidecar("[[callout]]\nline = 40\nlabel = \"far\"\n");
        check_placements(&["```rust\nfn f() {}\n```\n"], &sidecars)
            .expect("no diff of the pair, so nothing to place");
    }

    #[test]
    fn a_diff_with_a_live_operand_takes_no_sidecar() {
        // The anchor names every `live:` operand `live`, whatever its path.
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "live..a-v2",
            "[[callout]]\nline = 2\nlabel = \"stray\"\n",
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = small_diff().replace(
            "data-listing-diff-left=\"a-v1\"",
            "data-listing-diff-left=\"live\"",
        );
        let out = splice_chapter(&content, SupportedRenderer::Html, &sidecars).unwrap();
        assert!(!out.contains("stray"), "got:\n{out}");
        check_placements(&[&content], &sidecars).expect("a live diff places nothing to check");
    }

    #[test]
    fn a_diff_sidecar_label_that_an_inline_marker_in_the_diff_uses_fails() {
        let (_tmp, sidecars) =
            diff_sidecar("[[callout]]\nside = \"old\"\nline = 2\nlabel = \"twice\"\n");
        let content = format!(
            "```diff\n--- a-v1\n+++ a-v2\n@@ -1,2 +1,3 @@\n fn f() {{\n-    old();\n+    // CALLOUT: twice New call.\n+    new();\n```\n{DIFF_ANCHOR}"
        );
        match splice_chapter(&content, SupportedRenderer::Html, &sidecars) {
            Err(SpliceError::LabelCollision { label, .. }) => assert_eq!(label, "twice"),
            other => panic!("expected LabelCollision, got {other:?}"),
        }
    }

    #[test]
    fn a_diff_sidecar_does_not_attach_to_an_include_of_either_side() {
        let (_tmp, sidecars) = diff_sidecar("[[callout]]\nline = 1\nlabel = \"diff-only\"\n");
        let content = concat!(
            "```rust\nfn f() {\n```\n",
            "<div data-listing-tag=\"a-v2\" aria-hidden=\"true\"></div>\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap();
        assert!(!out.contains("diff-only"), "got:\n{out}");
    }

    #[test]
    fn a_diff_sidecar_entry_renders_as_a_pdf_note_and_a_prose_cross_ref_finds_it() {
        let (_tmp, sidecars) = diff_sidecar(
            "[[callout]]\nside = \"old\"\nline = 2\nlabel = \"removed\"\nbody = \"Gone now.\"\n",
        );
        let content = format!("{}\nSee {{{{#callout removed}}}}.\n", small_diff());
        let out = splice_chapter(&content, SupportedRenderer::TypstPdf, &sidecars).unwrap();
        assert!(
            out.contains("Gone now."),
            "PDF note must carry the body; got:\n{out}"
        );
        assert!(
            out.contains("See **[1]**."),
            "cross-ref must resolve; got:\n{out}"
        );
    }
}
