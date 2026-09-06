//! One home for the locator-anchor protocol between pipeline stages.
//!
//! The include and diff splicers serialise listing metadata as a
//! `<div data-listing-…>` element after each rendered fence, and the
//! numbering and callout passes re-parse it downstream. Before this module
//! the emit shape lived in `include.rs`/`diff.rs` while the parse grammar,
//! the scan tolerance, and the ranged-include header height were re-stated
//! in `number.rs` and `callout/` — change one side and badges silently
//! misplace. Everything shape-shaped now lives here; the scanning entry
//! points stay with their consumers but are built from these parts.

use crate::diff::LineRange;

/// How far past the closing fence (plus one optional newline) a parser will
/// look for the anchor's `<div `. The emitters place it immediately after
/// the fence, so this is pure defence: large enough to survive incidental
/// whitespace, small enough that an unrelated `<div>` further down the
/// chapter is never misread as a locator anchor.
pub(crate) const SCAN_TOLERANCE: usize = 64;

/// Number of header lines the include splicer prepends to a *ranged*
/// `{{#include listings/…}}` expansion — the callout sidecar
/// line-translation adds exactly this many lines when mapping a source line
/// into a ranged block. [`ranged_include_header`] is the only producer, and
/// a unit test pins its line count to this constant, so the two can no
/// longer drift apart silently.
pub(crate) const RANGED_INCLUDE_HEADER_LINES: usize = 2;

/// The two-line banner prepended to a ranged include, mirroring a unified
/// diff's `--- tag` / `@@ …` shape: the file's basename on line 1, the
/// range on line 2. Both lines carry the extension's single-line comment
/// prefix when one is known, so highlighters render them as metadata
/// rather than invalid code.
pub(crate) fn ranged_include_header(rel_path: &str, range: &LineRange) -> String {
    let basename = std::path::Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel_path);
    let prefix = std::path::Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(crate::callout::comment_prefix_for_extension)
        .map(|p| format!("{p} "))
        .unwrap_or_default();
    format!(
        "{prefix}{basename}\n{prefix}@@ {},{} @@",
        range.start.unwrap_or(1),
        range
            .end
            .map(|n| n.to_string())
            .unwrap_or_else(|| "EOF".to_string()),
    )
}

/// One operand of a rendered diff: the tag (or `live:<path>`), the manifest
/// `source` it was frozen from, and the slice shown.
pub(crate) struct DiffOperand<'a> {
    pub(crate) tag: &'a str,
    pub(crate) source: Option<&'a str>,
    pub(crate) range: Option<&'a LineRange>,
}

/// The author-supplied metadata an anchor carries alongside its operands.
/// Grouped because both emitters take all of it and neither varies it.
pub(crate) struct AnchorMeta<'a> {
    pub(crate) caption: Option<&'a str>,
    pub(crate) label: Option<&'a str>,
    /// `show-provenance="true|false"` on the directive, overriding the
    /// book-level flag in either direction. `None` when unset.
    pub(crate) show_provenance: Option<bool>,
}

/// The locator anchor for a frozen-listing include. Trailing newline
/// included — the anchor is a line of its own after the closing fence.
pub(crate) fn include_anchor(
    tag: &str,
    source: Option<&str>,
    range: Option<&LineRange>,
    meta: &AnchorMeta,
) -> String {
    let mut anchor = format!("<div data-listing-tag=\"{tag}\"");
    if let Some(source) = source {
        anchor.push_str(&format!(
            " data-listing-source=\"{}\"",
            crate::callout::html_escape(source)
        ));
    }
    if let Some(range) = range {
        anchor.push_str(&format!(" data-listing-tag-range=\"{}\"", range.render()));
    }
    push_meta(&mut anchor, meta);
    anchor.push_str(" aria-hidden=\"true\"></div>\n");
    anchor
}

/// The locator anchor for a rendered diff. Both operands are separate
/// attributes so a diff block is addressable by its (LEFT, RIGHT) pair —
/// unique even when multiple diffs share a RIGHT tag, and unambiguous
/// against include anchors.
pub(crate) fn diff_anchor(left: &DiffOperand, right: &DiffOperand, meta: &AnchorMeta) -> String {
    let mut anchor = format!(
        "<div data-listing-diff-left=\"{}\" data-listing-diff-right=\"{}\"",
        left.tag, right.tag
    );
    for (side, operand) in [("left", left), ("right", right)] {
        if let Some(source) = operand.source {
            anchor.push_str(&format!(
                " data-listing-diff-{side}-source=\"{}\"",
                crate::callout::html_escape(source)
            ));
        }
    }
    for (side, operand) in [("left", left), ("right", right)] {
        if let Some(r) = operand.range {
            anchor.push_str(&format!(
                " data-listing-diff-{side}-range=\"{}\"",
                r.render()
            ));
        }
    }
    push_meta(&mut anchor, meta);
    anchor.push_str(" aria-hidden=\"true\"></div>");
    anchor
}

fn push_meta(anchor: &mut String, meta: &AnchorMeta) {
    if let Some(caption) = meta.caption {
        anchor.push_str(&format!(
            " data-listing-caption=\"{}\"",
            crate::callout::html_escape(caption)
        ));
    }
    if let Some(label) = meta.label {
        anchor.push_str(&format!(
            " data-listing-label=\"{}\"",
            crate::callout::html_escape(label)
        ));
    }
    if let Some(show) = meta.show_provenance {
        anchor.push_str(&format!(" data-listing-show-provenance=\"{show}\""));
    }
}

/// Read a `name="value"` attribute's value out of an anchor element's
/// opening-tag text. The one attribute grammar every parser shares.
pub(crate) fn attr_value(div_text: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = div_text.find(&key)? + key.len();
    let end = div_text[start..].find('"')?;
    Some(div_text[start..start + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::parse_line_range;

    #[test]
    fn ranged_header_line_count_matches_the_constant() {
        // THE coupling this module exists to pin: the strip pass adds
        // RANGED_INCLUDE_HEADER_LINES when translating sidecar lines into a
        // ranged block, so the header must be exactly that many lines.
        let range = parse_line_range("28:50").expect("range");
        let header = ranged_include_header("listings/foo.rs", &range);
        assert_eq!(header.lines().count(), RANGED_INCLUDE_HEADER_LINES);
    }

    fn meta<'a>(
        caption: Option<&'a str>,
        label: Option<&'a str>,
        show_provenance: Option<bool>,
    ) -> AnchorMeta<'a> {
        AnchorMeta {
            caption,
            label,
            show_provenance,
        }
    }

    fn operand<'a>(tag: &'a str, source: Option<&'a str>) -> DiffOperand<'a> {
        DiffOperand {
            tag,
            source,
            range: None,
        }
    }

    #[test]
    fn include_anchor_round_trips_through_attr_value() {
        let range = parse_line_range("1:30").expect("range");
        let anchor = include_anchor(
            "foo-v1",
            Some("../src/foo.rs"),
            Some(&range),
            &meta(Some("Cap"), Some("lbl"), None),
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-tag").as_deref(),
            Some("foo-v1")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-source").as_deref(),
            Some("../src/foo.rs")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-tag-range").as_deref(),
            Some("1:30")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-caption").as_deref(),
            Some("Cap")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-label").as_deref(),
            Some("lbl")
        );
        assert!(anchor.ends_with("</div>\n"), "anchor is its own line");
    }

    #[test]
    fn include_anchor_omits_source_when_the_manifest_has_none() {
        let anchor = include_anchor("foo-v1", None, None, &meta(None, None, None));
        assert_eq!(attr_value(&anchor, "data-listing-source"), None);
    }

    #[test]
    fn diff_anchor_round_trips_through_attr_value() {
        let anchor = diff_anchor(
            &operand("a-v1", Some("../a.rs")),
            &operand("a-v2", Some("../a.rs")),
            &meta(Some("Cap"), None, None),
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-diff-left").as_deref(),
            Some("a-v1")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-diff-right").as_deref(),
            Some("a-v2")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-diff-left-source").as_deref(),
            Some("../a.rs")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-diff-right-source").as_deref(),
            Some("../a.rs")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-caption").as_deref(),
            Some("Cap")
        );
        assert_eq!(attr_value(&anchor, "data-listing-label"), None);
    }

    #[test]
    fn anchors_carry_the_per_directive_provenance_override() {
        let off = include_anchor("foo-v1", None, None, &meta(None, None, Some(false)));
        assert_eq!(
            attr_value(&off, "data-listing-show-provenance").as_deref(),
            Some("false")
        );
        let on = diff_anchor(
            &operand("a-v1", None),
            &operand("a-v2", None),
            &meta(None, None, Some(true)),
        );
        assert_eq!(
            attr_value(&on, "data-listing-show-provenance").as_deref(),
            Some("true")
        );
        let unset = include_anchor("foo-v1", None, None, &meta(None, None, None));
        assert_eq!(attr_value(&unset, "data-listing-show-provenance"), None);
    }

    #[test]
    fn diff_anchor_keeps_left_and_right_sources_distinct() {
        let anchor = diff_anchor(
            &operand("a-v1", Some("../a.rs")),
            &operand("b-v1", Some("../b.rs")),
            &meta(None, None, None),
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-diff-left-source").as_deref(),
            Some("../a.rs")
        );
        assert_eq!(
            attr_value(&anchor, "data-listing-diff-right-source").as_deref(),
            Some("../b.rs")
        );
    }

    #[test]
    fn header_comment_prefixes_by_extension() {
        let range = parse_line_range("5:9").expect("range");
        assert!(ranged_include_header("listings/foo.rs", &range).starts_with("// foo.rs"));
        assert!(ranged_include_header("listings/foo.yaml", &range).starts_with("# foo.yaml"));
    }
}
