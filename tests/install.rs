//! Integration tests for the Install the Preprocessor story (ch. 1).

use std::fs;
use std::path::{Path, PathBuf};

use mdbook_listings::install::{
    CSS_ASSET, CSS_ASSET_FILENAME, GITIGNORE_FILENAME, InstallOutcome, JS_ASSET, JS_ASSET_FILENAME,
    ensure_assets_fresh, ensure_gitignore, install,
};
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

/// `install` writes both asset paths into `.gitignore` (creating the file
/// if missing) so downstream books treat them as build artifacts (ch.6
/// slice 2 / AC 2).
#[test]
fn install_writes_gitignore_entries_for_both_assets() {
    let book = MinimalFixtureBook::new();

    mdbook_listings()
        .args(["install", "--book-root"])
        .arg(book.root())
        .assert()
        .success();

    let gitignore = fs::read_to_string(book.root().join(".gitignore")).expect(".gitignore");
    assert!(
        gitignore.lines().any(|l| l.trim() == CSS_ASSET_FILENAME),
        "`.gitignore` should list the CSS asset; got:\n{gitignore}",
    );
    assert!(
        gitignore.lines().any(|l| l.trim() == JS_ASSET_FILENAME),
        "`.gitignore` should list the JS asset; got:\n{gitignore}",
    );
}

/// `ensure_assets_fresh` writes the bundled bytes when the on-disk copies
/// are missing, returning `true` (something was written).
#[test]
fn ensure_assets_fresh_writes_when_missing() {
    let tmp = TempDir::new().expect("tempdir");

    let wrote = ensure_assets_fresh(tmp.path()).expect("ensure_assets_fresh");

    assert!(wrote, "should report a write when assets were missing");
    assert_eq!(
        fs::read(tmp.path().join(CSS_ASSET_FILENAME)).expect("css written"),
        CSS_ASSET,
    );
    assert_eq!(
        fs::read(tmp.path().join(JS_ASSET_FILENAME)).expect("js written"),
        JS_ASSET,
    );
}

/// `ensure_assets_fresh` is a no-op when both files already match the
/// bundled bytes — the preprocessor calls this on every build, so it must
/// not churn mtimes when nothing has changed.
#[test]
fn ensure_assets_fresh_is_noop_when_bytes_match() {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join(CSS_ASSET_FILENAME), CSS_ASSET).unwrap();
    fs::write(tmp.path().join(JS_ASSET_FILENAME), JS_ASSET).unwrap();

    let wrote = ensure_assets_fresh(tmp.path()).expect("ensure_assets_fresh");

    assert!(!wrote, "should report no-op when bytes already match");
}

/// `ensure_assets_fresh` overwrites stale on-disk bytes — this is what
/// keeps the rendered HTML in sync with the upgraded binary even when an
/// author skips re-running `install`.
#[test]
fn ensure_assets_fresh_overwrites_stale_bytes() {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join(CSS_ASSET_FILENAME), b"/* stale */").unwrap();
    fs::write(tmp.path().join(JS_ASSET_FILENAME), b"// stale\n").unwrap();

    let wrote = ensure_assets_fresh(tmp.path()).expect("ensure_assets_fresh");

    assert!(wrote, "should report a write when bytes were stale");
    assert_eq!(
        fs::read(tmp.path().join(CSS_ASSET_FILENAME)).expect("css refreshed"),
        CSS_ASSET,
    );
    assert_eq!(
        fs::read(tmp.path().join(JS_ASSET_FILENAME)).expect("js refreshed"),
        JS_ASSET,
    );
}

/// `ensure_gitignore` creates `.gitignore` with both entries when no file
/// exists.
#[test]
fn ensure_gitignore_creates_file_when_missing() {
    let tmp = TempDir::new().expect("tempdir");

    let wrote = ensure_gitignore(tmp.path()).expect("ensure_gitignore");

    assert!(wrote, "should report a write when .gitignore was missing");
    let gitignore = fs::read_to_string(tmp.path().join(".gitignore")).expect(".gitignore");
    assert!(gitignore.lines().any(|l| l.trim() == CSS_ASSET_FILENAME));
    assert!(gitignore.lines().any(|l| l.trim() == JS_ASSET_FILENAME));
}

/// `ensure_gitignore` appends only the missing entry, leaving any existing
/// author entries (and the entry that's already there) untouched.
#[test]
fn ensure_gitignore_appends_only_missing_entries() {
    let tmp = TempDir::new().expect("tempdir");
    let existing = "build/\nmdbook-listings.css\n";
    fs::write(tmp.path().join(".gitignore"), existing).unwrap();

    let wrote = ensure_gitignore(tmp.path()).expect("ensure_gitignore");

    assert!(
        wrote,
        "JS entry was missing, so .gitignore should be written"
    );
    let gitignore = fs::read_to_string(tmp.path().join(".gitignore")).expect(".gitignore");
    assert!(
        gitignore.contains("build/\n"),
        "existing author entries must survive; got:\n{gitignore}",
    );
    assert_eq!(
        gitignore
            .lines()
            .filter(|l| l.trim() == CSS_ASSET_FILENAME)
            .count(),
        1,
        "CSS entry must not be duplicated; got:\n{gitignore}",
    );
    assert!(
        gitignore.lines().any(|l| l.trim() == JS_ASSET_FILENAME),
        "JS entry must be appended; got:\n{gitignore}",
    );
}

/// `ensure_gitignore` is a no-op when both entries are already present —
/// matters because re-running `install` on a configured book must not
/// churn the file (AC 6 idempotency).
#[test]
fn ensure_gitignore_is_noop_when_complete() {
    let tmp = TempDir::new().expect("tempdir");
    let existing = format!("target/\n{CSS_ASSET_FILENAME}\n{JS_ASSET_FILENAME}\n");
    fs::write(tmp.path().join(".gitignore"), &existing).unwrap();

    let wrote = ensure_gitignore(tmp.path()).expect("ensure_gitignore");

    assert!(
        !wrote,
        "should report no-op when both entries already present"
    );
    let gitignore = fs::read_to_string(tmp.path().join(".gitignore")).expect(".gitignore");
    assert_eq!(gitignore, existing, ".gitignore must be byte-identical");
}

// ---------------------------------------------------------------------
// Targeted regression tests that close out MUTATION_DEBT.md entries
// from `scripts/mutants.sh 6e07b6a~1`. Each one pins a boolean path
// the prior tests left ambiguous, so the corresponding mutation in
// src/install.rs is now CAUGHT.
// ---------------------------------------------------------------------

/// `ensure_assets_fresh` returns `true` when only ONE asset was stale.
/// Without this test, the return expression `!css_already_correct ||
/// !js_already_correct` could be mutated to `&&` and survive — the
/// existing tests only exercise both-stale or both-correct.
/// Closes MUTATION_DEBT.md src/install.rs L57:29.
#[test]
fn ensure_assets_fresh_reports_write_when_only_one_asset_is_stale() {
    let tmp = TempDir::new().expect("tempdir");
    // CSS is correct (matches bundled bytes), JS is stale.
    fs::write(tmp.path().join(CSS_ASSET_FILENAME), CSS_ASSET).unwrap();
    fs::write(tmp.path().join(JS_ASSET_FILENAME), b"// stale\n").unwrap();

    let wrote = ensure_assets_fresh(tmp.path()).expect("ensure_assets_fresh");

    assert!(
        wrote,
        "should report a write when only one of the two assets was stale"
    );
}

/// `ensure_gitignore` inserts a separator newline when the existing
/// content lacks a trailing one. Without this test, the
/// `!new_contents.ends_with('\n')` check could be mutated (delete `!`
/// or swap `&&` for `||`) and the entries would be jammed onto the
/// previous line. Closes MUTATION_DEBT.md src/install.rs L77:8 and
/// L77:36 (both `delete !` mutations on the same line).
#[test]
fn ensure_gitignore_inserts_separator_when_existing_file_lacks_trailing_newline() {
    let tmp = TempDir::new().expect("tempdir");
    // No trailing newline on the existing entry.
    fs::write(tmp.path().join(GITIGNORE_FILENAME), "target/").unwrap();

    ensure_gitignore(tmp.path()).expect("ensure_gitignore");

    let gitignore = fs::read_to_string(tmp.path().join(GITIGNORE_FILENAME)).expect(".gitignore");
    let expected = format!("target/\n{CSS_ASSET_FILENAME}\n{JS_ASSET_FILENAME}\n");
    assert_eq!(
        gitignore, expected,
        "existing line without trailing newline must get a separator before the new entries"
    );
}

/// `ensure_gitignore` does NOT insert a second newline when the
/// existing content already ends with one. Without this test, the
/// `&&` in the separator-insert guard could be mutated to `||` and
/// produce a stray blank line. Closes MUTATION_DEBT.md src/install.rs
/// L77:33 (`replace && with ||`).
#[test]
fn ensure_gitignore_does_not_double_newline_when_existing_file_ends_with_newline() {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join(GITIGNORE_FILENAME), "target/\n").unwrap();

    ensure_gitignore(tmp.path()).expect("ensure_gitignore");

    let gitignore = fs::read_to_string(tmp.path().join(GITIGNORE_FILENAME)).expect(".gitignore");
    let expected = format!("target/\n{CSS_ASSET_FILENAME}\n{JS_ASSET_FILENAME}\n");
    assert_eq!(
        gitignore, expected,
        "trailing newline on existing content must NOT trigger a duplicate; got:\n{gitignore:?}"
    );
}

/// `install` reports `Installed` when only `book.toml` needed
/// rewriting (assets already match bundled bytes, `.gitignore`
/// already complete). Catches the `||` → `&&` mutation on the first
/// operand in the install-outcome decision. Closes
/// MUTATION_DEBT.md src/install.rs L119:24.
#[test]
fn install_reports_installed_when_only_book_toml_needs_change() {
    let book = MinimalFixtureBook::new();
    // Pre-seed assets at the bundled bytes and a complete .gitignore
    // so ensure_assets_fresh + ensure_gitignore both return false.
    fs::write(book.root().join(CSS_ASSET_FILENAME), CSS_ASSET).unwrap();
    fs::write(book.root().join(JS_ASSET_FILENAME), JS_ASSET).unwrap();
    fs::write(
        book.root().join(GITIGNORE_FILENAME),
        format!("{CSS_ASSET_FILENAME}\n{JS_ASSET_FILENAME}\n"),
    )
    .unwrap();

    let outcome = install(book.root()).expect("install");

    assert_eq!(
        outcome,
        InstallOutcome::Installed,
        "book.toml-only change should still report Installed"
    );
}

/// `install` reports `Installed` when only the asset bytes needed
/// refreshing (book.toml + `.gitignore` already correct). Catches the
/// `||` → `&&` mutation on the second-operand pair in the
/// install-outcome decision. Closes MUTATION_DEBT.md src/install.rs
/// L119:42.
#[test]
fn install_reports_installed_when_only_assets_need_change() {
    let book = MinimalFixtureBook::new();
    // First, run a full install so book.toml + .gitignore are
    // configured and the assets land at the bundled bytes.
    install(book.root()).expect("seed install");
    // Now corrupt the on-disk assets so ensure_assets_fresh will
    // overwrite them, but leave book.toml + .gitignore alone.
    fs::write(book.root().join(CSS_ASSET_FILENAME), b"/* stale */").unwrap();
    fs::write(book.root().join(JS_ASSET_FILENAME), b"// stale\n").unwrap();

    let outcome = install(book.root()).expect("second install");

    assert_eq!(
        outcome,
        InstallOutcome::Installed,
        "asset-only refresh should report Installed"
    );
}

/// `install` distinguishes a *missing* `book.toml` (NotFound — its own
/// friendly bail) from any other IO error (must surface the underlying
/// error so the author isn't told to re-init when the real problem is
/// e.g. unreadable bytes). Without this test, the `match` guard
/// `e.kind() == ErrorKind::NotFound` could be mutated to `true` and
/// every IO error would silently route to the NotFound bail. Closes
/// MUTATION_DEBT.md src/install.rs L94:19.
#[test]
fn install_routes_non_notfound_io_errors_to_the_generic_arm() {
    let book = MinimalFixtureBook::new();
    // Overwrite the seeded book.toml with invalid UTF-8 — fs::read_to_string
    // then returns io::ErrorKind::InvalidData, provably not NotFound.
    fs::write(book.root().join("book.toml"), [0xff, 0xfe, 0xfd]).unwrap();

    let err = install(book.root()).expect_err("install should error on bad UTF-8");
    let msg = format!("{err:#}");

    assert!(
        msg.contains("reading book config"),
        "expected the non-NotFound IO arm's context (\"reading book config at ...\"); got: {msg}",
    );
    assert!(
        !msg.contains("not found"),
        "a non-NotFound IO error must not be misreported as a missing file; got: {msg}",
    );
}
