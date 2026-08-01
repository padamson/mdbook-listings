use std::collections::{HashMap, HashSet};

use crate::fence::FencedBlocks;

use super::{
    Callout, CalloutRef, SidecarCallouts, SpliceError, SupportedRenderer,
    listing_number_after_fence, scoped_badge, split_callouts_for_block,
};

/// PDF splicer (slice 6 shape): keep the marker comment visible in the
/// rendered listing, append a markdown blockquote summarising each
/// callout below the block. Slice 8 will pivot this to strip + inline
/// badge marker.
pub(super) fn splice_callout_lists_pdf(
    content: &str,
    label_to_ordinal: &HashMap<String, CalloutRef>,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut emitted_anchor: HashSet<String> = HashSet::new();
    for block in FencedBlocks::new(content) {
        let (inline, sidecar) =
            split_callouts_for_block(block.info, block.body, content, block.close_end, sidecars)?;
        // PDF path doesn't strip markers (it keeps them visible in the
        // listing), so sidecar entries' source lines are also their
        // post-strip lines — no translation needed. Just merge and
        // sort by line for the blockquote ordering.
        let mut callouts = inline;
        callouts.extend(sidecar);
        callouts.sort_by_key(|c| c.line);
        if !callouts.is_empty() {
            let listing_number = listing_number_after_fence(content, block.close_end);
            out.push_str(&content[cursor..block.close_end]);
            out.push('\n');
            out.push_str(&render_callout_list(
                &callouts,
                label_to_ordinal,
                listing_number.as_deref(),
                &mut emitted_anchor,
                SupportedRenderer::TypstPdf,
            ));
            out.push('\n');
            cursor = block.close_end;
        }
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

/// PDF-only dispatch from the slice 6 splicer path. The HTML splicer
/// uses [`render_callout_overlay_html`] directly because it also rewrites
/// the listing body, not just the trailing block.
fn render_callout_list(
    callouts: &[Callout],
    _label_to_ordinal: &HashMap<String, CalloutRef>,
    listing_number: Option<&str>,
    _emitted_anchor: &mut HashSet<String>,
    renderer: SupportedRenderer,
) -> String {
    match renderer {
        SupportedRenderer::Html => unreachable!("HTML uses render_callout_overlay_html directly"),
        SupportedRenderer::TypstPdf => render_callout_list_pdf(callouts, listing_number),
    }
}

// CALLOUT: pdf-emit Markdown blockquote with bold ordinal + label, one paragraph per callout. typst-pdf renders this as a quoted note block; bodyless markers render as just the label.
fn render_callout_list_pdf(callouts: &[Callout], listing_number: Option<&str>) -> String {
    let mut s = String::new();
    for (idx, c) in callouts.iter().enumerate() {
        let badge = scoped_badge(listing_number, idx + 1);
        if idx > 0 {
            s.push_str("> \n");
        }
        match &c.body {
            Some(body) => {
                s.push_str(&format!("> **[{badge}] {}** — {body}\n", c.label));
            }
            None => {
                s.push_str(&format!("> **[{badge}] {}**\n", c.label));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callout::splice_chapter;

    #[test]
    fn splice_chapter_pdf_picks_up_callouts_from_added_diff_lines_only() {
        // The PDF emitter emits per-block callouts for diff fences as a
        // markdown blockquote (slice 6 shape). Like the HTML path, only
        // added (`+`) markers earn an entry: a context-line marker is an
        // unchanged callout (already noted on the full include) and a
        // removed one is gone in the new state.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,3 +1,4 @@\n",
            " fn unchanged() {}\n",
            "-fn removed() {}\n",
            "+// CALLOUT: added-marker Body for a freshly added marker.\n",
            " // CALLOUT: context-marker Body for a marker that survived the diff.\n",
            "```\n",
        );
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        // `context-marker` still appears inside the diff fence itself (the
        // PDF emitter doesn't strip diff content); we only need the
        // appended blockquote to omit it.
        let blockquote = out.split("```\n\n").nth(1).unwrap_or("");
        assert!(
            blockquote.contains("[1] added-marker"),
            "added line marker should render in pdf blockquote; got:\n{out}",
        );
        assert!(
            !blockquote.contains("context-marker"),
            "context line marker should not render in pdf blockquote; got:\n{blockquote}",
        );
    }

    #[test]
    fn splice_chapter_pdf_skips_callouts_on_removed_diff_lines() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1 +1 @@\n",
            "-// CALLOUT: gone-marker This callout was removed.\n",
            "+// CALLOUT: kept-marker This one stays.\n",
            "```\n",
        );
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        // `gone-marker` will still appear inside the diff fence itself
        // (PDF emitter doesn't strip diff content); we only need the
        // appended blockquote to omit it.
        let blockquote = out.split("```\n\n").nth(1).unwrap_or("");
        assert!(blockquote.contains("[1] kept-marker"));
        assert!(
            !blockquote.contains("gone-marker"),
            "removed-line markers should not render in the appended blockquote; got:\n{blockquote}",
        );
    }

    #[test]
    fn splice_chapter_pdf_renderer_emits_blockquote_per_callout_list() {
        let content = concat!(
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "# CALLOUT: anchor-only\n",
            "```\n",
        );
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(
            !out.contains("<dl"),
            "PDF renderer must not emit raw HTML; got:\n{out}",
        );
        assert!(
            out.contains("> **[1] greeting** — Says hello."),
            "expected blockquote with bold ordinal+label and body; got:\n{out}",
        );
        assert!(
            out.contains("> **[2] anchor-only**"),
            "expected label-only callout to render with just label; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_pdf_renderer_replaces_cross_ref_with_bracketed_badge() {
        let content = concat!(
            "Reference {{#callout greeting}} here.\n\n",
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "```\n",
        );
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
        assert!(
            out.contains("**[1]**"),
            "expected bracketed bold ordinal in prose; got:\n{out}",
        );
        assert!(
            !out.contains("<a "),
            "PDF renderer must not emit raw HTML anchor; got:\n{out}",
        );
    }
}
