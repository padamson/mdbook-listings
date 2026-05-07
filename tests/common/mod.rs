//! Shared helpers for the per-story integration test files. Each file under
//! `tests/` compiles as its own crate, so the helper is brought in via
//! `mod common;`. Different test binaries use different subsets — silence
//! the dead-code warnings that fire for the unused parts in each binary.
#![allow(dead_code)]

use assert_cmd::Command;

pub mod e2e_harness;

pub fn mdbook_listings() -> Command {
    Command::cargo_bin("mdbook-listings").expect("binary should be built by cargo-test")
}
