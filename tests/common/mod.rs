//! Shared helpers for the per-story integration test files. Each file under
//! `tests/` compiles as its own crate, so the helper is brought in via
//! `mod common;`.

use assert_cmd::Command;

pub fn mdbook_listings() -> Command {
    Command::cargo_bin("mdbook-listings").expect("binary should be built by cargo-test")
}
