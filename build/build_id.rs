//! Shared by `build.rs` and `tests/build_id.rs`, so the decision is tested
//! without running git.

/// The version `--version` prints: `version`, with the commit `sha`
/// appended unless `v<version>` is among the tags at HEAD, which makes it
/// the release. No `sha` means no git (a crates.io install), and prints the
/// bare version.
pub fn build_id(version: &str, sha: Option<&str>, tags_at_head: &[&str]) -> String {
    let release_tag = format!("v{version}");
    match sha {
        Some(sha) if !tags_at_head.contains(&release_tag.as_str()) => {
            format!("{version} ({sha})")
        }
        _ => version.to_string(),
    }
}
