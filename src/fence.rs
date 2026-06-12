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

#[derive(Clone, Copy)]
struct OpenFence<'a> {
    info: &'a str,
    opener: Fence,
    body_start: usize,
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

/// Iterator over the closed fenced blocks of a chapter. An unclosed fence
/// at end-of-input yields nothing — without a closer there is no block.
pub(crate) struct FencedBlocks<'a> {
    content: &'a str,
    line_start: usize,
    open: Option<OpenFence<'a>>,
}

impl<'a> FencedBlocks<'a> {
    pub(crate) fn new(content: &'a str) -> Self {
        FencedBlocks {
            content,
            line_start: 0,
            open: None,
        }
    }
}

impl<'a> Iterator for FencedBlocks<'a> {
    type Item = FencedBlock<'a>;

    fn next(&mut self) -> Option<FencedBlock<'a>> {
        let content = self.content;
        let len = content.len();
        while self.line_start < len {
            let line_end = match content[self.line_start..].find('\n') {
                Some(off) => self.line_start + off,
                None => len,
            };
            let line = &content[self.line_start..line_end];
            let mut block = None;
            match self.open {
                None => {
                    if let Some((info, opener)) = fence_open_info(line) {
                        self.open = Some(OpenFence {
                            info,
                            opener,
                            body_start: line_end + 1,
                        });
                    }
                }
                Some(o) => {
                    if line_closes_fence(line, o.opener) {
                        let close_end = if line_end < len {
                            line_end + 1
                        } else {
                            line_end
                        };
                        block = Some(FencedBlock {
                            info: o.info,
                            body: &content[o.body_start..self.line_start],
                            body_start: o.body_start,
                            close_end,
                        });
                        self.open = None;
                    }
                }
            }
            self.line_start = if line_end == len { len } else { line_end + 1 };
            if block.is_some() {
                return block;
            }
        }
        None
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
}
