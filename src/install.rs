//! `install` subcommand: configures an existing book to use mdbook-listings.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

/// Compiled in so `cargo install mdbook-listings` produces a self-contained
/// binary with nothing external to fetch at install time.
pub const CSS_ASSET: &[u8] = include_bytes!("../assets/mdbook-listings.css");
pub const JS_ASSET: &[u8] = include_bytes!("../assets/mdbook-listings.js");

/// Catches builds that stripped or replaced the asset — a missing sentinel
/// means the bundled bytes are not the expected build-time asset.
pub const CSS_ASSET_SENTINEL: &str = "mdbook-listings-css-v6";
pub const JS_ASSET_SENTINEL: &str = "mdbook-listings-js-v6";

/// Shared between the writer and the registrar so the two can't drift.
pub const CSS_ASSET_FILENAME: &str = "mdbook-listings.css";
pub const JS_ASSET_FILENAME: &str = "mdbook-listings.js";
pub const GITIGNORE_FILENAME: &str = ".gitignore";

/// Always overwrites — install ships the bundled bytes, not whatever a
/// stale on-disk copy happens to contain.
pub fn write_css_asset(book_root: &Path) -> Result<()> {
    let path = book_root.join(CSS_ASSET_FILENAME);
    fs::write(&path, CSS_ASSET).with_context(|| format!("writing CSS asset to {}", path.display()))
}

pub fn write_js_asset(book_root: &Path) -> Result<()> {
    let path = book_root.join(JS_ASSET_FILENAME);
    fs::write(&path, JS_ASSET).with_context(|| format!("writing JS asset to {}", path.display()))
}

/// Idempotent: writes the bundled CSS/JS to the book root only when the
/// on-disk bytes differ from the binary's embedded asset. Called by both
/// `install` (one-time setup) and the preprocessor (every build), so a
/// downstream book always renders against assets matching the binary
/// version — no manual reinstall required after `cargo install --force`.
/// Returns `true` iff anything was written.
pub fn ensure_assets_fresh(book_root: &Path) -> Result<bool> {
    let css_path = book_root.join(CSS_ASSET_FILENAME);
    let css_already_correct = fs::read(&css_path)
        .ok()
        .is_some_and(|bytes| bytes.as_slice() == CSS_ASSET);
    if !css_already_correct {
        write_css_asset(book_root)?;
    }
    let js_path = book_root.join(JS_ASSET_FILENAME);
    let js_already_correct = fs::read(&js_path)
        .ok()
        .is_some_and(|bytes| bytes.as_slice() == JS_ASSET);
    if !js_already_correct {
        write_js_asset(book_root)?;
    }
    Ok(!css_already_correct || !js_already_correct)
}

/// Idempotent: ensures both asset filenames are present as whole-line
/// entries in the book's `.gitignore`. Creates the file if missing.
/// Existing entries are left untouched; missing ones are appended.
/// Returns `true` iff `.gitignore` was written.
pub fn ensure_gitignore(book_root: &Path) -> Result<bool> {
    let path = book_root.join(GITIGNORE_FILENAME);
    let original = fs::read_to_string(&path).unwrap_or_default();
    let needed = [CSS_ASSET_FILENAME, JS_ASSET_FILENAME];
    let missing: Vec<&str> = needed
        .iter()
        .copied()
        .filter(|entry| !original.lines().any(|l| l.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }
    let mut new_contents = original.clone();
    if !new_contents.is_empty() && !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }
    for entry in missing {
        new_contents.push_str(entry);
        new_contents.push('\n');
    }
    fs::write(&path, new_contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Idempotent: book.toml and the CSS asset on disk are only rewritten if
/// they differ from what install would produce.
pub fn install(book_root: &Path) -> Result<InstallOutcome> {
    let book_toml_path = book_root.join("book.toml");
    let original = match fs::read_to_string(&book_toml_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "book.toml not found at {} — install requires an existing mdbook book directory; run `mdbook init` first.",
                book_toml_path.display(),
            );
        }
        Err(e) => {
            return Err(anyhow::Error::from(e))
                .with_context(|| format!("reading book config at {}", book_toml_path.display()));
        }
    };
    let mut config = BookConfig::parse(&original)?;
    config.register_listings_preprocessor();
    config.register_listings_css();
    config.register_listings_js();
    let new = config.render();

    let toml_changed = new != original;
    if toml_changed {
        fs::write(&book_toml_path, new)
            .with_context(|| format!("writing book config at {}", book_toml_path.display()))?;
    }
    let assets_written = ensure_assets_fresh(book_root)?;
    let gitignore_changed = ensure_gitignore(book_root)?;

    Ok(if toml_changed || assets_written || gitignore_changed {
        InstallOutcome::Installed
    } else {
        InstallOutcome::Unchanged
    })
}

/// Lets the CLI tell the author whether a re-install was a no-op (AC 3).
#[derive(Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed,
    Unchanged,
}

/// Newtype over [`toml_edit::DocumentMut`] so callers don't depend on
/// `toml_edit` directly and the `register_*` methods have a domain type
/// to attach to.
#[derive(Debug)]
pub struct BookConfig(DocumentMut);

impl BookConfig {
    pub fn parse(s: &str) -> Result<Self> {
        s.parse::<DocumentMut>()
            .map(BookConfig)
            .context("book config is not valid TOML")
    }

    pub fn render(&self) -> String {
        self.0.to_string()
    }

    /// Idempotent: a second call on an already-registered config is a no-op
    /// in the rendered output. The listings entry always declares
    /// `before = ["links"]` so the include splicer sees raw
    /// `{{#include listings/...}}` directives before mdbook's built-in
    /// `links` preprocessor expands them. If `[preprocessor.admonish]` is
    /// also registered, `"admonish"` is added to the same `before` list so
    /// the callout → admonish-note pipeline produces correctly styled PDF
    /// output.
    pub fn register_listings_preprocessor(&mut self) {
        let preprocessor = subtable_mut(self.0.as_table_mut(), "preprocessor");
        let admonish_present = preprocessor.contains_key("admonish");
        let listings = subtable_mut(preprocessor, "listings");
        listings["command"] = toml_edit::value("mdbook-listings");
        let mut before = Array::new();
        if admonish_present {
            before.push("admonish");
        }
        before.push("links");
        listings["before"] = toml_edit::value(before);
    }

    /// Idempotent: duplicate entries are not appended.
    pub fn register_listings_css(&mut self) {
        register_html_asset(self.0.as_table_mut(), "additional-css", CSS_ASSET_FILENAME);
    }

    /// Idempotent: duplicate entries are not appended.
    pub fn register_listings_js(&mut self) {
        register_html_asset(self.0.as_table_mut(), "additional-js", JS_ASSET_FILENAME);
    }
}

fn register_html_asset(root: &mut Table, key: &'static str, filename: &str) {
    let entry = format!("./{filename}");
    let html = subtable_mut(subtable_mut(root, "output"), "html");
    let array = html
        .entry(key)
        .or_insert_with(|| Item::Value(Value::Array(Array::new())))
        .as_value_mut()
        .unwrap_or_else(|| panic!("{key} must be a value"))
        .as_array_mut()
        .unwrap_or_else(|| panic!("{key} must be an array"));
    if !array.iter().any(|v| v.as_str() == Some(entry.as_str())) {
        array.push(entry);
    }
}

/// Get a mutable reference to `parent[key]` as a `Table`, creating the
/// child table if absent. Replaces the open-coded
/// `entry().or_insert_with(...).as_table_mut().expect(...)` chain.
fn subtable_mut<'a>(parent: &'a mut Table, key: &str) -> &'a mut Table {
    parent
        .entry(key)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .expect("entry must be a table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_asset_is_non_empty() {
        assert!(!CSS_ASSET.is_empty(), "bundled CSS asset must not be empty");
    }

    #[test]
    fn css_asset_contains_sentinel() {
        let contents = std::str::from_utf8(CSS_ASSET).expect("CSS asset must be UTF-8");
        assert!(
            contents.contains(CSS_ASSET_SENTINEL),
            "bundled CSS asset must contain sentinel `{CSS_ASSET_SENTINEL}`; got:\n{contents}",
        );
    }

    #[test]
    fn js_asset_is_non_empty() {
        assert!(!JS_ASSET.is_empty(), "bundled JS asset must not be empty");
    }

    #[test]
    fn js_asset_contains_sentinel() {
        let contents = std::str::from_utf8(JS_ASSET).expect("JS asset must be UTF-8");
        assert!(
            contents.contains(JS_ASSET_SENTINEL),
            "bundled JS asset must contain sentinel `{JS_ASSET_SENTINEL}`; got:\n{contents}",
        );
    }

    #[test]
    fn book_config_round_trip_preserves_comments_and_ordering() {
        let input = "\
# top comment
[book]
title = \"Test\"

# preprocessor comment
[preprocessor.admonish]
command = \"mdbook-admonish\"

[output.html]
";
        let cfg = BookConfig::parse(input).expect("parse");
        assert_eq!(cfg.render(), input);
    }

    #[test]
    fn book_config_parse_rejects_invalid_toml() {
        let err = BookConfig::parse("[book\nbroken = ").unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("not valid TOML"),
            "diagnostic should name the failure mode; got: {msg}"
        );
    }

    #[test]
    fn book_config_register_listings_preprocessor_adds_entry() {
        let mut cfg = BookConfig::parse("[book]\ntitle = \"Test\"\n").unwrap();
        cfg.register_listings_preprocessor();
        let rendered = cfg.render();
        assert!(
            rendered.contains("[preprocessor.listings]"),
            "rendered config should declare [preprocessor.listings]; got:\n{rendered}",
        );
        assert!(
            rendered.contains(r#"command = "mdbook-listings""#),
            "rendered config should set command = \"mdbook-listings\"; got:\n{rendered}",
        );
    }

    #[test]
    fn book_config_register_listings_preprocessor_is_idempotent() {
        let input = "[book]\ntitle = \"Test\"\n";
        let mut cfg = BookConfig::parse(input).unwrap();
        cfg.register_listings_preprocessor();
        let after_first = cfg.render();

        let mut cfg2 = BookConfig::parse(&after_first).unwrap();
        cfg2.register_listings_preprocessor();
        let after_second = cfg2.render();

        assert_eq!(after_first, after_second, "register must be idempotent");
    }

    #[test]
    fn book_config_register_listings_css_adds_entry() {
        let mut cfg = BookConfig::parse("[book]\ntitle = \"Test\"\n").unwrap();
        cfg.register_listings_css();
        let rendered = cfg.render();
        assert!(
            rendered.contains("[output.html]"),
            "rendered config should declare [output.html]; got:\n{rendered}",
        );
        assert!(
            rendered.contains(r#"additional-css = ["./mdbook-listings.css"]"#),
            "rendered config should reference the CSS asset; got:\n{rendered}",
        );
    }

    #[test]
    fn book_config_register_listings_js_adds_entry() {
        let mut cfg = BookConfig::parse("[book]\ntitle = \"Test\"\n").unwrap();
        cfg.register_listings_js();
        let rendered = cfg.render();
        assert!(
            rendered.contains(r#"additional-js = ["./mdbook-listings.js"]"#),
            "rendered config should reference the JS asset; got:\n{rendered}",
        );
    }

    #[test]
    fn book_config_register_listings_js_is_idempotent() {
        let input = "[book]\ntitle = \"Test\"\n";
        let mut cfg = BookConfig::parse(input).unwrap();
        cfg.register_listings_js();
        let after_first = cfg.render();
        let mut cfg2 = BookConfig::parse(&after_first).unwrap();
        cfg2.register_listings_js();
        let after_second = cfg2.render();
        assert_eq!(
            after_first, after_second,
            "register_listings_js must be idempotent"
        );
    }

    #[test]
    fn book_config_register_listings_preprocessor_orders_before_admonish_and_links_when_admonish_present()
     {
        let input = "[preprocessor.admonish]\ncommand = \"mdbook-admonish\"\n";
        let mut cfg = BookConfig::parse(input).unwrap();
        cfg.register_listings_preprocessor();
        let rendered = cfg.render();
        assert!(
            rendered.contains(r#"before = ["admonish", "links"]"#),
            "listings should declare before = [\"admonish\", \"links\"]; got:\n{rendered}",
        );
        assert!(
            rendered.contains("[preprocessor.admonish]"),
            "admonish should still be registered; got:\n{rendered}",
        );
    }

    #[test]
    fn book_config_register_listings_preprocessor_orders_before_links_when_admonish_absent() {
        // The include splicer requires `before = ["links"]` so it sees raw
        // `{{#include listings/...}}` before mdbook's built-in `links`
        // expands them. Without this, the splicer silently no-ops.
        let mut cfg = BookConfig::parse("[book]\ntitle = \"Test\"\n").unwrap();
        cfg.register_listings_preprocessor();
        let rendered = cfg.render();
        assert!(
            rendered.contains(r#"before = ["links"]"#),
            "listings should declare before = [\"links\"] when admonish is absent; got:\n{rendered}",
        );
    }

    #[test]
    fn book_config_register_listings_preprocessor_idempotent_with_admonish_present() {
        let input = "[preprocessor.admonish]\ncommand = \"mdbook-admonish\"\n";
        let mut cfg = BookConfig::parse(input).unwrap();
        cfg.register_listings_preprocessor();
        let after_first = cfg.render();

        let mut cfg2 = BookConfig::parse(&after_first).unwrap();
        cfg2.register_listings_preprocessor();
        let after_second = cfg2.render();

        assert_eq!(
            after_first, after_second,
            "register must be idempotent when admonish is present"
        );
    }

    #[test]
    fn book_config_register_listings_css_is_idempotent() {
        let input = "[book]\ntitle = \"Test\"\n";
        let mut cfg = BookConfig::parse(input).unwrap();
        cfg.register_listings_css();
        let after_first = cfg.render();

        let mut cfg2 = BookConfig::parse(&after_first).unwrap();
        cfg2.register_listings_css();
        let after_second = cfg2.render();

        assert_eq!(
            after_first, after_second,
            "register_listings_css must be idempotent"
        );
    }
}
