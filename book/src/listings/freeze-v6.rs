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
    let previous_tag =
        previous_listing_for_source(&manifest, &source_rel_str, opts.tag).map(|l| l.tag.clone());

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

/// Version-prefix tokens accepted when deriving a default tag.
/// The set is deliberately small and hyphen-separated
/// (`<basename>-v3`, `<basename>-rev3`) so that "compose3" or
/// "draft7" — which could be deliberate names or typos — don't
/// silently autopilot into a `compose4` / `draft8` suggestion the
/// author didn't ask for.
const VERSION_PREFIXES: &[&str] = &["v", "ver", "rev", "version"];

/// Derive `<basename>-<prefix><N>` from the source path + manifest.
///
/// - If no prior listing exists for this source: returns
///   `<basename>-v1` (the canonical Rust convention; first author
///   to freeze a given source establishes `v` for it).
/// - If prior listings exist and at least one matches
///   `<basename>-<prefix><N>` where `<prefix>` is in
///   [`VERSION_PREFIXES`]: returns `<basename>-<prefix>(maxN + 1)`,
///   carrying the most-recently-added matching listing's prefix
///   so a mid-stream switch (the author started with `v1`, then
///   moved to `rev1`/`rev2`) keeps using the new convention.
/// - If prior listings exist but none match the allowlist
///   (e.g. a `<basename>-ch<NN>-phase<N>` chapter/phase scheme):
///   returns `TagDerivationError::UnrecognisedConvention` so the
///   author knows to pass `--tag` explicitly.
///
/// Pub so the CLI can attempt derivation when `--tag` is omitted.
pub fn derive_default_tag(
    manifest: &Manifest,
    source: &Path,
    book_root: &Path,
) -> Result<String, TagDerivationError> {
    let basename = source.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        TagDerivationError::UnusableSourceName {
            source: source.display().to_string(),
        }
    })?;

    let source_rel = relativize(source, book_root);
    let source_rel_str =
        path_to_string(&source_rel).map_err(|_| TagDerivationError::UnusableSourceName {
            source: source.display().to_string(),
        })?;

    let priors: Vec<&Listing> = manifest
        .listings
        .iter()
        .filter(|l| l.source == source_rel_str)
        .collect();

    if priors.is_empty() {
        return Ok(format!("{basename}-v1"));
    }

    let matches: Vec<(&str, u64)> = priors
        .iter()
        .filter_map(|l| parse_version_suffix(&l.tag, basename))
        .collect();

    if matches.is_empty() {
        return Err(TagDerivationError::UnrecognisedConvention {
            basename: basename.to_string(),
            example_prior_tag: priors.last().map(|l| l.tag.clone()).unwrap_or_default(),
        });
    }

    let max_n = matches.iter().map(|(_, n)| *n).max().expect("non-empty");
    let prefix = matches.last().map(|(p, _)| *p).expect("non-empty");
    Ok(format!("{basename}-{prefix}{}", max_n + 1))
}

/// Parse `<basename>-<prefix><N>` from `tag`. Returns `(prefix, N)`
/// when the tag matches one of [`VERSION_PREFIXES`] and `N` is a
/// non-negative integer; returns `None` otherwise. Pub for test
/// access only.
fn parse_version_suffix<'t>(tag: &'t str, basename: &str) -> Option<(&'t str, u64)> {
    let after_basename = tag.strip_prefix(basename)?.strip_prefix('-')?;
    for &prefix in VERSION_PREFIXES {
        if let Some(rest) = after_basename.strip_prefix(prefix)
            && let Ok(n) = rest.parse::<u64>()
        {
            // Slice the prefix back out of `tag` so we can return a
            // reference into the original `&'t str` lifetime — runs
            // from len(basename + '-') to that + len(prefix).
            let start = basename.len() + 1;
            let end = start + prefix.len();
            return Some((&tag[start..end], n));
        }
    }
    None
}

/// Errors raised by [`derive_default_tag`] when a default can't be
/// produced. The CLI converts these into actionable messages directing
/// the author to pass `--tag` explicitly.
#[derive(Debug)]
pub enum TagDerivationError {
    /// The source path doesn't have a usable file stem (no name, or
    /// non-UTF-8 bytes that can't be normalised).
    UnusableSourceName { source: String },
    /// Prior listings exist for this source but none match the
    /// allowlist convention, so we can't safely guess the next tag.
    UnrecognisedConvention {
        basename: String,
        example_prior_tag: String,
    },
}

impl std::fmt::Display for TagDerivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagDerivationError::UnusableSourceName { source } => write!(
                f,
                "source {source} has no usable file stem; pass --tag explicitly",
            ),
            TagDerivationError::UnrecognisedConvention {
                basename,
                example_prior_tag,
            } => write!(
                f,
                "can't auto-derive a default tag: prior listings for `{basename}` use a \
                 convention (`{example_prior_tag}`) that isn't `<basename>-(v|ver|rev|version)<N>`; \
                 pass --tag explicitly",
            ),
        }
    }
}

impl std::error::Error for TagDerivationError {}

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

/// Pub so the CLI can render the book-relative path with forward slashes
/// regardless of platform — Windows `Path::join` produces backslashes, but
/// `{{#include …}}` / `{{#diff …}}` directives and on-disk manifest entries
/// must always be forward-slash form so cross-platform-built books render
/// identically.
pub fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.replace('\\', "/"))
        .ok_or_else(|| anyhow!("path {} is not valid UTF-8", path.display()))
}

/// Shared with `verify`, which must hash frozen bytes exactly the way
/// `freeze` recorded them.
pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
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

    #[test]
    fn path_to_string_normalises_backslashes_to_forward_slashes() {
        // Constructing a Path with a literal backslash works on every
        // platform (it's just a character in the OsStr). The regression
        // we're guarding is that Windows-built PathBufs reach the
        // chapter output with `\` separators — `path_to_string` must
        // unconditionally rewrite them to `/` so book directives are
        // cross-platform.
        let p = Path::new("src/listings\\foo.rs");
        assert_eq!(path_to_string(p).unwrap(), "src/listings/foo.rs");
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

    /// Empty manifest + first freeze for the source → `<basename>-v1`.
    /// `v` is the canonical Rust convention; first author establishes it
    /// without per-source configuration.
    #[test]
    fn derive_default_tag_returns_v1_when_no_prior_listing() {
        let m = Manifest {
            version: 1,
            listings: vec![],
        };
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("foo.rs");
        let tag = derive_default_tag(&m, &source, &book_root).unwrap();
        assert_eq!(tag, "foo-v1");
    }

    /// Single prior `<basename>-v1` → `<basename>-v2`. Most common case.
    #[test]
    fn derive_default_tag_bumps_single_prior_v_match() {
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("foo.rs");
        let source_rel = path_to_string(&relativize(&source, &book_root)).unwrap();
        let m = Manifest {
            version: 1,
            listings: vec![listing("foo-v1", &source_rel)],
        };
        let tag = derive_default_tag(&m, &source, &book_root).unwrap();
        assert_eq!(tag, "foo-v2");
    }

    /// Multiple priors → bump from the HIGHEST N, not the count. The
    /// `v3` here came after `v5`/`v7` in insertion order; the next
    /// should still be `v8`, not `v4`.
    #[test]
    fn derive_default_tag_bumps_from_max_n_not_count() {
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("foo.rs");
        let source_rel = path_to_string(&relativize(&source, &book_root)).unwrap();
        let m = Manifest {
            version: 1,
            listings: vec![
                listing("foo-v5", &source_rel),
                listing("foo-v7", &source_rel),
                listing("foo-v3", &source_rel),
            ],
        };
        let tag = derive_default_tag(&m, &source, &book_root).unwrap();
        assert_eq!(tag, "foo-v8");
    }

    /// `<basename>-rev<N>` honoured as an allowlist prefix.
    #[test]
    fn derive_default_tag_honours_rev_prefix() {
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("foo.rs");
        let source_rel = path_to_string(&relativize(&source, &book_root)).unwrap();
        let m = Manifest {
            version: 1,
            listings: vec![listing("foo-rev3", &source_rel)],
        };
        let tag = derive_default_tag(&m, &source, &book_root).unwrap();
        assert_eq!(tag, "foo-rev4");
    }

    /// `<basename>-ver<N>` and `<basename>-version<N>` honoured too.
    #[test]
    fn derive_default_tag_honours_ver_and_version_prefixes() {
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("foo.rs");
        let source_rel = path_to_string(&relativize(&source, &book_root)).unwrap();
        let m_ver = Manifest {
            version: 1,
            listings: vec![listing("foo-ver7", &source_rel)],
        };
        assert_eq!(
            derive_default_tag(&m_ver, &source, &book_root).unwrap(),
            "foo-ver8",
        );
        let m_version = Manifest {
            version: 1,
            listings: vec![listing("foo-version2", &source_rel)],
        };
        assert_eq!(
            derive_default_tag(&m_version, &source, &book_root).unwrap(),
            "foo-version3",
        );
    }

    /// Mixed prefixes (`v`, then `rev`): the most-recently-inserted
    /// matching prefix wins, so a mid-stream convention switch sticks.
    #[test]
    fn derive_default_tag_picks_most_recent_prefix_when_mixed() {
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("foo.rs");
        let source_rel = path_to_string(&relativize(&source, &book_root)).unwrap();
        let m = Manifest {
            version: 1,
            listings: vec![
                listing("foo-v1", &source_rel),
                listing("foo-v2", &source_rel),
                listing("foo-rev3", &source_rel),
            ],
        };
        let tag = derive_default_tag(&m, &source, &book_root).unwrap();
        assert_eq!(tag, "foo-rev4");
    }

    /// Prior listings exist for the source but none match the allowlist
    /// (a `<basename>-ch<NN>-phase<N>` chapter/phase scheme is a
    /// motivating real case). Surfacing the unrecognised convention
    /// with the example tag tells the author EXACTLY what their
    /// existing scheme is so the `--tag` fix is one keystroke away.
    #[test]
    fn derive_default_tag_errors_when_prior_listings_use_unrecognised_convention() {
        let book_root = std::env::current_dir().unwrap();
        let source = book_root.join("compose.yaml");
        let source_rel = path_to_string(&relativize(&source, &book_root)).unwrap();
        let m = Manifest {
            version: 1,
            listings: vec![listing("compose-yaml-ch02-phase1", &source_rel)],
        };
        let err = derive_default_tag(&m, &source, &book_root).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("compose-yaml-ch02-phase1"),
            "diagnostic should quote the unrecognised prior tag; got: {msg}"
        );
        assert!(
            msg.contains("--tag"),
            "diagnostic should direct the author to pass --tag; got: {msg}"
        );
    }

    /// Other source files' listings don't pollute the derivation —
    /// only entries matching the current source path count.
    #[test]
    fn derive_default_tag_ignores_listings_for_other_sources() {
        let book_root = std::env::current_dir().unwrap();
        let foo = book_root.join("foo.rs");
        let bar = book_root.join("bar.rs");
        let foo_rel = path_to_string(&relativize(&foo, &book_root)).unwrap();
        let bar_rel = path_to_string(&relativize(&bar, &book_root)).unwrap();
        let m = Manifest {
            version: 1,
            listings: vec![
                listing("bar-v1", &bar_rel),
                listing("bar-v2", &bar_rel),
                listing("foo-v3", &foo_rel),
            ],
        };
        // foo's next should be v4, NOT v3 (which would be bar-v3 → +1).
        let tag = derive_default_tag(&m, &foo, &book_root).unwrap();
        assert_eq!(tag, "foo-v4");
    }
}
