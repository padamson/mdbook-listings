//! Integration tests for the `mdbook-listings list` subcommand.
//! Each test runs the binary via `assert_cmd` against a tempdir book root
//! and asserts on stdout shape.

use std::fs;

use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

/// An empty manifest produces no output rows. Stays quiet rather than
/// printing a header or "no listings" banner — keeps the command pipe-
/// friendly and predictable across script consumers.
#[test]
fn list_prints_nothing_when_manifest_is_empty() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();

    mdbook_listings()
        .args(["list", "--book-root"])
        .arg(&book_root)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

/// One row per listing, in manifest insertion order, tab-separated:
/// `<tag>\t<frozen-relative-path>\t<source-relative-path>`. The order
/// matches `listings.toml`'s `[[listing]]` order so the most-recently-
/// added entries land at the bottom — chronological awareness without
/// requiring a separate timestamp field.
#[test]
fn list_prints_one_tab_separated_row_per_listing_in_insertion_order() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path().join("book");
    fs::create_dir_all(&book_root).unwrap();
    let source_a = tmp.path().join("a.yaml");
    let source_b = tmp.path().join("b.yaml");
    fs::write(&source_a, "a: 1\n").unwrap();
    fs::write(&source_b, "b: 2\n").unwrap();

    mdbook_listings()
        .args(["freeze", "--tag", "a-v1", "--book-root"])
        .arg(&book_root)
        .arg(&source_a)
        .assert()
        .success();
    mdbook_listings()
        .args(["freeze", "--tag", "b-v1", "--book-root"])
        .arg(&book_root)
        .arg(&source_b)
        .assert()
        .success();

    mdbook_listings()
        .args(["list", "--book-root"])
        .arg(&book_root)
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "a-v1\tsrc/listings/a-v1.yaml\t",
        ))
        .stdout(contains("\nb-v1\tsrc/listings/b-v1.yaml\t"));
}

/// The source column carries the same string the manifest recorded — the
/// relative path from the book root that `freeze` computed via its own
/// path normaliser. Important: forward slashes regardless of OS, matching
/// the rest of the book's directive shape.
#[test]
fn list_source_column_matches_manifest_normalised_path() {
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
        .success();

    mdbook_listings()
        .args(["list", "--book-root"])
        .arg(&book_root)
        .assert()
        .success()
        // The third column is the source path — content depends on the
        // tempdir layout, but the row shape is fixed: three tab-separated
        // columns ending in a newline, and the source must end in
        // `compose.yaml`.
        .stdout(predicate::str::is_match(r"^compose-v1\tsrc/listings/compose-v1\.yaml\t.+/compose\.yaml\n$").unwrap());
}
