//! Install the browsers the bundled Playwright driver expects.
//!
//! Run before the e2e suite: `cargo run --example install-browsers`.
//!
//! The revision comes from playwright-rs itself, which is the point. Naming
//! a Playwright version here (or in the workflow) would be a second thing to
//! keep in step with the crate, and dependabot cannot see a version buried
//! in a shell step — so it drifts on the next bump and every e2e test dies
//! at launch with `BrowserNotInstalled`.
//!
//! `install_browsers` adds `--with-deps` on Linux by itself, so CI gets the
//! system libraries it needs and a local macOS run doesn't ask for sudo.
//!
//! playwright-rs also ships a `playwright-rs install` bin behind its `cli`
//! feature, but reaching it from CI means `cargo install playwright-rs`,
//! which either floats to the latest release or pins a version — both
//! reintroduce a second version to keep in step. This example rides the
//! lockfile, so there is exactly one.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    playwright_rs::install_browsers(Some(&["chromium"])).await?;
    Ok(())
}
