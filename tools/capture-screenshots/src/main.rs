//! Screenshot a named listing from the built book.
//!
//! Pass a listing tag (`e2e-callouts-v5`) and the tool finds the chapter
//! that references it, locates the rendered `<pre>` in the HTML, and
//! writes a PNG. Two locator strategies are tried in order:
//!
//! 1. `[data-listing-tag="TAG"]` — emitted by the diff splicer after every
//!    `{{#diff}}` block so diff-based listings are always addressable.
//! 2. `button[id="callout-LABEL"]` — when the listing file carries one or
//!    more `CALLOUT:` markers the tool reads the first label from source
//!    and finds the badge in the rendered overlay.

use std::path::{Path, PathBuf};

use clap::Parser;
use playwright_rs::Playwright;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Parser)]
#[command(version, about = "Screenshot a named listing from the built book")]
struct Cli {
    /// Tag of the listing to screenshot (e.g. `e2e-callouts-v5`).
    // CALLOUT: cli-parse
    tag: String,

    /// Root of the book workspace (directory containing book.toml).
    /// Defaults to `book/` in the workspace root.
    #[arg(long)]
    book_root: Option<PathBuf>,

    /// Output PNG path. Default: `<book-root>/src/images/<tag>.png`.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // CALLOUT: cli-parse
    let cli = Cli::parse();

    let book_root = cli
        .book_root
        .unwrap_or_else(|| PathBuf::from(MANIFEST_DIR).join("../../book"))
        .canonicalize()?;

    let src_dir = book_root.join("src");
    let html_dir = book_root.join("build").join("html");
    let listings_dir = src_dir.join("listings");

    let out = cli
        .out
        .unwrap_or_else(|| src_dir.join("images").join(format!("{}.png", cli.tag)));

    // CALLOUT: discover Scans book/src/*.md for the chapter that includes or diffs the listing tag.
    let chapter_slug = find_chapter_for_tag(&src_dir, &cli.tag)
        .ok_or_else(|| format!("no chapter references listing `{}`", cli.tag))?;
    println!("→ chapter: {chapter_slug}");

    let callout_label =
        find_listing_source(&listings_dir, &cli.tag).and_then(|p| first_callout_label(&p));

    let html_path = html_dir.join(format!("{chapter_slug}.html"));
    if !html_path.exists() {
        return Err(format!("chapter HTML not found: {}", html_path.display()).into());
    }
    let url = format!("file://{}", html_path.display());

    let locator_js = build_locator_js(&cli.tag, callout_label.as_deref());

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
    let result: String = page.evaluate_value(&locator_js).await?;
    if result == "not-found" {
        return Err(format!(
            "could not locate `{}` in {chapter_slug}.html \
             (tried data-listing-tag and callout-badge selectors)",
            cli.tag
        )
        .into());
    }
    println!("→ located via {result}");

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let png = page
        .locator("#__capture_target__")
        .await
        .screenshot(None)
        .await?;
    std::fs::write(&out, png)?;
    println!("✓ wrote {}", out.display());

    browser.close().await?;
    Ok(())
}

/// Builds the JavaScript snippet that finds the listing's `<pre>` and assigns
/// it a temporary `id` the Playwright locator can address. Tries the
/// `data-listing-tag` anchor (diff blocks) first, then the callout-badge
/// approach (include blocks whose source has a `CALLOUT:` marker).
fn build_locator_js(tag: &str, callout_label: Option<&str>) -> String {
    let badge_branch = match callout_label {
        Some(label) => format!(
            r#"
    const btn = document.querySelector('button[id="callout-{label}"]');
    if (btn) {{
        const overlay = btn.closest('.callout-overlay');
        const pre = overlay && overlay.previousElementSibling;
        if (pre && pre.tagName === 'PRE') {{
            pre.setAttribute('id', '__capture_target__');
            return 'badge-locator';
        }}
    }}"#
        ),
        None => String::new(),
    };

    format!(
        r#"(() => {{
    const anchor = document.querySelector('[data-listing-tag="{tag}"]');
    if (anchor) {{
        let el = anchor.previousElementSibling;
        while (el && el.tagName !== 'PRE') el = el.previousElementSibling;
        if (el) {{ el.setAttribute('id', '__capture_target__'); return 'tag-locator'; }}
    }}{badge_branch}
    return 'not-found';
}})()"#
    )
}

/// Scans `src_dir` for a `.md` file that references `tag` on a
/// `{{#diff` or `{{#include listings/` line. Returns the chapter's
/// filename stem (e.g. `ch04-render-inline-callouts`).
fn find_chapter_for_tag(src_dir: &Path, tag: &str) -> Option<String> {
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
        let referenced = content.lines().any(|line| {
            (line.contains("{{#diff") || line.contains("{{#include listings/"))
                && line.contains(tag)
        });
        if referenced {
            return path.file_stem().and_then(|s| s.to_str()).map(String::from);
        }
    }
    None
}

/// Returns the path to the frozen listing file whose stem matches `tag`.
fn find_listing_source(listings_dir: &Path, tag: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(listings_dir).ok()?;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.file_stem().and_then(|s| s.to_str()) == Some(tag) {
            return Some(path);
        }
    }
    None
}

/// Returns the label of the first `CALLOUT:` marker found in `listing_path`.
fn first_callout_label(listing_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(listing_path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim_start();
        for prefix in ["// CALLOUT: ", "# CALLOUT: ", "-- CALLOUT: "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Some(label) = rest.split_whitespace().next().filter(|l| !l.is_empty()) {
                    return Some(label.to_string());
                }
            }
        }
    }
    None
}
