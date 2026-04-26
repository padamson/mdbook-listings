//! `install` subcommand: configures an existing book to use mdbook-listings.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

/// Compiled in so `cargo install mdbook-listings` produces a self-contained
/// binary with nothing external to fetch at install time.
pub const CSS_ASSET: &[u8] = include_bytes!("../assets/mdbook-listings.css");

/// Catches builds that stripped or replaced the asset — a missing sentinel
/// means the bundled bytes are not the expected build-time asset.
pub const CSS_ASSET_SENTINEL: &str = "mdbook-listings-css-v1";

/// Filename the CSS asset is written under at install time. Shared between
/// the file-copy side and the `[output.html].additional-css` entry so the
/// two can't drift.
pub const CSS_ASSET_FILENAME: &str = "mdbook-listings.css";

/// Write the bundled [`CSS_ASSET`] to `<book_root>/<CSS_ASSET_FILENAME>`,
/// creating or overwriting the file. Existing content is replaced
/// unconditionally — this is the "ship the version this binary was
/// built with" semantics the install command wants.
pub fn write_css_asset(book_root: &Path) -> Result<()> {
    let path = book_root.join(CSS_ASSET_FILENAME);
    fs::write(&path, CSS_ASSET).with_context(|| format!("writing CSS asset to {}", path.display()))
}

/// Newtype over [`toml_edit::DocumentMut`] so future install methods
/// (register preprocessor, add additional-css) have a domain type to
/// attach to and so callers don't depend on `toml_edit` directly.
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

    /// Add (or confirm the presence of) `[preprocessor.listings]` with
    /// `command = "mdbook-listings"`. Idempotent — a second call on an
    /// already-registered config produces identical rendered output.
    pub fn register_listings_preprocessor(&mut self) {
        let preprocessor = self
            .0
            .as_table_mut()
            .entry("preprocessor")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .expect("[preprocessor] must be a table");
        let listings = preprocessor
            .entry("listings")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .expect("[preprocessor.listings] must be a table");
        listings["command"] = toml_edit::value("mdbook-listings");
    }

    /// Add `./<CSS_ASSET_FILENAME>` to `[output.html].additional-css`,
    /// creating the section + array as needed. Idempotent — duplicate
    /// entries are not appended.
    pub fn register_listings_css(&mut self) {
        let entry = format!("./{CSS_ASSET_FILENAME}");
        let array = self
            .0
            .as_table_mut()
            .entry("output")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .expect("[output] must be a table")
            .entry("html")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .expect("[output.html] must be a table")
            .entry("additional-css")
            .or_insert_with(|| Item::Value(Value::Array(Array::new())))
            .as_value_mut()
            .expect("additional-css must be a value")
            .as_array_mut()
            .expect("additional-css must be an array");
        if !array.iter().any(|v| v.as_str() == Some(entry.as_str())) {
            array.push(entry);
        }
    }
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
