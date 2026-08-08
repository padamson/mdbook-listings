//! `verify`: the CI gate behind the book's core promise — the code it
//! shows is real. A frozen listing is "verified" when it is still the
//! intact snapshot that `freeze` recorded; current source is never
//! consulted, because diverging from a moving codebase is what freezing
//! is *for*.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::callout::{callouts_for_block, comment_prefix_for_extension, parse_callouts};
use crate::diff::splice_chapter as splice_diffs;
use crate::directive::{FencePolicy, line_number, scan_directives, split_caption, split_label};
use crate::fence::FencedBlocks;
use crate::freeze::hex_sha256;
use crate::include::{parse_listing_includes, splice_chapter as splice_includes};
use crate::manifest::{Listing, Manifest};

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
    check_unreferenced_markers(book_root, &manifest, &mut report);
    check_slice_ends_on_marker(book_root, &mut report);
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
            let (args, _caption) = split_caption(occ.args);
            let (args, _label) = split_label(&args);
            let path = args.trim();
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
            let (args, _caption) = split_caption(occ.args);
            let (args, _label) = split_label(&args);
            // Drop a `context=N` token so it isn't miscounted as an operand,
            // matching the diff parser.
            let tokens: Vec<&str> = args
                .split_whitespace()
                .filter(|t| !t.starts_with("context="))
                .collect();
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
            let (args, _caption) = split_caption(occ.args);
            let (args, _label) = split_label(&args);
            let tokens: Vec<&str> = args
                .split_whitespace()
                .filter(|t| !t.starts_with("context="))
                .collect();
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

/// Every `CALLOUT:` marker that renders a badge should be picked up by a
/// `{{#callout <label>}}` somewhere in the book's prose. The reverse
/// direction already fails the build (an unknown label errors), but this
/// direction matches the ordinary authoring slip — marker added while
/// editing the source, prose never written — and fails into a page that
/// looks fine. A warning, not an error: the badge's hover text is
/// self-contained, so annotation without prose stays a legitimate choice.
///
/// Only markers that actually render count. A marker in a frozen file no
/// chapter shows — an old version kept as a diff operand, a line outside
/// every include's slice, a context line in a diff — produces no badge,
/// so warning on it would be noise. Verify runs the same include and diff
/// splices as the build and asks the same question the callout pass does:
/// which markers does this block badge?
fn check_unreferenced_markers(book_root: &Path, manifest: &Manifest, report: &mut VerifyReport) {
    let referenced = referenced_callout_labels(book_root);
    let src_dir = chapter_src_dir(book_root);
    let mut reported: HashSet<(String, String)> = HashSet::new();
    for (rel, content) in chapter_markdown(book_root) {
        let chapter_abs = book_root.join(&rel);
        let chapter_path = chapter_abs.strip_prefix(&src_dir).ok();
        let chapter_dir = chapter_abs
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| src_dir.clone());
        // A chapter the splicers reject is skipped: whatever is broken
        // there is the reference or integrity pass's finding.
        let Ok(expanded) = splice_includes(&content, &src_dir, chapter_path) else {
            continue;
        };
        let Ok(expanded) = splice_diffs(&expanded, manifest, book_root, chapter_path, &chapter_dir)
        else {
            continue;
        };
        for block in FencedBlocks::new(&expanded) {
            for callout in callouts_for_block(block.info, block.body) {
                if referenced.contains(&callout.label) {
                    continue;
                }
                let Some(listing) = block_listing(&expanded, block.close_end, manifest) else {
                    continue;
                };
                let Some(line) = marker_line_in_frozen(book_root, listing, &callout.label) else {
                    continue;
                };
                if reported.insert((listing.frozen.clone(), callout.label.clone())) {
                    report.warning(format!(
                        "{}:{}: CALLOUT marker `{}` is referenced by no {{{{#callout}}}} directive",
                        listing.frozen, line, callout.label,
                    ));
                }
            }
        }
    }
}

/// A sliced include whose end line is a `CALLOUT:` marker renders the
/// badge while excluding the line it annotates — the badge attaches to
/// nothing. Slice bounds are line numbers and shift every refreeze, so a
/// range that ended cleanly one week ends on a marker the next, and the
/// page still looks finished. A marker on the file's last line is skipped:
/// its badge clamps to the last visible line in every rendering, sliced or
/// not, so the slice is not what broke it.
fn check_slice_ends_on_marker(book_root: &Path, report: &mut VerifyReport) {
    let src_dir = chapter_src_dir(book_root);
    for (rel, content) in chapter_markdown(book_root) {
        for d in parse_listing_includes(&content) {
            let Some(range) = &d.range else {
                continue;
            };
            let Some(end) = range.end else {
                continue;
            };
            let Some(prefix) = Path::new(&d.rel_path)
                .extension()
                .and_then(|e| e.to_str())
                .and_then(comment_prefix_for_extension)
            else {
                continue;
            };
            // A dangling include is the reference pass's finding.
            let Ok(file) = fs::read_to_string(src_dir.join(&d.rel_path)) else {
                continue;
            };
            if end >= file.lines().count() {
                continue;
            }
            let Some(marker_line) = file.lines().nth(end - 1) else {
                continue;
            };
            let Some(callout) = parse_callouts(marker_line, prefix).into_iter().next() else {
                continue;
            };
            report.warning(format!(
                "{rel}:{}: slice {}:{} ends on the CALLOUT marker `{}`; \
                 the line it annotates ({}) is outside the range",
                line_number(&content, d.span.start),
                d.rel_path,
                range.render(),
                callout.label,
                end + 1,
            ));
        }
    }
}

/// The manifest record behind a rendered block, read off the locator
/// anchor after its closing fence. For a diff block that is the right-hand
/// operand: only added lines carry badges, and added lines belong to the
/// right side. A `live:` operand names no record and returns `None`.
fn block_listing<'m>(
    content: &str,
    close_end: usize,
    manifest: &'m Manifest,
) -> Option<&'m Listing> {
    let tail = &content[close_end..];
    let after_newline = tail.strip_prefix('\n').unwrap_or(tail);
    let div_open = after_newline.find("<div ")?;
    if div_open > crate::anchor::SCAN_TOLERANCE {
        return None;
    }
    let div_end = after_newline[div_open..].find('>')? + div_open;
    let div_text = &after_newline[div_open..div_end];
    let tag = crate::anchor::attr_value(div_text, "data-listing-tag")
        .or_else(|| crate::anchor::attr_value(div_text, "data-listing-diff-right"))?;
    manifest.find(&tag)
}

/// Locate `label`'s marker line in the listing's frozen file. The rendered
/// block may be a slice or a diff, so the line within the block is not the
/// line in the file; re-parsing the frozen source gives the position an
/// author can jump to.
fn marker_line_in_frozen(book_root: &Path, listing: &Listing, label: &str) -> Option<usize> {
    let prefix = Path::new(&listing.frozen)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(comment_prefix_for_extension)?;
    let content = fs::read_to_string(book_root.join(&listing.frozen)).ok()?;
    parse_callouts(&content, prefix)
        .into_iter()
        .find(|c| c.label == label)
        .map(|c| c.line)
}

/// Labels named by a `{{#callout <label>}}` directive in any chapter.
/// Same fence policy as the splicer, so documentation examples (inside
/// fenced blocks, inline backticks, or backslash-escaped) don't count.
fn referenced_callout_labels(book_root: &Path) -> HashSet<String> {
    let mut labels = HashSet::new();
    for (_rel, content) in chapter_markdown(book_root) {
        for occ in scan_directives(&content, "{{#callout ", FencePolicy::SkipInside) {
            labels.insert(occ.args.trim().to_string());
        }
    }
    labels
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
    fn check_references_validates_diff_operands_despite_context_arg() {
        let (_t, root, manifest) = book_with_demo();
        // The `context=6` token must not be miscounted as an operand: the
        // diff is still a valid 2-operand form, so `ghost` is flagged.
        fs::write(
            root.join("src/ch.md"),
            "{{#diff demo-v1 ghost context=6}}\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_references(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 1, "got {:?}", report.findings);
        assert!(report.findings[0].message.contains("ghost"));
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
    fn check_references_tolerates_caption_on_include_and_diff() {
        let (_t, root, manifest) = book_with_demo();
        fs::write(
            root.join("src/ch.md"),
            "```rust\n{{#include listings/demo-v1.rs caption=\"A demo\"}}\n```\n\n\
             {{#diff demo-v1 demo-v1 caption=\"A diff\"}}\n",
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

    /// Build a book root whose frozen listing carries one inline
    /// `# CALLOUT:` marker, returning the temp dir, root, and manifest.
    fn book_with_marked_listing() -> (TempDir, PathBuf, Manifest) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        let body = b"key: value\n# CALLOUT: greeting Says hello.\nfoo: bar\n";
        fs::write(root.join("src/listings/demo-v1.yaml"), body).unwrap();
        let manifest = manifest_with(vec![listing_for(
            "demo-v1",
            "src/listings/demo-v1.yaml",
            body,
        )]);
        (tmp, root, manifest)
    }

    /// A fenced include of the fixture's marked listing.
    const DEMO_INCLUDE: &str = "```yaml\n{{#include listings/demo-v1.yaml}}\n```\n";

    /// Two frozen versions of a listing, for `{{#diff demo-v1 demo-v2}}`.
    fn book_with_diffed_listings(v1: &[u8], v2: &[u8]) -> (TempDir, PathBuf, Manifest) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        fs::write(root.join("src/listings/demo-v1.yaml"), v1).unwrap();
        fs::write(root.join("src/listings/demo-v2.yaml"), v2).unwrap();
        let manifest = manifest_with(vec![
            listing_for("demo-v1", "src/listings/demo-v1.yaml", v1),
            listing_for("demo-v2", "src/listings/demo-v2.yaml", v2),
        ]);
        (tmp, root, manifest)
    }

    #[test]
    fn check_unreferenced_markers_warns_with_path_line_and_label() {
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(root.join("src/ch.md"), DEMO_INCLUDE).unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        assert_eq!(report.findings[0].severity, Severity::Warning);
        let m = &report.findings[0].message;
        assert!(m.contains("src/listings/demo-v1.yaml:2"), "got: {m}");
        assert!(m.contains("`greeting`"), "got: {m}");
        assert!(m.contains("{{#callout}}"), "got: {m}");
    }

    #[test]
    fn check_unreferenced_markers_silent_when_a_directive_references_the_label() {
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            format!("{DEMO_INCLUDE}\nThe marker {{{{#callout greeting}}}} is picked up here.\n"),
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_silent_when_no_chapter_renders_the_listing() {
        // A marker in a frozen file no chapter shows (an old version kept
        // as diff history) renders no badge, so it must not warn.
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(root.join("src/ch.md"), "Prose without any listing.\n").unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_silent_when_slice_excludes_the_marker() {
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/demo-v1.yaml:1:1}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_slice_covering_the_marker_reports_the_frozen_line() {
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/demo-v1.yaml:2:3}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        // Line 2 of the FILE, not of the slice — the diagnostic must point
        // where the author can jump to.
        assert!(
            report.findings[0]
                .message
                .contains("src/listings/demo-v1.yaml:2"),
            "got: {}",
            report.findings[0].message,
        );
    }

    #[test]
    fn check_unreferenced_markers_silent_on_a_diff_context_marker() {
        // The marker is unchanged between the two versions, so the diff
        // shows it as a context line — no badge renders.
        let (_t, root, manifest) = book_with_diffed_listings(
            b"a: 1\n# CALLOUT: ctx Unchanged note.\nb: 2\n",
            b"a: 1\n# CALLOUT: ctx Unchanged note.\nb: 3\n",
        );
        fs::write(root.join("src/ch.md"), "{{#diff demo-v1 demo-v2}}\n").unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_warns_on_a_diff_added_marker() {
        // The marker is new in the right operand, so the diff badges it;
        // the diagnostic points into the right side's frozen file.
        let (_t, root, manifest) =
            book_with_diffed_listings(b"a: 1\n", b"a: 1\n# CALLOUT: added New note.\n");
        fs::write(root.join("src/ch.md"), "{{#diff demo-v1 demo-v2}}\n").unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        let m = &report.findings[0].message;
        assert!(m.contains("src/listings/demo-v2.yaml:2"), "got: {m}");
        assert!(m.contains("`added`"), "got: {m}");
    }

    #[test]
    fn block_listing_honors_the_anchor_scan_tolerance() {
        // Same 64-byte boundary the callout pass enforces: an anchor at
        // the tolerance is read, one byte past it is not.
        let (_t, _root, manifest) = book_with_marked_listing();
        let anchored = |pad: usize| {
            format!(
                "```yaml\nkey: value\n```\n{}<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
                "x".repeat(pad),
            )
        };
        let at_64 = anchored(64);
        let close_end = at_64.rfind("```\n").map(|i| i + 4).unwrap();
        assert!(block_listing(&at_64, close_end, &manifest).is_some());
        let at_65 = anchored(65);
        let close_end = at_65.rfind("```\n").map(|i| i + 4).unwrap();
        assert!(block_listing(&at_65, close_end, &manifest).is_none());
    }

    #[test]
    fn check_slice_end_warns_when_the_end_line_is_a_marker() {
        // Marker on line 2 annotates line 3; a slice ending on line 2
        // renders a badge attached to nothing.
        let (_t, root, _m) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            "intro\n\n```yaml\n{{#include listings/demo-v1.yaml:1:2}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        assert_eq!(report.findings[0].severity, Severity::Warning);
        let m = &report.findings[0].message;
        assert!(m.contains("ch.md:4"), "expects chapter:line; got: {m}");
        assert!(m.contains("1:2"), "got: {m}");
        assert!(m.contains("`greeting`"), "got: {m}");
        assert!(m.contains("(3)"), "expects the annotated line; got: {m}");
    }

    #[test]
    fn check_slice_end_silent_when_the_slice_stops_before_the_marker() {
        let (_t, root, _m) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/demo-v1.yaml:1:1}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_slice_end_silent_when_the_slice_covers_the_annotated_line() {
        let (_t, root, _m) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/demo-v1.yaml:1:3}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_slice_end_silent_on_open_ended_and_full_file_includes() {
        // An open end runs to EOF and a full-file include has no slice;
        // neither can exclude a marker's annotated line.
        let (_t, root, _m) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/demo-v1.yaml:2:}}\n```\n\n\
             ```yaml\n{{#include listings/demo-v1.yaml}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_slice_end_silent_when_the_marker_is_the_last_file_line() {
        // A marker with no following line clamps its badge to the last
        // visible line in every rendering, sliced or not — the slice is
        // not what broke it.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        fs::write(
            root.join("src/listings/tail-v1.yaml"),
            "key: value\n# CALLOUT: tail At the end.\n",
        )
        .unwrap();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/tail-v1.yaml:1:2}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_slice_end_applies_to_snippets_includes_too() {
        // Snippets aren't frozen, but their markers badge the same way,
        // so the same slice slip breaks them the same way.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/snippets")).unwrap();
        fs::write(
            root.join("src/snippets/demo.rs"),
            "fn a() {}\n// CALLOUT: snip Note.\nfn b() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/ch.md"),
            "```rust\n{{#include snippets/demo.rs:1:2}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
        assert!(report.findings[0].message.contains("`snip`"));
    }

    #[test]
    fn check_slice_end_silent_on_a_missing_included_file() {
        // A dangling include is the reference pass's finding.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/ghost.yaml:1:2}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_slice_ends_on_marker(&root, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_warns_once_for_a_listing_shown_in_two_chapters() {
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(root.join("src/ch01.md"), DEMO_INCLUDE).unwrap();
        fs::write(root.join("src/ch02.md"), DEMO_INCLUDE).unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_reference_may_live_in_any_chapter() {
        // The prose that picks up a marker can sit in a different chapter
        // than the one embedding the listing — references are book-global.
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(
            root.join("src/ch01.md"),
            "```yaml\n{{#include listings/demo-v1.yaml}}\n```\n",
        )
        .unwrap();
        fs::write(root.join("src/ch02.md"), "See {{#callout greeting}}.\n").unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_ignores_documentation_example_references() {
        // A directive inside a fenced block or inline backticks is a
        // documentation example, not a pickup — the splicer leaves those
        // literal, so verify must not count them as references.
        let (_t, root, manifest) = book_with_marked_listing();
        fs::write(
            root.join("src/ch.md"),
            format!(
                "{DEMO_INCLUDE}\n```text\n{{{{#callout greeting}}}}\n```\n\n\
                 Use `{{{{#callout greeting}}}}` to refer.\n"
            ),
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert_eq!(report.findings.len(), 1, "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_skips_listings_without_comment_syntax() {
        // A .css frozen file has no recognised single-line comment prefix,
        // so nothing in it can be an inline marker even when rendered.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        let body = b"/* CALLOUT: nope */\n.a { color: red; }\n";
        fs::write(root.join("src/listings/style-v1.css"), body).unwrap();
        let manifest = manifest_with(vec![listing_for(
            "style-v1",
            "src/listings/style-v1.css",
            body,
        )]);
        fs::write(
            root.join("src/ch.md"),
            "```css\n{{#include listings/style-v1.css}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_silent_on_missing_frozen_file() {
        // A missing snapshot fails the include splice; that is the
        // integrity pass's finding — this pass must not crash or report.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        let manifest = manifest_with(vec![listing_for(
            "gone-v1",
            "src/listings/gone-v1.yaml",
            b"x\n",
        )]);
        fs::write(
            root.join("src/ch.md"),
            "```yaml\n{{#include listings/gone-v1.yaml}}\n```\n",
        )
        .unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
    }

    #[test]
    fn check_unreferenced_markers_reports_every_orphan_marker() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        let body = b"# CALLOUT: one First.\nkey: value\n# CALLOUT: two Second.\n";
        fs::write(root.join("src/listings/demo-v1.yaml"), body).unwrap();
        let manifest = manifest_with(vec![listing_for(
            "demo-v1",
            "src/listings/demo-v1.yaml",
            body,
        )]);
        fs::write(root.join("src/ch.md"), DEMO_INCLUDE).unwrap();

        let mut report = VerifyReport::default();
        check_unreferenced_markers(&root, &manifest, &mut report);
        assert_eq!(report.findings.len(), 2, "got {:?}", report.findings);
        let messages: Vec<&str> = report.findings.iter().map(|f| f.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("`one`")), "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("`two`")), "{messages:?}");
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
