//! `install` subcommand: configures an existing book to use mdbook-listings.

/// Compiled in so `cargo install mdbook-listings` produces a self-contained
/// binary with nothing external to fetch at install time.
pub const CSS_ASSET: &[u8] = include_bytes!("../assets/mdbook-listings.css");

/// Catches builds that stripped or replaced the asset — a missing sentinel
/// means the bundled bytes are not the expected build-time asset.
pub const CSS_ASSET_SENTINEL: &str = "mdbook-listings-css-v1";

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
}
