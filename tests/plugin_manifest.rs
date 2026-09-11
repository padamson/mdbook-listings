//! The plugin manifests are distributed artifacts: `/plugin install` copies
//! them into every consumer's plugin cache, and their `license` field is
//! what plugin directories and audit tooling read. The crate went
//! `MIT OR Apache-2.0` in 0.2.0 and both manifests kept saying `MIT` for a
//! month, because nothing compared them to `Cargo.toml`. This does, so the
//! three cannot drift apart again without a red test.

use serde_json::Value;

fn manifest(name: &str) -> Value {
    let path = format!("{}/.claude-plugin/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn plugin_manifests_declare_the_crates_license() {
    // Cargo exposes `[package] license` at compile time, so the crate side
    // needs no TOML parsing and cannot be misread.
    let crate_license = env!("CARGO_PKG_LICENSE");

    assert_eq!(
        manifest("plugin.json")["license"],
        crate_license,
        "plugin.json `license` must match Cargo.toml"
    );
    assert_eq!(
        manifest("marketplace.json")["plugins"][0]["license"],
        crate_license,
        "marketplace.json plugin entry `license` must match Cargo.toml"
    );
}
