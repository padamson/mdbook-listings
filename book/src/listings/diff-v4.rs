//! Parses `{{#diff <left> <right>}}` directives out of chapter markdown,
//! resolves each operand tag to bytes via the manifest, and renders the
//! pair as unified-diff text.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::manifest::Manifest;

/// `span` covers the directive in full (`{{#diff …}}` inclusive) so callers
/// can replace the whole substring in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDirective {
    pub left: String,
    pub right: String,
    pub span: Range<usize>,
}

/// Returns every well-formed `{{#diff a b}}` directive in `content`.
/// Backslash-escaped directives, wrong-arity matches, and any directive
/// whose start byte falls inside a fenced code block are skipped — the
/// fence rule lets a chapter quote literal directive examples (e.g. a
/// frozen test fixture) without the preprocessor consuming them.
pub fn parse_directives(content: &str) -> Vec<DiffDirective> {
    const PREFIX: &[u8] = b"{{#diff";
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    for_each_directive_position(content, |i| {
        if i > 0 && bytes[i - 1] == b'\\' {
            return PREFIX.len();
        }
        let inner_start = i + PREFIX.len();
        let Some(end_rel) = content[inner_start..].find("}}") else {
            return content.len() - i;
        };
        let directive_end = inner_start + end_rel + 2;
        let tokens: Vec<&str> = content[inner_start..inner_start + end_rel]
            .split_whitespace()
            .collect();
        if tokens.len() == 2 {
            out.push(DiffDirective {
                left: tokens[0].to_string(),
                right: tokens[1].to_string(),
                span: i..directive_end,
            });
        }
        directive_end - i
    });
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
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ResolveErrorKind::UnknownTag => None,
            ResolveErrorKind::FrozenFileMissing { source, .. } => Some(source),
        }
    }
}

/// Returns at the first failing operand so callers surface one missing tag
/// at a time — the second tag's resolution can wait for the rebuild after
/// the first fix.
pub fn resolve(
    directive: &DiffDirective,
    manifest: &Manifest,
    book_root: &Path,
) -> Result<ResolvedDiff, ResolveError> {
    let (left_label, left_bytes) = resolve_operand(&directive.left, manifest, book_root)?;
    let (right_label, right_bytes) = resolve_operand(&directive.right, manifest, book_root)?;
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
) -> Result<(String, Vec<u8>), ResolveError> {
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

/// Byte positions of `\` characters that immediately precede a `{{#diff`
/// substring outside fenced code blocks. The splicer drops these so the
/// literal directive renders to the reader.
pub fn parse_escapes(content: &str) -> Vec<usize> {
    const PREFIX: &[u8] = b"{{#diff";
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    for_each_directive_position(content, |i| {
        if i > 0 && bytes[i - 1] == b'\\' {
            out.push(i - 1);
        }
        PREFIX.len()
    });
    out
}

/// Walks `content` byte-wise, skipping fenced code blocks, and invokes
/// `visit(i)` at every byte offset `i` where `{{#diff` starts. The closure
/// returns how many bytes to advance past the match — letting callers
/// consume the whole directive (or just the prefix) without re-scanning.
fn for_each_directive_position<F>(content: &str, mut visit: F)
where
    F: FnMut(usize) -> usize,
{
    const PREFIX: &[u8] = b"{{#diff";
    let bytes = content.as_bytes();
    let mut in_fence = false;
    let mut line_start = 0;
    while line_start < bytes.len() {
        let line_end = match content[line_start..].find('\n') {
            Some(off) => line_start + off,
            None => bytes.len(),
        };
        if line_is_code_fence(&bytes[line_start..line_end]) {
            in_fence = !in_fence;
        } else if !in_fence {
            let mut i = line_start;
            while i + PREFIX.len() <= line_end {
                if &bytes[i..i + PREFIX.len()] == PREFIX {
                    let advance = visit(i);
                    i += advance.max(1);
                } else {
                    i += 1;
                }
            }
        }
        line_start = line_end + 1;
    }
}

fn line_is_code_fence(line: &[u8]) -> bool {
    let leading_spaces = line.iter().take_while(|&&b| b == b' ').count();
    if leading_spaces > 3 {
        return false;
    }
    let rest = &line[leading_spaces..];
    rest.starts_with(b"```") || rest.starts_with(b"~~~")
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
/// block of unified-diff text and strip the leading `\` from any
/// `\{{#diff …}}` escape so the literal directive renders to the reader.
/// Bytes outside those spans are copied through unchanged.
pub fn splice_chapter(
    content: &str,
    manifest: &Manifest,
    book_root: &Path,
    chapter_path: Option<&Path>,
) -> Result<String, SpliceError> {
    let directives = parse_directives(content);
    let escapes = parse_escapes(content);

    let mut edits: Vec<(usize, usize, String)> =
        Vec::with_capacity(directives.len() + escapes.len());

    for d in directives {
        let resolved = resolve(&d, manifest, book_root).map_err(|source| SpliceError {
            chapter_path: chapter_path.map(Path::to_path_buf),
            line: line_number(content, d.span.start),
            source,
        })?;
        let left = String::from_utf8_lossy(&resolved.left_bytes);
        let right = String::from_utf8_lossy(&resolved.right_bytes);
        let body = render(&left, &right, &resolved.left_label, &resolved.right_label);
        edits.push((d.span.start, d.span.end, format!("```diff\n{body}```")));
    }
    for pos in escapes {
        edits.push((pos, pos + 1, String::new()));
    }

    edits.sort_by_key(|(start, _, _)| *start);

    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        out.push_str(&content[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
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
    fn parse_escapes_skips_inside_fenced_code_blocks() {
        let s = "outside \\{{#diff a b}}\n\n```\nlet s = \"\\{{#diff x y}}\";\n```\n";
        let escapes = parse_escapes(s);
        assert_eq!(escapes.len(), 1, "fenced escape should be skipped");
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
            span: 0..0,
        };

        (tmp, manifest, directive)
    }

    #[test]
    fn resolve_returns_bytes_and_labels_for_known_tags() {
        let (tmp, manifest, directive) = fixture(b"line one\nline two\n", b"line one\nline TWO\n");
        let resolved = resolve(&directive, &manifest, tmp.path()).expect("resolve");
        assert_eq!(resolved.left_label, "left-tag");
        assert_eq!(resolved.right_label, "right-tag");
        assert_eq!(resolved.left_bytes, b"line one\nline two\n");
        assert_eq!(resolved.right_bytes, b"line one\nline TWO\n");
    }

    #[test]
    fn resolve_returns_unknown_tag_error_for_missing_left_operand() {
        let (tmp, manifest, mut directive) = fixture(b"a", b"b");
        directive.left = "nope".into();

        let err = resolve(&directive, &manifest, tmp.path()).expect_err("should fail");
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

        let err = resolve(&directive, &manifest, tmp.path()).expect_err("should fail");
        assert_eq!(err.tag, "also-nope");
        assert!(matches!(err.kind, ResolveErrorKind::UnknownTag));
    }

    #[test]
    fn resolve_returns_frozen_file_missing_when_disk_lacks_frozen_copy() {
        let (tmp, manifest, directive) = fixture(b"a", b"b");
        fs::remove_file(tmp.path().join("src/listings/left-tag.txt")).unwrap();

        let err = resolve(&directive, &manifest, tmp.path()).expect_err("should fail");
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
    fn parse_escapes_returns_positions_of_backslashes_before_diff_directives() {
        let s = "use \\{{#diff a b}} verbatim and \\{{#diff c d}} again";
        let escapes = parse_escapes(s);
        assert_eq!(escapes.len(), 2);
        assert_eq!(&s[escapes[0]..=escapes[0]], "\\");
        assert_eq!(&s[escapes[1]..=escapes[1]], "\\");
    }

    #[test]
    fn parse_escapes_ignores_unescaped_directives() {
        let s = "{{#diff a b}}";
        assert!(parse_escapes(s).is_empty());
    }

    #[test]
    fn splice_chapter_replaces_directive_with_diff_fence_and_preserves_surroundings() {
        let (tmp, manifest, _) = fixture(b"line one\nline two\n", b"line one\nline TWO\n");
        let chapter_path = Path::new("ch99.md");
        let content = "Before paragraph.\n\n{{#diff left-tag right-tag}}\n\nAfter paragraph.\n";
        let out = splice_chapter(content, &manifest, tmp.path(), Some(chapter_path)).unwrap();

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
    }

    #[test]
    fn splice_chapter_strips_leading_backslash_from_escaped_directives() {
        let (tmp, manifest, _) = fixture(b"a", b"b");
        let content = "Use \\{{#diff a b}} to render a diff.\n";
        let out = splice_chapter(content, &manifest, tmp.path(), None).unwrap();
        assert_eq!(out, "Use {{#diff a b}} to render a diff.\n");
    }

    #[test]
    fn splice_chapter_short_circuits_with_chapter_path_and_line_for_unknown_tag() {
        let (tmp, manifest, _) = fixture(b"a", b"b");
        let chapter_path = Path::new("src/ch99-foo.md");
        let content = "intro\n\nmore\n\n{{#diff missing-tag right-tag}}\n";
        let err =
            splice_chapter(content, &manifest, tmp.path(), Some(chapter_path)).expect_err("err");
        assert_eq!(err.line, 5, "directive sits on line 5; got: {err}");
        assert_eq!(err.chapter_path.as_deref(), Some(chapter_path));
        let msg = format!("{err}");
        assert!(
            msg.contains("src/ch99-foo.md:5") && msg.contains("`missing-tag`"),
            "diagnostic should name file:line and tag; got: {msg}",
        );
    }
}
