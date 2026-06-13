//! `verify`: the CI gate behind the book's core promise — the code it
//! shows is real. A frozen listing is "verified" when it is still the
//! intact snapshot that `freeze` recorded; current source is never
//! consulted, because diverging from a moving codebase is what freezing
//! is *for*.

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::freeze::hex_sha256;
use crate::manifest::Manifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Breaks the book's claim to show real code — fails the build.
    Error,
    /// Worth a look, but the book is still sound — reported, exit 0.
    Warning,
}

#[derive(Debug)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct VerifyReport {
    pub findings: Vec<Finding>,
    pub listings_checked: usize,
}

impl VerifyReport {
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }
}

/// Run every verify pass against the book at `book_root`.
pub fn verify(book_root: &Path) -> Result<VerifyReport> {
    let manifest = Manifest::load(book_root)?;
    let mut report = VerifyReport::default();
    check_snapshot_integrity(book_root, &manifest, &mut report);
    Ok(report)
}

/// Each manifest record's frozen file must exist and still hash to the
/// sha256 recorded at freeze time. A mismatch usually means someone
/// "fixed" the snapshot in place instead of refreezing.
fn check_snapshot_integrity(book_root: &Path, manifest: &Manifest, report: &mut VerifyReport) {
    for listing in &manifest.listings {
        report.listings_checked += 1;
        let frozen_abs = book_root.join(&listing.frozen);
        let bytes = match fs::read(&frozen_abs) {
            Ok(bytes) => bytes,
            Err(_) => {
                report.findings.push(Finding {
                    severity: Severity::Error,
                    message: format!(
                        "frozen listing `{}` is missing: {}",
                        listing.tag, listing.frozen,
                    ),
                });
                continue;
            }
        };
        if hex_sha256(&bytes) != listing.sha256 {
            report.findings.push(Finding {
                severity: Severity::Error,
                message: format!(
                    "frozen listing `{}` no longer matches its recorded sha256: {} \
                     (edited after freezing? refreeze or restore the snapshot)",
                    listing.tag, listing.frozen,
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Listing;
    use tempfile::TempDir;

    fn listing_for(tag: &str, frozen: &str, bytes: &[u8]) -> Listing {
        Listing {
            tag: tag.to_string(),
            source: "../src/demo.rs".to_string(),
            frozen: frozen.to_string(),
            sha256: hex_sha256(bytes),
        }
    }

    fn manifest_with(listings: Vec<Listing>) -> Manifest {
        Manifest {
            version: crate::manifest::MANIFEST_VERSION,
            listings,
        }
    }

    #[test]
    fn intact_snapshot_produces_no_findings() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        fs::write(root.join("src/listings/demo-v1.rs"), b"fn main() {}\n").unwrap();
        let manifest = manifest_with(vec![listing_for(
            "demo-v1",
            "src/listings/demo-v1.rs",
            b"fn main() {}\n",
        )]);

        let mut report = VerifyReport::default();
        check_snapshot_integrity(root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
        assert_eq!(report.listings_checked, 1);
    }

    #[test]
    fn tampered_snapshot_is_an_error_naming_tag_and_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        fs::write(
            root.join("src/listings/demo-v1.rs"),
            b"fn main() { /* edited */ }\n",
        )
        .unwrap();
        let manifest = manifest_with(vec![listing_for(
            "demo-v1",
            "src/listings/demo-v1.rs",
            b"fn main() {}\n",
        )]);

        let mut report = VerifyReport::default();
        check_snapshot_integrity(root, &manifest, &mut report);
        assert_eq!(report.error_count(), 1);
        let msg = &report.findings[0].message;
        assert!(msg.contains("demo-v1"), "got: {msg}");
        assert!(msg.contains("src/listings/demo-v1.rs"), "got: {msg}");
        assert!(msg.contains("sha256"), "got: {msg}");
    }

    #[test]
    fn missing_snapshot_is_an_error_not_a_crash() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let manifest = manifest_with(vec![listing_for(
            "demo-v1",
            "src/listings/demo-v1.rs",
            b"fn main() {}\n",
        )]);

        let mut report = VerifyReport::default();
        check_snapshot_integrity(root, &manifest, &mut report);
        assert_eq!(report.error_count(), 1);
        assert!(report.findings[0].message.contains("missing"));
    }

    #[test]
    fn every_listing_is_checked_even_after_a_failure() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        fs::write(root.join("src/listings/ok-v1.rs"), b"ok\n").unwrap();
        let manifest = manifest_with(vec![
            listing_for("gone-v1", "src/listings/gone-v1.rs", b"gone\n"),
            listing_for("ok-v1", "src/listings/ok-v1.rs", b"ok\n"),
        ]);

        let mut report = VerifyReport::default();
        check_snapshot_integrity(root, &manifest, &mut report);
        assert_eq!(report.listings_checked, 2);
        assert_eq!(report.error_count(), 1);
    }
}
