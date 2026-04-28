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
/// Backslash-escaped directives (`\{{#diff …}}`, matching mdbook's
/// `{{#include}}` convention) and wrong-arity matches are skipped silently —
/// the caller is in a better position to surface either with chapter-source
/// context.
pub fn parse_directives(content: &str) -> Vec<DiffDirective> {
    const PREFIX: &[u8] = b"{{#diff";
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + PREFIX.len() <= bytes.len() {
        if &bytes[i..i + PREFIX.len()] != PREFIX {
            i += 1;
            continue;
        }
        if i > 0 && bytes[i - 1] == b'\\' {
            i += PREFIX.len();
            continue;
        }
        let inner_start = i + PREFIX.len();
        let Some(end_rel) = content[inner_start..].find("}}") else {
            break;
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
        i = directive_end;
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
}
