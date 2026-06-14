//! Parses `{{#diff <left> <right>}}` directives out of chapter markdown,
//! resolves each operand tag to bytes via the manifest, and renders the
//! pair as unified-diff text.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::directive::{FencePolicy, line_number, scan_directives};
use crate::manifest::Manifest;

/// `span` covers the directive in full (`{{#diff …}}` inclusive) so callers
/// can replace the whole substring in one pass.
///
/// `left_range` and `right_range` are present when the directive carries
/// optional 3rd and 4th `START:END` arguments — `{{#diff a b 1:50 1:60}}`
/// renders only those slices of each operand. Empty endpoints mean "to
/// start" or "to end". Two ranges (one per operand) because line numbers
/// shift between versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDirective {
    pub left: String,
    pub right: String,
    pub left_range: Option<LineRange>,
    pub right_range: Option<LineRange>,
    pub caption: Option<String>,
    pub span: Range<usize>,
}

/// 1-based inclusive line range. `None` endpoints mean "to start"
/// (`start`) or "to end" (`end`). Out-of-range endpoints are clamped to
/// the file's actual line count silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: Option<usize>,
    pub end: Option<usize>,
}

impl LineRange {
    /// Render as the `START:END` form used in directives and anchors.
    /// Empty endpoints render as the empty string.
    pub fn render(&self) -> String {
        let s = self.start.map(|n| n.to_string()).unwrap_or_default();
        let e = self.end.map(|n| n.to_string()).unwrap_or_default();
        format!("{s}:{e}")
    }

    /// Slice `text` to the range's line span, 1-based inclusive. Returns
    /// the substring that includes lines `[start..=end]` (clamped). The
    /// returned substring preserves trailing newlines.
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        let total = text.lines().count();
        let start_1 = self.start.unwrap_or(1).max(1);
        let end_1 = self.end.unwrap_or(total).min(total);
        if start_1 > end_1 || total == 0 {
            return "";
        }
        // Find byte offsets for line `start_1` and `end_1 + 1` (or EOF).
        let mut byte_start = 0usize;
        let mut current_line = 1usize;
        let bytes = text.as_bytes();
        while current_line < start_1 && byte_start < bytes.len() {
            match text[byte_start..].find('\n') {
                Some(off) => byte_start += off + 1,
                None => return "",
            }
            current_line += 1;
        }
        let mut byte_end = byte_start;
        let mut line = current_line;
        while line <= end_1 && byte_end < bytes.len() {
            match text[byte_end..].find('\n') {
                Some(off) => byte_end += off + 1,
                None => {
                    byte_end = bytes.len();
                    break;
                }
            }
            line += 1;
        }
        &text[byte_start..byte_end]
    }
}

/// Parses one `START:END` token. Returns `None` when the form is malformed
/// (the directive is then skipped just like a wrong-arity `{{#diff}}`).
/// Empty endpoints are allowed; `:` (both empty) means whole file. Numeric
/// endpoints must be positive (zero rejected).
pub fn parse_line_range(tok: &str) -> Option<LineRange> {
    let (s, e) = tok.split_once(':')?;
    let start = if s.is_empty() {
        None
    } else {
        let n: usize = s.parse().ok()?;
        if n == 0 {
            return None;
        }
        Some(n)
    };
    let end = if e.is_empty() {
        None
    } else {
        let n: usize = e.parse().ok()?;
        if n == 0 {
            return None;
        }
        Some(n)
    };
    Some(LineRange { start, end })
}

/// Returns every well-formed `{{#diff a b}}` directive in `content`.
/// Backslash-escaped directives, wrong-arity matches, and any directive
/// whose start byte falls inside a fenced code block are skipped — the
/// fence rule lets a chapter quote literal directive examples (e.g. a
/// frozen test fixture) without the preprocessor consuming them.
pub fn parse_directives(content: &str) -> Vec<DiffDirective> {
    let mut out = Vec::new();
    for occ in scan_directives(content, "{{#diff", FencePolicy::SkipInside) {
        let (args, caption) = crate::directive::split_caption(occ.args);
        let tokens: Vec<&str> = args.split_whitespace().collect();
        let parsed = match tokens.as_slice() {
            [l, r] => Some((l.to_string(), r.to_string(), None, None)),
            [l, r, lr, rr] => match (parse_line_range(lr), parse_line_range(rr)) {
                (Some(left_range), Some(right_range)) => Some((
                    l.to_string(),
                    r.to_string(),
                    Some(left_range),
                    Some(right_range),
                )),
                _ => None,
            },
            _ => None,
        };
        if let Some((left, right, left_range, right_range)) = parsed {
            out.push(DiffDirective {
                left,
                right,
                left_range,
                right_range,
                caption,
                span: occ.span,
            });
        }
    }
    out
}

/// Labels become the `--- <left_label>` / `+++ <right_label>` headers in
/// the rendered output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDiff {
    pub left_label: String,
    pub left_bytes: Vec<u8>,
    pub right_label: String,
    pub right_bytes: Vec<u8>,
}

/// `tag` is exposed so callers can compose a chapter-source-located
/// diagnostic that names the offending operand.
#[derive(Debug)]
pub struct ResolveError {
    pub tag: String,
    pub kind: ResolveErrorKind,
}

#[derive(Debug)]
pub enum ResolveErrorKind {
    UnknownTag,
    FrozenFileMissing {
        frozen_path: PathBuf,
        source: std::io::Error,
    },
    LiveFileMissing {
        live_path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ResolveErrorKind::UnknownTag => {
                write!(f, "no listing with tag `{}` in manifest", self.tag)
            }
            ResolveErrorKind::FrozenFileMissing {
                frozen_path,
                source,
            } => write!(
                f,
                "manifest tag `{}` references frozen file `{}` which cannot be read: {}",
                self.tag,
                frozen_path.display(),
                source,
            ),
            ResolveErrorKind::LiveFileMissing { live_path, source } => write!(
                f,
                "live operand `{}` cannot be read at `{}`: {}",
                self.tag,
                live_path.display(),
                source,
            ),
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ResolveErrorKind::UnknownTag => None,
            ResolveErrorKind::FrozenFileMissing { source, .. } => Some(source),
            ResolveErrorKind::LiveFileMissing { source, .. } => Some(source),
        }
    }
}

/// Returns at the first failing operand so callers surface one missing tag
/// at a time — the second tag's resolution can wait for the rebuild after
/// the first fix. `live_base` is the absolute directory `live:<rel_path>`
/// operands resolve against; the splicer passes the chapter's source
/// directory so authors can reference siblings the same way they would for
/// `{{#include}}`.
pub fn resolve(
    directive: &DiffDirective,
    manifest: &Manifest,
    book_root: &Path,
    live_base: &Path,
) -> Result<ResolvedDiff, ResolveError> {
    let (left_label, left_bytes) =
        resolve_operand(&directive.left, manifest, book_root, live_base)?;
    let (right_label, right_bytes) =
        resolve_operand(&directive.right, manifest, book_root, live_base)?;
    Ok(ResolvedDiff {
        left_label,
        left_bytes,
        right_label,
        right_bytes,
    })
}

fn resolve_operand(
    operand: &str,
    manifest: &Manifest,
    book_root: &Path,
    live_base: &Path,
) -> Result<(String, Vec<u8>), ResolveError> {
    if let Some(rel_path) = operand.strip_prefix("live:") {
        let live_path = live_base.join(rel_path);
        let bytes = std::fs::read(&live_path).map_err(|source| ResolveError {
            tag: operand.to_string(),
            kind: ResolveErrorKind::LiveFileMissing {
                live_path: live_path.clone(),
                source,
            },
        })?;
        return Ok((operand.to_string(), bytes));
    }
    let listing = manifest.find(operand).ok_or_else(|| ResolveError {
        tag: operand.to_string(),
        kind: ResolveErrorKind::UnknownTag,
    })?;
    let frozen_path = book_root.join(&listing.frozen);
    let bytes = std::fs::read(&frozen_path).map_err(|source| ResolveError {
        tag: operand.to_string(),
        kind: ResolveErrorKind::FrozenFileMissing {
            frozen_path: frozen_path.clone(),
            source,
        },
    })?;
    Ok((operand.to_string(), bytes))
}

/// Identical inputs return a one-line notice rather than the empty string
/// `similar` would otherwise produce — a fence body that's just the header
/// looks broken to a reader.
pub fn render(left: &str, right: &str, left_label: &str, right_label: &str) -> String {
    if left == right {
        return format!("(no changes between {left_label} and {right_label})\n");
    }
    similar::TextDiff::from_lines(left, right)
        .unified_diff()
        .context_radius(3)
        .header(left_label, right_label)
        .to_string()
}

/// Shift the line numbers in every `@@ -A,B +C,D @@` hunk header by
/// `left_offset` and `right_offset` respectively. Used when a sliced
/// `{{#diff a b LR LR}}` directive feeds only a fragment of each
/// listing to `similar` — without the shift the rendered hunk headers
/// would be relative to the slice (`@@ -3,18 +3,28 @@`) rather than the
/// absolute line numbers in the original files (`@@ -58,18 +148,28 @@`),
/// and readers would have no way to map a `+` line in the rendered diff
/// back to its position in the parent listing.
///
/// Hunk headers are the only diff syntax that carries line numbers, so
/// every other line passes through verbatim. Lines that look like `@@`
/// headers but aren't well-formed are left alone.
pub fn shift_hunk_headers(diff_text: &str, left_offset: usize, right_offset: usize) -> String {
    if left_offset == 0 && right_offset == 0 {
        return diff_text.to_string();
    }
    let mut out = String::with_capacity(diff_text.len());
    for line in diff_text.split_inclusive('\n') {
        let trailing_newline = line.ends_with('\n');
        let body = line.strip_suffix('\n').unwrap_or(line);
        if let Some(shifted) = shift_one_hunk_header(body, left_offset, right_offset) {
            out.push_str(&shifted);
            if trailing_newline {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Returns `Some(shifted_line)` when `body` is a well-formed unified-diff
/// `@@ -A[,B] +C[,D] @@[ context]` hunk header, with the line numbers
/// shifted by the offsets. Returns `None` otherwise so the caller passes
/// the line through verbatim.
fn shift_one_hunk_header(body: &str, left_offset: usize, right_offset: usize) -> Option<String> {
    let rest = body.strip_prefix("@@ ")?;
    // `rest` looks like "-A[,B] +C[,D] @@[ context]" — split off the
    // closing "@@" and the optional context that follows it.
    let (ranges, suffix) = rest.split_once(" @@")?;
    let parts: Vec<&str> = ranges.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let left = parts[0].strip_prefix('-')?;
    let right = parts[1].strip_prefix('+')?;
    let (l_start, l_count) = parse_hunk_range(left)?;
    let (r_start, r_count) = parse_hunk_range(right)?;
    Some(format!(
        "@@ -{},{} +{},{} @@{}",
        l_start + left_offset,
        l_count,
        r_start + right_offset,
        r_count,
        suffix,
    ))
}

fn parse_hunk_range(s: &str) -> Option<(usize, usize)> {
    if let Some((a, b)) = s.split_once(',') {
        Some((a.parse().ok()?, b.parse().ok()?))
    } else {
        let n: usize = s.parse().ok()?;
        Some((n, 1))
    }
}

/// Failure shape carrying enough chapter context to point an author straight
/// at the offending directive.
#[derive(Debug)]
pub struct SpliceError {
    pub chapter_path: Option<PathBuf>,
    pub line: usize,
    pub source: ResolveError,
}

impl std::fmt::Display for SpliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.chapter_path {
            Some(p) => write!(f, "{}:{}: {}", p.display(), self.line, self.source),
            None => write!(f, "<unknown chapter>:{}: {}", self.line, self.source),
        }
    }
}

impl std::error::Error for SpliceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Replace every `{{#diff …}}` directive in `content` with a fenced ` ```diff `
/// block of unified-diff text. Bytes outside those spans are copied through
/// unchanged. `chapter_dir` is the absolute directory the chapter's source
/// markdown lives in; `live:<rel>` operands resolve against it the same way
/// mdbook's `{{#include <rel>}}` does.
pub fn splice_chapter(
    content: &str,
    manifest: &Manifest,
    book_root: &Path,
    chapter_path: Option<&Path>,
    chapter_dir: &Path,
) -> Result<String, SpliceError> {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for d in parse_directives(content) {
        let resolved =
            resolve(&d, manifest, book_root, chapter_dir).map_err(|source| SpliceError {
                chapter_path: chapter_path.map(Path::to_path_buf),
                line: line_number(content, d.span.start),
                source,
            })?;
        let left_full = String::from_utf8_lossy(&resolved.left_bytes);
        let right_full = String::from_utf8_lossy(&resolved.right_bytes);
        let left_sliced: &str = match &d.left_range {
            Some(r) => r.slice(&left_full),
            None => &left_full,
        };
        let right_sliced: &str = match &d.right_range {
            Some(r) => r.slice(&right_full),
            None => &right_full,
        };
        let body = render(
            left_sliced,
            right_sliced,
            &resolved.left_label,
            &resolved.right_label,
        );
        // When a range is set, similar's hunk headers are relative to the
        // slice (line 1 of the slice = line N of the original). Shift them
        // back to absolute line numbers so readers can map a +/- line in
        // the rendered diff to its real position in the parent listing.
        let left_offset = d
            .left_range
            .and_then(|r| r.start)
            .map(|n| n - 1)
            .unwrap_or(0);
        let right_offset = d
            .right_range
            .and_then(|r| r.start)
            .map(|n| n - 1)
            .unwrap_or(0);
        let body = shift_hunk_headers(&body, left_offset, right_offset);
        // Escape `{{` in the rendered diff body — same reason as in
        // the include splicer.
        let body = body.replace("{{", "\\{{");
        out.push_str(&content[cursor..d.span.start]);
        out.push_str("```diff\n");
        out.push_str(&body);
        out.push_str("```\n");
        // CALLOUT: diff-anchor-dual Locator anchor for the capture-screenshots tool. Both operands are emitted as separate data-attributes so the tool can locate a diff block by its (LEFT, RIGHT) pair — unique even when multiple diffs share the same RIGHT tag, and unambiguous against the include splicer's `data-listing-tag` anchors.
        let mut anchor = format!(
            "<div data-listing-diff-left=\"{}\" data-listing-diff-right=\"{}\"",
            d.left, d.right,
        );
        if let Some(r) = &d.left_range {
            anchor.push_str(&format!(" data-listing-diff-left-range=\"{}\"", r.render()));
        }
        if let Some(r) = &d.right_range {
            anchor.push_str(&format!(
                " data-listing-diff-right-range=\"{}\"",
                r.render()
            ));
        }
        anchor.push_str(" aria-hidden=\"true\"></div>");
        out.push_str(&anchor);
        cursor = d.span.end;
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_directives_extracts_well_formed_directive() {
        let s = "before {{#diff old-tag new-tag}} after";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1, "expected one directive; got {got:?}");
        assert_eq!(got[0].left, "old-tag");
        assert_eq!(got[0].right, "new-tag");
        assert_eq!(&s[got[0].span.clone()], "{{#diff old-tag new-tag}}");
    }

    #[test]
    fn parse_directives_handles_multiple_occurrences() {
        let s = "{{#diff a b}} mid {{#diff c d}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].left, "a");
        assert_eq!(got[1].right, "d");
        assert_eq!(&s[got[0].span.clone()], "{{#diff a b}}");
        assert_eq!(&s[got[1].span.clone()], "{{#diff c d}}");
    }

    #[test]
    fn parse_directives_skips_escaped_form() {
        let s = "use \\{{#diff a b}} verbatim";
        let got = parse_directives(s);
        assert!(
            got.is_empty(),
            "escaped directive should not parse; got {got:?}",
        );
    }

    #[test]
    fn parse_directives_tolerates_extra_whitespace_around_operands() {
        let s = "{{#diff   a    b   }}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].left, "a");
        assert_eq!(got[0].right, "b");
    }

    #[test]
    fn parse_directives_accepts_optional_line_ranges() {
        let s = "{{#diff a b 1:50 1:60}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].left, "a");
        assert_eq!(got[0].right, "b");
        assert_eq!(
            got[0].left_range,
            Some(LineRange {
                start: Some(1),
                end: Some(50)
            })
        );
        assert_eq!(
            got[0].right_range,
            Some(LineRange {
                start: Some(1),
                end: Some(60)
            })
        );
    }

    #[test]
    fn parse_directives_extracts_caption_and_keeps_operands() {
        let s = "{{#diff a b caption=\"Method and act\"}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].left, "a");
        assert_eq!(got[0].right, "b");
        assert_eq!(got[0].caption.as_deref(), Some("Method and act"));
    }

    #[test]
    fn parse_directives_caption_coexists_with_ranges() {
        let s = "{{#diff a b 1:50 1:60 caption=\"Sliced\"}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1, "got {got:?}");
        assert_eq!(got[0].left, "a");
        assert_eq!(
            got[0].left_range,
            Some(LineRange {
                start: Some(1),
                end: Some(50)
            })
        );
        assert_eq!(got[0].caption.as_deref(), Some("Sliced"));
    }

    #[test]
    fn parse_directives_accepts_open_endpoints_in_ranges() {
        let cases = [
            (
                "{{#diff a b 200: 220:}}",
                LineRange {
                    start: Some(200),
                    end: None,
                },
                LineRange {
                    start: Some(220),
                    end: None,
                },
            ),
            (
                "{{#diff a b :100 :100}}",
                LineRange {
                    start: None,
                    end: Some(100),
                },
                LineRange {
                    start: None,
                    end: Some(100),
                },
            ),
            (
                "{{#diff a b : :}}",
                LineRange {
                    start: None,
                    end: None,
                },
                LineRange {
                    start: None,
                    end: None,
                },
            ),
        ];
        for (s, expected_l, expected_r) in cases {
            let got = parse_directives(s);
            assert_eq!(got.len(), 1, "input `{s}` -> {got:?}");
            assert_eq!(got[0].left_range, Some(expected_l), "input `{s}`");
            assert_eq!(got[0].right_range, Some(expected_r), "input `{s}`");
        }
    }

    #[test]
    fn parse_directives_rejects_malformed_or_negative_range() {
        for s in [
            "{{#diff a b 1 1}}",       // no colon — not a range
            "{{#diff a b 0:5 1:5}}",   // zero start rejected
            "{{#diff a b 1:0 1:5}}",   // zero end rejected
            "{{#diff a b 1:abc 1:5}}", // non-numeric
            "{{#diff a b -1:5 1:5}}",  // negative
            "{{#diff a b 1:5 1:5 x}}", // 5 args
        ] {
            let got = parse_directives(s);
            assert!(
                got.is_empty(),
                "malformed range directive `{s}` should not parse; got {got:?}",
            );
        }
    }

    #[test]
    fn line_range_slice_returns_inclusive_lines_with_trailing_newlines_preserved() {
        let text = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
        let r = LineRange {
            start: Some(2),
            end: Some(4),
        };
        assert_eq!(r.slice(text), "beta\ngamma\ndelta\n");
    }

    #[test]
    fn line_range_slice_handles_open_endpoints() {
        let text = "1\n2\n3\n4\n5\n";
        assert_eq!(
            LineRange {
                start: None,
                end: Some(2)
            }
            .slice(text),
            "1\n2\n",
        );
        assert_eq!(
            LineRange {
                start: Some(4),
                end: None
            }
            .slice(text),
            "4\n5\n",
        );
        assert_eq!(
            LineRange {
                start: None,
                end: None
            }
            .slice(text),
            "1\n2\n3\n4\n5\n",
        );
    }

    #[test]
    fn line_range_slice_clamps_out_of_range_endpoints() {
        let text = "1\n2\n3\n";
        assert_eq!(
            LineRange {
                start: Some(2),
                end: Some(999)
            }
            .slice(text),
            "2\n3\n",
            "end > line count clamps to end of file",
        );
        assert_eq!(
            LineRange {
                start: Some(999),
                end: Some(1000)
            }
            .slice(text),
            "",
            "fully out-of-range yields empty slice",
        );
    }

    #[test]
    fn shift_hunk_headers_rewrites_left_and_right_starts_by_offsets() {
        let diff = "--- a\n+++ b\n@@ -3,18 +3,28 @@\n context\n+added\n";
        let shifted = shift_hunk_headers(diff, 55, 145);
        assert!(
            shifted.contains("@@ -58,18 +148,28 @@"),
            "expected shifted hunk header; got:\n{shifted}",
        );
        assert!(
            shifted.contains("--- a\n+++ b\n"),
            "non-hunk lines must pass through unchanged; got:\n{shifted}",
        );
        assert!(
            shifted.contains(" context\n+added\n"),
            "body lines must pass through unchanged; got:\n{shifted}",
        );
    }

    #[test]
    fn shift_hunk_headers_handles_multiple_hunks_in_one_diff() {
        let diff = "--- a\n+++ b\n@@ -1,3 +1,3 @@\n line1\n@@ -10,2 +10,2 @@\n line10\n";
        let shifted = shift_hunk_headers(diff, 100, 200);
        assert!(
            shifted.contains("@@ -101,3 +201,3 @@"),
            "first hunk shifted; got:\n{shifted}",
        );
        assert!(
            shifted.contains("@@ -110,2 +210,2 @@"),
            "second hunk shifted; got:\n{shifted}",
        );
    }

    #[test]
    fn shift_hunk_headers_passes_zero_offsets_through_unchanged() {
        let diff = "--- a\n+++ b\n@@ -3,18 +3,28 @@\n line\n";
        assert_eq!(shift_hunk_headers(diff, 0, 0), diff);
    }

    #[test]
    fn shift_hunk_headers_handles_short_form_with_no_count() {
        // `@@ -A +C @@` (no `,B`/`,D`) is a valid unified-diff form when
        // the hunk is exactly one line on each side. The implementation
        // expands it to the explicit `,1` form when shifting.
        let diff = "@@ -3 +3 @@ context\n";
        let shifted = shift_hunk_headers(diff, 10, 20);
        assert!(
            shifted.contains("@@ -13,1 +23,1 @@ context"),
            "short form expanded with shift; got:\n{shifted}",
        );
    }

    #[test]
    fn shift_hunk_headers_leaves_malformed_at_at_lines_alone() {
        // `@@ ` followed by something that isn't a well-formed range
        // pair should not be rewritten.
        let diff = "@@ not a real header\nbody\n";
        assert_eq!(shift_hunk_headers(diff, 5, 5), diff);
    }

    #[test]
    fn line_range_slice_handles_single_line_range() {
        let text = "alpha\nbeta\ngamma\n";
        assert_eq!(
            LineRange {
                start: Some(2),
                end: Some(2)
            }
            .slice(text),
            "beta\n",
        );
    }

    #[test]
    fn parse_directives_skips_malformed_arity() {
        for s in ["{{#diff only-one}}", "{{#diff a b c}}", "{{#diff}}"] {
            let got = parse_directives(s);
            assert!(
                got.is_empty(),
                "malformed directive `{s}` should not parse; got {got:?}",
            );
        }
    }

    #[test]
    fn parse_directives_accepts_arbitrary_operand_strings() {
        let s = "{{#diff live:src/foo.rs new-tag}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].left, "live:src/foo.rs");
        assert_eq!(got[0].right, "new-tag");
    }

    #[test]
    fn parse_directives_skips_directives_inside_fenced_code_blocks() {
        let s = "outside {{#diff a b}}\n\n```rust\nlet s = \"{{#diff inner-a inner-b}}\";\n```\n\nmore {{#diff c d}}\n";
        let got = parse_directives(s);
        assert_eq!(got.len(), 2, "fenced one should be skipped; got {got:?}");
        assert_eq!(got[0].left, "a");
        assert_eq!(got[1].left, "c");
    }

    #[test]
    fn parse_directives_handles_tilde_fences() {
        let s = "~~~\n{{#diff a b}}\n~~~\n";
        assert!(parse_directives(s).is_empty());
    }

    #[test]
    fn parse_directives_does_not_close_outer_fence_on_shorter_inner_fence() {
        // CommonMark: a 3-backtick line inside a 4-backtick fence is
        // literal text, not a closer — so the {{#diff a b}} example below
        // it is still inside the fence and must not parse. The directive
        // after the real 4-backtick closer is the positive control.
        let s = concat!(
            "````markdown\n",
            "```\n",
            "{{#diff a b}}\n",
            "````\n",
            "{{#diff c d}}\n",
        );
        let got = parse_directives(s);
        assert_eq!(
            got.len(),
            1,
            "only the post-fence directive should parse; got {got:?}"
        );
        assert_eq!(got[0].left, "c");
        assert_eq!(got[0].right, "d");
    }

    #[test]
    fn parse_directives_skips_inside_inline_code_spans() {
        let s = "Use `{{#diff a b}}` in prose.\n";
        assert!(
            parse_directives(s).is_empty(),
            "directive inside inline backticks should be skipped",
        );
    }

    #[test]
    fn parse_directives_picks_up_directive_after_a_closed_inline_code_span() {
        let s = "the syntax is `{{#diff a b}}` and {{#diff c d}}\n";
        let got = parse_directives(s);
        assert_eq!(
            got.len(),
            1,
            "only the bare directive should parse; got {got:?}"
        );
        assert_eq!(got[0].left, "c");
        assert_eq!(got[0].right, "d");
    }

    use crate::manifest::{MANIFEST_VERSION, Manifest};
    use std::fs;
    use tempfile::TempDir;

    /// Build a tempdir book root with two frozen files plus an in-memory
    /// manifest pointing at them. `span` is unused by the resolver, so the
    /// returned directive carries an empty range.
    fn fixture(left_bytes: &[u8], right_bytes: &[u8]) -> (TempDir, Manifest, DiffDirective) {
        let tmp = TempDir::new().expect("tempdir");
        let listings_dir = tmp.path().join("src").join("listings");
        fs::create_dir_all(&listings_dir).unwrap();
        fs::write(listings_dir.join("left-tag.txt"), left_bytes).unwrap();
        fs::write(listings_dir.join("right-tag.txt"), right_bytes).unwrap();

        let manifest = Manifest {
            version: MANIFEST_VERSION,
            listings: vec![
                crate::manifest::Listing {
                    tag: "left-tag".into(),
                    source: "../left.txt".into(),
                    frozen: "src/listings/left-tag.txt".into(),
                    sha256: "0".repeat(64),
                },
                crate::manifest::Listing {
                    tag: "right-tag".into(),
                    source: "../right.txt".into(),
                    frozen: "src/listings/right-tag.txt".into(),
                    sha256: "0".repeat(64),
                },
            ],
        };

        let directive = DiffDirective {
            left: "left-tag".into(),
            right: "right-tag".into(),
            left_range: None,
            right_range: None,
            caption: None,
            span: 0..0,
        };

        (tmp, manifest, directive)
    }

    #[test]
    fn resolve_returns_bytes_and_labels_for_known_tags() {
        let (tmp, manifest, directive) = fixture(b"line one\nline two\n", b"line one\nline TWO\n");
        let resolved = resolve(&directive, &manifest, tmp.path(), tmp.path()).expect("resolve");
        assert_eq!(resolved.left_label, "left-tag");
        assert_eq!(resolved.right_label, "right-tag");
        assert_eq!(resolved.left_bytes, b"line one\nline two\n");
        assert_eq!(resolved.right_bytes, b"line one\nline TWO\n");
    }

    #[test]
    fn resolve_returns_unknown_tag_error_for_missing_left_operand() {
        let (tmp, manifest, mut directive) = fixture(b"a", b"b");
        directive.left = "nope".into();

        let err = resolve(&directive, &manifest, tmp.path(), tmp.path()).expect_err("should fail");
        assert_eq!(err.tag, "nope");
        assert!(matches!(err.kind, ResolveErrorKind::UnknownTag));
        let msg = format!("{err}");
        assert!(
            msg.contains("`nope`"),
            "diagnostic should name the missing tag; got: {msg}",
        );
    }

    #[test]
    fn resolve_returns_unknown_tag_error_for_missing_right_operand() {
        let (tmp, manifest, mut directive) = fixture(b"a", b"b");
        directive.right = "also-nope".into();

        let err = resolve(&directive, &manifest, tmp.path(), tmp.path()).expect_err("should fail");
        assert_eq!(err.tag, "also-nope");
        assert!(matches!(err.kind, ResolveErrorKind::UnknownTag));
    }

    #[test]
    fn resolve_returns_bytes_for_live_operand() {
        let (tmp, manifest, mut directive) = fixture(b"a", b"b");
        fs::write(tmp.path().join("live-source.txt"), "live one\nlive two\n").unwrap();
        directive.left = "live:live-source.txt".into();

        let resolved = resolve(&directive, &manifest, tmp.path(), tmp.path()).expect("resolve");
        assert_eq!(resolved.left_label, "live:live-source.txt");
        assert_eq!(resolved.left_bytes, b"live one\nlive two\n");
    }

    #[test]
    fn resolve_resolves_live_operand_against_live_base_not_book_root() {
        let (tmp, manifest, mut directive) = fixture(b"a", b"b");
        let chapter_dir = tmp.path().join("src").join("chapters");
        fs::create_dir_all(&chapter_dir).unwrap();
        fs::write(chapter_dir.join("sibling.txt"), "from chapter dir\n").unwrap();
        directive.left = "live:sibling.txt".into();

        let resolved = resolve(&directive, &manifest, tmp.path(), &chapter_dir).expect("resolve");
        assert_eq!(resolved.left_bytes, b"from chapter dir\n");
    }

    #[test]
    fn resolve_returns_live_file_missing_when_disk_lacks_live_path() {
        let (tmp, manifest, mut directive) = fixture(b"a", b"b");
        directive.left = "live:nope.txt".into();

        let err = resolve(&directive, &manifest, tmp.path(), tmp.path()).expect_err("should fail");
        assert_eq!(err.tag, "live:nope.txt");
        match &err.kind {
            ResolveErrorKind::LiveFileMissing { live_path, .. } => {
                assert!(
                    live_path.ends_with("nope.txt"),
                    "diagnostic should name the absent file; got {live_path:?}",
                );
            }
            other => panic!("expected LiveFileMissing; got {other:?}"),
        }
    }

    #[test]
    fn resolve_returns_frozen_file_missing_when_disk_lacks_frozen_copy() {
        let (tmp, manifest, directive) = fixture(b"a", b"b");
        fs::remove_file(tmp.path().join("src/listings/left-tag.txt")).unwrap();

        let err = resolve(&directive, &manifest, tmp.path(), tmp.path()).expect_err("should fail");
        assert_eq!(err.tag, "left-tag");
        match &err.kind {
            ResolveErrorKind::FrozenFileMissing { frozen_path, .. } => {
                assert!(
                    frozen_path.ends_with("src/listings/left-tag.txt"),
                    "diagnostic should name the absent file; got {frozen_path:?}",
                );
            }
            other => panic!("expected FrozenFileMissing; got {other:?}"),
        }
    }

    #[test]
    fn render_produces_unified_diff_with_headers_for_differing_inputs() {
        let out = render("line one\nline two\n", "line one\nline TWO\n", "old", "new");
        assert!(out.contains("--- old"), "expected --- header; got:\n{out}");
        assert!(out.contains("+++ new"), "expected +++ header; got:\n{out}");
        assert!(
            out.contains("-line two"),
            "expected removed line; got:\n{out}"
        );
        assert!(
            out.contains("+line TWO"),
            "expected added line; got:\n{out}"
        );
    }

    #[test]
    fn render_returns_no_changes_notice_for_identical_inputs() {
        let out = render("same\nbytes\n", "same\nbytes\n", "old", "new");
        assert_eq!(out, "(no changes between old and new)\n");
    }

    #[test]
    fn render_returns_no_changes_notice_for_two_empty_inputs() {
        let out = render("", "", "old", "new");
        assert_eq!(out, "(no changes between old and new)\n");
    }

    #[test]
    fn render_marks_pure_additions_with_plus_prefix() {
        let out = render("a\n", "a\nb\n", "old", "new");
        assert!(out.contains("+b"), "expected added line `b`; got:\n{out}");
        assert!(
            !out.lines().any(|l| l.starts_with('-')
                && !l.starts_with("---")
                && l.trim_start_matches('-').contains("a")),
            "no removal expected on a pure addition; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_replaces_directive_with_diff_fence_and_preserves_surroundings() {
        let (tmp, manifest, _) = fixture(b"line one\nline two\n", b"line one\nline TWO\n");
        let chapter_path = Path::new("ch99.md");
        let content = "Before paragraph.\n\n{{#diff left-tag right-tag}}\n\nAfter paragraph.\n";
        let out = splice_chapter(
            content,
            &manifest,
            tmp.path(),
            Some(chapter_path),
            tmp.path(),
        )
        .unwrap();

        assert!(out.starts_with("Before paragraph.\n"), "got:\n{out}");
        assert!(out.ends_with("After paragraph.\n"), "got:\n{out}");
        assert!(
            out.contains("```diff\n"),
            "expected diff fence; got:\n{out}"
        );
        assert!(
            out.contains("--- left-tag"),
            "expected left header; got:\n{out}"
        );
        assert!(
            out.contains("+++ right-tag"),
            "expected right header; got:\n{out}"
        );
        assert!(
            !out.contains("{{#diff"),
            "directive should be consumed; got:\n{out}",
        );
        assert!(
            out.contains("data-listing-diff-left=\"left-tag\""),
            "expected diff-left anchor attribute; got:\n{out}",
        );
        assert!(
            out.contains("data-listing-diff-right=\"right-tag\""),
            "expected diff-right anchor attribute; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_slices_both_listings_to_ranges_and_diffs_slices_only() {
        // Two 5-line files; differ on lines 2 and 4. With ranges 1:2 / 1:2
        // the diff sees only lines 1-2 of each — the line-2 difference
        // shows up but the line-4 one doesn't.
        let (tmp, manifest, _) = fixture(
            b"line1\nold-2\nline3\nold-4\nline5\n",
            b"line1\nnew-2\nline3\nnew-4\nline5\n",
        );
        let content = "{{#diff left-tag right-tag 1:2 1:2}}\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None, tmp.path()).unwrap();
        assert!(
            out.contains("-old-2"),
            "expected line-2 removal in slice; got:\n{out}",
        );
        assert!(
            out.contains("+new-2"),
            "expected line-2 addition in slice; got:\n{out}",
        );
        assert!(
            !out.contains("old-4") && !out.contains("new-4"),
            "lines past the range should not appear in the rendered diff; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_emits_absolute_line_numbers_in_hunk_headers_for_sliced_diff() {
        // Both files differ on absolute line 60. With ranges 55:65 / 55:65
        // the slice contains line 60 at slice-relative position 6. Pre-fix
        // the hunk header read `@@ -... +... @@` keyed to slice positions;
        // post-fix it must read `@@ -...60... +...60... @@` so a reader
        // can map the diff's `+`/`-` lines back to absolute line numbers
        // in the parent file.
        let mut left = String::new();
        let mut right = String::new();
        for i in 1..=70 {
            left.push_str(&format!("line{i}\n"));
            if i == 60 {
                right.push_str("line60-CHANGED\n");
            } else {
                right.push_str(&format!("line{i}\n"));
            }
        }
        let (tmp, manifest, _) = fixture(left.as_bytes(), right.as_bytes());
        let content = "{{#diff left-tag right-tag 55:65 55:65}}\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None, tmp.path()).unwrap();
        // The hunk header should reference absolute line numbers in the
        // 55-65 window (likely `@@ -57,9 +57,9 @@` since the diff context
        // around line 60 covers 57-63 absolutely).
        let hunk_line = out
            .lines()
            .find(|l| l.starts_with("@@ "))
            .unwrap_or_else(|| panic!("expected a hunk header in:\n{out}"));
        // The starting line numbers must fall within the slice window
        // [55, 65] — pre-fix they were < 55 (slice-relative).
        let parts: Vec<&str> = hunk_line.split_whitespace().collect();
        let left_start: usize = parts[1]
            .trim_start_matches('-')
            .split(',')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let right_start: usize = parts[2]
            .trim_start_matches('+')
            .split(',')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            (55..=65).contains(&left_start),
            "left hunk start must be inside the [55,65] absolute window; got `{hunk_line}` (left_start={left_start})",
        );
        assert!(
            (55..=65).contains(&right_start),
            "right hunk start must be inside the [55,65] absolute window; got `{hunk_line}` (right_start={right_start})",
        );
    }

    #[test]
    fn splice_chapter_emits_range_data_attributes_when_ranges_present() {
        let (tmp, manifest, _) = fixture(b"a\nb\nc\nd\n", b"a\nB\nc\nD\n");
        let content = "{{#diff left-tag right-tag 1:2 1:3}}\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None, tmp.path()).unwrap();
        assert!(
            out.contains(r#"data-listing-diff-left-range="1:2""#),
            "expected left-range data attribute; got:\n{out}",
        );
        assert!(
            out.contains(r#"data-listing-diff-right-range="1:3""#),
            "expected right-range data attribute; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_preserves_callout_markers_inside_sliced_diff_for_callout_splicer_downstream()
    {
        // The chapter pipeline runs splice_diffs THEN splice_callouts. A
        // CALLOUT marker that lives inside the slice window must survive
        // the sliced diff render so the downstream callout splicer can
        // find it and emit a badge. This test asserts the survival; the
        // end-to-end badge rendering is covered by the e2e suite.
        let mut left = String::new();
        let mut right = String::new();
        for i in 1..=30 {
            left.push_str(&format!("// row {i}\n"));
            if i == 15 {
                right.push_str("// CALLOUT: sliced-marker Verifies callouts inside a sliced diff range survive.\n");
            } else {
                right.push_str(&format!("// row {i}\n"));
            }
        }
        let (tmp, manifest, _) = fixture(left.as_bytes(), right.as_bytes());
        let content = "{{#diff left-tag right-tag 10:20 10:20}}\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None, tmp.path()).unwrap();
        assert!(
            out.contains("CALLOUT: sliced-marker"),
            "callout marker on line 15 (inside the 10:20 slice) must survive into the rendered diff body so the callout splicer can pick it up; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_drops_callout_marker_outside_sliced_range() {
        // A CALLOUT marker outside the slice window must NOT appear in
        // the rendered diff — neither in the diff body nor in any future
        // badge — because the slice never reached it.
        let mut left = String::new();
        let mut right = String::new();
        for i in 1..=30 {
            left.push_str(&format!("// row {i}\n"));
            if i == 25 {
                right.push_str("// CALLOUT: outside-slice This must not survive.\n");
            } else {
                right.push_str(&format!("// row {i}\n"));
            }
        }
        let (tmp, manifest, _) = fixture(left.as_bytes(), right.as_bytes());
        let content = "{{#diff left-tag right-tag 1:10 1:10}}\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None, tmp.path()).unwrap();
        assert!(
            !out.contains("outside-slice"),
            "marker outside the 1:10 slice must not appear; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_omits_range_data_attributes_when_no_ranges() {
        let (tmp, manifest, _) = fixture(b"a\nb\n", b"a\nB\n");
        let content = "{{#diff left-tag right-tag}}\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None, tmp.path()).unwrap();
        assert!(
            !out.contains("data-listing-diff-left-range"),
            "no left-range attr expected without ranges; got:\n{out}",
        );
        assert!(
            !out.contains("data-listing-diff-right-range"),
            "no right-range attr expected without ranges; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_short_circuits_with_chapter_path_and_line_for_unknown_tag() {
        let (tmp, manifest, _) = fixture(b"a", b"b");
        let chapter_path = Path::new("src/ch99-foo.md");
        let content = "intro\n\nmore\n\n{{#diff missing-tag right-tag}}\n";
        let err = splice_chapter(
            content,
            &manifest,
            tmp.path(),
            Some(chapter_path),
            tmp.path(),
        )
        .expect_err("err");
        assert_eq!(err.line, 5, "directive sits on line 5; got: {err}");
        assert_eq!(err.chapter_path.as_deref(), Some(chapter_path));
        let msg = format!("{err}");
        assert!(
            msg.contains("src/ch99-foo.md:5") && msg.contains("`missing-tag`"),
            "diagnostic should name file:line and tag; got: {msg}",
        );
    }
}
