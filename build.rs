//! Emits `CRATE_VERSION_WITH_BUILD` for the CLI's `--version`: the crate
//! version, suffixed with the short git sha when this is not a release
//! build. A binary built from `main` and one from the last release
//! otherwise print the same thing, which makes "rebuild from main to pick
//! up the fix" unverifiable and lets a stale binary impersonate a current
//! one.

fn main() {
    emit_build_version();
}

/// Three cases, deliberately:
/// - no git (crates.io install, source tarball): bare crate version — a
///   released artifact must not carry a hash that can't be reproduced from
///   what was published;
/// - HEAD exactly at the release tag `v<version>`: bare crate version —
///   that IS the release, however it was built;
/// - anything else: `<version> (<short sha>)`.
///
/// No dirty-tree marker: nothing re-runs this script on an uncommitted
/// edit, so the marker would be wrong as often as right.
fn emit_build_version() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let build_id = match git_short_sha() {
        Some(sha) if !at_release_tag(&version) => format!("{version} ({sha})"),
        _ => version,
    };
    println!("cargo:rustc-env=CRATE_VERSION_WITH_BUILD={build_id}");

    // Freshness: `.git/HEAD` covers checkouts and branch switches; the
    // branch's ref file covers commits. Absent when refs are packed, which
    // just means the sha can lag until the next checkout.
    let git_dir = std::path::Path::new(".git");
    if !git_dir.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.strip_prefix("ref: ")
        && git_dir.join(reference.trim()).exists()
    {
        println!("cargo:rerun-if-changed=.git/{}", reference.trim());
    }
}

fn git_short_sha() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

fn at_release_tag(version: &str) -> bool {
    std::process::Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|tag| tag.trim() == format!("v{version}"))
}
