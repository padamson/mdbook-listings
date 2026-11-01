//! Shared scanner for `{{#name …}}` directive occurrences in chapter
//! markdown. The include, diff, and callout passes each parse different
//! arguments and apply different fence policies, but the occurrence
//! grammar — escape handling, inline-code and HTML-comment detection, fence
//! membership — must agree between them or directives get consumed in one
//! pass that another would have left alone.

use std::ops::Range;

use crate::anchor::ListingWidth;
use crate::fence::{indent_before, line_start};

/// One unescaped directive occurrence outside inline code, HTML comments,
/// and code blocks the passes don't walk.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DirectiveOccurrence<'a> {
    /// Raw text between the prefix and the closing `}}`, untrimmed —
    /// argument grammar is the caller's business. May span lines: the
    /// closing braces are found by an unbounded forward search.
    pub(crate) args: &'a str,
    /// Byte range of the full `{{#… }}` text.
    pub(crate) span: Range<usize>,
    /// The fence the occurrence starts in the body of, when it is one the
    /// passes walk (only produced under [`FencePolicy::Annotate`]).
    pub(crate) fence: Option<HostFence<'a>>,
    /// What around a prose occurrence keeps the block a pass emits from
    /// rendering there: a blockquote, whose `>` the block's lines can't
    /// carry; an HTML block, where the browser shows markdown as text; or
    /// the paragraph above, when an indent of 4 or more makes the line a
    /// continuation of it. `None` in plain prose and in a fence.
    pub(crate) enclosure: Option<Misplacement>,
}

/// The fence an occurrence sits in, for the pass that replaces the
/// occurrence inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostFence<'a> {
    /// One past the closing fence line, where a trailing anchor goes.
    pub(crate) close_end: usize,
    /// The fence's prefix (see `FencedBlock::prefix`), for every line the
    /// pass puts inside or after it; the occurrence's own indent within
    /// the fence is the author's layout, not the page's.
    pub(crate) prefix: &'a str,
}

/// What to do with an occurrence that starts in the body of a fence the
/// passes walk. One in any other code block is never yielded.
#[derive(Clone, Copy, Debug)]
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
/// span are skipped so chapters can quote directive syntax verbatim, and so
/// are ones inside an HTML comment, which the page never shows: a listing
/// commented out there must not be numbered. So is any occurrence in a code
/// block the passes can't edit (indented, unclosed, or behind a `>`),
/// quoted or not. mdbook's `links` pass expands `{{#include}}` in
/// all of these all the same, so for includes only the backslash form is a
/// safe quote; the include pass reports the rest
/// (`include::rejected_includes`).
pub(crate) fn scan_directives<'a>(
    content: &'a str,
    prefix: &str,
    policy: FencePolicy,
) -> Vec<DirectiveOccurrence<'a>> {
    // Parsed only once a candidate turns up: most scans find nothing.
    let parsed = std::cell::OnceCell::new();
    let regions = || parsed.get_or_init(|| Regions::of(content));

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
        let (fence, enclosure) = match (regions().place(at), policy) {
            (Place::Prose, _) => (None, regions().enclosure(at)),
            (Place::Fence(fence), FencePolicy::Annotate) => (Some(fence), None),
            (Place::Fence(_), FencePolicy::SkipInside) | (Place::Skipped, _) => {
                cursor = after_prefix;
                continue;
            }
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
            fence,
            enclosure,
        });
        cursor = span_end;
    }
    out
}

/// Where a directive sits on the page CommonMark renders.
enum Place<'a> {
    /// Rendered markdown, or an HTML block, where a directive whose output
    /// is inline HTML still renders.
    Prose,
    /// The body of a fence the passes walk.
    Fence(HostFence<'a>),
    /// Hidden or quoted, or in a code block the passes can't edit.
    Skipped,
}

/// Where CommonMark leaves a directive unrendered, and which fenced block
/// holds it, from one parse of the chapter.
struct Regions<'a> {
    content: &'a str,
    /// Inline code spans, HTML comments, fence opener lines, and code blocks
    /// the passes can't edit: a directive there is a quoted example, hidden,
    /// part of an info string, or shown as written. A code span counts
    /// however many backticks open it and whatever line it ends on.
    hidden: Vec<Range<usize>>,
    /// HTML blocks, where CommonMark reads no markdown but the browser
    /// renders HTML.
    html: Vec<Range<usize>>,
    /// Blockquotes, with the lazy lines CommonMark lets continue a quoted
    /// paragraph without a `>`.
    quotes: Vec<Range<usize>>,
    /// List items, each with the column its content starts in, so an
    /// occurrence's indent reads against its item's text and not the
    /// margin.
    items: Vec<(Range<usize>, usize)>,
    /// The `[body_start, close_end)` of each block `fence::fenced_block`
    /// accepts, the same blocks `FencedBlocks` yields, with its prefix.
    fences: Vec<(Range<usize>, &'a str)>,
}

impl<'a> Regions<'a> {
    fn of(content: &'a str) -> Self {
        use pulldown_cmark::{Event, Tag};

        let mut hidden = Vec::new();
        let mut html = Vec::new();
        let mut quotes = Vec::new();
        let mut items = Vec::new();
        let mut fences = Vec::new();
        for (event, span) in crate::fence::cmark_parser(content).into_offset_iter() {
            match &event {
                Event::Code(_) => hidden.push(span),
                Event::Start(Tag::CodeBlock(_)) => {
                    match crate::fence::fenced_block(content, &event, span.clone()) {
                        // The opener's info string is never prose.
                        Some(b) => {
                            hidden.push(span.start..b.body_start);
                            fences.push((b.body_start..b.close_end, b.prefix));
                        }
                        // Indented, unclosed, or behind a `>`: the passes
                        // edit bytes and can't splice into it without
                        // breaking the page.
                        None => hidden.push(span),
                    }
                }
                Event::Start(Tag::HtmlBlock) => {
                    push_comments(content, span.clone(), &mut hidden);
                    html.push(span);
                }
                Event::InlineHtml(_) => push_comments(content, span, &mut hidden),
                Event::Start(Tag::BlockQuote(_)) => quotes.push(span),
                // The parse starts an item at its marker.
                Event::Start(Tag::Item) => {
                    items.push((span.clone(), item_content_column(content, span.start)));
                }
                _ => {}
            }
        }
        Regions {
            content,
            hidden,
            html,
            quotes,
            items,
            fences,
        }
    }

    /// What around `pos` keeps a block from rendering there, when something
    /// does. A quote counts only with its `>` on the occurrence's own line:
    /// without one the line is a lazy continuation of the quoted paragraph,
    /// which a block at the margin ends, so the block renders after the
    /// quote. A quote wins over an HTML block inside it: the `>` is the
    /// harder fact. An indent of 4 or more past the item's text, where no
    /// `>` or text precedes the occurrence, continues the paragraph above.
    fn enclosure(&self, pos: usize) -> Option<Misplacement> {
        let before = &self.content[line_start(self.content, pos)..pos];
        if within(&self.quotes, pos).is_some() && before.contains('>') {
            Some(Misplacement::BlockQuote)
        } else if within(&self.html, pos).is_some() {
            Some(Misplacement::HtmlBlock)
        } else if indent_before(self.content, pos)
            .is_some_and(|indent| indent.len() > self.item_content_column(pos) + 3)
        {
            Some(Misplacement::Indented)
        } else {
            None
        }
    }

    /// The column the innermost list item around `pos` starts its content
    /// in; 0 outside any item. Items are recorded in document order, so of
    /// those holding `pos` the last is the innermost.
    fn item_content_column(&self, pos: usize) -> usize {
        self.items
            .iter()
            .rev()
            .find(|(item, _)| item.contains(&pos))
            .map_or(0, |(_, column)| *column)
    }

    /// Where a directive at `pos` sits. In a fence body or an HTML block the
    /// backticks are the sample's own quoting (a markdown example showing
    /// `` `{{#diff a b}}` ``), so an odd count before the directive on its
    /// line marks it as quoted.
    fn place(&self, pos: usize) -> Place<'a> {
        if within(&self.hidden, pos).is_some() {
            return Place::Skipped;
        }
        if let Some((fence, prefix)) = self.fences.iter().find(|(r, _)| r.contains(&pos)) {
            return if self.quoted_on_its_line(pos) {
                Place::Skipped
            } else {
                Place::Fence(HostFence {
                    close_end: fence.end,
                    prefix,
                })
            };
        }
        if within(&self.html, pos).is_some() && self.quoted_on_its_line(pos) {
            return Place::Skipped;
        }
        Place::Prose
    }

    fn quoted_on_its_line(&self, pos: usize) -> bool {
        let line = &self.content[line_start(self.content, pos)..pos];
        line.bytes().filter(|&b| b == b'`').count() % 2 == 1
    }
}

fn within(ranges: &[Range<usize>], pos: usize) -> Option<&Range<usize>> {
    ranges.iter().find(|r| r.contains(&pos))
}

/// Push the HTML comments in an HTML block or inline span onto `out`. A comment ends at its
/// first `-->`, or with the block or span that holds it; a `<!--` anywhere
/// CommonMark reads as text (in code, after a backslash, in a paragraph that
/// ends before any `-->`) never reaches here.
fn push_comments(content: &str, span: Range<usize>, out: &mut Vec<Range<usize>>) {
    let html = &content[span.clone()];
    let mut cursor = 0;
    while let Some(rel) = html[cursor..].find("<!--") {
        let at = cursor + rel;
        let end = html[at + 2..]
            .find("-->")
            .map_or(html.len(), |e| at + 2 + e + 3);
        out.push(span.start + at..span.start + end);
        cursor = end;
    }
}

/// Why the block a directive expands to would not render as one where the
/// directive sits. Every pass that emits a block refuses the directive with
/// this, in the build and in `verify`, in the same words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Misplacement {
    /// Sharing its line with other text, so the block could not start one.
    MidLine,
    /// Inside a blockquote: each line inserted would need a `>`.
    BlockQuote,
    /// Inside an HTML block, where the browser shows markdown as text.
    HtmlBlock,
    /// Indented 4 or more past its list item's text, or the margin, under a
    /// line of text: markdown reads the line as more of that paragraph, so
    /// no block can start on it.
    Indented,
    /// In a fence an earlier directive already fills: a block holds one
    /// listing, and the anchor after the fence names one.
    SharedFence,
}

impl Misplacement {
    /// The problem and its fix, to follow the directive's own text.
    pub fn describe(self) -> &'static str {
        match self {
            Misplacement::MidLine => {
                "shares its line with other text; put it on a line of its own so the block \
                 it expands to can start one"
            }
            Misplacement::BlockQuote => {
                "is inside a blockquote, and the lines it expands to can't carry the `>`; \
                 put it after the quote"
            }
            Misplacement::HtmlBlock => {
                "is inside an HTML block, where what it expands to shows as text; add a \
                 blank line between the HTML and the directive"
            }
            Misplacement::Indented => {
                "is indented 4 or more past the text above it, which makes it more of that \
                 paragraph; outdent it so the block it expands to can start one"
            }
            Misplacement::SharedFence => {
                "is the second directive in its code block, which holds one listing; give \
                 it a block of its own"
            }
        }
    }
}

/// The column a list item's content starts in, read from its marker at
/// `marker_start`: the marker, then the 1 to 4 blanks after it, or one
/// blank when there are 5 or more (the rest is indented code) or the line
/// ends at the marker.
fn item_content_column(content: &str, marker_start: usize) -> usize {
    let line = &content[marker_start..];
    let line = line.split_once('\n').map_or(line, |(l, _)| l);
    let marker = line.find([' ', '\t']).unwrap_or(line.len());
    let blanks = line[marker..]
        .bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count();
    let blanks = if (1..=4).contains(&blanks) { blanks } else { 1 };
    marker_start - line_start(content, marker_start) + marker + blanks
}

/// The diagnostic for `directive` (its text as written) sitting where `why`
/// says it can't, as the build and `verify` both print it after their
/// `file:line:` prefix.
pub fn misplaced_message(directive: &str, why: Misplacement) -> String {
    format!("{directive} {}", why.describe())
}

/// Why the block the directive at `occ` expands to would not render as one,
/// or `None` when it would: the enclosure the scan recorded, else text
/// sharing its line.
pub(crate) fn misplacement(content: &str, occ: &DirectiveOccurrence) -> Option<Misplacement> {
    occ.enclosure.or_else(|| {
        indent_before(content, occ.span.start)
            .is_none()
            .then_some(Misplacement::MidLine)
    })
}

/// 1-based line number of `byte_offset` in `content`, for diagnostics.
pub(crate) fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Lift a `caption="..."` token off a directive's args, returning the
/// remaining args and the caption text. Shared so the include and diff
/// passes parse captions identically. The first `"` ends the value.
pub(crate) fn split_caption(args: &str) -> (String, Option<String>) {
    split_quoted_kv(args, "caption=\"")
}

/// Lift a `label="..."` token off a directive's args — the stable name a
/// `{{#listing-ref}}` resolves to the listing's current number. Same
/// grammar as `caption=`.
pub(crate) fn split_label(args: &str) -> (String, Option<String>) {
    split_quoted_kv(args, "label=\"")
}

/// Lift a `lang="..."` token off a directive's args — the highlight
/// language for a self-contained `{{#include}}`, overriding the one its
/// file extension implies. Same grammar as `caption=`.
pub(crate) fn split_lang(args: &str) -> (String, Option<String>) {
    split_quoted_kv(args, "lang=\"")
}

/// Lift a `show-provenance="true|false"` token off a directive's args —
/// the per-directive override for the book-level `show-listing-provenance`
/// flag, in either direction. Same grammar as `caption=`; a value that is
/// neither `true` nor `false` is dropped along with the token, leaving the
/// book-level flag to decide rather than failing the build.
pub(crate) fn split_show_provenance(args: &str) -> (String, Option<bool>) {
    let (rest, value) = split_quoted_kv(args, "show-provenance=\"");
    (rest, value.and_then(|v| v.parse::<bool>().ok()))
}

/// Lift a `width="normal|wide"` token off a directive's args — the
/// per-directive override for the book-level `listing-width`. An unknown
/// value is dropped with the token, as for `show-provenance`.
pub(crate) fn split_width(args: &str) -> (String, Option<ListingWidth>) {
    let (rest, value) = split_quoted_kv(args, "width=\"");
    (rest, value.as_deref().and_then(ListingWidth::parse))
}

/// Lift a `wrap="true|false"` token off a directive's args — the
/// per-directive override for the book-level `listing-wrap`. An unknown
/// value is dropped with the token, as for `show-provenance`.
pub(crate) fn split_wrap(args: &str) -> (String, Option<bool>) {
    let (rest, value) = split_quoted_kv(args, "wrap=\"");
    (rest, value.and_then(|v| v.parse::<bool>().ok()))
}

/// Shared lifter for `key="value"` directive arguments. The first `"` ends
/// the value; an unterminated value leaves the args untouched rather than
/// guess. The key may sit anywhere among the args; the surrounding tokens
/// are re-joined so several quoted keys (`label="x" caption="y"`) can be
/// lifted in sequence, in either order.
fn split_quoted_kv(args: &str, key: &str) -> (String, Option<String>) {
    // Only a whole token: `nowrap="…"` holds `wrap="` but is not that key.
    let Some(start) = args.match_indices(key).map(|(i, _)| i).find(|&i| {
        args[..i]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    }) else {
        return (args.to_string(), None);
    };
    let value_start = start + key.len();
    let Some(rel_end) = args[value_start..].find('"') else {
        return (args.to_string(), None);
    };
    let value = args[value_start..value_start + rel_end].to_string();
    let before = args[..start].trim_end();
    let after = args[value_start + rel_end + 1..].trim_start();
    let rest = if after.is_empty() {
        before.to_string()
    } else if before.is_empty() {
        after.to_string()
    } else {
        format!("{before} {after}")
    };
    (rest, Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan<'a>(content: &'a str, prefix: &str) -> Vec<DirectiveOccurrence<'a>> {
        scan_directives(content, prefix, FencePolicy::SkipInside)
    }

    #[test]
    fn split_label_lifts_quoted_label() {
        let (rest, label) = split_label("listings/foo.rs label=\"claim-layer\"");
        assert_eq!(rest, "listings/foo.rs");
        assert_eq!(label.as_deref(), Some("claim-layer"));
    }

    #[test]
    fn split_label_none_when_absent() {
        let (rest, label) = split_label("a b 1:2 1:3");
        assert_eq!(rest, "a b 1:2 1:3");
        assert_eq!(label, None);
    }

    #[test]
    fn caption_and_label_lift_in_either_order() {
        let (rest, caption) = split_caption("a b label=\"x\" caption=\"The cap\"");
        let (rest, label) = split_label(&rest);
        assert_eq!(rest, "a b");
        assert_eq!(caption.as_deref(), Some("The cap"));
        assert_eq!(label.as_deref(), Some("x"));

        let (rest, caption) = split_caption("caption=\"The cap\" label=\"x\" a b");
        let (rest, label) = split_label(&rest);
        assert_eq!(rest, "a b");
        assert_eq!(caption.as_deref(), Some("The cap"));
        assert_eq!(label.as_deref(), Some("x"));
    }

    #[test]
    fn split_label_unterminated_left_untouched() {
        let (rest, label) = split_label("a b label=\"oops");
        assert_eq!(rest, "a b label=\"oops");
        assert_eq!(label, None);
    }

    #[test]
    fn scan_finds_occurrence_with_raw_args_and_full_span() {
        let s = "before {{#demo a b}} after";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].args, "a b");
        assert_eq!(got[0].span, 7..20);
        assert_eq!(&s[got[0].span.clone()], "{{#demo a b}}");
        assert_eq!(got[0].fence, None);
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
    fn scan_skips_occurrence_inside_a_double_backtick_span() {
        // Two backticks precede the directive, an even count, but it sits
        // inside the ``…`` span.
        assert!(scan("quote ``{{#demo a b}}`` here\n", "{{#demo ").is_empty());
    }

    #[test]
    fn scan_finds_occurrence_after_a_double_backtick_span_holding_a_backtick() {
        let got = scan("``a ` b`` then {{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_skips_occurrence_inside_a_code_span_that_crosses_a_line_break() {
        let got = scan(
            "A span `quoting\n{{#demo a b}}` across lines.\n",
            "{{#demo ",
        );
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn scan_finds_occurrence_after_a_backslash_escaped_backtick() {
        let got = scan("A literal \\` then {{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_annotate_skips_a_backtick_quoted_occurrence_inside_a_fence() {
        // A markdown sample quoting the directive; CommonMark has no code
        // spans inside a fence, so the line's own backticks decide.
        let s = "```markdown\nUse `{{#demo a b}}` here.\n{{#demo c d}}\n```\n";
        let got = scan_directives(s, "{{#demo ", FencePolicy::Annotate);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_skips_a_backtick_quoted_occurrence_in_an_html_block() {
        // An HTML block has no inline code to parse, so the sample's own
        // backticks mark the directive as quoted.
        let got = scan("<div>\nUse `{{#demo a b}}`\n</div>\n", "{{#demo ");
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn scan_skips_an_unquoted_occurrence_in_a_code_block_the_passes_dont_walk() {
        // The passes edit bytes and can't carry a `> ` through, so a splice
        // into one of these blocks breaks the page.
        for (what, s) in [
            (
                "blockquoted fence",
                "> ```markdown\n> {{#demo a b}}\n> ```\n\n{{#demo c d}}\n",
            ),
            (
                "indented code",
                "Example:\n\n    {{#demo a b}}\n\n{{#demo c d}}\n",
            ),
            (
                "unclosed fence",
                "{{#demo c d}}\n\n```markdown\n{{#demo a b}}\n",
            ),
        ] {
            for policy in [FencePolicy::SkipInside, FencePolicy::Annotate] {
                let got = scan_directives(s, "{{#demo ", policy);
                let args: Vec<_> = got.iter().map(|o| o.args).collect();
                assert_eq!(args, ["c d"], "{what}, {policy:?}");
            }
        }
    }

    #[test]
    fn scan_annotate_yields_close_end_for_an_occurrence_in_a_fence_in_a_list_item() {
        // The passes carry the item's indent through, so the fence is
        // walked like one at the top level.
        let s = "10. Step\n\n    ```markdown\n    {{#demo a b}}\n    ```\n\n{{#demo c d}}\n";
        let got = scan_directives(s, "{{#demo ", FencePolicy::Annotate);
        let args: Vec<_> = got.iter().map(|o| o.args).collect();
        assert_eq!(args, ["a b", "c d"]);
        let close_end = s.find("    ```\n").unwrap() + "    ```\n".len();
        assert_eq!(
            got[0].fence,
            Some(HostFence {
                close_end,
                prefix: "    "
            })
        );
        assert_eq!(got[1].fence, None);
        let skipped: Vec<_> = scan(s, "{{#demo ").iter().map(|o| o.args).collect();
        assert_eq!(skipped, ["c d"]);
    }

    #[test]
    fn scan_records_the_blockquote_or_html_block_around_a_prose_occurrence() {
        for (what, s, want) in [
            ("plain prose", "{{#demo a b}}\n", None),
            ("list item", "- Step\n\n  {{#demo a b}}\n", None),
            (
                "blockquote",
                "> {{#demo a b}}\n",
                Some(Misplacement::BlockQuote),
            ),
            (
                "quoted line of a quoted paragraph",
                "> text\n> {{#demo a b}}\n",
                Some(Misplacement::BlockQuote),
            ),
            (
                "mid-line in a quote",
                "> see {{#demo a b}}\n",
                Some(Misplacement::BlockQuote),
            ),
            // A block at the margin ends the quoted paragraph, so the
            // block renders after the quote where the author put it.
            (
                "lazy continuation of a quoted paragraph",
                "> text\n{{#demo a b}}\n",
                None,
            ),
            (
                "lazy continuation of a quoted paragraph in a list item",
                "- > text\n  {{#demo a b}}\n",
                None,
            ),
            (
                "html block",
                "<div>\n{{#demo a b}}\n</div>\n",
                Some(Misplacement::HtmlBlock),
            ),
            (
                "html block inside a blockquote",
                "> <div>\n> {{#demo a b}}\n> </div>\n",
                Some(Misplacement::BlockQuote),
            ),
        ] {
            let got = scan(s, "{{#demo ");
            assert_eq!(got.len(), 1, "{what}: got {got:?}");
            assert_eq!(got[0].enclosure, want, "{what}");
        }
    }

    #[test]
    fn scan_records_an_indent_that_continues_the_paragraph_above() {
        // Four or more past the container's text, a line can't start a
        // block: it is more of the paragraph above it. The container is the
        // innermost list item, whose content column its marker sets.
        for (what, s, want) in [
            (
                "four at the top level",
                "Some text\n    {{#demo a b}}\n",
                Some(Misplacement::Indented),
            ),
            (
                "three at the top level",
                "Some text\n   {{#demo a b}}\n",
                None,
            ),
            (
                "four past the item's text",
                "- item\n      {{#demo a b}}\n",
                Some(Misplacement::Indented),
            ),
            (
                "three past the item's text",
                "- item\n     {{#demo a b}}\n",
                None,
            ),
            (
                "four past a wide marker's text",
                "10. item\n        {{#demo a b}}\n",
                Some(Misplacement::Indented),
            ),
            (
                "three past a wide marker's text",
                "10. item\n       {{#demo a b}}\n",
                None,
            ),
            (
                "four past a nested item's text",
                "- a\n  - b\n        {{#demo a b}}\n",
                Some(Misplacement::Indented),
            ),
            (
                "three past a nested item's text",
                "- a\n  - b\n       {{#demo a b}}\n",
                None,
            ),
            (
                "four under a quoted line",
                "> text\n    {{#demo a b}}\n",
                Some(Misplacement::Indented),
            ),
            (
                "four past the text of a later sibling item",
                "- a\n- b\n      {{#demo a b}}\n",
                Some(Misplacement::Indented),
            ),
        ] {
            let got = scan(s, "{{#demo ");
            assert_eq!(got.len(), 1, "{what}: got {got:?}");
            assert_eq!(got[0].enclosure, want, "{what}");
        }
    }

    #[test]
    fn item_content_column_is_the_marker_then_one_to_four_blanks() {
        for (what, s, marker_at, want) in [
            ("bullet", "- x\n", 0, 2),
            ("one-digit ordinal", "1. x\n", 0, 3),
            ("two-digit ordinal", "10. x\n", 0, 4),
            ("three blanks", "-   x\n", 0, 4),
            ("four blanks", "-    x\n", 0, 5),
            ("five blanks is indented code after one", "-     x\n", 0, 2),
            ("empty item", "-\n  x\n", 0, 2),
            ("nested marker", "- a\n  - x\n", 6, 4),
            ("task marker", "- [ ] x\n", 0, 2),
        ] {
            assert_eq!(item_content_column(s, marker_at), want, "{what}");
        }
    }

    #[test]
    fn an_occurrence_in_a_fence_has_no_enclosure() {
        let got = scan_directives(
            "- Step\n\n  ```rust\n  {{#demo a b}}\n  ```\n",
            "{{#demo ",
            FencePolicy::Annotate,
        );
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].enclosure, None);
    }

    #[test]
    fn misplacement_is_the_enclosure_else_the_text_sharing_the_line() {
        for (what, s, want) in [
            ("own line", "  {{#demo a b}}\n", None),
            (
                "mid-line",
                "see {{#demo a b}}\n",
                Some(Misplacement::MidLine),
            ),
            (
                "list marker on the line",
                "- {{#demo a b}}\n",
                Some(Misplacement::MidLine),
            ),
            (
                "blockquote",
                "> {{#demo a b}}\n",
                Some(Misplacement::BlockQuote),
            ),
        ] {
            let got = scan(s, "{{#demo ");
            assert_eq!(misplacement(s, &got[0]), want, "{what}");
        }
    }

    #[test]
    fn misplaced_message_names_the_directive_then_what_is_around_it() {
        for (why, word) in [
            (Misplacement::MidLine, "line"),
            (Misplacement::BlockQuote, "blockquote"),
            (Misplacement::HtmlBlock, "HTML block"),
            (Misplacement::Indented, "indented"),
            (Misplacement::SharedFence, "code block"),
        ] {
            let msg = misplaced_message("{{#diff a b}}", why);
            assert!(msg.starts_with("{{#diff a b}} "), "{why:?}: got {msg}");
            assert!(msg.contains(word), "{why:?}: got {msg}");
        }
    }

    #[test]
    fn scan_splits_a_table_row_before_reading_its_code_spans() {
        // mdBook renders with tables on: the pipe ends the cell, so the
        // backticks on either side never pair into a span.
        let s = "| a | b |\n|---|---|\n| `x | {{#demo a b}} y` |\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_annotate_yields_the_fence_for_an_in_fence_occurrence() {
        // The fence's prefix, not the occurrence's own indent, is what the
        // pass carries onto the lines it writes.
        let s = "```rust\n    {{#demo a b}}\n```\nafter\n";
        let got = scan_directives(s, "{{#demo ", FencePolicy::Annotate);
        assert_eq!(got.len(), 1);
        let close_end = s.rfind("```\n").unwrap() + 4;
        assert_eq!(
            got[0].fence,
            Some(HostFence {
                close_end,
                prefix: ""
            })
        );
    }

    #[test]
    fn scan_annotate_yields_no_fence_outside_one() {
        let got = scan_directives("{{#demo a b}}\n", "{{#demo ", FencePolicy::Annotate);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].fence, None);
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
    fn scan_skips_an_occurrence_in_a_fence_openers_info_string() {
        // An even backtick count (or none, for tildes) on the opener line
        // used to read as unquoted, and the diff pass spliced into the fence.
        for (what, s) in [
            (
                "four backticks",
                "````markdown {{#demo a b}}\nbody\n````\n\n{{#demo c d}}\n",
            ),
            (
                "tildes",
                "~~~markdown {{#demo a b}}\nbody\n~~~\n\n{{#demo c d}}\n",
            ),
            (
                "blockquoted",
                "> ````markdown {{#demo a b}}\n> body\n> ````\n\n{{#demo c d}}\n",
            ),
            (
                "in a list item",
                "10. Step\n\n    ````markdown {{#demo a b}}\n    body\n    ````\n\n{{#demo c d}}\n",
            ),
            (
                "unclosed",
                "{{#demo c d}}\n\n````markdown {{#demo a b}}\nbody\n",
            ),
            (
                "opener at end of file",
                "{{#demo c d}}\n\n````markdown {{#demo a b}}",
            ),
        ] {
            for policy in [FencePolicy::SkipInside, FencePolicy::Annotate] {
                let got = scan_directives(s, "{{#demo ", policy);
                let args: Vec<_> = got.iter().map(|o| o.args).collect();
                assert_eq!(args, ["c d"], "{what}, {policy:?}");
            }
        }
    }

    #[test]
    fn scan_skips_occurrence_inside_an_html_comment() {
        let s = "<!--\n{{#demo a b}}\n-->\n{{#demo c d}}\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_annotate_skips_a_fenced_occurrence_inside_an_html_comment() {
        let s = "<!--\n```rust\n{{#demo a b}}\n```\n-->\n";
        let got = scan_directives(s, "{{#demo ", FencePolicy::Annotate);
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn scan_skips_occurrence_inside_an_inline_html_comment() {
        let s = "Prose before the comment <!-- {{#demo a b}} --> {{#demo c d}}\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].args, "c d");
    }

    #[test]
    fn scan_treats_an_unclosed_comment_at_line_start_as_running_to_the_end() {
        // CommonMark: an HTML block opened by `<!--` ends only at `-->`.
        assert!(scan("<!--\n{{#demo a b}}\n", "{{#demo ").is_empty());
    }

    #[test]
    fn scan_treats_an_unclosed_mid_line_comment_opener_as_text() {
        // Inline, an unclosed `<!--` is not a comment at all.
        let got = scan("a <!-- b\n{{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_counts_the_shortest_comment_as_closed() {
        // `<!-->` is a complete comment, so it hides nothing after it.
        let got = scan("<!-->\n{{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_ignores_a_comment_opener_inside_a_fence() {
        let got = scan("```html\n<!--\n```\n{{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_ignores_a_comment_opener_in_inline_code() {
        let got = scan("Write `<!--` to open.\n{{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_ignores_a_backslash_escaped_comment_opener() {
        let got = scan("\\<!-- {{#demo a b}} -->\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_finds_raw_html_text_before_a_comment_in_the_same_block() {
        let got = scan("<div>{{#demo a b}}<!-- c --></div>\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_skips_a_comment_partway_into_an_html_block() {
        let got = scan("Intro.\n\n<div><!-- {{#demo a b}} --></div>\n", "{{#demo ");
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn scan_finds_raw_html_text_right_after_a_comment_in_the_same_block() {
        let got = scan("Intro.\n\n<div><!-- c -->{{#demo a b}}</div>\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_ends_an_unclosed_inline_opener_with_its_paragraph() {
        // The `-->` inside the later fence must not close the prose opener.
        let s = "Prose mentions <!-- here\n\n{{#demo a b}}\n\n```html\n<!-- x -->\n```\n";
        let got = scan(s, "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_skips_occurrence_inside_a_comment_in_a_blockquote() {
        let got = scan("> <!--\n> {{#demo a b}}\n> -->\n", "{{#demo ");
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn scan_ignores_a_comment_opener_in_an_indented_code_block() {
        let got = scan("    <!--\n\n{{#demo a b}}\n", "{{#demo ");
        assert_eq!(got.len(), 1, "got {got:?}");
    }

    #[test]
    fn scan_skips_a_comment_that_opens_where_a_fence_closes() {
        let got = scan("```\nx\n```\n<!--\n{{#demo a b}}\n-->\n", "{{#demo ");
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn scan_finds_a_block_comment_after_an_unclosed_inline_opener() {
        let got = scan("a <!-- b\n\n<!--\n{{#demo a b}}\n", "{{#demo ");
        assert!(got.is_empty(), "got {got:?}");
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

    #[test]
    fn split_caption_lifts_trailing_quoted_caption_with_spaces() {
        let (rest, cap) = split_caption("listings/foo.rs caption=\"The claim layer\"");
        assert_eq!(rest, "listings/foo.rs");
        assert_eq!(cap.as_deref(), Some("The claim layer"));
    }

    #[test]
    fn split_caption_none_when_absent() {
        let (rest, cap) = split_caption("a b 1:2 1:3");
        assert_eq!(rest, "a b 1:2 1:3");
        assert_eq!(cap, None);
    }

    #[test]
    fn split_show_provenance_lifts_both_boolean_values() {
        let (rest, show) = split_show_provenance("listings/foo.rs show-provenance=\"false\"");
        assert_eq!(rest, "listings/foo.rs");
        assert_eq!(show, Some(false));

        let (rest, show) = split_show_provenance("a b show-provenance=\"true\"");
        assert_eq!(rest, "a b");
        assert_eq!(show, Some(true));
    }

    #[test]
    fn split_show_provenance_none_when_absent() {
        let (rest, show) = split_show_provenance("a b 1:2");
        assert_eq!(rest, "a b 1:2");
        assert_eq!(show, None);
    }

    #[test]
    fn split_show_provenance_drops_a_non_boolean_value() {
        // The token is consumed but yields no override, so the book-level
        // flag decides. Failing the build over a typo in an opt-in display
        // argument would be a harsher response than the feature warrants.
        let (rest, show) = split_show_provenance("a b show-provenance=\"yes\"");
        assert_eq!(rest, "a b");
        assert_eq!(show, None);
    }

    #[test]
    fn split_width_lifts_both_widths() {
        let (rest, width) = split_width("listings/foo.rs width=\"wide\"");
        assert_eq!(rest, "listings/foo.rs");
        assert_eq!(width, Some(ListingWidth::Wide));

        let (rest, width) = split_width("a b width=\"normal\" 1:2 1:3");
        assert_eq!(rest, "a b 1:2 1:3");
        assert_eq!(width, Some(ListingWidth::Normal));
    }

    #[test]
    fn split_wrap_lifts_either_value_and_drops_an_unknown_one() {
        for (args, want) in [
            ("listings/foo.rs wrap=\"true\"", Some(true)),
            ("listings/foo.rs wrap=\"false\"", Some(false)),
            ("listings/foo.rs wrap=\"yes\"", None),
            ("listings/foo.rs", None),
        ] {
            assert_eq!(
                split_wrap(args),
                ("listings/foo.rs".to_string(), want),
                "{args}"
            );
        }
    }

    #[test]
    fn a_key_is_lifted_only_as_a_whole_token() {
        // `nowrap="true"` is not `wrap="true"` with a stray `no` left behind.
        let args = "listings/foo.rs nowrap=\"true\"";
        assert_eq!(split_wrap(args), (args.to_string(), None));
        assert_eq!(
            split_wrap("nowrap=\"x\" wrap=\"true\""),
            ("nowrap=\"x\"".to_string(), Some(true)),
            "the whole-token occurrence after it is still found"
        );
    }

    #[test]
    fn split_width_none_when_absent() {
        let (rest, width) = split_width("a b 1:2");
        assert_eq!(rest, "a b 1:2");
        assert_eq!(width, None);
    }

    #[test]
    fn split_width_drops_an_unknown_value() {
        // As with show-provenance: the token goes, and the book default
        // decides.
        let (rest, width) = split_width("a b width=\"full\"");
        assert_eq!(rest, "a b");
        assert_eq!(width, None);
    }

    #[test]
    fn split_caption_leaves_args_intact_on_unterminated_quote() {
        let (rest, cap) = split_caption("a b caption=\"oops");
        assert_eq!(rest, "a b caption=\"oops");
        assert_eq!(cap, None);
    }
}
