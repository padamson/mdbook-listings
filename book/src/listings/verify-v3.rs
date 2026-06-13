//! `verify`: the CI gate behind the book's core promise — the code it
//! shows is real. A frozen listing is "verified" when it is still the
//! intact snapshot that `freeze` recorded; current source is never
//! consulted, because diverging from a moving codebase is what freezing
//! is *for*.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::directive::{FencePolicy, line_number, scan_directives};
use crate::freeze::hex_sha256;
use crate::manifest::Manifest;

/// Where frozen listings live, relative to the book root. Matches
/// `freeze`'s `LISTINGS_SUBDIR` — frozen files always land here regardless
/// of the book's configured `src`.
const LISTINGS_REL: &str = "src/listings";

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

    fn error(&mut self, message: String) {
        self.findings.push(Finding {
            severity: Severity::Error,
            message,
        });
    }

    fn warning(&mut self, message: String) {
        self.findings.push(Finding {
            severity: Severity::Warning,
            message,
        });
    }
}

/// Run every verify pass against the book at `book_root`.
pub fn verify(book_root: &Path) -> Result<VerifyReport> {
    let manifest = Manifest::load(book_root)?;
    let mut report = VerifyReport::default();
    check_snapshot_integrity(book_root, &manifest, &mut report);
    check_references(book_root, &manifest, &mut report);
    check_sidecars(book_root, &manifest, &mut report);
    check_orphans(book_root, &manifest, &mut report);
    check_live_operands(book_root, &mut report);
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

/// Every `{{#include listings/TAG…}}` path and every `{{#diff}}` tag
/// operand in chapter prose must name a manifest record. A dangling
/// reference is an error the build would also hit; verify reports it with
/// chapter:line up front. `live:` operands are not resolution targets
/// (they show current source); they are audited separately.
fn check_references(book_root: &Path, manifest: &Manifest, report: &mut VerifyReport) {
    let tags: HashSet<&str> = manifest.listings.iter().map(|l| l.tag.as_str()).collect();
    for (rel, content) in chapter_markdown(book_root) {
        for occ in scan_directives(&content, "{{#include ", FencePolicy::Annotate) {
            let path = occ.args.trim();
            // Only listings/ includes resolve to a frozen tag; snippets/
            // and other paths are not manifest records.
            let Some(rest) = path.strip_prefix("listings/") else {
                continue;
            };
            // Drop any `:start:end` range suffix.
            let file = rest.split(':').next().unwrap_or(rest);
            // A `listings/<tag>.callouts.toml` include displays a sidecar
            // file, not a frozen listing — its existence is the sidecar
            // pass's job, not a tag reference here.
            if file.ends_with(".callouts.toml") {
                continue;
            }
            let stem = Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if !tags.contains(stem) {
                report.error(format!(
                    "{rel}:{}: {{{{#include listings/{file}}}}} names no frozen listing `{stem}`",
                    line_number(&content, occ.span.start),
                ));
            }
        }
        for occ in scan_directives(&content, "{{#diff", FencePolicy::SkipInside) {
            let tokens: Vec<&str> = occ.args.split_whitespace().collect();
            // The diff splicer only processes 2-token (whole-file) or
            // 4-token (with ranges) forms; the first two tokens are the
            // operands. Other arities are left literal, so don't validate.
            if tokens.len() != 2 && tokens.len() != 4 {
                continue;
            }
            for operand in &tokens[..2] {
                if operand.starts_with("live:") {
                    continue;
                }
                if manifest.find(operand).is_none() {
                    report.error(format!(
                        "{rel}:{}: {{{{#diff}}}} operand `{operand}` names no frozen listing",
                        line_number(&content, occ.span.start),
                    ));
                }
            }
        }
    }
}

/// Each `<tag>.callouts.toml` sidecar must sit next to a real frozen
/// listing — its `<tag>` must match a manifest record's frozen-file stem.
/// A dangling sidecar attaches annotations to nothing and the build never
/// complains, so verify treats it as a broken reference.
fn check_sidecars(book_root: &Path, manifest: &Manifest, report: &mut VerifyReport) {
    let stems: HashSet<&str> = manifest
        .listings
        .iter()
        .filter_map(|l| Path::new(&l.frozen).file_stem().and_then(|s| s.to_str()))
        .collect();
    for name in listing_dir_entries(book_root) {
        let Some(stem) = name.strip_suffix(".callouts.toml") else {
            continue;
        };
        if !stems.contains(stem) {
            report.error(format!(
                "sidecar `{name}` names no frozen listing `{stem}` (its annotations attach to nothing)",
            ));
        }
    }
}

/// A frozen file under `src/listings/` that no manifest record claims is
/// an orphan — reported as a warning (stray, not broken). Sidecars are
/// handled by [`check_sidecars`], not here.
fn check_orphans(book_root: &Path, manifest: &Manifest, report: &mut VerifyReport) {
    let claimed: HashSet<&str> = manifest
        .listings
        .iter()
        .map(|l| l.frozen.as_str())
        .collect();
    for name in listing_dir_entries(book_root) {
        if name.ends_with(".callouts.toml") {
            continue;
        }
        let rel = format!("{LISTINGS_REL}/{name}");
        if !claimed.contains(rel.as_str()) {
            report.warning(format!(
                "orphan frozen file: {rel} (no manifest record claims it)"
            ));
        }
    }
}

/// Report every `live:` diff operand. A `live:` operand renders current
/// source instead of a frozen snapshot, so that spot tracks a moving
/// codebase — the freeze stability guarantee is deliberately traded away.
/// This is a warning, not an error: it's a legitimate choice the author
/// should simply be able to see at a glance.
fn check_live_operands(book_root: &Path, report: &mut VerifyReport) {
    for (rel, content) in chapter_markdown(book_root) {
        for occ in scan_directives(&content, "{{#diff", FencePolicy::SkipInside) {
            let tokens: Vec<&str> = occ.args.split_whitespace().collect();
            if tokens.len() != 2 && tokens.len() != 4 {
                continue;
            }
            for operand in &tokens[..2] {
                if let Some(path) = operand.strip_prefix("live:") {
                    report.warning(format!(
                        "{rel}:{}: {{{{#diff}}}} uses a live operand `live:{path}` — \
                         shows current source, not a frozen snapshot, so freeze \
                         stability is traded away here",
                        line_number(&content, occ.span.start),
                    ));
                }
            }
        }
    }
}

/// Top-level file names in `<book_root>/src/listings/` (no subdirectories).
fn listing_dir_entries(book_root: &Path) -> Vec<String> {
    let dir = book_root.join(LISTINGS_REL);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

/// All `*.md` files under the book's configured `src` directory (default
/// `src`), excluding the `listings/` subtree (frozen content, not
/// chapters). Returns `(book-relative display path, content)` pairs.
fn chapter_markdown(book_root: &Path) -> Vec<(String, String)> {
    let src = chapter_src_dir(book_root);
    let listings = book_root.join(LISTINGS_REL);
    let mut out = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        if dir == listings {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md")
                && let Ok(content) = fs::read_to_string(&path)
            {
                let rel = path
                    .strip_prefix(book_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, content));
            }
        }
    }
    out
}

/// The book's chapter source directory: `[book] src` from `book.toml`,
/// defaulting to `src` when absent or unparsable.
fn chapter_src_dir(book_root: &Path) -> PathBuf {
    let src = fs::read_to_string(book_root.join("book.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|t| {
            t.get("book")?
                .as_table()?
                .get("src")?
                .as_str()
                .map(String::from)
        })
        .unwrap_or_else(|| "src".to_string());
    book_root.join(src)
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

    /// Build a book root with a `src/listings/` dir and a manifest record
    /// for `demo-v1`, returning the temp dir and its path.
    fn book_with_demo() -> (TempDir, PathBuf, Manifest) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        fs::write(root.join("src/listings/demo-v1.rs"), b"x\n").unwrap();
        let manifest = manifest_with(vec![listing_for(
            "demo-v1",
            "src/listings/demo-v1.rs",
            b"x\n",
        )]);
        (tmp, root, manifest)
    }

    #[test]
    fn check_references_flags_unknown_diff_operand_with_chapter_and_line() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "intro\n\n{{#diff demo-v1 ghost-v1}}\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 1);
        let m = &report.findings[0].message;
        assert!(m.contains("ghost-v1"), "got: {m}");
        assert!(m.contains("ch.md:3"), "expects chapter:line; got: {m}");
    }

    #[test]
    fn check_references_accepts_known_diff_operands_and_skips_live() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "{{#diff demo-v1 live:../src/foo.rs}}\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0, "got {:?}", report.findings);
    }

    #[test]
    fn check_references_ignores_wrong_arity_diff() {
        let (_t, root, manifest) = book_with_demo();
        // Three tokens: the diff splicer leaves this literal, so verify
        // must not validate its operands (no false positive on `ghost`).
        fs::write(root.join("src/ch.md"), "{{#diff demo-v1 ghost extra}}\n").unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0, "got {:?}", report.findings);
    }

    #[test]
    fn check_references_flags_unknown_include_and_accepts_known() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "```rust\n{{#include listings/ghost.rs}}\n```\n\n\
             ```rust\n{{#include listings/demo-v1.rs}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 1, "got {:?}", report.findings);
        assert!(report.findings[0].message.contains("ghost"));
    }

    #[test]
    fn check_references_resolves_include_with_range_suffix() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "```rust\n{{#include listings/demo-v1.rs:1:1}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0, "got {:?}", report.findings);
    }

    #[test]
    fn check_references_ignores_sidecar_toml_includes() {
        // A chapter that includes a `.callouts.toml` to display it (ch.6
        // does this) is not referencing a frozen listing — its existence
        // is the sidecar pass's job, not a tag reference here.
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "```toml\n{{#include listings/demo-v1.callouts.toml}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0, "got {:?}", report.findings);
    }

    #[test]
    fn check_live_operands_warns_with_chapter_and_line() {
        let (_t, root, _m) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "intro\n\n{{#diff demo-v1 live:../src/foo.rs}}\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_live_operands(&root, &mut report);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        assert_eq!(report.findings[0].severity, Severity::Warning);
        let m = &report.findings[0].message;
        assert!(m.contains("live:../src/foo.rs"), "got: {m}");
        assert!(m.contains("ch.md:3"), "got: {m}");
    }

    #[test]
    fn check_live_operands_ignores_wrong_arity_diff() {
        // A 3-token diff is left literal by the splicer, so its operands
        // (live: or not) are not audited — pins the arity guard.
        let (_t, root, _m) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "{{#diff demo-v1 live:../src/foo.rs extra}}\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_live_operands(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_live_operands_silent_when_no_live_operand() {
        let (_t, root, _m) = book_with_demo();
        fs::write(root.join("src/ch.md"), "{{#diff demo-v1 demo-v1}}\n").unwrap();

        let mut report = VerifyReport::default();
        check_live_operands(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_references_ignores_snippets_includes() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "```rust\n{{#include snippets/whatever.rs}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0, "got {:?}", report.findings);
    }

    #[test]
    fn check_references_skips_the_listings_subtree() {
        let (_t, root, manifest) = book_with_demo();
        // A frozen .md listing must not be scanned as a chapter.
        fs::write(
            root.join("src/listings/frozen-doc.md"),
            "{{#diff ghost-a ghost-b}}\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0, "got {:?}", report.findings);
    }

    #[test]
    fn check_sidecars_flags_dangling_and_accepts_matching() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/listings/demo-v1.callouts.toml"),
            "[[callout]]\nline=1\nlabel=\"a\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/listings/ghost.callouts.toml"),
            "[[callout]]\nline=1\nlabel=\"b\"\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_sidecars(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 1, "got {:?}", report.findings);
        let m = &report.findings[0].message;
        assert!(m.contains("ghost"), "got: {m}");
        assert!(m.contains("callouts.toml"), "got: {m}");
    }

    #[test]
    fn check_orphans_warns_on_unclaimed_file_and_ignores_sidecars() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(root.join("src/listings/orphan.rs"), b"stray\n").unwrap();
        // A sidecar must not be reported as an orphan.
        fs::write(
            root.join("src/listings/demo-v1.callouts.toml"),
            "[[callout]]\nline=1\nlabel=\"a\"\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_orphans(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        assert_eq!(report.findings[0].severity, Severity::Warning);
        assert!(report.findings[0].message.contains("orphan.rs"));
    }

    #[test]
    fn chapter_src_dir_honors_book_toml_and_defaults_to_src() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        assert_eq!(chapter_src_dir(root), root.join("src"));
        fs::write(root.join("book.toml"), "[book]\nsrc = \"text\"\n").unwrap();
        assert_eq!(chapter_src_dir(root), root.join("text"));
    }
}
