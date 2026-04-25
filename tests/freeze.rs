//! Integration tests for the Freeze a Listing story (ch. 2). These pin the
//! error-path acceptance criteria that aren't covered by the book's own use
//! of the freeze primitive on its own listings.

use std::fs;

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
