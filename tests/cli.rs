//! CLI scaffolding tests: help text, version reporting, and the
//! `supports <renderer>` handshake mdbook performs when discovering
//! preprocessors. These are not tied to any user story; they were shipped
//! by the CLI-scaffolding chore.

use predicates::str::contains;
use std::path::Path;

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
        .stdout(contains("verify"))
        // Anchored to the commands column: `list` alone would match
        // `mdbook-listings` in the usage line.
        .stdout(predicates::str::is_match(r"(?m)^\s+remove\s").unwrap())
        .stdout(predicates::str::is_match(r"(?m)^\s+list\s").unwrap());
}

#[test]
fn version_prints_the_string_the_build_script_computed() {
    // build.rs decides once per build whether to suffix a sha; the binary
    // prints that decision verbatim. The decision itself is unit-tested in
    // tests/build_id.rs.
    let text = version_output();
    assert_eq!(
        text,
        concat!("mdbook-listings ", env!("CRATE_VERSION_WITH_BUILD")),
    );
}

#[test]
fn version_carries_a_sha_exactly_when_head_is_off_the_release_tag() {
    // git is the oracle, asked here and not through build.rs, so a build
    // script that stops consulting git, or misreads the answer, fails. Both
    // branches assert: outside this crate's own checkout, or with HEAD on
    // the release tag, the bare version is the whole contract.
    let text = version_output();
    let bare = concat!("mdbook-listings ", env!("CARGO_PKG_VERSION"));
    // The crate's own checkout, not a repository around it (a vendored
    // copy, a home directory under git), whose commit is not this build's.
    let own_checkout = git(&["rev-parse", "--show-toplevel"]).is_some_and(|top| {
        Path::new(top.trim()).canonicalize().ok()
            == Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize().ok()
    });
    // `--no-column`: with `column.ui=always`, git puts several tags on a
    // line even into a pipe.
    let on_release_tag = git(&["tag", "--no-column", "--points-at", "HEAD"]).is_some_and(|tags| {
        tags.lines()
            .any(|t| t == concat!("v", env!("CARGO_PKG_VERSION")))
    });
    // `None` where git cannot read HEAD (a `git init` with no commit yet),
    // which build.rs answers with the bare version too.
    let head = own_checkout.then(|| git(&["rev-parse", "HEAD"])).flatten();
    match head.filter(|_| !on_release_tag) {
        Some(head) => {
            let sha = text
                .strip_prefix(bare)
                .and_then(|suffix| suffix.strip_prefix(" ("))
                .and_then(|suffix| suffix.strip_suffix(')'))
                .unwrap_or_else(|| {
                    panic!("a non-release build from a git checkout must report its commit; got `{text}`")
                });
            // By prefix against the full sha: the length git abbreviates
            // to grows with the object count, so the short sha asked for
            // now can be longer than the one baked in at build time. Never
            // the full sha, which is not what `--version` prints.
            let head = head.trim();
            assert!(
                (7..head.len()).contains(&sha.len()) && head.starts_with(sha),
                "`{sha}` must abbreviate HEAD, {head}"
            );
        }
        None => assert_eq!(
            text, bare,
            "a release or non-git build reports the bare version and nothing else"
        ),
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
