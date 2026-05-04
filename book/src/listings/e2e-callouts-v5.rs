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

    // Slice 7+ shape: marker line stripped, badge is a <button> in the
    // sibling overlay; label-only markers emit no body popover.
    let body_present: String = page
        .evaluate_value(
            "(() => { \
                const btn = document.querySelector('button[id=\"callout-cli-parse\"]'); \
                if (!btn) return 'NOT_FOUND'; \
                const body = document.getElementById('callout-body-cli-parse'); \
                return body ? 'YES' : 'NO'; \
            })()",
        )
        .await
        .expect("evaluate");
    assert_ne!(
        body_present, "NOT_FOUND",
        "expected button#callout-cli-parse to exist on rendered ch. 4",
    );
    assert_eq!(
        body_present, "NO",
        "label-only callout must not have a body popover; got body presence: {body_present}",
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
            "(() => document.querySelector('button[id=\"callout-cross-ref-emit\"]') ? 'YES' : 'NO')()",
        )
        .await
        .expect("evaluate anchor presence");
    assert_eq!(
        target_present, "YES",
        "expected listing-side button#callout-cross-ref-emit to exist as the cross-ref's target",
    );

    browser.close().await.expect("close browser");
}

#[tokio::test]
async fn callout_marker_comment_is_stripped_and_body_reveals_on_hover() {
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    // Pick a listing whose pre is known to contain a CALLOUT-bearing
    // include: the cross-ref-emit marker is in the slice 6 snippet
    // include, which the slice 7 splicer should have stripped from the
    // rendered <pre>. Verify the literal "CALLOUT: cross-ref-emit"
    // string is gone from that pre.
    let pre_text: String = page
        .evaluate_value(
            "(() => { \
                const btn = document.querySelector('button[id=\"callout-cross-ref-emit\"]'); \
                if (!btn) return 'NO_BTN'; \
                const overlay = btn.closest('.callout-overlay'); \
                const pre = overlay && overlay.previousElementSibling; \
                return pre ? pre.textContent : 'NO_PRE'; \
            })()",
        )
        .await
        .expect("evaluate pre text");
    assert!(
        !pre_text.contains("CALLOUT: cross-ref-emit"),
        "expected marker comment line to be stripped from the include's <pre>; \
         got pre.textContent containing the marker:\n{pre_text}",
    );

    // The body popover starts hidden and becomes visible after hovering
    // its triggering badge. Use Playwright's hover().
    let badge = page
        .locator("button[id=\"callout-body-emit-source\"], button[id=\"callout-cross-ref-emit\"]")
        .await;
    badge.first().hover(None).await.expect("hover badge");

    let body_visible: String = page
        .evaluate_value(
            "(() => { \
                const body = document.getElementById('callout-body-cross-ref-emit'); \
                if (!body) return 'NO_BODY'; \
                const cs = window.getComputedStyle(body); \
                return cs.display === 'none' ? 'HIDDEN' : 'VISIBLE'; \
            })()",
        )
        .await
        .expect("evaluate body visibility");
    assert_eq!(
        body_visible, "VISIBLE",
        "expected body popover to become visible after hovering badge; got: {body_visible}",
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
