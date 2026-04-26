//! Integration tests for the Install the Preprocessor story (ch. 1).

use std::fs;
use std::path::{Path, PathBuf};

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
