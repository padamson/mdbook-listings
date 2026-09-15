//! The skill's frontmatter is a distributed artifact: `npx skills add`
//! copies SKILL.md into every consumer, `metadata.version` is the only
//! version a consumer can compare on refresh, and `license` is what skill
//! directories and audit tooling read. The crate went `MIT OR Apache-2.0`
//! in 0.2.0 and the manifests that used to sit beside this file kept saying
//! `MIT` for a month, because nothing compared them to `Cargo.toml`. The
//! version guard script checks that the version moves per commit; this
//! checks that the fields are there and the licence matches on every test
//! run, with prek or without.

fn frontmatter() -> String {
    let path = format!(
        "{}/skills/mdbook-listings/SKILL.md",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let front = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .expect("SKILL.md must open with a `---` frontmatter block");
    front.to_string()
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches(['"', '\'']).to_string()
}

/// A top-level `key:` in the frontmatter, unquoted.
fn top_level(front: &str, key: &str) -> Option<String> {
    front.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k == key).then(|| unquote(v))
    })
}

/// The `version:` under `metadata:`, unquoted. A line scan rather than a
/// YAML parser, and the same scan the version guard script runs: two
/// readers of one field must accept exactly the same bytes, so neither gets
/// a grammar the other lacks. Inside the `metadata:` block, the first
/// `version:` line; blank lines are skipped, the block ends at the first
/// unindented line.
fn metadata_version(front: &str) -> Option<String> {
    let mut in_metadata = false;
    for line in front.lines() {
        if line.starts_with("metadata:") {
            in_metadata = true;
            continue;
        }
        if !in_metadata || line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            break;
        }
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        if key == "version" {
            return Some(unquote(value));
        }
    }
    None
}

#[test]
fn skill_frontmatter_declares_the_crates_license() {
    // Cargo exposes `[package] license` at compile time, so the crate side
    // needs no TOML parsing and cannot be misread.
    let front = frontmatter();
    assert_eq!(
        top_level(&front, "license").as_deref(),
        Some(env!("CARGO_PKG_LICENSE")),
        "SKILL.md `license` must match Cargo.toml"
    );
}

#[test]
fn skill_frontmatter_carries_a_metadata_version() {
    let front = frontmatter();
    let version =
        metadata_version(&front).expect("SKILL.md frontmatter must carry `metadata.version`");
    let dotted_digits = version
        .split('.')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    assert!(
        version.matches('.').count() == 2 && dotted_digits,
        "SKILL.md `metadata.version` must be MAJOR.MINOR.PATCH, got {version:?}"
    );
}
