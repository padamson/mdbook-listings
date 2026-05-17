//! `mdbook-listings freeze`: snapshot a source file into the book-local
//! listings directory and record it in the manifest.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

use crate::manifest::{Listing, Manifest};

/// Relative path from a book root to the frozen-listings directory.
pub const LISTINGS_SUBDIR: &str = "src/listings";

/// Options accepted by [`freeze`]. Mirrors the CLI flags 1:1 so the binary
/// layer stays a thin adapter.
#[derive(Debug)]
pub struct FreezeOptions<'a> {
    pub book_root: &'a Path,
    pub tag: &'a str,
    pub source: &'a Path,
    pub force: bool,
}

/// Outcome of a freeze invocation. Callers use this to render a summary line
/// and to drive tests.
#[derive(Debug, PartialEq, Eq)]
pub enum FreezeOutcome {
    /// New listing with this tag; frozen file and manifest entry created.
    Created,
    /// Tag already existed and the source content is byte-identical to the
    /// frozen copy on disk; nothing was changed.
    Unchanged,
    /// Tag already existed and the source differs; frozen file and manifest
    /// entry were overwritten (only possible with `--force`).
    Replaced,
}

/// Result of a freeze invocation: the outcome plus optional metadata the CLI
/// surfaces in its success block (most-recent prior tag for the same source,
/// when one exists).
#[derive(Debug)]
pub struct FreezeReport {
    pub outcome: FreezeOutcome,
    /// Most-recent listing in the manifest with the same `source` path as
    /// the just-frozen tag (excluding the just-frozen tag itself). `None`
    /// when this is the first listing for the source.
    pub previous_tag: Option<String>,
}

/// Freeze `opts.source` into `<book_root>/src/listings/<tag>.<ext>` and upsert
/// the corresponding entry in `<book_root>/listings.toml`.
pub fn freeze(opts: FreezeOptions<'_>) -> Result<FreezeReport> {
    let source_bytes = fs::read(opts.source)
        .with_context(|| format!("reading source file {}", opts.source.display()))?;
    let source_sha = hex_sha256(&source_bytes);

    let frozen_rel = frozen_relative_path(opts.tag, opts.source)?;
    let frozen_abs = opts.book_root.join(&frozen_rel);

    let mut manifest = Manifest::load(opts.book_root)?;

    let outcome = match manifest.find(opts.tag) {
        Some(existing) if existing.sha256 == source_sha && frozen_abs.exists() => {
            FreezeOutcome::Unchanged
        }
        Some(_existing) if !opts.force => {
            bail!(
                "tag `{}` already frozen with different content; re-run with --force to overwrite",
                opts.tag
            )
        }
        Some(_existing) => FreezeOutcome::Replaced,
        None => FreezeOutcome::Created,
    };

    let source_rel = relativize(opts.source, opts.book_root);
    let source_rel_str = path_to_string(&source_rel)?;
    // Compute prior-tag BEFORE upsert so the just-frozen tag can't match
    // itself in the `Replaced` case.
    let previous_tag = previous_listing_for_source(&manifest, &source_rel_str, opts.tag)
        .map(|l| l.tag.clone());

    if outcome != FreezeOutcome::Unchanged {
        if let Some(parent) = frozen_abs.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("creating frozen-listings directory {}", parent.display())
            })?;
        }
        fs::write(&frozen_abs, &source_bytes)
            .with_context(|| format!("writing frozen file {}", frozen_abs.display()))?;

        manifest.upsert(Listing {
            tag: opts.tag.to_string(),
            source: source_rel_str,
            frozen: path_to_string(&frozen_rel)?,
            sha256: source_sha,
        });
        manifest.save(opts.book_root)?;
    }

    Ok(FreezeReport {
        outcome,
        previous_tag,
    })
}

/// Walk the manifest's entries in reverse insertion order and return the
/// first listing whose `source` matches `source_rel_str` and whose `tag`
/// is NOT `current_tag`. Pub so the CLI can derive a diff-directive
/// suggestion after a successful freeze.
pub fn previous_listing_for_source<'m>(
    manifest: &'m Manifest,
    source_rel_str: &str,
    current_tag: &str,
) -> Option<&'m Listing> {
    manifest
        .listings
        .iter()
        .rev()
        .find(|l| l.source == source_rel_str && l.tag != current_tag)
}

/// Pub so the CLI can echo the path on every successful freeze without
/// re-deriving the format from CLI args.
pub fn frozen_relative_path(tag: &str, source: &Path) -> Result<PathBuf> {
    if tag.is_empty() {
        bail!("tag must be non-empty");
    }
    if tag.contains(['/', '\\', '.']) {
        bail!("tag `{tag}` contains disallowed character (/, \\, or .)");
    }
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("source {} has no file extension", source.display()))?;
    Ok(Path::new(LISTINGS_SUBDIR).join(format!("{tag}.{ext}")))
}

fn relativize(path: &Path, base: &Path) -> PathBuf {
    pathdiff(path, base).unwrap_or_else(|| path.to_path_buf())
}

/// Minimal relative-path computation: if `path` is under `base`, strip the
/// prefix; otherwise walk up from `base` with `..` segments.
fn pathdiff(path: &Path, base: &Path) -> Option<PathBuf> {
    let path = path.canonicalize().ok()?;
    let base = base.canonicalize().ok()?;
    let mut path_components: Vec<_> = path.components().collect();
    let mut base_components: Vec<_> = base.components().collect();
    while let (Some(p), Some(b)) = (path_components.first(), base_components.first()) {
        if p == b {
            path_components.remove(0);
            base_components.remove(0);
        } else {
            break;
        }
    }
    let mut result = PathBuf::new();
    for _ in &base_components {
        result.push("..");
    }
    for c in &path_components {
        result.push(c.as_os_str());
    }
    Some(result)
}

fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.replace('\\', "/"))
        .ok_or_else(|| anyhow!("path {} is not valid UTF-8", path.display()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_sha256_matches_known_vector() {
        // Well-known sha256("") per FIPS 180-4.
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn frozen_path_composes_tag_and_extension() {
        let p = frozen_relative_path("compose-v1", Path::new("compose.yaml")).unwrap();
        assert_eq!(p, Path::new("src/listings/compose-v1.yaml"));
    }

    #[test]
    fn frozen_path_rejects_tag_with_slash() {
        assert!(frozen_relative_path("foo/bar", Path::new("x.yaml")).is_err());
    }

    #[test]
    fn frozen_path_rejects_empty_tag() {
        assert!(frozen_relative_path("", Path::new("x.yaml")).is_err());
    }

    #[test]
    fn frozen_path_rejects_extensionless_source() {
        assert!(frozen_relative_path("tag", Path::new("Makefile")).is_err());
    }

    fn listing(tag: &str, source: &str) -> Listing {
        Listing {
            tag: tag.to_string(),
            source: source.to_string(),
            frozen: format!("src/listings/{tag}.rs"),
            sha256: String::new(),
        }
    }

    #[test]
    fn previous_listing_returns_none_when_manifest_empty() {
        let m = Manifest {
            version: 1,
            listings: vec![],
        };
        assert!(previous_listing_for_source(&m, "../src/foo.rs", "foo-v1").is_none());
    }

    #[test]
    fn previous_listing_returns_none_when_no_prior_matches_source() {
        let m = Manifest {
            version: 1,
            listings: vec![listing("bar-v1", "../src/bar.rs")],
        };
        assert!(previous_listing_for_source(&m, "../src/foo.rs", "foo-v1").is_none());
    }

    #[test]
    fn previous_listing_returns_none_when_only_match_is_current_tag() {
        let m = Manifest {
            version: 1,
            listings: vec![listing("foo-v1", "../src/foo.rs")],
        };
        assert!(previous_listing_for_source(&m, "../src/foo.rs", "foo-v1").is_none());
    }

    #[test]
    fn previous_listing_returns_most_recent_prior_for_same_source() {
        let m = Manifest {
            version: 1,
            listings: vec![
                listing("foo-v1", "../src/foo.rs"),
                listing("bar-v1", "../src/bar.rs"),
                listing("foo-v2", "../src/foo.rs"),
                listing("baz-v1", "../src/baz.rs"),
            ],
        };
        let prev = previous_listing_for_source(&m, "../src/foo.rs", "foo-v3").unwrap();
        assert_eq!(prev.tag, "foo-v2");
    }

    #[test]
    fn previous_listing_skips_current_tag_and_picks_next_most_recent() {
        let m = Manifest {
            version: 1,
            listings: vec![
                listing("foo-v1", "../src/foo.rs"),
                listing("foo-v2", "../src/foo.rs"),
                listing("foo-v3", "../src/foo.rs"),
            ],
        };
        let prev = previous_listing_for_source(&m, "../src/foo.rs", "foo-v3").unwrap();
        assert_eq!(prev.tag, "foo-v2");
    }
}
