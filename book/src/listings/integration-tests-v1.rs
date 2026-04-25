//! CLI-level integration tests: exercise the compiled `mdbook-listings`
//! binary as an external caller would, asserting subcommand outputs, exit
//! codes, and on-disk side effects.

use std::fs;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn mdbook_listings() -> Command {
    Command::cargo_bin("mdbook-listings").expect("binary should be built by cargo-test")
}

#[test]
fn help_lists_all_subcommands() {
    mdbook_listings()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("supports"))
        .stdout(contains("install"))
        .stdout(contains("freeze"))
        .stdout(contains("verify"));
}

#[test]
fn version_matches_cargo_pkg_version() {
    mdbook_listings()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn supports_html_exits_zero() {
    mdbook_listings()
        .args(["supports", "html"])
        .assert()
        .success();
}

#[test]
fn supports_typst_pdf_exits_zero() {
    mdbook_listings()
        .args(["supports", "typst-pdf"])
        .assert()
        .success();
}

#[test]
fn supports_unknown_renderer_exits_one() {
    mdbook_listings()
        .args(["supports", "epub"])
        .assert()
        .failure()
        .code(1);
}

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
