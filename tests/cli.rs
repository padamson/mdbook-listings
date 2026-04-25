//! CLI scaffolding tests: help text, version reporting, and the
//! `supports <renderer>` handshake mdbook performs when discovering
//! preprocessors. These are not tied to any user story; they were shipped
//! by the CLI-scaffolding chore.

use predicates::str::contains;

mod common;
use common::mdbook_listings;

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
