use std::path::Path;

use super::SpliceError;
use super::parse::{ALL_COMMENT_PREFIXES, comment_prefix_for_language, parse_line};

/// Result of one `strip_marker_lines*` pass.
#[derive(Debug)]
pub(super) struct StripResult {
    /// Block text with inline marker lines removed.
    pub(super) body: String,
    /// Post-strip 1-based line where each inline marker's badge now lands
    /// (one entry per inline marker, in source-encounter order).
    pub(super) post_strip_lines: Vec<usize>,
    /// 1-based source line of each stripped marker (one entry per inline
    /// marker, in source order). Used by the sidecar-merge step to
    /// translate sidecar source lines into post-strip positions.
    pub(super) stripped_source_lines: Vec<usize>,
    /// Total visible lines in `body` (used for overlay sizing).
    pub(super) total_lines: usize,
}

/// Compute the rewritten block body (marker lines removed) plus the
/// metadata the overlay renderer + sidecar-merge step both need.
pub(super) fn strip_marker_lines(block_text: &str, info: &str) -> StripResult {
    let prefix = comment_prefix_for_language(info);
    let lines: Vec<&str> = block_text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(block_text.len());
    let mut post_strip_lines: Vec<usize> = Vec::new();
    let mut stripped_source_lines: Vec<usize> = Vec::new();
    let mut emitted_count: usize = 0;
    for (idx, raw_line) in lines.iter().enumerate() {
        let line_no_newline = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let is_marker = prefix
            .and_then(|p| parse_line(line_no_newline, p, 0))
            .is_some();
        if is_marker {
            // Marker stripped; the next non-marker line we emit becomes the
            // line the badge points at. If we're at the end of the block we
            // still record a line — the badge clamps to the last visible
            // line of the listing.
            let target = (emitted_count + 1).max(1);
            post_strip_lines.push(target);
            stripped_source_lines.push(idx + 1);
        } else {
            out.push_str(raw_line);
            emitted_count += 1;
        }
    }
    StripResult {
        body: out,
        post_strip_lines,
        stripped_source_lines,
        total_lines: emitted_count,
    }
}

// Diff-aware strip: the marker comment is removed from the rendered diff
// either way; its diff prefix decides whether it leaves a badge behind.
pub(super) fn strip_marker_lines_diff(block_text: &str) -> StripResult {
    let lines: Vec<&str> = block_text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(block_text.len());
    let mut post_strip_lines: Vec<usize> = Vec::new();
    let mut stripped_source_lines: Vec<usize> = Vec::new();
    let mut emitted_count: usize = 0;
    for (idx, raw_line) in lines.iter().enumerate() {
        let line_no_newline = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        // Diff metadata lines pass through unchanged.
        if line_no_newline.starts_with("---")
            || line_no_newline.starts_with("+++")
            || line_no_newline.starts_with("@@")
            || line_no_newline.starts_with('\\')
        {
            out.push_str(raw_line);
            emitted_count += 1;
            continue;
        }
        // Identify the diff-line prefix and try to parse the trailing
        // payload as a marker against any known comment prefix.
        let (prefix_char, payload) = if let Some(rest) = line_no_newline.strip_prefix('+') {
            (Some('+'), rest)
        } else if let Some(rest) = line_no_newline.strip_prefix('-') {
            (Some('-'), rest)
        } else if let Some(rest) = line_no_newline.strip_prefix(' ') {
            (Some(' '), rest)
        } else {
            (None, line_no_newline)
        };
        let is_marker = ALL_COMMENT_PREFIXES
            .iter()
            .any(|p| parse_line(payload, p, 0).is_some());
        if is_marker {
            // CALLOUT: strip-diff An added (`+`) marker is a new or edited callout, so it earns a badge; the recorded post-strip line lands it on the row the comment used to occupy.
            if matches!(prefix_char, Some('+')) {
                let target = (emitted_count + 1).max(1);
                post_strip_lines.push(target);
                stripped_source_lines.push(idx + 1);
            }
            // CALLOUT: strip-diff-skip Context (` `) and removed (`-`) markers fall through with no badge: the unchanged one is already badged on its full `{{#include}}`, the removed one is gone in the new state.
        } else {
            out.push_str(raw_line);
            emitted_count += 1;
        }
    }
    StripResult {
        body: out,
        post_strip_lines,
        stripped_source_lines,
        total_lines: emitted_count,
    }
}

/// Anchor information extracted from a `<div data-listing-tag>` element
/// that the include splicer emits after each `{{#include listings/...}}`
/// expansion. `range_start_source_line` is `Some(N)` when the include
/// was a sliced range starting at source line N — the sidecar `source_line`
/// → block_text-line translation needs to know N and that the include
/// splicer prepends 2 header lines for ranged slices.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ListingAnchor<'c> {
    pub(super) tag: &'c str,
    pub(super) range_start_source_line: Option<usize>,
}

/// Peek past the closing fence at `close_end` for the include
/// splicer's `<div data-listing-tag="<tag>"[ data-listing-tag-range="A:B"]...>`
/// anchor. Returns `None` when no anchor is present. Tolerates one
/// trailing newline between the fence and the anchor (the include
/// splicer emits exactly one).
pub(super) fn listing_anchor_after_fence<'c>(
    content: &'c str,
    close_end: usize,
) -> Option<ListingAnchor<'c>> {
    let tail = &content[close_end..];
    let after_newline = tail.strip_prefix('\n').unwrap_or(tail);
    // Find the anchor element, then its `data-listing-tag` attribute anywhere
    // inside it — the numbering pass may have stamped `data-listing-number`
    // ahead of the tag, so the tag is not necessarily the first attribute.
    let anchor_open = after_newline.find("<div ")?;
    if anchor_open > 64 {
        return None;
    }
    // The full element fits on one line, so cap the search at the closing `>`.
    let div_end = after_newline[anchor_open..]
        .find('>')
        .map(|i| anchor_open + i)
        .unwrap_or(after_newline.len());
    let div_text = &after_newline[anchor_open..div_end];
    // Only include anchors carry `data-listing-tag`; a diff anchor has none
    // and correctly yields `None` (it bears no sidecar callouts).
    const TAG_KEY: &str = "data-listing-tag=\"";
    let tag_start = div_text.find(TAG_KEY)? + TAG_KEY.len();
    let tag_end = div_text[tag_start..].find('"')?;
    let tag = &div_text[tag_start..tag_start + tag_end];
    // Look for an optional `data-listing-tag-range="A:B"` attribute on the
    // same anchor element.
    let range_start_source_line = div_text
        .find("data-listing-tag-range=\"")
        .and_then(|r_open| {
            let r_value_start = r_open + "data-listing-tag-range=\"".len();
            let r_value_end = div_text[r_value_start..].find('"')?;
            let r_value = &div_text[r_value_start..r_value_start + r_value_end];
            // Range render shape is `<start>:<end>` or `<start>:` —
            // parse the start integer; ignore the rest.
            r_value.split(':').next()?.parse::<usize>().ok()
        });
    Some(ListingAnchor {
        tag,
        range_start_source_line,
    })
}

/// Back-compat shim for the ordinal pass + tests that only need the tag.
pub(super) fn listing_tag_after_fence(content: &str, close_end: usize) -> Option<&str> {
    listing_anchor_after_fence(content, close_end).map(|a| a.tag)
}

/// Number of header lines the include splicer prepends to a ranged
/// `{{#include listings/...}}` expansion. The header is `<basename>\n@@
/// start,end @@\n` — exactly 2 lines, both commented when the source's
/// extension maps to a known single-line comment prefix.
const RANGED_INCLUDE_HEADER_LINES: usize = 2;

/// Translate a sidecar entry's source-file line into the corresponding
/// 1-based line within the rendered fenced block (`block_text`). For a
/// full-file include (`anchor.range_start_source_line` is `None`) source
/// line N is at block_text line N. For a ranged include starting at
/// source line S, source line N is at block_text line
/// (N - S + 1) + 2 (the 2 prepended header lines).
pub(super) fn source_line_to_block_line(source_line: usize, anchor: &ListingAnchor<'_>) -> usize {
    match anchor.range_start_source_line {
        None => source_line,
        Some(start) => (source_line.saturating_sub(start).saturating_add(1))
            .saturating_add(RANGED_INCLUDE_HEADER_LINES),
    }
}

/// Translate a sidecar entry's block-text line (already mapped from
/// source file via [`source_line_to_block_line`]) into the post-strip
/// line where its badge should appear. The shift equals the number of
/// inline marker lines stripped at-or-before the block line; if the
/// block line itself is in [`StripResult::stripped_source_lines`]
/// (i.e. the author pointed the sidecar at a line that the strip pass
/// removed), returns `Err`. `source_line_reported` is the original
/// source-file line the author wrote, used in error messages so the
/// diagnostic points at what the author actually typed.
pub(super) fn translate_sidecar_line_to_post_strip(
    block_line: usize,
    stripped_source_lines: &[usize],
    tag: &str,
    sidecar_path: Option<&Path>,
    label: &str,
    source_line_reported: usize,
) -> Result<usize, SpliceError> {
    if stripped_source_lines.contains(&block_line) {
        return Err(SpliceError::SidecarLineOnStrippedMarker {
            label: label.to_string(),
            listing_tag: tag.to_string(),
            source_line: source_line_reported,
            sidecar_path: sidecar_path.map(Path::to_path_buf).unwrap_or_default(),
        });
    }
    let shift = stripped_source_lines
        .iter()
        .filter(|&&s| s < block_line)
        .count();
    Ok(block_line.saturating_sub(shift).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_tag_after_fence_finds_anchor_immediately_after_fence() {
        let content = concat!(
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"compose-v1\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.find("```\n").unwrap()
            + content[content.find("```\n").unwrap()..]
                .find("```\n")
                .map(|i| i + "```".len())
                .unwrap();
        let close_end = close_end + 1; // include the trailing newline of the close-fence line
        let tag = listing_tag_after_fence(content, close_end);
        assert_eq!(tag, Some("compose-v1"));
    }

    #[test]
    fn listing_tag_after_fence_returns_none_when_no_anchor() {
        let content = "```rust\nlet x = 1;\n```\n\nSome prose.\n";
        let close = content.find("```\n").unwrap() + 4;
        assert_eq!(listing_tag_after_fence(content, close), None);
    }

    #[test]
    fn translate_sidecar_line_with_no_strips_is_identity() {
        let result =
            translate_sidecar_line_to_post_strip(5, &[], "demo-v1", None, "label", 5).unwrap();
        assert_eq!(result, 5);
    }

    #[test]
    fn translate_sidecar_line_shifts_by_count_of_stripped_lines_before_it() {
        // Two inline markers stripped at block_text lines 2 and 4.
        // A sidecar callout at block_text line 7 should render at
        // post-strip line 7 - 2 = 5.
        let result =
            translate_sidecar_line_to_post_strip(7, &[2, 4], "demo-v1", None, "label", 7).unwrap();
        assert_eq!(result, 5);
    }

    #[test]
    fn translate_sidecar_line_errors_when_source_line_is_a_stripped_marker() {
        let err = translate_sidecar_line_to_post_strip(3, &[3, 7], "demo-v1", None, "collide", 3)
            .unwrap_err();
        match err {
            SpliceError::SidecarLineOnStrippedMarker {
                label, source_line, ..
            } => {
                assert_eq!(label, "collide");
                assert_eq!(source_line, 3);
            }
            other => panic!("expected SidecarLineOnStrippedMarker, got {other:?}"),
        }
    }

    #[test]
    fn source_line_to_block_line_is_identity_for_full_file_include() {
        let anchor = ListingAnchor {
            tag: "demo-v1",
            range_start_source_line: None,
        };
        assert_eq!(source_line_to_block_line(7, &anchor), 7);
    }

    #[test]
    fn source_line_to_block_line_offsets_for_ranged_include_with_header() {
        // Range starting at source line 28 — block_text layout is:
        //   block 1: `// foo.rs` (header line 1)
        //   block 2: `// @@ 28,50 @@` (header line 2)
        //   block 3: source line 28
        //   block 4: source line 29
        //   ...
        // So source line 32 → block_text line 3 + (32 - 28) = 7.
        let anchor = ListingAnchor {
            tag: "demo-v1",
            range_start_source_line: Some(28),
        };
        assert_eq!(source_line_to_block_line(32, &anchor), 7);
    }

    #[test]
    fn listing_anchor_after_fence_extracts_range_when_present() {
        let content = concat!(
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"foo-v1\" data-listing-tag-range=\"28:50\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.find("```\n").unwrap()
            + content[content.find("```\n").unwrap()..]
                .find("```\n")
                .map(|i| i + "```".len())
                .unwrap()
            + 1;
        let anchor = listing_anchor_after_fence(content, close_end).unwrap();
        assert_eq!(anchor.tag, "foo-v1");
        assert_eq!(anchor.range_start_source_line, Some(28));
    }

    /// Pinned even though the HTML diff path doesn't consume
    /// `stripped_source_lines` today — a future "sidecar on diffs"
    /// extension would, and a wrong recording shape would silently
    /// misplace badges. Only badge-bearing (`+`) markers are recorded;
    /// context (` `) and removed (`-`) marker lines are stripped from the
    /// body but earn no badge, so they don't appear here.
    #[test]
    fn strip_marker_lines_diff_records_source_line_numbers_of_added_markers() {
        let block_text = concat!(
            "+// CALLOUT: first body.\n",
            "+let x = 1;\n",
            " // CALLOUT: second body.\n",
            " let y = 2;\n",
        );
        let result = strip_marker_lines_diff(block_text);
        assert_eq!(
            result.stripped_source_lines,
            vec![1],
            "only the added (`+`) marker at line 1 should be recorded; the \
             context marker at line 3 is stripped but earns no badge",
        );
    }

    #[test]
    fn listing_anchor_after_fence_accepts_anchor_at_64_byte_offset() {
        let close_to_anchor: String = "x".repeat(64);
        let content = format!(
            "```rust\nlet x = 1;\n```\n{close_to_anchor}<div data-listing-tag=\"demo\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content
            .rfind("```\n")
            .map(|i| i + 4)
            .expect("fence close present");
        let anchor = listing_anchor_after_fence(&content, close_end).unwrap();
        assert_eq!(anchor.tag, "demo");
    }

    #[test]
    fn listing_anchor_after_fence_rejects_anchor_at_65_byte_offset() {
        let close_to_anchor: String = "x".repeat(65);
        let content = format!(
            "```rust\nlet x = 1;\n```\n{close_to_anchor}<div data-listing-tag=\"demo\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content
            .rfind("```\n")
            .map(|i| i + 4)
            .expect("fence close present");
        assert!(listing_anchor_after_fence(&content, close_end).is_none());
    }

    #[test]
    fn listing_anchor_after_fence_range_is_none_for_full_file_include() {
        let content = concat!(
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"foo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.find("```\n").unwrap()
            + content[content.find("```\n").unwrap()..]
                .find("```\n")
                .map(|i| i + "```".len())
                .unwrap()
            + 1;
        let anchor = listing_anchor_after_fence(content, close_end).unwrap();
        assert_eq!(anchor.tag, "foo-v1");
        assert_eq!(anchor.range_start_source_line, None);
    }
}
