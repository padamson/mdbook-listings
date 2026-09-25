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
fn version_prints_the_string_the_build_script_computed() {
    // build.rs decides once per build whether to suffix a sha; the binary
    // prints that decision verbatim. Its shape has its own test below.
    let text = version_output();
    assert_eq!(
        text,
        concat!("mdbook-listings ", env!("CRATE_VERSION_WITH_BUILD")),
    );
}

#[test]
fn version_is_the_crate_version_with_an_optional_short_sha() {
    // Contract, not environment: `mdbook-listings <version>` exactly, plus an
    // optional ` (<7+ hex digits>)` build id. Bare is valid (crates.io
    // install, or HEAD sitting on the release tag); anything else must be a
    // well-formed short sha.
    let text = version_output();
    let prefix = concat!("mdbook-listings ", env!("CARGO_PKG_VERSION"));
    let suffix = text
        .strip_prefix(prefix)
        .unwrap_or_else(|| panic!("version must start with `{prefix}`; got `{text}`"));
    if suffix.is_empty() {
        return;
    }
    let sha = suffix
        .strip_prefix(" (")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("suffix must be ` (<sha>)`; got `{suffix}`"));
    assert!(
        sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()),
        "build id must be a short git sha; got `{sha}`"
    );
}

#[test]
fn version_carries_a_sha_exactly_when_head_is_off_the_release_tag() {
    // git is the oracle, asked a different question than build.rs asks
    // (`tag --points-at`, not `describe --exact-match`), so a build script
    // that stops consulting git, or misreads the answer, fails here. Both
    // branches assert: outside git, or with HEAD on the release tag, the
    // bare version is the whole contract.
    let text = version_output();
    let bare = concat!("mdbook-listings ", env!("CARGO_PKG_VERSION"));
    let in_git = git(&["rev-parse", "--git-dir"]).is_some();
    let on_release_tag = git(&["tag", "--points-at", "HEAD"]).is_some_and(|tags| {
        tags.lines()
            .any(|t| t == concat!("v", env!("CARGO_PKG_VERSION")))
    });
    if in_git && !on_release_tag {
        assert!(
            text.len() > bare.len(),
            "a non-release build from a git checkout must report its commit; got `{text}`"
        );
    } else {
        assert_eq!(
            text, bare,
            "a release or non-git build reports the bare version and nothing else"
        );
    }
}

/// stdout of a git command that succeeded, `None` for no git or a failure.
fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    String::from_utf8(out.stdout).ok()
}

fn version_output() -> String {
    let output = mdbook_listings()
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(output)
        .expect("utf-8 version output")
        .trim()
        .to_string()
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
