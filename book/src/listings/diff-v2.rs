//! Parses `{{#diff <left> <right>}}` directives out of chapter markdown and
//! resolves each operand tag to the bytes the renderer will diff against.
//! The unified-diff renderer that consumes a [`ResolvedDiff`] lands in slice
//! 4 of the *Show Diffs Between Slices* story.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::manifest::Manifest;

/// One parsed `{{#diff <left> <right>}}` directive. `span` indexes into the
/// chapter content the parser was handed and covers the directive in full
/// (`{{#diff …}}` inclusive) so the splicer can replace the whole substring
/// in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDirective {
    pub left: String,
    pub right: String,
    pub span: Range<usize>,
}

/// Walks `content` and returns every well-formed `{{#diff a b}}` directive.
/// Directives prefixed with a backslash (`\{{#diff …}}`, matching mdbook's
/// `{{#include}}` escape convention) are skipped here; the splicer that
/// lands later strips the leading backslash so the literal directive renders
/// to the reader. Directives with the wrong arity (`{{#diff a}}`,
/// `{{#diff a b c}}`) are silently skipped — the resolver in the next slice
/// surfaces the useful diagnostic, and being over-eager here would fight it.
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

/// The bytes plus labels needed to render a unified diff. Labels become the
/// `--- <left_label>` / `+++ <right_label>` headers in the rendered output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDiff {
    pub left_label: String,
    pub left_bytes: Vec<u8>,
    pub right_label: String,
    pub right_bytes: Vec<u8>,
}

/// Why an operand could not be turned into bytes. The splicer in slice 5
/// wraps this with the chapter source path and 1-based line number derived
/// from the directive's span — the location context AC 3 demands.
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

/// Look each operand of `directive` up in `manifest`, read the frozen bytes
/// from `<book_root>/<listing.frozen>`, and return them paired with labels
/// the renderer can use in unified-diff headers. Stops at the first failing
/// operand: if the left tag is unknown the right tag is not consulted, so
/// the diagnostic the splicer surfaces names a single missing tag rather
/// than two.
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

    /// Build a tempdir book root with two frozen files and an in-memory
    /// manifest pointing at them. The directive's span is irrelevant to the
    /// resolver — slice 5 uses it to compute line numbers when surfacing
    /// errors — so the helper hands back a span-less directive built from
    /// just the operand tags.
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
}
