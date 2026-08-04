//! Managed code listings for mdbook.
//!
//! The public API is [`cli::run`], which `src/main.rs` shims over, plus
//! [`install`], which `tests/install.rs` drives directly. Everything else
//! is crate-private: this is a preprocessor binary, not a library anyone
//! builds against, and a public surface it never promised turns every
//! added error variant and struct field into a breaking release.

pub mod cli;
pub mod install;

pub(crate) mod anchor;
pub(crate) mod callout;
pub(crate) mod diff;
pub(crate) mod directive;
pub(crate) mod fence;
pub(crate) mod freeze;
pub(crate) mod include;
pub(crate) mod list_of_listings;
pub(crate) mod listing_ref;
pub(crate) mod manifest;
pub(crate) mod number;
pub(crate) mod pipeline;
pub(crate) mod verify;
