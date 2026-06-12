//! Shared scanner for `{{#name …}}` directive occurrences in chapter
//! markdown. The include, diff, and callout passes each parse different
//! arguments and apply different fence policies, but the occurrence
//! grammar — escape handling, inline-code detection, fence membership —
//! must agree between them or directives get consumed in one pass that
//! another would have left alone.

use std::ops::Range;

use crate::fence::FencedBlocks;

/// One unescaped directive occurrence outside inline code.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DirectiveOccurrence<'a> {
    /// Raw text between the prefix and the closing `}}`, untrimmed —
    /// argument grammar is the caller's business. May span lines: the
    /// closing braces are found by an unbounded forward search.
    pub(crate) args: &'a str,
    /// Byte range of the full `{{#… }}` text.
    pub(crate) span: Range<usize>,
    /// `Some(close_end)` when the occurrence starts inside a fenced code
    /// block (only produced under [`FencePolicy::Annotate`]); the value is
    /// the fence's close_end so callers can place trailing anchors.
    pub(crate) fence_close_end: Option<usize>,
}

/// What to do with an occurrence that starts inside a fenced code block.
#[derive(Clone, Copy)]
pub(crate) enum FencePolicy {
    /// Yield it, tagged with its fence's `close_end`, consuming through its
    /// `}}` — for the include pass, where fenced directives are the primary
    /// case.
    Annotate,
    /// Don't yield it, and resume scanning just past the prefix rather than
    /// past its `}}` — a fenced opener whose closing braces lie beyond the
    /// fence must not swallow a real directive that follows the fence.
    SkipInside,
}

/// Scan `content` for `prefix` occurrences (`"{{#include "`, `"{{#diff"`,
/// `"{{#callout "` — exact literals, including any trailing space).
/// Backslash-escaped occurrences and ones sitting inside an inline code
/// span are skipped so chapters can quote directive syntax verbatim.
pub(crate) fn scan_directives<'a>(
    content: &'a str,
    prefix: &str,
    policy: FencePolicy,
) -> Vec<DirectiveOccurrence<'a>> {
    let fences: Vec<(usize, usize)> = FencedBlocks::new(content)
        .map(|b| (b.body_start, b.close_end))
        .collect();
    let in_fence = |pos: usize| {
        fences
            .iter()
            .find(|&&(start, end)| pos >= start && pos < end)
    };

    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = content[cursor..].find(prefix) {
        let at = cursor + rel;
        let after_prefix = at + prefix.len();
        if at > 0 && bytes[at - 1] == b'\\' {
            cursor = after_prefix;
            continue;
        }
        // Count single backticks between the line start and the
        // occurrence: an odd count means it sits between `…` markers — a
        // quoted example in prose, not a directive. (Heuristic: double-
        // backtick spans are not modelled.)
        let line = content[..at]
            .rsplit_once('\n')
            .map_or(&content[..at], |(_, t)| t);
        let backticks_before = line.bytes().filter(|&b| b == b'`').count();
        if backticks_before % 2 == 1 {
            cursor = after_prefix;
            continue;
        }
        let fence_close_end = match (in_fence(at), policy) {
            (Some(&(_, close_end)), FencePolicy::Annotate) => Some(close_end),
            (Some(_), FencePolicy::SkipInside) => {
                cursor = after_prefix;
                continue;
            }
            (None, _) => None,
        };
        let Some(close_rel) = content[after_prefix..].find("}}") else {
            // No closing braces anywhere ahead — no later occurrence can
            // have them either.
            break;
        };
        let span_end = after_prefix + close_rel + 2;
        out.push(DirectiveOccurrence {
            args: &content[after_prefix..after_prefix + close_rel],
            span: at..span_end,
            fence_close_end,
        });
        cursor = span_end;
    }
    out
}

/// 1-based line number of `byte_offset` in `content`, for diagnostics.
pub(crate) fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan<'a>(content: &'a str, prefix: &str) -> Vec<DirectiveOccurrence<'a>> {
        scan_directives(content, prefix, FencePolicy::SkipInside)
    }

    #[test]
    fn scan_finds_occurrence_with_raw_args_and_full_span() {
        let s = "before {{#demo a b}} after";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].args, "a b");
        assert_eq!(got[0].span, 7..20);
        assert_eq!(&s[got[0].span.clone()], "{{#demo a b}}");
        assert_eq!(got[0].fence_close_end, None);
    }

    #[test]
    fn scan_skips_backslash_escaped_occurrence() {
        let s = "literal \\{{#demo a b}} and real {{#demo c d}}";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_skips_occurrence_inside_inline_backticks() {
        let s = "use `{{#demo a b}}` in prose\n";
        assert!(scan(s, "{{#demo ").is_empty());
    }

    #[test]
    fn scan_finds_occurrence_after_closed_inline_code_span() {
        let s = "the syntax is `{{#demo a b}}` and {{#demo c d}}\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_annotate_yields_fence_close_end_for_in_fence_occurrence() {
        let s = "```rust\n{{#demo a b}}\n```\nafter\n";
        let got = scan_directives(s, "{{#demo ", FencePolicy::Annotate);
        assert_eq!(got.len(), 1);
        let close_end = s
            .find("```\n")
            .map(|_| s.rfind("```\n").unwrap() + 4)
            .unwrap();
        assert_eq!(got[0].fence_close_end, Some(close_end));
    }

    #[test]
    fn scan_annotate_yields_none_close_end_outside_fence() {
        let got = scan_directives("{{#demo a b}}\n", "{{#demo ", FencePolicy::Annotate);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].fence_close_end, None);
    }

    #[test]
    fn scan_skip_inside_omits_in_fence_occurrence() {
        let s = "```\n{{#demo a b}}\n```\n{{#demo c d}}\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_skip_inside_resumes_past_opener_so_post_fence_directive_survives() {
        // The fenced opener has no closing braces until AFTER the fence;
        // consuming through them would swallow the real directive.
        let s = "```\n{{#demo a b\n```\n{{#demo c d}}\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_allows_close_braces_on_a_later_line() {
        let s = "{{#demo a\nb}} after\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].args, "a\nb");
    }

    #[test]
    fn scan_yields_nothing_when_close_braces_missing() {
        assert!(scan("{{#demo a b", "{{#demo ").is_empty());
    }

    #[test]
    fn scan_does_not_close_outer_fence_on_shorter_inner_fence() {
        let s = "````markdown\n```\n{{#demo a b}}\n````\n{{#demo c d}}\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn line_number_counts_newlines_before_offset_one_based() {
        let s = "one\ntwo\nthree\n";
        assert_eq!(line_number(s, 0), 1);
        assert_eq!(line_number(s, 4), 2);
        assert_eq!(line_number(s, s.find("three").unwrap()), 3);
    }
}
