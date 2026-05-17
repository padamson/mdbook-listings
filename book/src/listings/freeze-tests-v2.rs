//! Integration tests for the Freeze a Listing story (ch. 2). These pin the
//! error-path acceptance criteria that aren't covered by the book's own use
//! of the freeze primitive on its own listings.

use std::fs;

use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

/// `freeze` rejects re-running with the same tag but a now-different source
/// content unless `--force` is given. Without this guard, an author who edits
/// a source file and re-runs `freeze` would silently lose the previously
/// frozen bytes.
#[test]
fn freeze_rejects_conflicting_content_without_force() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source = tmp.path().join("compose.yaml");
    fs::write(&source, "a: 1\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "t", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success();

    fs::write(&source, "a: 2\n").unwrap();
    mdbook_listings()
        .args(["freeze", "--tag", "t", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .failure()
        .stderr(contains("already frozen"));
}

/// `freeze` rejects re-running with the same tag but content from an entirely
/// different source file, unless `--force` is given. The tag is the identity;
/// without this guard, an author who accidentally re-uses a tag for a new
/// source would clobber the previously frozen bytes silently.
#[test]
fn freeze_rejects_duplicate_tag_from_different_source() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source_a = tmp.path().join("a.yaml");
    let source_b = tmp.path().join("b.yaml");
    fs::write(&source_a, "a: 1\n").unwrap();
    fs::write(&source_b, "b: 2\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "t", "--book-root"])
        .arg(&book_root)
        .arg(&source_a)
        .assert()
        .success();

    mdbook_listings()
        .args(["freeze", "--tag", "t", "--book-root"])
        .arg(&book_root)
        .arg(&source_b)
        .assert()
        .failure()
        .stderr(contains("already frozen"));
}

/// Without the frozen path + `{{#include …}}` lines, the author has to grep
/// `listings.toml` after every freeze to learn what to paste into the chapter.
#[test]
fn freeze_prints_frozen_path_and_include_directive_on_created() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source = tmp.path().join("compose.yaml");
    fs::write(&source, "a: 1\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "demo", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success()
        .stdout(contains("created: demo"))
        .stdout(contains("src/listings/demo.yaml"))
        .stdout(contains("{{#include listings/demo.yaml}}"));
}

/// Re-running freeze without source changes is a common "give me the include
/// line again" path; the `Unchanged` outcome must surface the same supplement.
#[test]
fn freeze_prints_frozen_path_and_include_directive_on_unchanged() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source = tmp.path().join("compose.yaml");
    fs::write(&source, "a: 1\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "demo", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success();

    mdbook_listings()
        .args(["freeze", "--tag", "demo", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success()
        .stdout(contains("unchanged: demo"))
        .stdout(contains("src/listings/demo.yaml"))
        .stdout(contains("{{#include listings/demo.yaml}}"));
}

/// When a prior listing exists for the same source path, the CLI also prints
/// a ready-to-paste `{{#diff <prev> <new>}}` line — the second piece of
/// per-freeze friction (every versioned freeze in this book pairs a new
/// include with a new diff against the prior version).
#[test]
fn freeze_prints_diff_suggestion_when_prior_listing_exists_for_same_source() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source = tmp.path().join("compose.yaml");
    fs::write(&source, "a: 1\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "compose-v1", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success()
        .stdout(predicates::str::contains("diff:").not());

    fs::write(&source, "a: 2\n").unwrap();
    mdbook_listings()
        .args(["freeze", "--tag", "compose-v2", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success()
        .stdout(contains("created: compose-v2"))
        .stdout(contains("{{#diff compose-v1 compose-v2}}"));
}

/// The diff suggestion only fires for prior listings of the SAME source. A
/// fresh source path without prior listings stays quiet — no false-positive
/// diff against an unrelated tag.
#[test]
fn freeze_omits_diff_suggestion_when_no_prior_listing_for_same_source() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let other_source = tmp.path().join("other.yaml");
    fs::write(&other_source, "x: 1\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "other-v1", "--book-root"])
        .arg(&book_root)
        .arg(&other_source)
        .assert()
        .success();

    let source = tmp.path().join("compose.yaml");
    fs::write(&source, "a: 1\n").unwrap();
    mdbook_listings()
        .args(["freeze", "--tag", "compose-v1", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success()
        .stdout(contains("created: compose-v1"))
        .stdout(predicates::str::contains("diff:").not());
}

/// A re-frozen tag must be just as discoverable as a freshly created one;
/// the `Replaced` outcome must surface the same supplement.
#[test]
fn freeze_prints_frozen_path_and_include_directive_on_replaced() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source = tmp.path().join("compose.yaml");
    fs::write(&source, "a: 1\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "demo", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success();

    fs::write(&source, "a: 2\n").unwrap();
    mdbook_listings()
        .args(["freeze", "--tag", "demo", "--force", "--book-root"])
        .arg(&book_root)
        .arg(&source)
        .assert()
        .success()
        .stdout(contains("replaced: demo"))
        .stdout(contains("src/listings/demo.yaml"))
        .stdout(contains("{{#include listings/demo.yaml}}"));
}
