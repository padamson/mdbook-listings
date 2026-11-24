//! Integration tests for `mdbook-listings remove`: retiring a frozen
//! listing the book no longer uses, without hand-editing `listings.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

/// A temp book with two listings frozen by the real `freeze`, so the
/// manifest is exactly what production writes.
struct Book {
    _tmp: TempDir,
    root: PathBuf,
}

impl Book {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("book");
        fs::create_dir_all(&root).unwrap();
        let book = Self { _tmp: tmp, root };
        book.freeze("catalog-v1", "items: []\n");
        book.freeze("menu-v1", "dishes: []\n");
        book
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn freeze(&self, tag: &str, body: &str) {
        let source = self.root.parent().unwrap().join(format!("{tag}.yaml"));
        fs::write(&source, body).unwrap();
        mdbook_listings()
            .args(["freeze", "--tag", tag, "--book-root"])
            .arg(&self.root)
            .arg(&source)
            .assert()
            .success();
    }

    fn listing(&self, file: &str) -> PathBuf {
        self.root.join("src/listings").join(file)
    }

    fn write_listing(&self, file: &str, content: &str) {
        fs::write(self.listing(file), content).unwrap();
    }

    fn write_chapter(&self, name: &str, content: &str) {
        fs::write(self.root.join("src").join(name), content).unwrap();
    }

    fn manifest(&self) -> String {
        fs::read_to_string(self.root.join("listings.toml")).unwrap()
    }

    fn remove(&self, tag: &str) -> assert_cmd::assert::Assert {
        mdbook_listings()
            .args(["remove", tag, "--book-root"])
            .arg(&self.root)
            .assert()
    }

    fn assert_gone(&self, file: &str) {
        let path = self.listing(file);
        assert!(!path.exists(), "{} still exists", path.display());
    }

    fn assert_present(&self, file: &str) {
        let path = self.listing(file);
        assert!(path.exists(), "{} was deleted", path.display());
    }

    fn assert_unrecorded(&self, tag: &str) {
        let manifest = self.manifest();
        assert!(
            !manifest.contains(&format!("tag = \"{tag}\"")),
            "`{tag}` is still recorded:\n{manifest}"
        );
    }

    /// `remove` fails naming each of `expected`, and changes nothing.
    fn assert_refused(&self, tag: &str, expected: &[&str]) {
        let before = self.manifest();
        let listings = self.listing_files();
        let mut assert = self.remove(tag).failure();
        for text in expected {
            assert = assert.stderr(contains(*text));
        }
        assert_eq!(self.manifest(), before, "listings.toml changed");
        assert_eq!(self.listing_files(), listings, "src/listings/ changed");
    }

    fn listing_files(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root.join("src/listings"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn verify(&self) -> assert_cmd::assert::Assert {
        mdbook_listings()
            .args(["verify", "--book-root"])
            .arg(&self.root)
            .assert()
    }
}

#[test]
fn remove_drops_the_entry_and_the_frozen_file() {
    let book = Book::new();

    book.remove("catalog-v1")
        .success()
        .stdout(contains("removed: catalog-v1"))
        .stdout(contains("deleted: src/listings/catalog-v1.yaml"));

    book.assert_gone("catalog-v1.yaml");
    book.assert_unrecorded("catalog-v1");
    // The other listing is untouched, and is all `list` shows.
    book.assert_present("menu-v1.yaml");
    mdbook_listings()
        .args(["list", "--book-root"])
        .arg(book.root())
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^menu-v1\t[^\n]*\n$").unwrap());
}

#[test]
fn a_book_verifies_after_a_remove() {
    let book = Book::new();
    book.write_chapter("ch01.md", "{{#include listings/menu-v1.yaml}}\n");
    book.write_listing("catalog-v1.callouts.toml", "");

    book.remove("catalog-v1").success();

    book.verify().success().stderr(predicate::str::is_empty());
}

#[test]
fn remove_deletes_the_listings_own_sidecar() {
    let book = Book::new();
    book.write_listing("catalog-v1.callouts.toml", "");

    book.remove("catalog-v1")
        .success()
        .stdout(contains("deleted: src/listings/catalog-v1.callouts.toml"));

    book.assert_gone("catalog-v1.callouts.toml");
}

// No chapter renders that diff any more, or the remove would have refused,
// so its annotations attach to nothing.
#[test]
fn remove_deletes_a_diff_sidecar_that_names_the_listing() {
    let book = Book::new();
    book.write_listing("catalog-v1..menu-v1.callouts.toml", "");

    book.remove("catalog-v1").success().stdout(contains(
        "deleted: src/listings/catalog-v1..menu-v1.callouts.toml",
    ));

    book.assert_gone("catalog-v1..menu-v1.callouts.toml");
}

#[test]
fn remove_deletes_a_diff_sidecar_with_the_listing_on_its_new_side() {
    let book = Book::new();
    book.write_listing("catalog-v1..menu-v1.callouts.toml", "");

    book.remove("menu-v1").success();

    book.assert_gone("catalog-v1..menu-v1.callouts.toml");
}

#[test]
fn remove_keeps_sidecars_that_name_other_listings() {
    let book = Book::new();
    book.freeze("menu-v2", "dishes: [soup]\n");
    book.write_listing("menu-v1.callouts.toml", "");
    book.write_listing("menu-v1..menu-v2.callouts.toml", "");

    book.remove("catalog-v1").success();

    book.assert_present("menu-v1.callouts.toml");
    book.assert_present("menu-v1..menu-v2.callouts.toml");
}

// The record goes before the frozen file, so a frozen file that cannot be
// deleted is left as an orphan, which verify only warns about.
#[test]
fn remove_reports_a_frozen_file_it_cannot_delete_and_drops_the_record() {
    let book = Book::new();
    let frozen = book.listing("catalog-v1.yaml");
    fs::remove_file(&frozen).unwrap();
    fs::create_dir(&frozen).unwrap();

    book.remove("catalog-v1")
        .failure()
        .stderr(contains("deleting"))
        .stderr(contains("catalog-v1.yaml"));

    book.assert_unrecorded("catalog-v1");
    book.verify().success();
}

// Sidecars go first, so one that cannot be deleted leaves the record in
// place and the remove can be run again once it is fixed.
#[test]
fn remove_keeps_the_record_when_a_sidecar_cannot_be_deleted() {
    let book = Book::new();
    fs::create_dir(book.listing("catalog-v1.callouts.toml")).unwrap();
    let before = book.manifest();

    book.remove("catalog-v1")
        .failure()
        .stderr(contains("deleting"))
        .stderr(contains("catalog-v1.callouts.toml"));

    assert_eq!(book.manifest(), before);
    book.assert_present("catalog-v1.yaml");
}

#[test]
fn remove_refuses_a_listing_a_chapter_includes() {
    let book = Book::new();
    book.write_chapter(
        "ch01.md",
        "# One\n\nText.\n\n{{#include listings/catalog-v1.yaml}}\n",
    );

    book.assert_refused(
        "catalog-v1",
        &["`catalog-v1` is still used", "src/ch01.md:5"],
    );
}

#[test]
fn remove_refuses_a_listing_a_diff_compares() {
    let book = Book::new();
    book.write_chapter("ch02.md", "{{#diff catalog-v1 menu-v1}}\n");

    book.assert_refused("catalog-v1", &["src/ch02.md:1"]);
}

#[test]
fn remove_refuses_a_listing_whose_sidecar_a_chapter_includes() {
    let book = Book::new();
    book.write_listing("catalog-v1.callouts.toml", "");
    book.write_chapter(
        "ch03.md",
        "```toml\n{{#include listings/catalog-v1.callouts.toml}}\n```\n",
    );

    book.assert_refused("catalog-v1", &["src/ch03.md:2"]);
    book.assert_present("catalog-v1.callouts.toml");
}

#[test]
fn remove_refuses_the_new_side_of_an_included_diff_sidecar() {
    let book = Book::new();
    book.write_listing("catalog-v1..menu-v1.callouts.toml", "");
    book.write_chapter(
        "ch03.md",
        "```toml\n{{#include listings/catalog-v1..menu-v1.callouts.toml}}\n```\n",
    );

    book.assert_refused("menu-v1", &["src/ch03.md:2"]);
    book.assert_present("catalog-v1..menu-v1.callouts.toml");
}

// mdBook resolves an include from the chapter's own directory, so a
// chapter in a subdirectory can reach the frozen file by another path.
#[test]
fn remove_refuses_a_listing_reached_from_a_subdirectory() {
    let book = Book::new();
    fs::create_dir_all(book.root().join("src/part1")).unwrap();
    book.write_chapter(
        "part1/ch01.md",
        "```yaml\n{{#include ../listings/catalog-v1.yaml}}\n```\n",
    );

    book.assert_refused("catalog-v1", &["src/part1/ch01.md:2"]);
}

#[test]
fn remove_refuses_a_listing_a_rustdoc_include_reads() {
    let book = Book::new();
    book.write_chapter(
        "ch04.md",
        "```rust\n{{#rustdoc_include listings/catalog-v1.yaml:2}}\n```\n",
    );

    book.assert_refused("catalog-v1", &["src/ch04.md:2"]);
}

// mdBook leaves a backslash-escaped directive as text, so it is no use.
#[test]
fn remove_passes_over_an_escaped_include() {
    let book = Book::new();
    book.write_chapter(
        "ch05.md",
        "Write `\\{{#include listings/catalog-v1.yaml}}` to show it.\n",
    );

    book.remove("catalog-v1").success();
}

#[test]
fn remove_names_every_use_it_found_once() {
    let book = Book::new();
    book.write_chapter(
        "ch01.md",
        "{{#include listings/catalog-v1.yaml caption=\"The catalog\"}}\n",
    );
    book.write_chapter("ch02.md", "{{#diff catalog-v1 catalog-v1}}\n");

    let out = book
        .remove("catalog-v1")
        .failure()
        .get_output()
        .stderr
        .clone();
    let err = String::from_utf8(out).unwrap();
    assert_eq!(err.matches("src/ch01.md:1").count(), 1, "stderr:\n{err}");
    assert_eq!(err.matches("src/ch02.md:1").count(), 1, "stderr:\n{err}");
}

#[test]
fn remove_refuses_a_record_whose_frozen_file_is_outside_the_listings_dir() {
    let book = Book::new();
    let outside = book.root().join("keep.txt");
    fs::write(&outside, "not a listing\n").unwrap();
    let manifest = book
        .manifest()
        .replace("src/listings/catalog-v1.yaml", "src/listings/../keep.txt");
    fs::write(book.root().join("listings.toml"), &manifest).unwrap();

    book.remove("catalog-v1")
        .failure()
        .stderr(contains("outside src/listings/"));

    assert!(outside.exists(), "{} was deleted", outside.display());
    assert_eq!(book.manifest(), manifest);
}

#[test]
fn remove_fails_on_a_tag_the_manifest_does_not_hold() {
    let book = Book::new();
    let before = book.manifest();

    book.remove("catalog-v9")
        .failure()
        .stderr(contains("no frozen listing is tagged `catalog-v9`"));

    assert_eq!(book.manifest(), before);
}

// A frozen file deleted by hand is the half-finished retirement `verify`
// reports; `remove` finishes it rather than refusing.
#[test]
fn remove_drops_an_entry_whose_frozen_file_is_already_gone() {
    let book = Book::new();
    fs::remove_file(book.listing("catalog-v1.yaml")).unwrap();

    book.remove("catalog-v1")
        .success()
        .stdout(contains("removed: catalog-v1"))
        .stdout(contains("deleted:").not());

    book.assert_unrecorded("catalog-v1");
}

// The listings directory deleted by hand takes the frozen file with it:
// the same half-finished retirement, finished the same way.
#[test]
fn remove_drops_an_entry_whose_listings_dir_is_gone() {
    let book = Book::new();
    fs::remove_dir_all(book.root().join("src/listings")).unwrap();

    book.remove("catalog-v1").success();

    book.assert_unrecorded("catalog-v1");
}

// A listings directory that cannot be read hides the sidecars `remove`
// has to delete, so it stops before changing anything.
#[test]
fn remove_refuses_when_the_listings_dir_cannot_be_read() {
    let book = Book::new();
    let dir = book.root().join("src/listings");
    fs::remove_dir_all(&dir).unwrap();
    fs::write(&dir, "not a directory\n").unwrap();
    let before = book.manifest();

    book.remove("catalog-v1")
        .failure()
        .stderr(contains("reading"));

    assert_eq!(book.manifest(), before);
}

// A `{{#callout}}` resolves only against listings its own chapter shows,
// so a chapter citing one of this listing's callouts also shows the
// listing, and the refusal covers the citation too.
#[test]
fn remove_refuses_a_listing_whose_callout_a_chapter_cites() {
    let book = Book::new();
    book.freeze("hello-v1", "a: 1\n# CALLOUT: greeting Says hello.\nb: 2\n");
    book.write_chapter(
        "ch06.md",
        "{{#include listings/hello-v1.yaml}}\n\nSee {{#callout greeting}}.\n",
    );

    book.assert_refused("hello-v1", &["src/ch06.md:1"]);
}
