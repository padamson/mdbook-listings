use std::collections::HashSet;

use crate::fence::FencedBlocks;

use super::strip::{
    listing_anchor_after_fence, source_line_to_block_line, strip_marker_lines,
    strip_marker_lines_diff, translate_sidecar_line_to_post_strip,
};
use super::{
    Callout, SidecarCallouts, SpliceError, listing_number_after_fence, scoped_badge,
    split_callouts_for_block,
};

/// HTML splicer: for non-diff fenced blocks with markers, strip the marker
/// comment lines from the rendered listing and append a sibling
/// `<div class="callout-overlay">` carrying one interactive `<button>` +
/// hover-popover `<div>` per marker, each tagged with the post-strip
/// `data-callout-line` so CSS can position it on the line that previously
/// held the marker. Diff fences pass through unchanged — diffs show
/// history, the canonical anchor lives on the include's badge.
pub(super) fn splice_callout_lists_html(
    content: &str,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut emitted_anchor: HashSet<String> = HashSet::new();
    for block in FencedBlocks::new(content) {
        let (inline, sidecar) =
            split_callouts_for_block(block.info, block.body, content, block.close_end, sidecars)?;
        let is_diff = block.info == "diff";
        // Diff blocks always go through the strip pass even when no `+`/` `
        // callouts exist — `-`-side markers still need to be dropped from
        // the rendered body.
        if inline.is_empty() && sidecar.is_empty() && !is_diff {
            continue;
        }
        let strip = if is_diff {
            strip_marker_lines_diff(block.body)
        } else {
            strip_marker_lines(block.body, block.info)
        };
        if is_diff && inline.is_empty() && sidecar.is_empty() && strip.body == block.body {
            // No-op diff: no markers of any kind to rewrite.
            continue;
        }
        // Pair each inline callout with its already-computed post-strip
        // line, then add each sidecar callout. Sidecar lines are
        // SOURCE-file lines; translate via the anchor's range info
        // (if any) into block-body lines, then strip-aware translate
        // into post-strip lines. Sort by post-strip position so badges
        // emit in visual reading order.
        let mut positioned: Vec<(Callout, usize)> = inline
            .into_iter()
            .zip(strip.post_strip_lines.iter().copied())
            .collect();
        let anchor = listing_anchor_after_fence(content, block.close_end);
        let sidecar_path = anchor.as_ref().and_then(|a| sidecars.path_for_tag(a.tag));
        for entry in sidecar {
            let source_line = entry.line;
            let label = entry.label.clone();
            let block_line = match anchor.as_ref() {
                Some(a) => source_line_to_block_line(source_line, a),
                None => source_line,
            };
            let p = translate_sidecar_line_to_post_strip(
                block_line,
                &strip.stripped_source_lines,
                anchor.as_ref().map(|a| a.tag).unwrap_or(""),
                sidecar_path,
                &label,
                source_line,
            )?;
            positioned.push((entry, p));
        }
        positioned.sort_by_key(|(_, p)| *p);
        let (callouts, post_strip_lines): (Vec<_>, Vec<_>) = positioned.into_iter().unzip();
        let pre_fence = &content[cursor..block.body_start];
        let close_fence_line = closing_fence_text(content, block.close_end);
        out.push_str(pre_fence);
        out.push_str(&strip.body);
        if !strip.body.is_empty() && !strip.body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(close_fence_line);
        out.push('\n');
        let listing_number = listing_number_after_fence(content, block.close_end);
        out.push_str(&render_callout_overlay_html(
            &callouts,
            &post_strip_lines,
            strip.total_lines,
            listing_number.as_deref(),
            &mut emitted_anchor,
        ));
        out.push('\n');
        cursor = block.close_end;
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

fn closing_fence_text(content: &str, close_end: usize) -> &str {
    // close_end is one past the trailing newline of the closing fence
    // (or equal to bytes.len() if the file ends without a trailing newline).
    let end = close_end.saturating_sub(1);
    let slice = &content[..end];
    let line_start = slice.rfind('\n').map(|i| i + 1).unwrap_or(0);
    &content[line_start..end]
}

/// HTML overlay (slice 7 shape): one interactive `<button>` per marker
/// laid out in an absolutely-positioned overlay sibling of the rendered
/// listing. `data-callout-line` (1-based, post-strip) lets CSS or JS
/// position each badge on the line that previously held the marker
/// comment. The body lives in a sibling `<div>` shown on hover/focus
/// via the bundled mdbook-listings.css.
// CALLOUT: html-overlay One button per marker, each tagged with post-strip line; body renders in a hover-popover sibling div for label-bearing markers and is omitted entirely when body is None.
fn render_callout_overlay_html(
    callouts: &[Callout],
    post_strip_lines: &[usize],
    total_lines: usize,
    listing_number: Option<&str>,
    emitted_anchor: &mut HashSet<String>,
) -> String {
    let mut s = String::new();
    s.push_str("<div class=\"callout-overlay\" data-callout-overlay>\n");
    for (idx, c) in callouts.iter().enumerate() {
        let ordinal = idx + 1;
        let badge = scoped_badge(listing_number, ordinal);
        let label_esc = html_escape(&c.label);
        let line = post_strip_lines.get(idx).copied().unwrap_or(1);
        // CALLOUT: body-id-dedup The button id, body div id, and the button's `aria-describedby` are all dedup'd in lockstep on the first occurrence per label. Without lockstep dedup, the same label appearing in two blocks (e.g. an include and a diff `+` line, both processed for badges in slice 8) would emit duplicate body div ids — invalid HTML, rejected by playwright-rs's strict-mode locator.
        let is_first_occurrence = emitted_anchor.insert(c.label.clone());
        let id_attr = if is_first_occurrence {
            format!(" id=\"callout-{label_esc}\"")
        } else {
            String::new()
        };
        // The body div's `id` and the button's `aria-describedby` are
        // dedup'd identically: only the first occurrence per label gets
        // them. Subsequent occurrences still hover-reveal (CSS uses the
        // adjacent-sibling combinator inside .callout-entry, not the id),
        // but cannot be cross-referenced from prose — by design, since
        // `{{#callout LABEL}}` resolves to the canonical first-occurrence
        // anchor.
        let body_id_attr = if is_first_occurrence {
            format!(" id=\"callout-body-{label_esc}\"")
        } else {
            String::new()
        };
        let aria_describedby_attr = if is_first_occurrence {
            format!(" aria-describedby=\"callout-body-{label_esc}\"")
        } else {
            String::new()
        };
        // Per-callout author override: `--align=left` on the marker
        // surfaces as `data-callout-align="left"`, letting the runtime
        // JS skip its viewport-aware detection and pin the popover left
        // (over the listing) regardless of available right-side gutter.
        let align_attr = match c.options.get("align") {
            Some(value) if value == "left" || value == "right" => {
                format!(" data-callout-align=\"{value}\"")
            }
            _ => String::new(),
        };
        s.push_str(&format!(
            "  <div class=\"callout-entry\" data-callout-line=\"{line}\"{align_attr} \
             style=\"--callout-line: {line}; --callout-listing-lines: {total_lines};\">\n",
        ));
        s.push_str(&format!(
            "    <button type=\"button\" class=\"callout-badge\"{id_attr} \
             data-callout-badge=\"{label_esc}\" data-callout-ordinal=\"{ordinal}\"\
             {aria_describedby_attr}>{badge}</button>\n",
        ));
        if let Some(body) = &c.body {
            s.push_str(&format!(
                "    <div class=\"callout-body\"{body_id_attr} role=\"tooltip\">{}</div>\n",
                render_inline_markdown(body),
            ));
        }
        s.push_str("  </div>\n");
    }
    s.push_str("</div>");
    s
}

// CALLOUT: html-escape Standard HTML escapes plus `{` → `&#123;` so a callout body that documents a `{{#callout LABEL}}` or `{{#diff a b}}` directive (rendered into the overlay HTML, which sits OUTSIDE its fenced code block) doesn't get its example syntax mistaken for a real directive by the cross-ref scanner downstream. The browser still renders `&#123;&#123;` as `{{` visually.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('{', "&#123;")
}

// Render `body` as inline markdown (backticks → <code>, *em*, **strong**,
// [text](url)) for emission into the callout overlay popover.
fn render_inline_markdown(body: &str) -> String {
    use pulldown_cmark::{Event, Parser, html};
    // CALLOUT: raw-html-neutralisation Callout bodies come from code comments, not trusted markdown — `<script>` in a YAML comment must render as `&lt;script&gt;`, not execute. Remapping every `Event::Html`/`Event::InlineHtml` to `Event::Text` forces raw HTML through pulldown-cmark's text-escaping path.
    let parser = Parser::new(body).map(|event| match event {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        other => other,
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);
    let trimmed = rendered.trim_end_matches('\n');
    // CALLOUT: inline-only-output pulldown-cmark wraps inline content in a single `<p>...</p>`. Callout bodies are inline annotations — the synthetic paragraph would break popover layout — so we strip it. Block-level markdown still parses but won't strip cleanly; that's a deliberate cue that the body shape isn't right for the construct.
    let stripped = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
        .unwrap_or(trimmed);
    // CALLOUT: curly-brace-escape pulldown-cmark escapes `&`, `<`, `>`, `"` for text but leaves `{` alone. The cross-ref scanner downstream looks for `{{...}}`; breaking the opening `{{` is enough to neutralise it, matching the pre-slice `html_escape` behaviour (only `{` was ever escaped — `}` always survived).
    stripped.replace('{', "&#123;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callout::{SupportedRenderer, splice_chapter};

    #[test]
    fn splice_chapter_html_strips_markers_and_emits_overlay_with_badges() {
        let content = concat!(
            "Before paragraph.\n\n",
            "```yaml\n",
            "service: greeting\n",
            "# CALLOUT: greeting-name The service identifier.\n",
            "endpoint: /hello\n",
            "# CALLOUT: endpoint-path\n",
            "```\n\n",
            "After paragraph.\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(out.contains("Before paragraph.\n"));
        assert!(out.contains("After paragraph.\n"));
        assert!(
            !out.contains("# CALLOUT:"),
            "marker comment line must be stripped from rendered listing; got:\n{out}",
        );
        assert!(
            out.contains("<div class=\"callout-overlay\""),
            "expected overlay sibling div; got:\n{out}",
        );
        assert!(out.contains("data-callout-badge=\"greeting-name\""));
        assert!(out.contains("data-callout-ordinal=\"1\""));
        assert!(out.contains("data-callout-badge=\"endpoint-path\""));
        assert!(out.contains("data-callout-ordinal=\"2\""));
        assert!(
            out.contains("<div class=\"callout-body\"") && out.contains("The service identifier."),
            "expected body popover for marker with body; got:\n{out}",
        );
        assert!(
            !out.contains("id=\"callout-body-endpoint-path\""),
            "label-only callout should have no body popover; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_leaves_block_alone_when_no_markers_present() {
        let content = "```yaml\nservice: greeting\nendpoint: /hello\n```\n";
        assert_eq!(
            splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
                .expect("splice"),
            content
        );
    }

    #[test]
    fn splice_chapter_skips_block_with_unknown_language() {
        let content = "```\n# CALLOUT: anchor body text\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(!out.contains("data-callout-badge"));
    }

    #[test]
    fn splice_chapter_handles_two_blocks_independently_for_per_listing_numbering() {
        let content = "\
            ```yaml\n\
            # CALLOUT: a-one\n\
            ```\n\n\
            ```rust\n\
            // CALLOUT: b-one\n\
            // CALLOUT: b-two\n\
            ```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(out.contains("data-callout-badge=\"a-one\""));
        assert!(out.contains("data-callout-badge=\"b-one\""));
        assert!(out.contains("data-callout-badge=\"b-two\""));
        let a_one_ordinal = out
            .split("data-callout-badge=\"a-one\"")
            .nth(1)
            .and_then(|s| s.split("data-callout-ordinal=\"").nth(1))
            .unwrap_or("");
        assert!(
            a_one_ordinal.starts_with("1\""),
            "first listing's first marker should be ordinal 1; got prefix {}",
            &a_one_ordinal[..a_one_ordinal.len().min(10)],
        );
        let b_two_ordinal = out
            .split("data-callout-badge=\"b-two\"")
            .nth(1)
            .and_then(|s| s.split("data-callout-ordinal=\"").nth(1))
            .unwrap_or("");
        assert!(
            b_two_ordinal.starts_with("2\""),
            "second listing's second marker should be ordinal 2; got prefix {}",
            &b_two_ordinal[..b_two_ordinal.len().min(10)],
        );
    }

    #[test]
    fn splice_chapter_html_strips_added_marker_lines_from_diff_and_emits_badge() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,1 +1,2 @@\n",
            " fn unchanged() {}\n",
            "+// CALLOUT: added-marker Body for an added marker.\n",
            "+fn added() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            !out.contains("// CALLOUT: added-marker"),
            "added marker comment line should be stripped from rendered diff; got:\n{out}",
        );
        assert!(
            out.contains("data-callout-badge=\"added-marker\""),
            "expected badge for the added marker; got:\n{out}",
        );
        assert!(
            out.contains("+fn added() {}"),
            "non-marker `+` line should survive; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_strips_context_marker_lines_from_diff_without_badge() {
        // An unchanged callout on a context line is noise in a diff: the
        // comment is still stripped (it never shows as raw text), but no
        // badge renders — the callout is already badged wherever the
        // listing is shown in full via `{{#include}}`.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,2 @@\n",
            " // CALLOUT: unchanged-marker A marker carried over unchanged.\n",
            " fn carried() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            !out.contains("// CALLOUT: unchanged-marker"),
            "context marker comment line should still be stripped; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-badge=\"unchanged-marker\""),
            "context-line marker must not produce a badge; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_badges_changed_callout_on_unchanged_code_line() {
        // A callout whose body is edited while the code line it annotates
        // stays the same shows up as a `-old`/`+new` marker pair with the
        // code line as context. The `+` marker still earns a badge, landing
        // on the unchanged code line — a *changed* callout is not lost.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,2 @@\n",
            "-// CALLOUT: edited-marker Old body.\n",
            "+// CALLOUT: edited-marker New body.\n",
            " fn unchanged() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("data-callout-badge=\"edited-marker\""),
            "edited callout should still render a badge; got:\n{out}",
        );
        assert!(
            !out.contains("Old body."),
            "the removed (`-`) side's old body should not render; got:\n{out}",
        );
        assert!(
            out.contains("New body."),
            "the added (`+`) side's new body should render; got:\n{out}",
        );
        assert!(
            out.contains("+fn unchanged() {}") || out.contains(" fn unchanged() {}"),
            "the unchanged code line should survive in the diff; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_drops_removed_marker_lines_from_diff_with_no_badge() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,1 @@\n",
            "-// CALLOUT: gone-marker Removed in this slice.\n",
            " fn unchanged() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            !out.contains("// CALLOUT: gone-marker"),
            "removed marker comment line should be dropped, not visible; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-badge=\"gone-marker\""),
            "removed-side marker must not produce a badge; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_diff_badge_ordinals_skip_suppressed_context_markers() {
        // A context-line callout above an added-line callout no longer
        // consumes an ordinal: the added marker is the only rendered badge,
        // so it numbers 1, not 2.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,3 @@\n",
            " // CALLOUT: ctx-marker An unchanged callout above.\n",
            " fn carried() {}\n",
            "+// CALLOUT: new-marker A freshly added callout below.\n",
            "+fn added() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("data-callout-badge=\"new-marker\" data-callout-ordinal=\"1\""),
            "added marker should be ordinal 1 once the context marker is suppressed; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-badge=\"ctx-marker\""),
            "context marker must not render a badge; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_dedups_body_id_when_label_appears_in_two_blocks() {
        // The button id and the body div id are dedup'd in lockstep: the
        // first occurrence per label gets `id="callout-LABEL"` AND
        // `id="callout-body-LABEL"`; subsequent occurrences emit neither.
        // Otherwise the rendered HTML would have duplicate ids and the
        // browser's strict-mode locator would refuse to resolve the body.
        let content = concat!(
            "```rust\n",
            "// CALLOUT: shared-label First body.\n",
            "fn one() {}\n",
            "```\n\n",
            "```rust\n",
            "// CALLOUT: shared-label Second body.\n",
            "fn two() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let id_count = out.matches("id=\"callout-shared-label\"").count();
        let body_id_count = out.matches("id=\"callout-body-shared-label\"").count();
        assert_eq!(
            id_count, 1,
            "expected exactly one id=\"callout-shared-label\"; got {id_count} in:\n{out}",
        );
        assert_eq!(
            body_id_count, 1,
            "expected exactly one id=\"callout-body-shared-label\"; got {body_id_count} in:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_dedups_id_when_label_appears_in_diff_then_include() {
        // First non-empty fenced block to contain a label gets the
        // `id="callout-LABEL"` anchor. Subsequent occurrences (same label
        // in another block) emit the badge but skip the id so the HTML
        // stays valid (no duplicate IDs).
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1 +1,2 @@\n",
            " fn unchanged() {}\n",
            "+// CALLOUT: same-label First occurrence is in a diff.\n",
            "```\n\n",
            "```rust\n",
            "// CALLOUT: same-label Second occurrence is in an include.\n",
            "fn body() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let id_count = out.matches("id=\"callout-same-label\"").count();
        assert_eq!(
            id_count, 1,
            "expected exactly one id=\"callout-same-label\" across the chapter; got {id_count} in:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_does_not_close_outer_fence_on_shorter_inner_fence() {
        let content = concat!(
            "````rust\n",
            "let s = \"```yaml\\n# CALLOUT: not-real-marker\\n```\";\n",
            "// CALLOUT: real-marker This one should be picked up.\n",
            "````\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("data-callout-badge=\"real-marker\""),
            "expected the marker outside the embedded ```yaml string to render; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-badge=\"not-real-marker\""),
            "the marker inside the embedded string is YAML, not Rust — and the outer fence is rust; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_escapes_curly_braces_in_body_to_protect_cross_ref_scanner() {
        // A callout body that documents the `{{#callout LABEL}}` syntax
        // would, post-overlay-emit, land OUTSIDE its fenced code block
        // — the overlay div is a sibling of the pre. Without escaping,
        // the cross-ref scanner downstream sees the literal directive
        // text and tries to resolve `LABEL`, failing the build.
        let content =
            "```rust\n// CALLOUT: lbl Authors write `{{#callout LABEL}}` to cross-ref.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = out
            .split("<div class=\"callout-body\"")
            .nth(1)
            .unwrap_or("")
            .split("</div>")
            .next()
            .unwrap_or("");
        assert!(
            body.contains("&#123;&#123;#callout LABEL"),
            "expected `{{` escaped to `&#123;` so the cross-ref scanner can't see it; got body:\n{body}",
        );
        assert!(
            !body.contains("{{#callout LABEL"),
            "raw `{{#callout LABEL}}` must not survive into the overlay body; got body:\n{body}",
        );
    }

    #[test]
    fn splice_chapter_html_escapes_label_and_body() {
        let content = "```yaml\n# CALLOUT: lbl Body with <script> in it.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        // Scope the check to the rendered callout-body div, since the
        // overlay is now followed by a measurement <script> emitted by
        // the splicer itself (not user content).
        let body = out
            .split("<div class=\"callout-body\"")
            .nth(1)
            .unwrap_or("")
            .split("</div>")
            .next()
            .unwrap_or("");
        assert!(
            body.contains("&lt;script&gt;"),
            "callout body must escape user-supplied <script>; got:\n{body}",
        );
        assert!(
            !body.contains("<script>"),
            "callout body must not contain raw <script>; got:\n{body}",
        );
    }

    fn extract_callout_body(out: &str) -> &str {
        out.split("<div class=\"callout-body\"")
            .nth(1)
            .unwrap_or("")
            .split("</div>")
            .next()
            .unwrap_or("")
    }

    #[test]
    fn callout_body_renders_inline_backticks_as_code_spans() {
        let content =
            "```rust\n// CALLOUT: lbl Read the `PORT` env var, fall back to `3000`.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<code>PORT</code>") && body.contains("<code>3000</code>"),
            "expected backticks rendered as <code> spans; got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_renders_strong_and_emphasis() {
        let content = "```rust\n// CALLOUT: lbl A **bold** and *italic* note.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<strong>bold</strong>") && body.contains("<em>italic</em>"),
            "expected **/* rendered as <strong>/<em>; got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_renders_inline_link() {
        let content = "```rust\n// CALLOUT: lbl See [docs](https://example.com/).\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<a href=\"https://example.com/\">docs</a>"),
            "expected [text](url) rendered as anchor; got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_curly_brace_escape_survives_inside_code_span() {
        // Authors documenting the `{{#callout LABEL}}` directive will
        // wrap it in backticks for clarity. The inline-markdown render
        // must produce <code>...</code>, AND the `{` escape must still
        // apply inside that code span so the cross-ref scanner downstream
        // (which searches for `{{...}}`) doesn't see a real directive.
        // Only `{` needs escaping — breaking the opening `{{` is
        // sufficient; trailing `}}` survives, matching pre-markdown behaviour.
        let content =
            "```rust\n// CALLOUT: lbl Authors write `{{#callout LABEL}}` to cross-ref.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<code>&#123;&#123;#callout LABEL}}</code>"),
            "expected `{{` escaped inside <code> (and `}}` left as-is, matching old behaviour); got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_plain_text_passes_through_unchanged() {
        let content = "```rust\n// CALLOUT: lbl Just a plain sentence with no markup.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("role=\"tooltip\">Just a plain sentence with no markup."),
            "plain body must follow the opening tag directly (no <p> wrapper); got body:\n{body}",
        );
    }

    #[test]
    fn splice_chapter_emits_id_only_on_first_occurrence_of_repeated_label() {
        // Same source file shown via {{#include}} and {{#diff}} produces two
        // dl entries for the same label; only the first carries id="callout-X"
        // so the rendered HTML stays valid (no duplicate IDs).
        let content = concat!(
            "```rust\n",
            "// CALLOUT: same Body.\n",
            "```\n\n",
            "```diff\n",
            "+// CALLOUT: same Body.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let id_count = out.matches("id=\"callout-same\"").count();
        assert_eq!(
            id_count, 1,
            "expected exactly one id=\"callout-same\"; got {id_count} in:\n{out}",
        );
    }

    #[test]
    fn render_callout_overlay_html_emits_data_callout_align_when_align_option_set() {
        // The HTML emission side: an `--align=left` option on a callout
        // must surface as a `data-callout-align="left"` attribute on the
        // entry so the runtime JS knows to skip the viewport-aware
        // auto-detection and pin the popover left.
        let content =
            "```yaml\n# CALLOUT: pinned-left --align=left A body that should open left.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains(r#"data-callout-align="left""#),
            "entry must carry data-callout-align=\"left\" when the option is set; got:\n{out}",
        );
    }

    #[test]
    fn render_callout_overlay_html_omits_data_callout_align_when_no_option() {
        // The negative case: a callout WITHOUT --align=... gets no data
        // attribute. The runtime JS then uses viewport-aware detection.
        let content = "```yaml\n# CALLOUT: regular A body with default alignment.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            !out.contains("data-callout-align"),
            "entry must not carry data-callout-align when --align is not set; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_does_not_double_newline_when_rewritten_body_ends_with_newline() {
        let content = "```rust\n// CALLOUT: foo bar.\nlet x = 1;\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        // Find the position right before the closing fence. The
        // rewritten body must end with exactly one `\n`, then `` ``` ``.
        assert!(
            !out.contains("\n\n```\n"),
            "must not emit a blank line between body and closing fence; got:\n{out}",
        );
        assert!(
            out.contains("let x = 1;\n```\n"),
            "expected body line immediately followed by closing fence; got:\n{out}",
        );
    }

    /// When every line is a marker, the rewritten body is empty. The
    /// guard must not emit a stray `\n` between an empty body and the
    /// closing fence.
    #[test]
    fn splice_chapter_does_not_emit_newline_when_rewritten_body_is_empty() {
        let content = "```rust\n// CALLOUT: only-marker body.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("```rust\n```\n"),
            "expected fence-open immediately followed by fence-close (empty body); got:\n{out}",
        );
    }
}
