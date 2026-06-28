//! Numbering pass: labels each of a chapter's listings `Listing N.M` and
//! renders a caption line before it. Runs after the include and diff passes
//! (so every numbered listing already carries a locator anchor) and before
//! the callout pass (which reads the `data-listing-number` this pass writes
//! onto each anchor to scope its badges).
//!
//! M is the listing's 1-based position among the chapter's numbered listings
//! in document order — the order their locator anchors appear, the only point
//! where include and diff listings are interleaved in one stream. N is the
//! chapter's dotted section number.

use crate::callout::SupportedRenderer;
use crate::fence::FencedBlocks;

/// A numbered listing's locator anchor.
struct Anchor {
    /// Byte offset of the anchor's opening `<div`.
    div_start: usize,
    /// The `data-listing-caption` value, still HTML-escaped as stored on the
    /// anchor.
    caption: Option<String>,
}

/// A numbered listing, surfaced for the book-wide List-of-Listings index.
/// Carries the rendered number (`5.1`), the link-target id stamped onto its
/// caption div (`listing-5-1`), and the caption still HTML-escaped as stored
/// on the anchor.
pub struct ListingRef {
    pub number: String,
    pub caption: Option<String>,
    pub id: String,
}

/// Splice listing numbers and captions into `content`, returning the rewritten
/// content and the numbered listings it found, in document order, for the
/// List-of-Listings index.
///
/// `chapter_number` is the chapter's dotted section number (`[5]` → `5`,
/// `[5, 2]` → `5.2`); `None` for an unnumbered (draft/prefix) chapter.
/// `number_listings` is the `[preprocessor.listings] number-listings` opt-in.
/// A listing's number renders only when the flag is on and the chapter has a
/// number; its caption renders whenever one is present. A numbered listing's
/// caption div also gains an `id` so the index can link to it. When neither
/// piece applies to any listing, `content` is returned unchanged with no refs.
pub fn splice_chapter(
    content: &str,
    chapter_number: Option<&[u32]>,
    number_listings: bool,
    renderer: SupportedRenderer,
) -> (String, Vec<ListingRef>) {
    // (opener_start, anchor) for each block immediately followed by a locator
    // anchor, in document order. Plain code blocks and snippets have no anchor
    // and are not listings.
    let mut listings: Vec<(usize, Anchor)> = Vec::new();
    for block in FencedBlocks::new(content) {
        if let Some(anchor) = anchor_after_fence(content, block.close_end) {
            listings.push((opener_line_start(content, block.body_start), anchor));
        }
    }
    if listings.is_empty() {
        return (content.to_string(), Vec::new());
    }

    let prefix = chapter_number
        .filter(|n| !n.is_empty())
        .map(|n| n.iter().map(u32::to_string).collect::<Vec<_>>().join("."));

    // Each numbered listing contributes up to two edits: a caption element
    // inserted before its opening fence, and a `data-listing-number` attribute
    // spliced into its anchor. Both are pure insertions; collect them and
    // apply in ascending position order. A numbered listing also yields one
    // `ListingRef` for the index.
    let mut edits: Vec<(usize, String)> = Vec::new();
    let mut refs: Vec<ListingRef> = Vec::new();
    for (i, (opener_start, anchor)) in listings.iter().enumerate() {
        let number = match (&prefix, number_listings) {
            (Some(p), true) => Some(format!("{p}.{}", i + 1)),
            _ => None,
        };
        let id = number.as_deref().map(listing_id);
        if let Some(element) = render_caption(
            number.as_deref(),
            id.as_deref(),
            anchor.caption.as_deref(),
            renderer,
        ) {
            edits.push((*opener_start, element));
        }
        if let Some(n) = number {
            edits.push((
                anchor.div_start + "<div".len(),
                format!(" data-listing-number=\"{n}\""),
            ));
            refs.push(ListingRef {
                number: n,
                caption: anchor.caption.clone(),
                id: id.expect("a numbered listing always has an id"),
            });
        }
    }
    if edits.is_empty() {
        return (content.to_string(), refs);
    }
    edits.sort_by_key(|(pos, _)| *pos);

    let mut out = String::with_capacity(content.len() + edits.len() * 48);
    let mut cursor = 0;
    for (pos, text) in edits {
        out.push_str(&content[cursor..pos]);
        out.push_str(&text);
        cursor = pos;
    }
    out.push_str(&content[cursor..]);
    (out, refs)
}

/// The HTML link-target id for a numbered listing: `5.1` → `listing-5-1`.
fn listing_id(number: &str) -> String {
    format!("listing-{}", number.replace('.', "-"))
}

/// The visible caption line for a listing, or `None` when there is neither a
/// number nor a caption to show. HTML emits a `<div class="listing-caption">`;
/// the typst-pdf backend can't pass raw `<div>` through, so it gets a bold
/// markdown line instead. The caption arrives HTML-escaped (it round-trips
/// through an anchor attribute): correct as-is for HTML text, unescaped back
/// to source text for the PDF markdown line.
fn render_caption(
    number: Option<&str>,
    id: Option<&str>,
    caption_escaped: Option<&str>,
    renderer: SupportedRenderer,
) -> Option<String> {
    if number.is_none() && caption_escaped.is_none() {
        return None;
    }
    // The element is spliced above the opening fence, external to the
    // listing. The trailing blank line is load-bearing: without it the markdown
    // parser glues the `<div>` to the fence and renders it as escaped inline
    // text instead of a standalone block above the <pre>.
    match renderer {
        SupportedRenderer::Html => {
            let caption = caption_escaped.map(str::to_string);
            let text = label_text(number, caption.as_deref());
            // A numbered listing carries an id so the List-of-Listings index
            // can link to it; an unnumbered caption has no link target.
            let id_attr = id.map(|i| format!(" id=\"{i}\"")).unwrap_or_default();
            Some(format!(
                "<div class=\"listing-caption\"{id_attr}>{text}</div>\n\n"
            ))
        }
        SupportedRenderer::TypstPdf => {
            let caption = caption_escaped.map(html_unescape);
            let text = label_text(number, caption.as_deref());
            Some(format!("**{text}**\n\n"))
        }
    }
}

/// Join the optional `Listing N.M` label and the optional caption with an
/// em-dash, in whichever combination is present (the caller guarantees at
/// least one is).
pub(crate) fn label_text(number: Option<&str>, caption: Option<&str>) -> String {
    match (number, caption) {
        (Some(n), Some(c)) => format!("Listing {n} — {c}"),
        (Some(n), None) => format!("Listing {n}"),
        (None, Some(c)) => c.to_string(),
        (None, None) => String::new(),
    }
}

/// Find the locator anchor the include or diff splicer drops immediately past
/// a listing's closing fence. `None` for any other block. Tolerates the one
/// optional newline the splicers may place between the fence and the anchor.
fn anchor_after_fence(content: &str, close_end: usize) -> Option<Anchor> {
    let nl = usize::from(content[close_end..].starts_with('\n'));
    let div_start = close_end + nl;
    let tail = &content[div_start..];
    if !(tail.starts_with("<div data-listing-tag=\"")
        || tail.starts_with("<div data-listing-diff-left=\""))
    {
        return None;
    }
    // The whole anchor element is one line; bound the attribute search at the
    // `>` that closes the opening tag.
    let div_text = &tail[..tail.find('>')?];
    Some(Anchor {
        div_start,
        caption: attr_value(div_text, "data-listing-caption"),
    })
}

/// Read a `name="value"` attribute's value out of an element's opening tag.
fn attr_value(div_text: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = div_text.find(&key)? + key.len();
    let end = div_text[start..].find('"')?;
    Some(div_text[start..start + end].to_string())
}

/// Byte offset of the first character of the opener fence's line. `body_start`
/// is one past that line's trailing newline.
fn opener_line_start(content: &str, body_start: usize) -> usize {
    let newline = body_start.saturating_sub(1);
    content[..newline].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Reverse [`crate::callout::html_escape`]'s five entities. `&amp;` last so a
/// value that escaped to e.g. `&amp;lt;` restores to `&lt;`, not `<`.
fn html_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#123;", "{")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use SupportedRenderer::{Html, TypstPdf};

    /// A listing's code block plus the include anchor the include splicer
    /// drops past its closing fence.
    fn include_block(tag: &str, caption: Option<&str>) -> String {
        let cap = caption
            .map(|c| format!(" data-listing-caption=\"{c}\""))
            .unwrap_or_default();
        format!(
            "```rust\nfn {tag}() {{}}\n```\n<div data-listing-tag=\"{tag}\"{cap} aria-hidden=\"true\"></div>\n"
        )
    }

    /// A diff block plus the dual-attribute diff anchor (no trailing newline,
    /// as the diff splicer emits it).
    fn diff_block(left: &str, right: &str) -> String {
        format!(
            "```diff\n--- {left}\n+++ {right}\n-old\n+new\n```\n<div data-listing-diff-left=\"{left}\" data-listing-diff-right=\"{right}\" aria-hidden=\"true\"></div>"
        )
    }

    #[test]
    fn numbers_two_listings_in_document_order() {
        let content = format!(
            "intro\n\n{}\nmid\n\n{}\n",
            include_block("a", None),
            include_block("b", None)
        );
        let (out, _) = splice_chapter(&content, Some(&[5]), true, Html);
        assert!(
            out.contains(r#"<div class="listing-caption" id="listing-5-1">Listing 5.1</div>"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<div class="listing-caption" id="listing-5-2">Listing 5.2</div>"#),
            "{out}"
        );
    }

    #[test]
    fn interleaves_include_and_diff_anchors_in_one_sequence() {
        let content = format!("{}\n\n{}\n", include_block("a", None), diff_block("a", "b"));
        let (out, _) = splice_chapter(&content, Some(&[5]), true, Html);
        assert!(out.contains("Listing 5.1"), "include is 5.1; got:\n{out}");
        assert!(out.contains("Listing 5.2"), "diff is 5.2; got:\n{out}");
        // Both anchors carry the machine-readable number for the callout pass,
        // spliced just inside the opening `<div` so the element stays well-formed.
        assert!(
            out.contains(r#"<div data-listing-number="5.1" data-listing-tag="a""#),
            "number must land inside the include anchor; got:\n{out}",
        );
        assert!(
            out.contains(r#"<div data-listing-number="5.2" data-listing-diff-left="a""#),
            "number must land inside the diff anchor; got:\n{out}",
        );
    }

    #[test]
    fn subsection_number_prefixes_listing() {
        let content = include_block("a", None);
        let (out, _) = splice_chapter(&content, Some(&[5, 2]), true, Html);
        assert!(out.contains("Listing 5.2.1"), "got:\n{out}");
    }

    #[test]
    fn number_and_caption_join_with_em_dash() {
        let content = include_block("a", Some("The claim layer"));
        let (out, _) = splice_chapter(&content, Some(&[5]), true, Html);
        assert!(
            out.contains(
                r#"<div class="listing-caption" id="listing-5-1">Listing 5.1 — The claim layer</div>"#
            ),
            "got:\n{out}",
        );
    }

    #[test]
    fn flag_off_renders_caption_only_without_number_or_attribute() {
        let content = include_block("a", Some("Just a caption"));
        let (out, _) = splice_chapter(&content, Some(&[5]), false, Html);
        assert!(
            out.contains(r#"<div class="listing-caption">Just a caption</div>"#),
            "caption renders with the flag off; got:\n{out}",
        );
        assert!(
            !out.contains("Listing 5"),
            "no number with the flag off; got:\n{out}"
        );
        assert!(
            !out.contains("data-listing-number"),
            "no number attr with the flag off; got:\n{out}"
        );
    }

    #[test]
    fn flag_off_without_caption_is_byte_identical() {
        let content = include_block("a", None);
        let (out, _) = splice_chapter(&content, Some(&[5]), false, Html);
        assert_eq!(
            out, content,
            "flag off + no caption must pass through unchanged"
        );
    }

    #[test]
    fn flag_off_is_byte_identical_for_mixed_content_both_renderers() {
        // The non-breaking guarantee: with numbering off and no captions, the
        // pass touches nothing across a chapter mixing an include, a diff, a
        // plain (anchorless) code block, and prose — for both renderers.
        let content = concat!(
            "Intro prose.\n\n",
            "```rust\nfn a() {}\n```\n",
            "<div data-listing-tag=\"a\" aria-hidden=\"true\"></div>\n\n",
            "More prose.\n\n",
            "```diff\n--- a\n+++ b\n-old\n+new\n```\n",
            "<div data-listing-diff-left=\"a\" data-listing-diff-right=\"b\" aria-hidden=\"true\"></div>\n\n",
            "```rust\nlet plain = 1;\n```\n\n",
            "Tail.\n",
        );
        assert_eq!(splice_chapter(content, Some(&[5]), false, Html).0, content);
        assert_eq!(
            splice_chapter(content, Some(&[5]), false, TypstPdf).0,
            content
        );
    }

    #[test]
    fn unnumbered_chapter_renders_caption_only() {
        let content = include_block("a", Some("Caption"));
        let (out, _) = splice_chapter(&content, None, true, Html);
        assert!(
            out.contains(r#"<div class="listing-caption">Caption</div>"#),
            "got:\n{out}"
        );
        assert!(
            !out.contains("Listing"),
            "no number for an unnumbered chapter; got:\n{out}"
        );
        assert!(!out.contains("data-listing-number"), "got:\n{out}");
    }

    #[test]
    fn unnumbered_chapter_without_caption_is_byte_identical() {
        let content = include_block("a", None);
        let (out, _) = splice_chapter(&content, None, true, Html);
        assert_eq!(out, content);
    }

    #[test]
    fn plain_code_block_without_anchor_is_byte_identical() {
        let content = "```rust\nlet x = 1;\n```\n".to_string();
        let (out, _) = splice_chapter(&content, Some(&[5]), true, Html);
        assert_eq!(
            out, content,
            "a block with no locator anchor is not a listing"
        );
    }

    #[test]
    fn caption_element_lands_between_preceding_text_and_the_fence() {
        // Pins the opener line offset exactly: the caption must follow the
        // preceding prose (not jump to the start of the chapter) and sit
        // immediately before the opening fence (not a line early).
        let content = format!("intro\n\n{}", include_block("a", None));
        let (out, _) = splice_chapter(&content, Some(&[5]), true, Html);
        assert!(
            out.contains(
                "intro\n\n<div class=\"listing-caption\" id=\"listing-5-1\">Listing 5.1</div>\n\n```rust"
            ),
            "caption must sit as a standalone block above its fence, after the preceding text; got:\n{out}",
        );
    }

    #[test]
    fn finds_anchor_separated_from_fence_by_one_newline() {
        // The anchor detector tolerates one newline between the closing fence
        // and the anchor; a numbered listing must still be recognized.
        let content =
            "```rust\nfn a() {}\n```\n\n<div data-listing-tag=\"a\" aria-hidden=\"true\"></div>\n";
        let (out, _) = splice_chapter(content, Some(&[5]), true, Html);
        assert!(out.contains("Listing 5.1"), "got:\n{out}");
        assert!(
            out.contains(r#"<div data-listing-number="5.1" data-listing-tag="a""#),
            "got:\n{out}",
        );
    }

    #[test]
    fn html_keeps_caption_escaped() {
        // Caption arrives HTML-escaped on the anchor; HTML text wants it as-is.
        let content = include_block("a", Some("A &amp; B &lt;t&gt;"));
        let (out, _) = splice_chapter(&content, Some(&[5]), true, Html);
        assert!(
            out.contains("Listing 5.1 — A &amp; B &lt;t&gt;"),
            "got:\n{out}"
        );
    }

    #[test]
    fn pdf_renders_bold_markdown_and_unescapes_caption() {
        let content = include_block("a", Some("A &amp; B &lt;t&gt;"));
        let (out, _) = splice_chapter(&content, Some(&[5]), true, TypstPdf);
        assert!(out.contains("**Listing 5.1 — A & B <t>**"), "got:\n{out}");
        assert!(
            !out.contains(r#"class="listing-caption""#),
            "PDF must not emit a raw <div> caption element; got:\n{out}"
        );
    }

    #[test]
    fn html_unescape_reverses_all_five_entities() {
        assert_eq!(html_unescape("&amp;&lt;&gt;&quot;&#123;"), "&<>\"{");
    }
}
