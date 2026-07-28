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
fn version_reports_crate_version_with_optional_build_sha() {
    // Contract, not environment: `mdbook-listings <version>` exactly, plus an
    // optional ` (<7+ hex digits>)` build id. Bare is valid (crates.io
    // install, or HEAD sitting on the release tag); anything else must be a
    // well-formed short sha. Demanding the sha unconditionally would fail
    // tagged-release CI; forbidding it would fail every dev checkout.
    let output = mdbook_listings()
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 version output");
    let text = text.trim();
    let prefix = concat!("mdbook-listings ", env!("CARGO_PKG_VERSION"));
    assert!(
        text.starts_with(prefix),
        "version must start with `{prefix}`; got `{text}`"
    );
    let suffix = &text[prefix.len()..];
    if !suffix.is_empty() {
        let sha = suffix
            .strip_prefix(" (")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or_else(|| panic!("suffix must be ` (<sha>)`; got `{suffix}`"));
        assert!(
            sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "build id must be a short git sha; got `{sha}`"
        );
    }

    // In a git checkout whose HEAD is not the release tag, the build id must
    // actually be present — that is the whole point of the feature. Skipped
    // outside git (source tarball) and on a tagged release build.
    let in_git_checkout = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .is_ok_and(|o| o.status.success());
    let at_release_tag = std::process::Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|tag| tag.trim() == concat!("v", env!("CARGO_PKG_VERSION")));
    if in_git_checkout && !at_release_tag {
        assert!(
            !suffix.is_empty(),
            "a non-release build from a git checkout must report its commit; got bare `{text}`"
        );
    }
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
