//! Integration tests for `mdbook-listings verify`: the CI gate that a
//! book's frozen snapshots are intact — present on disk and still
//! matching the sha256 recorded at freeze time.

use std::fs;
use std::path::{Path, PathBuf};

use predicates::boolean::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

/// A temp book with one source file frozen via the real `freeze`
/// subcommand, so the manifest entry and sha256 are exactly what
/// production wrote.
struct FrozenFixtureBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl FrozenFixtureBook {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("book");
        fs::create_dir_all(&root).unwrap();
        let source = tmp.path().join("compose.yaml");
        fs::write(&source, "services:\n  web:\n    image: nginx\n").unwrap();
        mdbook_listings()
            .args(["freeze", "--tag", "compose-v1", "--book-root"])
            .arg(&root)
            .arg(&source)
            .assert()
            .success();
        Self { _tmp: tmp, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn frozen_path(&self) -> PathBuf {
        self.root.join("src/listings/compose-v1.yaml")
    }

    /// Freeze another source so a chapter can reference it as a real tag.
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

    /// Write a chapter markdown file under `src/`.
    fn write_chapter(&self, slug: &str, content: &str) {
        fs::write(self.root.join(format!("src/{slug}.md")), content).unwrap();
    }

    /// Write a raw file into `src/listings/` (e.g. an orphan or a sidecar).
    fn write_listing_file(&self, name: &str, content: &str) {
        fs::write(self.root.join("src/listings").join(name), content).unwrap();
    }
}

#[test]
fn verify_succeeds_when_all_frozen_listings_are_intact() {
    let book = FrozenFixtureBook::new();

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success()
        // Exact singular phrasing: "1 frozen listings checked" would be
        // the plural-logic bug.
        .stdout(contains("1 frozen listing checked"));
}

#[test]
fn verify_fails_when_a_frozen_file_was_edited_after_freezing() {
    let book = FrozenFixtureBook::new();
    // Simulate the classic mistake: "fixing" the snapshot in place
    // instead of refreezing, which silently breaks the book's claim
    // to show real code.
    fs::write(
        book.frozen_path(),
        "services:\n  web:\n    image: nginx:edited\n",
    )
    .unwrap();

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .failure()
        .stderr(contains("compose-v1"))
        .stderr(contains("src/listings/compose-v1.yaml"))
        .stderr(contains("sha256"));
}

#[test]
fn verify_fails_when_a_frozen_file_is_missing() {
    let book = FrozenFixtureBook::new();
    fs::remove_file(book.frozen_path()).unwrap();

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .failure()
        .stderr(contains("compose-v1"))
        .stderr(contains("src/listings/compose-v1.yaml"))
        .stderr(contains("missing"));
}

#[test]
fn verify_succeeds_on_a_book_with_no_manifest() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("book");
    fs::create_dir_all(&root).unwrap();

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(&root)
        .assert()
        .success();
}

#[test]
fn verify_fails_on_a_diff_operand_that_names_no_listing() {
    let book = FrozenFixtureBook::new();
    book.write_chapter("ch", "Diffing.\n\n{{#diff compose-v1 ghost-v1}}\n");

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .failure()
        .stderr(contains("ghost-v1"))
        .stderr(contains("ch.md"));
}

#[test]
fn verify_fails_on_an_include_that_names_no_listing() {
    let book = FrozenFixtureBook::new();
    book.write_chapter(
        "ch",
        "Including.\n\n```yaml\n{{#include listings/ghost.yaml}}\n```\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .failure()
        .stderr(contains("ghost"))
        .stderr(contains("ch.md"));
}

#[test]
fn verify_succeeds_when_every_reference_resolves() {
    let book = FrozenFixtureBook::new();
    book.freeze("other-v1", "other: true\n");
    book.write_chapter(
        "ch",
        "Real refs.\n\n```yaml\n{{#include listings/compose-v1.yaml}}\n```\n\n\
         {{#diff compose-v1 other-v1}}\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success();
}

#[test]
fn verify_accepts_label_and_caption_arguments_on_references() {
    // `label=` (the {{#listing-ref}} anchor name) and `caption=` are directive
    // arguments, not part of the path/operands — verify must lift them the
    // same way the render passes do or a valid reference reads as broken.
    let book = FrozenFixtureBook::new();
    book.freeze("other-v1", "other: true\n");
    book.write_chapter(
        "ch",
        // The first label contains a dot: naive path parsing that leaves the
        // label token attached misreads it as a file extension.
        "Real refs.\n\n```yaml\n{{#include listings/compose-v1.yaml label=\"compose.v1\" caption=\"The compose file\"}}\n```\n\n\
         {{#diff compose-v1 other-v1 label=\"compose-diff\" context=5}}\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success();
}

#[test]
fn verify_fails_on_a_sidecar_with_no_matching_listing() {
    let book = FrozenFixtureBook::new();
    book.write_listing_file(
        "ghost.callouts.toml",
        "[[callout]]\nline = 1\nlabel = \"x\"\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .failure()
        .stderr(contains("ghost"))
        .stderr(contains("callouts.toml"));
}

#[test]
fn verify_warns_on_an_orphan_frozen_file_but_succeeds() {
    let book = FrozenFixtureBook::new();
    book.write_listing_file("orphan.yaml", "stray: true\n");

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success()
        .stderr(contains("orphan"));
}

#[test]
fn verify_warns_on_a_rendered_callout_marker_no_directive_references_but_succeeds() {
    let book = FrozenFixtureBook::new();
    book.freeze(
        "marked-v1",
        "key: value\n# CALLOUT: orphan-note Never picked up.\n",
    );
    book.write_chapter(
        "ch",
        "Shown here.\n\n```yaml\n{{#include listings/marked-v1.yaml}}\n```\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success()
        .stderr(contains("warning:"))
        .stderr(contains("orphan-note"))
        .stderr(contains("src/listings/marked-v1.yaml:2"));
}

#[test]
fn verify_stays_silent_on_a_marker_in_a_listing_no_chapter_shows() {
    let book = FrozenFixtureBook::new();
    book.freeze(
        "marked-v1",
        "key: value\n# CALLOUT: orphan-note Never picked up.\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success()
        .stderr(contains("orphan-note").not());
}

#[test]
fn verify_warns_when_a_slice_ends_on_a_callout_marker_but_succeeds() {
    let book = FrozenFixtureBook::new();
    book.freeze(
        "sliced-v1",
        "key: value\n# CALLOUT: cut-off Annotates the line below.\nnext: line\n",
    );
    book.write_chapter(
        "ch",
        "Sliced.\n\n```yaml\n{{#include listings/sliced-v1.yaml:1:2}}\n```\n\n\
         Picked up as {{#callout cut-off}} here.\n",
    );

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(book.root())
        .assert()
        .success()
        .stderr(contains("warning:"))
        .stderr(contains("cut-off"))
        .stderr(contains("ch.md:4"));
}

#[test]
fn verify_reports_every_broken_listing_not_just_the_first() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().join("book");
    fs::create_dir_all(&root).unwrap();
    for (tag, body) in [("a-v1", "a: 1\n"), ("b-v1", "b: 2\n")] {
        let source = tmp.path().join(format!("{tag}.yaml"));
        fs::write(&source, body).unwrap();
        mdbook_listings()
            .args(["freeze", "--tag", tag, "--book-root"])
            .arg(&root)
            .arg(&source)
            .assert()
            .success();
    }
    fs::remove_file(root.join("src/listings/a-v1.yaml")).unwrap();
    fs::write(root.join("src/listings/b-v1.yaml"), "tampered\n").unwrap();

    mdbook_listings()
        .args(["verify", "--book-root"])
        .arg(&root)
        .assert()
        .failure()
        .stderr(contains("a-v1"))
        .stderr(contains("b-v1"))
        .stdout(contains("2 frozen listings checked"));
}
