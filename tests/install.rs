//! Integration tests for the Install the Preprocessor story (ch. 1).

use std::fs;
use std::path::{Path, PathBuf};

use predicates::str::contains;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

#[test]
fn install_registers_preprocessor_and_writes_css() {
    let book = MinimalFixtureBook::new();

    mdbook_listings()
        .args(["install", "--book-root"])
        .arg(book.root())
        .assert()
        .success();

    book.assert_preprocessor_registered();
    book.assert_css_asset_present();
}

/// The smallest mdbook that's still a valid book: a `book.toml` declaring
/// just the `[book]` table with a title, materialised in a TempDir whose
/// lifetime is tied to this struct so the filesystem clean-up is automatic.
struct MinimalFixtureBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalFixtureBook {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().to_path_buf();
        fs::write(root.join("book.toml"), "[book]\ntitle = \"Test\"\n").unwrap();
        Self { _tmp: tmp, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn assert_preprocessor_registered(&self) {
        let book_toml = fs::read_to_string(self.root.join("book.toml")).unwrap();
        assert!(
            book_toml.contains("[preprocessor.listings]"),
            "book.toml should register the preprocessor; got:\n{book_toml}",
        );
        assert!(
            book_toml.contains("mdbook-listings.css"),
            "book.toml should reference the CSS asset; got:\n{book_toml}",
        );
    }

    fn assert_css_asset_present(&self) {
        assert!(
            self.root.join("mdbook-listings.css").exists(),
            "CSS asset should be written to the book root",
        );
    }
}

/// `install` against a book that already has `[preprocessor.admonish]`
/// registers listings with `before = ["admonish"]` so the preprocessor
/// chain runs in the right order for PDF output (AC 6).
#[test]
fn install_orders_before_admonish_when_admonish_is_registered() {
    let tmp = TempDir::new().expect("tempdir");
    let book_root = tmp.path();
    fs::write(
        book_root.join("book.toml"),
        "[book]\ntitle = \"Test\"\n\n[preprocessor.admonish]\ncommand = \"mdbook-admonish\"\n",
    )
    .unwrap();

    mdbook_listings()
        .args(["install", "--book-root"])
        .arg(book_root)
        .assert()
        .success();

    let book_toml = fs::read_to_string(book_root.join("book.toml")).unwrap();
    assert!(
        book_toml.contains(r#"before = ["admonish", "links"]"#),
        "listings should be ordered before both admonish and links; got:\n{book_toml}",
    );
    assert!(
        book_toml.contains("[preprocessor.listings]"),
        "listings preprocessor should still be registered; got:\n{book_toml}",
    );
    assert!(
        book_toml.contains("[preprocessor.admonish]"),
        "admonish should still be registered (untouched); got:\n{book_toml}",
    );
}

/// `install` against a directory with no `book.toml` exits non-zero with a
/// diagnostic identifying what was expected (AC 5).
#[test]
fn install_rejects_missing_book_config() {
    let tmp = TempDir::new().expect("tempdir");

    mdbook_listings()
        .args(["install", "--book-root"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(contains("book.toml not found"));
}
