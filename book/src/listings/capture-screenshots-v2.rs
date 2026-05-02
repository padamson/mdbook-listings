//! Capture an element-scoped screenshot of a rendered chapter and write the
//! PNG to a known path. Used by ch. 4's slice-by-slice visual record so each
//! slice's narrative can embed a snapshot of how the chapter rendered the
//! day the slice shipped.

use std::path::PathBuf;

use clap::Parser;
use playwright_rs::Playwright;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Absolute path to the rendered chapter HTML to load.
    #[arg(long)]
    chapter_html: PathBuf,

    /// CSS selector for the element to screenshot.
    #[arg(long)]
    selector: String,

    /// Zero-based index when the selector matches multiple elements.
    /// Negative values count from the end (`-1` is the last match).
    #[arg(long, default_value_t = 0)]
    nth: i32,

    /// Absolute path to write the PNG to. Parent directories are created.
    #[arg(long)]
    out: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // CALLOUT: cli-parse
    let cli = Cli::parse();
    if let Some(parent) = cli.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let url = format!("file://{}", cli.chapter_html.display());

    let pw = Playwright::launch().await?;
    let browser = pw.chromium().launch().await?;
    let page = browser.new_page().await?;
    page.goto(&url, None).await?;

    // CALLOUT: locator-pick `--nth` disambiguates when the selector matches more than one element; zero-based, negative counts from the end.
    let target = page.locator(&cli.selector).await.nth(cli.nth);
    let png = target.screenshot(None).await?;
    std::fs::write(&cli.out, png)?;
    println!("✓ wrote {}", cli.out.display());

    browser.close().await?;
    Ok(())
}
