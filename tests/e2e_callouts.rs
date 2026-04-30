use std::path::PathBuf;

use playwright_rs::Playwright;

#[tokio::test]
#[ignore = "no rendered callouts in ch. 4 yet"]
async fn callout_badge_renders_with_data_attribute_in_ch04() {
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    let badge = page.locator("[data-callout-badge]").await;
    let count = badge.count().await.expect("count badges");
    assert!(
        count > 0,
        "expected at least one [data-callout-badge] element on rendered ch. 4; got 0",
    );
    let text = badge.first().text_content().await.expect("badge text");
    assert!(
        text.as_deref().is_some_and(|s| !s.trim().is_empty()),
        "expected badge text to be non-empty; got {text:?}",
    );

    browser.close().await.expect("close browser");
}

fn chapter_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("book")
        .join("build")
        .join("html")
        .join("ch04-render-inline-callouts.html")
}
