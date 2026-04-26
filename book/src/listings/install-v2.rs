//! `install` subcommand: configures an existing book to use mdbook-listings.

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

/// Compiled in so `cargo install mdbook-listings` produces a self-contained
/// binary with nothing external to fetch at install time.
pub const CSS_ASSET: &[u8] = include_bytes!("../assets/mdbook-listings.css");

/// Catches builds that stripped or replaced the asset — a missing sentinel
/// means the bundled bytes are not the expected build-time asset.
pub const CSS_ASSET_SENTINEL: &str = "mdbook-listings-css-v1";

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
}
