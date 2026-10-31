use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use crate::anchor::anchor_after_fence;
use crate::diff::parse_hunk_header;

use super::SpliceError;
use super::parse::{
    ALL_COMMENT_PREFIXES, comment_prefix_for_language, is_diff_metadata, parse_line,
};
use super::sidecar::DiffSide;

/// Result of one `strip_marker_lines*` pass.
#[derive(Debug)]
pub(super) struct StripResult {
    /// Block text with inline marker lines removed.
    pub(super) body: String,
    /// Post-strip 1-based line where each inline marker's badge now lands
    /// (one entry per inline marker, in source-encounter order).
    pub(super) post_strip_lines: Vec<usize>,
    /// 1-based block line of each stripped marker line, in block order,
    /// including a diff's context and removed markers, which earn no badge.
    /// Used by the sidecar-merge step to translate sidecar block lines into
    /// post-strip positions.
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
        if is_diff_metadata(line_no_newline) {
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
                post_strip_lines.push((emitted_count + 1).max(1));
            }
            stripped_source_lines.push(idx + 1);
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

impl ListingAnchor<'_> {
    /// The listing's source lines that `block_text` shows: the whole file for
    /// a full include, the slice (less its header lines) for a ranged one.
    pub(super) fn shown_source_lines(&self, block_text: &str) -> Range<usize> {
        let (start, header) = self.first_line_and_header();
        start..start + block_text.lines().count().saturating_sub(header)
    }

    /// The first source line the block shows, and how many header lines the
    /// include splicer put above it.
    fn first_line_and_header(&self) -> (usize, usize) {
        match self.range_start_source_line {
            None => (1, 0),
            Some(start) => (start, RANGED_INCLUDE_HEADER_LINES),
        }
    }
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
    // The numbering pass may have stamped `data-listing-number` ahead of the
    // tag, so the tag is not necessarily the first attribute.
    let div_text = anchor_after_fence(content, close_end)?;
    // Only include anchors carry `data-listing-tag`; a diff anchor has none
    // and correctly yields `None` (its sidecar is found by its operands).
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
            // Range render shape is `<start>:<end>`, `<start>:` or `:<end>`;
            // only the start matters, and an open one is line 1.
            match r_value.split(':').next()? {
                "" => Some(1),
                start => start.parse::<usize>().ok(),
            }
        });
    Some(ListingAnchor {
        tag,
        range_start_source_line,
    })
}

/// The (old, new) operand tags of the diff anchor after the fence that ends
/// at `close_end`, or `None` when the block is not a rendered diff of two
/// frozen listings. The anchor names every `live:` operand plain `live`,
/// so a sidecar keyed on it would attach to diffs of unrelated files.
pub(super) fn diff_operands_after_fence(
    content: &str,
    close_end: usize,
) -> Option<(String, String)> {
    let div_text = anchor_after_fence(content, close_end)?;
    let old = crate::anchor::attr_value(div_text, "data-listing-diff-left")?;
    let new = crate::anchor::attr_value(div_text, "data-listing-diff-right")?;
    (old != "live" && new != "live").then_some((old, new))
}

use crate::anchor::RANGED_INCLUDE_HEADER_LINES;

/// The block line (1-based) that shows each line of a rendered diff's old
/// and new file, read off its hunk headers; a line outside every hunk has
/// none. Inside a hunk the header's counts say where it ends, not each
/// line's first characters, so a removed line that reads `--- …` is content
/// rather than a file header.
pub(super) struct DiffLineMap {
    old: HashMap<usize, usize>,
    new: HashMap<usize, usize>,
}

impl DiffLineMap {
    pub(super) fn new(block_text: &str) -> Self {
        let mut map = Self {
            old: HashMap::new(),
            new: HashMap::new(),
        };
        let (mut old, mut new, mut old_left, mut new_left) = (0, 0, 0, 0);
        for (idx, line) in block_text.lines().enumerate() {
            let row = idx + 1;
            if old_left == 0 && new_left == 0 {
                if let Some(h) = parse_hunk_header(line) {
                    (old, new, old_left, new_left) =
                        (h.old_start, h.new_start, h.old_count, h.new_count);
                }
                continue;
            }
            match line.chars().next() {
                Some('\\') => {}
                Some('-') => {
                    map.old.insert(old, row);
                    (old, old_left) = (old + 1, old_left.saturating_sub(1));
                }
                Some('+') => {
                    map.new.insert(new, row);
                    (new, new_left) = (new + 1, new_left.saturating_sub(1));
                }
                _ => {
                    map.old.insert(old, row);
                    map.new.insert(new, row);
                    (old, old_left) = (old + 1, old_left.saturating_sub(1));
                    (new, new_left) = (new + 1, new_left.saturating_sub(1));
                }
            }
        }
        map
    }

    pub(super) fn block_line(&self, side: DiffSide, line: usize) -> Option<usize> {
        match side {
            DiffSide::Old => self.old.get(&line),
            DiffSide::New => self.new.get(&line),
        }
        .copied()
    }
}

/// Translate a sidecar entry's source-file line into the corresponding
/// 1-based line within the rendered fenced block (`block_text`). For a
/// full-file include (`anchor.range_start_source_line` is `None`) source
/// line N is at block_text line N. For a ranged include starting at
/// source line S, source line N is at block_text line
/// (N - S + 1) + 2 (the 2 prepended header lines).
pub(super) fn source_line_to_block_line(source_line: usize, anchor: &ListingAnchor<'_>) -> usize {
    let (start, header) = anchor.first_line_and_header();
    (source_line.saturating_sub(start).saturating_add(1)).saturating_add(header)
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
    fn strip_records_the_post_strip_line_the_badge_lands_on() {
        let got = strip_marker_lines("key: value\n# CALLOUT: x Note.\nfoo: bar\n", "yaml");
        // One line emitted before the marker, so the badge lands on the
        // NEXT emitted line: post-strip line 2.
        assert_eq!(got.post_strip_lines, vec![2]);
        assert_eq!(got.stripped_source_lines, vec![2]);
        assert_eq!(got.total_lines, 2);
        assert_eq!(got.body, "key: value\nfoo: bar\n");
    }

    #[test]
    fn diff_strip_counts_metadata_lines_toward_badge_position() {
        // Three metadata lines pass through and count as emitted, so the
        // added marker's badge lands on post-strip line 4.
        let got = strip_marker_lines_diff("--- a\n+++ b\n@@ -1 +1 @@\n+# CALLOUT: x New.\n ctx\n");
        assert_eq!(got.post_strip_lines, vec![4]);
        assert_eq!(got.stripped_source_lines, vec![4]);
        assert_eq!(got.total_lines, 4);
        assert_eq!(got.body, "--- a\n+++ b\n@@ -1 +1 @@\n ctx\n");
    }

    #[test]
    fn diff_strip_counts_content_lines_toward_badge_position() {
        let got = strip_marker_lines_diff(" ctx\n+// CALLOUT: y B\n more\n");
        assert_eq!(got.post_strip_lines, vec![2]);
        assert_eq!(got.total_lines, 2);
        assert_eq!(got.body, " ctx\n more\n");
    }

    #[test]
    fn listing_anchor_after_fence_finds_anchor_immediately_after_fence() {
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
        let tag = listing_anchor_after_fence(content, close_end).map(|a| a.tag);
        assert_eq!(tag, Some("compose-v1"));
    }

    #[test]
    fn listing_anchor_after_fence_returns_none_when_no_anchor() {
        let content = "```rust\nlet x = 1;\n```\n\nSome prose.\n";
        let close = content.find("```\n").unwrap() + 4;
        assert_eq!(
            listing_anchor_after_fence(content, close).map(|a| a.tag),
            None
        );
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

    /// A diff sidecar entry's row shifts up by every marker line stripped
    /// above it, whatever its side, so all of them are recorded; only the
    /// added one earns a badge.
    #[test]
    fn strip_marker_lines_diff_records_every_stripped_marker_line() {
        let block_text = concat!(
            "+// CALLOUT: first body.\n",
            "+let x = 1;\n",
            " // CALLOUT: second body.\n",
            " let y = 2;\n",
            "-// CALLOUT: third body.\n",
        );
        let result = strip_marker_lines_diff(block_text);
        assert_eq!(result.stripped_source_lines, vec![1, 3, 5]);
        assert_eq!(
            result.post_strip_lines,
            vec![1],
            "only the `+` marker is badged"
        );
    }

    const TWO_HUNKS: &str = concat!(
        "--- a\n",
        "+++ b\n",
        "@@ -3,3 +3,3 @@\n",
        " ctx3\n",
        "-old4\n",
        "+new4\n",
        " ctx5\n",
        "@@ -10,2 +10,2 @@ fn f()\n",
        "-old10\n",
        "+new10\n",
        " ctx11\n",
    );

    #[test]
    fn a_diff_line_map_puts_each_side_on_the_row_that_shows_it() {
        let map = DiffLineMap::new(TWO_HUNKS);
        for (side, line, row) in [
            (DiffSide::Old, 3, 4),
            (DiffSide::New, 3, 4),
            (DiffSide::Old, 4, 5),
            (DiffSide::New, 4, 6),
            (DiffSide::New, 5, 7),
            (DiffSide::Old, 10, 9),
            (DiffSide::New, 10, 10),
            (DiffSide::Old, 11, 11),
        ] {
            assert_eq!(
                map.block_line(side, line),
                Some(row),
                "{} line {line}",
                side.as_str()
            );
        }
    }

    #[test]
    fn a_diff_line_map_has_no_row_for_a_line_outside_every_hunk() {
        let map = DiffLineMap::new(TWO_HUNKS);
        for (side, line) in [
            (DiffSide::New, 2),
            (DiffSide::New, 6),
            (DiffSide::Old, 9),
            (DiffSide::New, 12),
        ] {
            assert_eq!(
                map.block_line(side, line),
                None,
                "{} line {line}",
                side.as_str()
            );
        }
    }

    #[test]
    fn a_diff_line_map_counts_a_removed_line_that_reads_like_a_file_header() {
        // A removed `-- comment` line renders as `--- comment`.
        let map = DiffLineMap::new("@@ -1,2 +1,1 @@\n--- comment\n ctx\n");
        assert_eq!(map.block_line(DiffSide::Old, 1), Some(2));
        assert_eq!(map.block_line(DiffSide::Old, 2), Some(3));
        assert_eq!(map.block_line(DiffSide::New, 1), Some(3));
    }

    #[test]
    fn a_diff_line_map_skips_the_no_newline_marker() {
        let map = DiffLineMap::new("@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n");
        assert_eq!(map.block_line(DiffSide::New, 1), Some(4));
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
