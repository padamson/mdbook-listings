use std::path::PathBuf;

use playwright_rs::Playwright;

#[tokio::test]
async fn label_only_callout_renders_badge_without_following_body() {
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    let next_tag: String = page
        .evaluate_value(
            "(() => { \
                const dt = document.querySelector('dt[id=\"callout-cli-parse\"]'); \
                if (!dt) return 'NOT_FOUND'; \
                const next = dt.nextElementSibling; \
                return next ? next.tagName : 'NONE'; \
            })()",
        )
        .await
        .expect("evaluate");
    assert_ne!(
        next_tag, "NOT_FOUND",
        "expected dt#callout-cli-parse to exist on rendered ch. 4",
    );
    assert_ne!(
        next_tag, "DD",
        "label-only callout's dt must not be followed by a <dd>; got <{next_tag}>",
    );

    browser.close().await.expect("close browser");
}

#[tokio::test]
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

#[tokio::test]
async fn callout_cross_ref_renders_as_anchor_to_listing_badge() {
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    let href: String = page
        .evaluate_value(
            "(() => { \
                const a = document.querySelector('a[data-callout-ref=\"cross-ref-emit\"]'); \
                return a ? a.getAttribute('href') : 'NOT_FOUND'; \
            })()",
        )
        .await
        .expect("evaluate href");
    assert_eq!(
        href, "#callout-cross-ref-emit",
        "expected prose-side cross-ref to point at listing badge anchor",
    );

    let target_present: String = page
        .evaluate_value(
            "(() => document.querySelector('dt[id=\"callout-cross-ref-emit\"]') ? 'YES' : 'NO')()",
        )
        .await
        .expect("evaluate anchor presence");
    assert_eq!(
        target_present, "YES",
        "expected listing-side dt#callout-cross-ref-emit to exist as the cross-ref's target",
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
