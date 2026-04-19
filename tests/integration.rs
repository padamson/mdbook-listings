use assert_cmd::Command;
use predicates::str::contains;

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

#[test]
fn freeze_stub_exits_nonzero() {
    mdbook_listings()
        .args(["freeze", "--tag", "demo", "dummy.yaml"])
        .assert()
        .failure()
        .stderr(contains("not yet implemented"));
}
