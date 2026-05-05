//! Screenshot a named listing from the built book.
//!
//! Two subcommands match the two listing-rendering shapes the
//! mdbook-listings preprocessor produces:
//!
//! ```text
//! capture-screenshots include LISTING            # {{#include listings/LISTING.ext}} block
//! capture-screenshots diff LEFT RIGHT            # {{#diff LEFT RIGHT}} block
//! ```
//!
//! Each subcommand:
//!
//! 1. Scans `book/src/*.md` for the chapter that references the
//!    target tag(s).
//! 2. Loads `book/build/html/<chapter-slug>.html`.
//! 3. Locates the rendered `<pre>` via the locator anchor the
//!    preprocessor emits — `[data-listing-tag]` for `include`,
//!    `[data-listing-diff-left][data-listing-diff-right]` for `diff`.
//! 4. Writes a PNG to `book/src/images/<default-name>.png` (or
//!    `--out` if specified). Output defaults: `<LISTING>.png` for
//!    include, `<LEFT>__to__<RIGHT>.png` for diff.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use playwright_rs::Playwright;
use tracing_subscriber::EnvFilter;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Parser)]
#[command(version, about = "Screenshot a named listing from the built book")]
struct Cli {
    // CALLOUT: cli-parse
    #[command(subcommand)]
    command: Command,

    /// Root of the book workspace (directory containing book.toml).
    /// Defaults to `book/` in the workspace root.
    #[arg(long, global = true)]
    book_root: Option<PathBuf>,

    /// Output PNG path. Defaults to `<book-root>/src/images/<derived-name>.png`
    /// where the derived name is `<LISTING>` for `include` and
    /// `<LEFT>__to__<RIGHT>` for `diff`.
    #[arg(long, global = true)]
    out: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Screenshot a `{{#include listings/LISTING.ext}}` block.
    Include {
        /// Tag (file stem) of the listing to screenshot.
        listing: String,
    },

    /// Screenshot a `{{#diff LEFT RIGHT}}` block.
    Diff {
        /// Left (old) operand of the diff directive.
        left: String,
        /// Right (new) operand of the diff directive.
        right: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // playwright-rs (unreleased v0.13.0 from `padamson/playwright-rust` main)
    // adds `#[tracing::instrument]` spans across its public async surface;
    // wiring up tracing_subscriber here makes every `goto`, `evaluate_value`,
    // `screenshot`, `browser.close`, etc. log a structured span. Default
    // filter `info` keeps the top-level operations visible without
    // descending into the per-RPC `debug` chatter; raise via
    // `RUST_LOG=capture_screenshots=debug,playwright_rs=debug` when
    // diagnosing locator or screenshot failures.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .compact()
        .init();

    let book_root = cli
        .book_root
        .unwrap_or_else(|| PathBuf::from(MANIFEST_DIR).join("../../book"))
        .canonicalize()?;
    let src_dir = book_root.join("src");
    let html_dir = book_root.join("build").join("html");

    // CALLOUT: subcommand-dispatch Each subcommand resolves into a `Job` carrying the discovery substring (for chapter-md scanning), the CSS selector (for the locator anchor in rendered HTML), the JavaScript that promotes the preceding `<pre>` to `id="__capture_target__"`, and the resolved output path.
    let job = Job::from(&cli.command, &cli.out, &src_dir);

    // CALLOUT: discover Scans book/src/*.md for the chapter that includes or diffs the tag(s).
    let chapter_slug = find_chapter_for_pattern(&src_dir, &job.discovery_pattern)
        .ok_or_else(|| format!("no chapter contains `{}`", job.discovery_pattern))?;
    println!("→ chapter: {chapter_slug}");

    let html_path = html_dir.join(format!("{chapter_slug}.html"));
    if !html_path.exists() {
        return Err(format!("chapter HTML not found: {}", html_path.display()).into());
    }
    let url = format!("file://{}", html_path.display());

    let pw = Playwright::launch().await?;
    let browser = pw.chromium().launch().await?;
    let page = browser.new_page().await?;
    page.goto(&url, None).await?;

    // mdbook's #menu-bar is position: sticky; element-scoped screenshots
    // capture overlapping viewport content, so the header would otherwise
    // appear on top of the listing. Demote it to static.
    let _: String = page
        .evaluate_value(
            r#"(() => {
                document.querySelectorAll('#menu-bar, .menu-bar')
                    .forEach(el => el.style.position = 'static');
                return 'ok';
            })()"#,
        )
        .await?;

    // CALLOUT: locate Sets id="__capture_target__" on the listing's <pre> via JavaScript so a stable CSS selector can target it for the screenshot.
    let result: String = page.evaluate_value(&job.locator_js).await?;
    if result == "not-found" {
        return Err(format!(
            "could not locate `{}` in {chapter_slug}.html via selector `{}`",
            job.discovery_pattern, job.css_selector,
        )
        .into());
    }
    println!("→ located via selector: {}", job.css_selector);

    if let Some(parent) = job.out.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let png = page
        .locator("#__capture_target__")
        .await
        .screenshot(None)
        .await?;
    std::fs::write(&job.out, png)?;
    println!("✓ wrote {}", job.out.display());

    browser.close().await?;
    Ok(())
}

/// All the per-subcommand inputs the rest of `main` needs in one place:
/// the substring used to scan chapter `.md` files for the right chapter,
/// the CSS selector that uniquely names the locator anchor in the rendered
/// HTML, the JavaScript that promotes the preceding `<pre>` to
/// `id="__capture_target__"`, and the resolved output path.
struct Job {
    discovery_pattern: String,
    css_selector: String,
    locator_js: String,
    out: PathBuf,
}

impl Job {
    fn from(cmd: &Command, out_override: &Option<PathBuf>, src_dir: &Path) -> Self {
        match cmd {
            Command::Include { listing } => {
                let css_selector = format!(r#"[data-listing-tag="{listing}"]"#);
                Job {
                    discovery_pattern: format!("{{{{#include listings/{listing}."),
                    locator_js: locator_js_for(&css_selector),
                    css_selector,
                    out: out_override
                        .clone()
                        .unwrap_or_else(|| src_dir.join("images").join(format!("{listing}.png"))),
                }
            }
            Command::Diff { left, right } => {
                let css_selector = format!(
                    r#"[data-listing-diff-left="{left}"][data-listing-diff-right="{right}"]"#
                );
                Job {
                    discovery_pattern: format!("{{{{#diff {left} {right}"),
                    locator_js: locator_js_for(&css_selector),
                    css_selector,
                    out: out_override.clone().unwrap_or_else(|| {
                        src_dir
                            .join("images")
                            .join(format!("{left}__to__{right}.png"))
                    }),
                }
            }
        }
    }
}

/// Walks back from the locator anchor to the most recent `<pre>` (skipping
/// any `<div class="callout-overlay">` sibling between them) and tags it
/// `id="__capture_target__"` so a stable CSS selector can drive the
/// element-scoped screenshot.
fn locator_js_for(css_selector: &str) -> String {
    format!(
        r#"(() => {{
    const anchor = document.querySelector('{css_selector}');
    if (!anchor) return 'not-found';
    let el = anchor.previousElementSibling;
    while (el && el.tagName !== 'PRE') el = el.previousElementSibling;
    if (!el) return 'not-found';
    el.setAttribute('id', '__capture_target__');
    return 'located';
}})()"#
    )
}

/// Scans `src_dir` for a `.md` file containing `pattern` (a substring like
/// `"{{#include listings/foo."` or `"{{#diff foo bar"`). Returns the
/// chapter's filename stem (e.g. `ch04-render-inline-callouts`).
fn find_chapter_for_pattern(src_dir: &Path, pattern: &str) -> Option<String> {
    let entries = std::fs::read_dir(src_dir).ok()?;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if content.contains(pattern) {
            return path.file_stem().and_then(|s| s.to_str()).map(String::from);
        }
    }
    None
}
