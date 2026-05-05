use std::path::PathBuf;

use playwright_rs::{Playwright, locator};

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

    let badge = page.locator(locator!("[data-callout-badge]")).await;
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
        .locator(locator!(
            "button[id=\"callout-body-emit-source\"], button[id=\"callout-cross-ref-emit\"]"
        ))
        .await;
    badge.first().hover(None).await.expect("hover badge");

    let body_visible: String = page
        .evaluate_value(
            "(() => { \
                const body = document.getElementById('callout-body-cross-ref-emit'); \
                if (!body) return 'NO_BODY'; \
                const cs = window.getComputedStyle(body); \
                return cs.visibility === 'hidden' ? 'HIDDEN' : 'VISIBLE'; \
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

#[tokio::test]
async fn every_callout_cross_ref_resolves_to_a_badge_with_matching_ordinal_and_text() {
    // Sweep guard for prose-side cross-refs: every `{{#callout LABEL}}`
    // directive renders as an `<a class="callout-badge callout-ref"
    // href="#callout-LABEL" data-callout-ref="LABEL"
    // data-callout-ordinal="N">N</a>`. For each one in the chapter, we
    // verify:
    //
    // 1. `href` matches `#callout-<data-callout-ref>` (no stale
    //    label-to-href drift)
    // 2. A `button[id="callout-LABEL"]` actually exists as the target
    //    (catches refs to labels whose only occurrence is in a `{{#diff}}`
    //    block, since the HTML splicer skips diffs for badge emission)
    // 3. The ref's `data-callout-ordinal` matches the target badge's
    //    `data-callout-ordinal` (catches numbering drift between the
    //    label-to-ordinal map's first-occurrence pass and the per-listing
    //    badge emission pass)
    // 4. The rendered text on the ref matches the target badge's rendered
    //    text (the visible numeral readers see)
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    let issues: String = page
        .evaluate_value(
            r#"(() => {
                const refs = Array.from(document.querySelectorAll('a[data-callout-ref]'));
                if (refs.length === 0) {
                    return 'NO_REFS_FOUND_AT_ALL';
                }
                const issues = [];
                refs.forEach(a => {
                    const label = a.getAttribute('data-callout-ref');
                    const refOrdinal = a.getAttribute('data-callout-ordinal');
                    const refText = (a.textContent || '').trim();
                    const expectedHref = '#callout-' + label;
                    const actualHref = a.getAttribute('href');
                    if (actualHref !== expectedHref) {
                        issues.push(`label "${label}": href="${actualHref}" but expected "${expectedHref}"`);
                    }
                    const target = document.querySelector('button[id="callout-' + label + '"]');
                    if (!target) {
                        issues.push(`label "${label}": no badge button#callout-${label} exists as the cross-ref target`);
                        return;
                    }
                    const targetOrdinal = target.getAttribute('data-callout-ordinal');
                    const targetText = (target.textContent || '').trim();
                    if (refOrdinal !== targetOrdinal) {
                        issues.push(`label "${label}": ref data-callout-ordinal="${refOrdinal}" but target badge data-callout-ordinal="${targetOrdinal}"`);
                    }
                    if (refText !== targetText) {
                        issues.push(`label "${label}": ref text "${refText}" but target badge text "${targetText}"`);
                    }
                });
                return issues.join('\n');
            })()"#,
        )
        .await
        .expect("evaluate cross-ref sweep");

    assert_ne!(
        issues, "NO_REFS_FOUND_AT_ALL",
        "expected at least one a[data-callout-ref] in the chapter; the chapter has no cross-refs to sweep",
    );
    assert!(
        issues.is_empty(),
        "callout cross-ref sweep found broken refs in the chapter:\n{issues}",
    );

    browser.close().await.expect("close browser");
}

#[tokio::test]
async fn every_cross_refed_label_has_a_visible_badge_in_the_chapter() {
    // Regression guard, scoped to labels the author actually points at:
    // every `{{#callout LABEL}}` directive in chapter prose renders an
    // `<a data-callout-ref="LABEL">`, and the matching badge
    // `button[id="callout-LABEL"]` must exist somewhere in the rendered
    // page. Catches the most common slice-shipping mistake — a cross-ref
    // to a marker whose only chapter occurrence is in a `{{#diff}}` block
    // (which the HTML splicer skips for badge emission, by design — the
    // canonical anchor lives on a non-diff include). Test-fixture
    // marker strings inside string literals (e.g. `// CALLOUT: greeting`
    // inside Rust unit-test source) are intentionally not flagged: the
    // author isn't pointing at them, so they're not part of the chapter's
    // user-facing cross-reference contract.
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    // For every `a[data-callout-ref]` (the prose-side cross-ref shape),
    // collect the label and check that a matching `button[id="callout-LABEL"]`
    // exists. Returns a comma-separated list of labels that have a cross-ref
    // pointing at them but no badge target.
    let unmatched: String = page
        .evaluate_value(
            r#"(() => {
                const refs = Array.from(document.querySelectorAll('a[data-callout-ref]'));
                const missing = new Set();
                refs.forEach(a => {
                    const label = a.getAttribute('data-callout-ref');
                    if (!label) return;
                    if (!document.querySelector('button[id="callout-' + label + '"]')) {
                        missing.add(label);
                    }
                });
                return Array.from(missing).sort().join(',');
            })()"#,
        )
        .await
        .expect("evaluate cross-ref-target scan");

    assert!(
        unmatched.is_empty(),
        "the following labels are cross-refed in chapter prose but have no \
         `button[id=\"callout-LABEL\"]` target in the rendered chapter — \
         most likely the cross-ref points at a marker whose only occurrence \
         is in a `{{{{#diff}}}}` block (which the HTML splicer skips for \
         badge emission). Add a non-diff `{{{{#include}}}}` of the source, \
         or extract a snippet, so the badge anchor lands. Broken labels: {unmatched}",
    );

    browser.close().await.expect("close browser");
}

#[tokio::test]
async fn clicking_each_cross_ref_scrolls_target_badge_into_viewport() {
    // End-to-end click-through guard: for every prose-side
    // `a[data-callout-ref]` in the chapter, click it and assert that the
    // target `button[id="callout-LABEL"]` ends up visible in the viewport
    // (the natural in-page anchor-jump behaviour). Catches the case where
    // the structural attributes line up (covered by the sweep tests
    // above) but the link doesn't actually navigate — e.g. the target
    // anchor is on an off-screen element with `display: none`, or the
    // chapter-internal hash routing was broken by a future theme change.
    //
    // The rebuild discipline for cross-chapter refs is out of scope:
    // chapter prose only references callouts in listings rendered in the
    // same chapter, by design.
    let chapter_html = chapter_path();
    let url = format!("file://{}", chapter_html.display());

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    page.goto(&url, None).await.expect("goto chapter");

    // Collect every cross-ref label first, in document order.
    let labels_csv: String = page
        .evaluate_value(
            r#"(() => Array.from(document.querySelectorAll('a[data-callout-ref]'))
                .map(a => a.getAttribute('data-callout-ref'))
                .filter(Boolean)
                .join(','))()"#,
        )
        .await
        .expect("collect cross-ref labels");
    let labels: Vec<&str> = labels_csv.split(',').filter(|s| !s.is_empty()).collect();
    assert!(
        !labels.is_empty(),
        "expected at least one a[data-callout-ref] in the chapter for click-through coverage",
    );

    let mut failures: Vec<String> = Vec::new();
    for label in &labels {
        // Reset the URL hash so each navigation is a fresh jump rather
        // than a no-op when clicking a ref that already happens to point
        // at the current hash.
        let _: String = page
            .evaluate_value(
                "(() => { history.replaceState(null, '', location.pathname); return 'ok'; })()",
            )
            .await
            .expect("reset hash");

        // Click the ref. Use evaluate to drive the click + scroll
        // synchronously so we don't need a brittle wait-for-scroll dance.
        let after_click: String = page
            .evaluate_value(&format!(
                r##"(() => {{
                    const a = document.querySelector('a[data-callout-ref="{label}"]');
                    if (!a) return 'NO_REF';
                    a.click();
                    const target = document.querySelector('button[id="callout-{label}"]');
                    if (!target) return 'NO_TARGET';
                    // Match the in-page anchor-jump semantics: scroll the
                    // target into view (the click on an `<a href="#...">`
                    // already does this, but force the layout settle).
                    target.scrollIntoView({{ block: 'center' }});
                    const r = target.getBoundingClientRect();
                    const inViewport = r.bottom > 0
                        && r.top < (window.innerHeight || document.documentElement.clientHeight)
                        && r.right > 0
                        && r.left < (window.innerWidth || document.documentElement.clientWidth);
                    const hash = location.hash;
                    return inViewport
                        ? 'OK:' + hash
                        : 'OFFSCREEN:hash=' + hash + ' rect=' + JSON.stringify(r);
                }})()"##
            ))
            .await
            .expect("click cross-ref");

        if !after_click.starts_with("OK:") {
            failures.push(format!("label `{label}`: {after_click}"));
        } else {
            let expected_hash = format!("#callout-{label}");
            let actual_hash = after_click.trim_start_matches("OK:");
            if actual_hash != expected_hash {
                failures.push(format!(
                    "label `{label}`: hash after click was `{actual_hash}` but expected `{expected_hash}`"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "click-through navigation failed for {} of {} cross-ref(s):\n  - {}",
        failures.len(),
        labels.len(),
        failures.join("\n  - "),
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
