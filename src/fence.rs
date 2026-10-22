//! CommonMark fenced-code-block walking, shared by the include, diff, and
//! callout passes so they all agree on what is and isn't inside a fence.

/// A fence opener's shape. The closer must match `char` and reach at least
/// `count` — tracking both is what keeps a shorter same-character fence
/// inside an outer block (e.g. a 3-backtick example inside a 4-backtick
/// fence) from closing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fence {
    pub(crate) char: u8,
    pub(crate) count: usize,
}

/// One closed fenced block, with byte offsets into the scanned content.
///
/// Span semantics: the block's membership range is
/// `[body_start, close_end)` — the opener line is excluded, the closing
/// fence line is included. Both the include and callout passes test
/// position membership against exactly this range, so a directive sitting
/// in an opener's info string counts as outside the block.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FencedBlock<'a> {
    /// The opener's info string, trimmed (empty when the fence has none).
    pub(crate) info: &'a str,
    /// The body between the fence lines, newline-inclusive; excludes the
    /// fence lines themselves.
    pub(crate) body: &'a str,
    /// Byte offset of the first body byte (just past the opener's newline).
    pub(crate) body_start: usize,
    /// One past the closing fence line's trailing newline, or the content
    /// length when the closer is the final line without one.
    pub(crate) close_end: usize,
}

/// A CommonMark parser with the options mdBook's renderer uses by default
/// (`mdbook_markdown::new_cmark_parser`), so every pass that asks "is this
/// code?" or "where does this block end?" reads the chapter as the page
/// does. Footnotes and definition lists are block containers that can end a
/// fence early; tables split a row before reading its code spans.
pub(crate) fn cmark_parser(content: &str) -> pulldown_cmark::Parser<'_> {
    use pulldown_cmark::Options;
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    opts.insert(Options::ENABLE_DEFINITION_LIST);
    opts.insert(Options::ENABLE_GFM);
    pulldown_cmark::Parser::new_ext(content, opts)
}

/// The closed fenced blocks of a chapter, in order. Blocks come from the
/// CommonMark parse, so a fence line inside an HTML comment or HTML block is
/// text, as it is on the page. Two limits keep the passes' byte-level
/// edits sound: the opener line itself must be a fence (indented at most 3
/// spaces, no `>` or list marker before it), since the passes do not strip
/// container prefixes from a body; and an unclosed fence yields nothing,
/// since without a closer there is no block.
pub(crate) struct FencedBlocks<'a> {
    blocks: std::vec::IntoIter<FencedBlock<'a>>,
}

impl<'a> FencedBlocks<'a> {
    pub(crate) fn new(content: &'a str) -> Self {
        use pulldown_cmark::{CodeBlockKind, Event, Tag};

        let mut blocks = Vec::new();
        for (event, range) in cmark_parser(content).into_offset_iter() {
            if !matches!(
                event,
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
            ) {
                continue;
            }
            if let Some(block) = closed_block(content, range) {
                blocks.push(block);
            }
        }
        FencedBlocks {
            blocks: blocks.into_iter(),
        }
    }
}

/// The block a parsed fenced code block spans, when its opener line is a
/// bare fence and its last line closes it.
fn closed_block(content: &str, range: std::ops::Range<usize>) -> Option<FencedBlock<'_>> {
    let line_start = content[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let opener_end = content[line_start..]
        .find('\n')
        .map_or(content.len(), |i| line_start + i);
    let (info, opener) = fence_open_info(&content[line_start..opener_end])?;
    let block = content[..range.end].trim_end_matches('\n');
    let close_start = block.rfind('\n').map_or(0, |i| i + 1);
    if close_start <= line_start || !line_closes_fence(&block[close_start..], opener) {
        return None;
    }
    let close_end = (block.len() + 1).min(content.len());
    let body_start = opener_end + 1;
    Some(FencedBlock {
        info,
        body: &content[body_start..close_start],
        body_start,
        close_end,
    })
}

impl<'a> Iterator for FencedBlocks<'a> {
    type Item = FencedBlock<'a>;

    fn next(&mut self) -> Option<FencedBlock<'a>> {
        self.blocks.next()
    }
}

/// Parse `line` as a fence opener: at most 3 leading spaces, then 3+
/// backticks or tildes. Returns the trimmed info string and the fence
/// shape, or `None` when the line opens nothing.
pub(crate) fn fence_open_info(line: &str) -> Option<(&str, Fence)> {
    let trimmed = line.trim_start();
    let leading_spaces = line.len() - trimmed.len();
    if leading_spaces > 3 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let fence_char = match bytes.first()? {
        b'`' => b'`',
        b'~' => b'~',
        _ => return None,
    };
    let count = bytes.iter().take_while(|&&b| b == fence_char).count();
    if count < 3 {
        return None;
    }
    Some((
        trimmed[count..].trim(),
        Fence {
            char: fence_char,
            count,
        },
    ))
}

/// CommonMark closes a fenced block only with a fence of the same character
/// at least as long as the opener and a blank info string. Same-character
/// fences shorter than the opener stay inside the block as literal text —
/// which is what lets included source files contain `\`\`\`yaml` inside
/// string literals without prematurely terminating the outer fence.
pub(crate) fn line_closes_fence(line: &str, opener: Fence) -> bool {
    let trimmed = line.trim_start();
    let leading_spaces = line.len() - trimmed.len();
    if leading_spaces > 3 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let count = bytes.iter().take_while(|&&b| b == opener.char).count();
    if count < opener.count {
        return false;
    }
    trimmed[count..].trim().is_empty()
}

/// The fence to wrap `body` in so the body cannot close it: one longer
/// than the longest fence-shaped run already in the body, and never
/// shorter than CommonMark's minimum of 3.
///
/// Only line-initial runs are counted (at most 3 leading spaces, matching
/// [`line_closes_fence`]) — backticks inside a line can't close a block.
/// A run is counted whatever follows it, so a body containing an opener
/// like ` ```rust ` widens the wrapper too, even though that line would
/// not have closed anything. The wider fence is free and it keeps the
/// emitted block readable when the body is itself Markdown.
pub(crate) fn fence_for_body(body: &str, char: u8) -> Fence {
    let longest_run = body
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if line.len() - trimmed.len() > 3 {
                return 0;
            }
            trimmed.bytes().take_while(|&b| b == char).count()
        })
        .filter(|&run| run >= 3)
        .max()
        .unwrap_or(0);
    Fence {
        char,
        count: longest_run.saturating_add(1).max(3),
    }
}

impl Fence {
    /// The fence line itself, without a trailing newline.
    pub(crate) fn render(&self) -> String {
        (self.char as char).to_string().repeat(self.count)
    }
}

/// Render `body` as a self-contained fenced block carrying `info` as its
/// opener's info string. Both the include and diff splicers emit their
/// blocks through here so a listing and a diff of that same listing are
/// wrapped by identical rules.
///
/// Two normalisations the callers used to each do for themselves: `{{` in
/// the body is escaped to `\{{` so mdbook's built-in `links` preprocessor
/// leaves directive-shaped bytes inside a listing alone, and the body is
/// reduced to exactly one trailing newline so the closing fence lands on
/// its own line with no blank line above it.
pub(crate) fn render_block(info: &str, body: &str) -> String {
    let fence = fence_for_body(body, b'`').render();
    let body = body.trim_end_matches('\n').replace("{{", "\\{{");
    format!("{fence}{info}\n{body}\n{fence}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(content: &str) -> Vec<FencedBlock<'_>> {
        FencedBlocks::new(content).collect()
    }

    #[test]
    fn fenced_blocks_yields_block_with_info_body_and_byte_offsets() {
        let content = "before\n```rust\nlet x = 1;\n```\nafter\n";
        let got = blocks(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].info, "rust");
        assert_eq!(got[0].body, "let x = 1;\n");
        assert_eq!(got[0].body_start, 15);
        assert_eq!(got[0].close_end, 30);
        assert_eq!(
            &content[got[0].body_start..got[0].close_end],
            "let x = 1;\n```\n"
        );
    }

    #[test]
    fn fenced_blocks_trims_info_string() {
        let got = blocks("```  rust extra  \nbody\n```\n");
        assert_eq!(got[0].info, "rust extra");
    }

    #[test]
    fn fenced_blocks_yields_tilde_fenced_block() {
        let got = blocks("~~~yaml\nkey: value\n~~~\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].info, "yaml");
        assert_eq!(got[0].body, "key: value\n");
    }

    #[test]
    fn fenced_blocks_accepts_closer_longer_than_opener() {
        let got = blocks("```\nbody\n`````\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "body\n");
    }

    #[test]
    fn fenced_blocks_keeps_shorter_same_char_fence_inside_block() {
        let got = blocks("````rust\n```\ninner\n```\n````\n");
        assert_eq!(
            got.len(),
            1,
            "the 3-backtick lines must not close the 4-backtick fence"
        );
        assert_eq!(got[0].body, "```\ninner\n```\n");
    }

    #[test]
    fn fenced_blocks_does_not_close_backtick_fence_with_tildes() {
        let got = blocks("```\nbody\n~~~\nmore\n```\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "body\n~~~\nmore\n");
    }

    #[test]
    fn fenced_blocks_requires_blank_info_on_closer() {
        let got = blocks("```\nbody\n```rust\nmore\n```\n");
        assert_eq!(
            got.len(),
            1,
            "a fence line with an info string opens, never closes"
        );
        assert_eq!(got[0].body, "body\n```rust\nmore\n");
    }

    #[test]
    fn fenced_blocks_tolerates_three_space_indent_on_opener_and_closer() {
        let got = blocks("   ```\nbody\n   ```\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "body\n");
    }

    #[test]
    fn fenced_blocks_rejects_four_space_indented_opener() {
        // A 4-space "opener" is indented code, so the later bare ``` opens
        // a fence that never closes — no block is yielded.
        let got = blocks("    ```\nbody\n```\n");
        assert!(got.is_empty(), "got {got:?}");
    }

    #[test]
    fn fenced_blocks_four_space_indented_closer_does_not_close() {
        let got = blocks("```\nbody\n    ```\nmore\n```\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "body\n    ```\nmore\n");
    }

    #[test]
    fn fenced_blocks_yields_nothing_for_unclosed_fence_at_eof() {
        assert!(blocks("```rust\nlet x = 1;\n").is_empty());
    }

    #[test]
    fn fenced_blocks_close_end_is_eof_when_closer_lacks_trailing_newline() {
        let content = "```\nbody\n```";
        let got = blocks(content);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].close_end, content.len());
    }

    #[test]
    fn fenced_blocks_yields_multiple_blocks_in_order() {
        let got = blocks("```a\none\n```\n\n```b\ntwo\n```\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].info, "a");
        assert_eq!(got[1].info, "b");
        assert!(got[0].close_end <= got[1].body_start);
    }

    #[test]
    fn fenced_blocks_yields_empty_body_for_back_to_back_fences() {
        let got = blocks("```\n```\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "");
    }

    #[test]
    fn a_fence_line_inside_an_html_comment_opens_nothing() {
        let s = "<!--\n```\nold\n-->\n\n```rust\n// CALLOUT: x Note.\nfn a() {}\n```\n";
        let got = blocks(s);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].info, "rust");
        assert_eq!(got[0].body, "// CALLOUT: x Note.\nfn a() {}\n");
    }

    #[test]
    fn a_fence_line_inside_an_html_block_opens_nothing() {
        let s = "<div>\n```\n</div>\n\n```rust\nfn a() {}\n```\n";
        let got = blocks(s);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].info, "rust");
    }

    #[test]
    fn a_fence_in_a_list_item_indented_under_four_spaces_is_a_block() {
        let got = blocks("- item\n\n  ```rust\n  fn a() {}\n  ```\n");
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].body, "  fn a() {}\n");
    }

    #[test]
    fn a_fence_behind_a_blockquote_marker_is_not_walked() {
        // Its lines carry `> ` prefixes the passes do not strip.
        assert_eq!(blocks("> ```rust\n> fn a() {}\n> ```\n"), vec![]);
    }

    #[test]
    fn a_fence_inside_a_markdown_fence_is_body_text() {
        let s = "````markdown\n```rust\nfn a() {}\n```\n````\n";
        let got = blocks(s);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].info, "markdown");
    }

    #[test]
    fn a_fence_that_outlives_its_definition_list_item_is_not_closed() {
        // As mdBook renders it, the definition ends the indented block early
        // and the column-0 fence opens a new one that never closes.
        assert_eq!(
            blocks("Term\n: Def\n\n  ```rust\n  fn a() {}\n```\n"),
            vec![]
        );
    }

    #[test]
    fn fence_open_info_requires_three_fence_chars() {
        assert!(fence_open_info("``").is_none());
        assert!(fence_open_info("~~").is_none());
        assert!(fence_open_info("```").is_some());
        assert!(fence_open_info("~~~").is_some());
    }

    #[test]
    fn line_closes_fence_requires_opener_count() {
        let opener = Fence {
            char: b'`',
            count: 4,
        };
        assert!(!line_closes_fence("```", opener));
        assert!(line_closes_fence("````", opener));
        assert!(line_closes_fence("`````", opener));
    }

    #[test]
    fn fence_for_body_uses_the_commonmark_minimum_for_ordinary_source() {
        let fence = fence_for_body("fn main() {\n    println!(\"hi\");\n}\n", b'`');
        assert_eq!(fence.count, 3);
        assert_eq!(fence.render(), "```");
    }

    #[test]
    fn fence_for_body_outgrows_a_fence_the_body_contains() {
        // The whole point: a listing that is itself Markdown must not
        // close the block we wrap it in.
        let body = "Example:\n\n```rust\nfn main() {}\n```\n";
        let fence = fence_for_body(body, b'`');
        assert_eq!(fence.count, 4);
        assert!(!line_closes_fence("```", fence));
    }

    #[test]
    fn fence_for_body_outgrows_the_longest_of_several_fences() {
        let body = "```\na\n```\n\n`````\nb\n`````\n\n````\nc\n````\n";
        assert_eq!(fence_for_body(body, b'`').count, 6);
    }

    #[test]
    fn fence_for_body_ignores_backticks_that_are_not_line_initial() {
        // Inline code spans can't close a block, so they must not widen it.
        let body = "let doc = \"see ```rust``` above\";\n";
        assert_eq!(fence_for_body(body, b'`').count, 3);
    }

    #[test]
    fn fence_for_body_ignores_a_run_indented_past_three_spaces() {
        // Four spaces of indent puts the run out of closing range.
        assert_eq!(fence_for_body("    ```\n", b'`').count, 3);
        assert_eq!(fence_for_body("   ```\n", b'`').count, 4);
    }

    #[test]
    fn fence_for_body_ignores_runs_shorter_than_a_fence() {
        assert_eq!(fence_for_body("``\n`\n", b'`').count, 3);
    }

    #[test]
    fn fence_for_body_sizes_tilde_fences_independently() {
        let body = "~~~~\nnested\n~~~~\n";
        let fence = fence_for_body(body, b'~');
        assert_eq!(fence.count, 5);
        assert_eq!(fence.render(), "~~~~~");
        // Backticks in the body are irrelevant to a tilde fence.
        assert_eq!(fence_for_body("```\n", b'~').count, 3);
    }
}
