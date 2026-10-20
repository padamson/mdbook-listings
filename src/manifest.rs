//! Freeze manifest: the TOML file that records every listing that has been
//! frozen into a book. Lives at `<book_root>/listings.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

/// Current manifest schema version. Bumped when the on-disk layout changes in
/// a way that requires a migration.
pub const MANIFEST_VERSION: u32 = 1;

/// Relative path from a book root to the manifest file.
pub const MANIFEST_FILENAME: &str = "listings.toml";

/// Top-level manifest document.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default, rename = "listing")]
    pub listings: Vec<Listing>,
}

/// One entry per frozen listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Listing {
    /// Human-readable identifier chosen by the author. Unique within the
    /// manifest.
    pub tag: String,
    /// Original source path, relative to the book root. Informational — shallow
    /// verify does not re-read this file (deep verify, deferred to a later
    /// release, will).
    pub source: String,
    /// Path to the frozen copy, relative to the book root (e.g.
    /// `src/listings/compose-v1.yaml`).
    pub frozen: String,
    /// Hex-encoded sha256 of the frozen file's byte content. Used by shallow
    /// verify to detect post-freeze tampering.
    pub sha256: String,
    /// Where a reader can see this snapshot, typically the source file's URL
    /// at the commit it was frozen from. The provenance line links the path
    /// to it; for a source outside the book's repository it is the only
    /// path shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Why `url` can't be a listing's `url`, or `Ok` when it can. The provenance
/// line renders it as a link in HTML and in the PDF's Markdown, so only a web
/// address with a host is accepted, and nothing that would end the PDF's
/// `<…>` link destination early. Checked both when `freeze` records a URL and
/// when the manifest loads, since `listings.toml` is also edited by hand.
pub(crate) fn check_url(url: &str) -> std::result::Result<(), String> {
    let rest = ["https://", "http://"].iter().find_map(|scheme| {
        url.get(..scheme.len())
            .filter(|s| s.eq_ignore_ascii_case(scheme))
            .map(|_| &url[scheme.len()..])
    });
    match rest {
        None => Err(format!("must be an http:// or https:// URL, got `{url}`")),
        Some("") => Err(format!("names no host, got `{url}`")),
        Some(_)
            if url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>') =>
        {
            Err(format!(
                "must not contain whitespace, `<` or `>`, got `{url}`"
            ))
        }
        Some(_) => Ok(()),
    }
}

impl Manifest {
    /// Load the manifest from `<book_root>/listings.toml`. Returns an empty
    /// manifest if the file does not exist.
    pub fn load(book_root: &Path) -> Result<Self> {
        let path = Self::path(book_root);
        if !path.exists() {
            return Ok(Self {
                version: MANIFEST_VERSION,
                listings: Vec::new(),
            });
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading manifest at {}", path.display()))?;
        let manifest: Manifest = toml::from_str(&text)
            .with_context(|| format!("parsing manifest at {}", path.display()))?;
        for listing in &manifest.listings {
            if let Some(url) = &listing.url {
                check_url(url).map_err(|e| {
                    anyhow!(
                        "manifest at {}: listing `{}` has url {e}",
                        path.display(),
                        listing.tag
                    )
                })?;
            }
        }
        if manifest.version != MANIFEST_VERSION {
            return Err(anyhow!(
                "manifest at {} has version {}, expected {}",
                path.display(),
                manifest.version,
                MANIFEST_VERSION
            ));
        }
        Ok(manifest)
    }

    /// Write the manifest to `<book_root>/listings.toml`, creating parent
    /// directories as needed.
    pub fn save(&self, book_root: &Path) -> Result<()> {
        let path = Self::path(book_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating manifest parent {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializing manifest to TOML")?;
        fs::write(&path, text)
            .with_context(|| format!("writing manifest to {}", path.display()))?;
        Ok(())
    }

    /// Look up a listing by tag.
    pub fn find(&self, tag: &str) -> Option<&Listing> {
        self.listings.iter().find(|l| l.tag == tag)
    }

    /// Insert or replace a listing by tag, keeping the vector in insertion
    /// order (existing entries retain their position).
    pub fn upsert(&mut self, listing: Listing) {
        match self.listings.iter().position(|l| l.tag == listing.tag) {
            Some(idx) => self.listings[idx] = listing,
            None => self.listings.push(listing),
        }
    }

    fn path(book_root: &Path) -> PathBuf {
        book_root.join(MANIFEST_FILENAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_rejects_unknown_manifest_version() {
        let tmp = TempDir::new().unwrap();
        let manifest_path = tmp.path().join(MANIFEST_FILENAME);
        fs::write(
            &manifest_path,
            "version = 99\n\n[[listing]]\n\
             tag = \"x\"\nsource = \"a\"\nfrozen = \"b\"\nsha256 = \"c\"\n",
        )
        .unwrap();

        let err = Manifest::load(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("version 99") && msg.contains(&MANIFEST_VERSION.to_string()),
            "diagnostic should name both the found and the expected version, got: {msg}",
        );
    }

    #[test]
    fn load_accepts_current_manifest_version() {
        let tmp = TempDir::new().unwrap();
        let manifest_path = tmp.path().join(MANIFEST_FILENAME);
        fs::write(&manifest_path, format!("version = {MANIFEST_VERSION}\n")).unwrap();

        let m = Manifest::load(tmp.path()).expect("current-version manifest should load");
        assert_eq!(m.version, MANIFEST_VERSION);
        assert!(m.listings.is_empty());
    }

    #[test]
    fn load_rejects_a_hand_edited_url_that_is_not_a_web_address() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join(MANIFEST_FILENAME),
            format!(
                "version = {MANIFEST_VERSION}\n\n[[listing]]\n\
                 tag = \"x-v1\"\nsource = \"a\"\nfrozen = \"b\"\nsha256 = \"c\"\n\
                 url = \"javascript:alert(1)\"\n"
            ),
        )
        .unwrap();
        let msg = Manifest::load(tmp.path()).unwrap_err().to_string();
        assert!(
            msg.contains("x-v1") && msg.contains("javascript:alert(1)"),
            "the error names the listing and the url; got: {msg}"
        );
    }

    #[test]
    fn check_url_accepts_a_web_address_with_parentheses() {
        assert_eq!(
            check_url("https://en.wikipedia.org/wiki/YAML_(format)"),
            Ok(())
        );
    }

    #[test]
    fn check_url_refuses_what_would_break_the_link_markup() {
        for url in [
            "https://example.com/a b",
            "https://example.com/<a",
            "https://example.com/a>",
            "https://example.com/a\tb",
            "https://example.com/a\u{7}b",
        ] {
            assert!(check_url(url).is_err(), "{url:?} should be refused");
        }
    }
}
