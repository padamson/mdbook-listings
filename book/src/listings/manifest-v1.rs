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
}
