use playwright_rs::{expect, locator};

mod common;
// CALLOUT: harness-import Pulls in the shared per-test e2e harness (tests/common/e2e_harness.rs) — every test in this file goes through `with_traced_chapter`, so per-test Playwright launch + trace recording + tracing_subscriber init all live in one place.
use common::e2e_harness::with_traced_chapter;

const CH05: &str = "ch05-render-inline-callouts";

#[tokio::test]
async fn label_only_callout_renders_badge_without_following_body() {
    // CALLOUT: harness-call Canonical call shape. The harness opens a per-test BrowserContext, navigates to the chapter HTML, starts a Playwright trace, runs the closure body with the resulting Page, and on panic saves the trace to target/playwright-traces/<name>.zip + prints a failed-action summary parsed via playwright-rs-trace.
    with_traced_chapter(
        "label_only_callout_renders_badge_without_following_body",
        CH05,
        |page| async move {
            let badge = page.locator(locator!("button#callout-cli-parse")).await;
            expect(badge)
                .to_have_count(1)
                .await
                .expect("label-only badge button must exist");
            let body = page.locator(locator!("#callout-body-cli-parse")).await;
            expect(body)
                .to_have_count(0)
                .await
                .expect("label-only callout must not have a body popover");
        },
    )
    .await;
}

#[tokio::test]
async fn callout_badge_renders_with_data_attribute_in_ch04() {
    with_traced_chapter(
        "callout_badge_renders_with_data_attribute_in_ch04",
        CH05,
        |page| async move {
            let badges = page.locator(locator!("[data-callout-badge]")).await;
            let count = badges.count().await.expect("count badges");
            assert!(count > 0, "expected at least one [data-callout-badge]; got 0");
            let text = badges.first().text_content().await.expect("badge text");
            assert!(
                text.as_deref().is_some_and(|s| !s.trim().is_empty()),
                "expected first badge text to be non-empty; got {text:?}",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn callout_cross_ref_renders_as_anchor_to_listing_badge() {
    with_traced_chapter(
        "callout_cross_ref_renders_as_anchor_to_listing_badge",
        CH05,
        |page| async move {
            let cross_ref = page
                .locator(locator!(r#"a[data-callout-ref="cross-ref-emit"]"#))
                .await;
            expect(cross_ref)
                .to_have_attribute("href", "#callout-cross-ref-emit")
                .await
                .expect("cross-ref href must point at listing badge anchor");
            let target = page
                .locator(locator!("button#callout-cross-ref-emit"))
                .await;
            expect(target)
                .to_have_count(1)
                .await
                .expect("listing-side badge button must exist as the cross-ref's target");
        },
    )
    .await;
}

#[tokio::test]
async fn callout_marker_comment_is_stripped_and_body_reveals_on_hover() {
    with_traced_chapter(
        "callout_marker_comment_is_stripped_and_body_reveals_on_hover",
        CH05,
        |page| async move {
            // Find the <pre> whose sibling overlay carries the cross-ref-emit
            // badge (xpath does the sibling traversal that CSS can't). The
            // splicer should have stripped the literal marker comment from
            // that pre's text.
            let pre = page
                .locator(locator!(
                    r#"xpath=//pre[following-sibling::div[1][.//button[@id="callout-cross-ref-emit"]]]"#
                ))
                .await;
            expect(pre.clone())
                .not()
                .to_contain_text("CALLOUT: cross-ref-emit")
                .await
                .expect("marker comment line must be stripped from the include's <pre>");

            // Body popover starts hidden and becomes visible after hovering its
            // triggering badge.
            let badge = page
                .locator(locator!("button#callout-cross-ref-emit"))
                .await;
            badge.hover(None).await.expect("hover badge");
            let body = page.locator(locator!("#callout-body-cross-ref-emit")).await;
            expect(body)
                .to_be_visible()
                .await
                .expect("body popover must become visible after hovering its badge");
        },
    )
    .await;
}

#[tokio::test]
async fn every_callout_cross_ref_resolves_to_a_badge_with_matching_ordinal_and_text() {
    // Sweep guard for prose-side cross-refs: every `{{#callout LABEL}}`
    // directive renders as an `<a class="callout-badge callout-ref"
    // href="#callout-LABEL" data-callout-ref="LABEL"
    // data-callout-ordinal="N">N</a>`. For each one we verify (via
    // playwright assertions, not JS-string sweeps) that:
    // 1. `href` matches `#callout-<data-callout-ref>`
    // 2. A `button[id="callout-LABEL"]` exists as the target
    // 3. The ref's `data-callout-ordinal` matches the target badge's
    // 4. The rendered text on the ref matches the target badge's text
    with_traced_chapter(
        "every_callout_cross_ref_resolves_to_a_badge_with_matching_ordinal_and_text",
        CH05,
        |page| async move {
            let refs = page.locator(locator!("a[data-callout-ref]")).await;
            let count = refs.count().await.expect("count refs");
            assert!(
                count > 0,
                "expected at least one a[data-callout-ref] in chapter"
            );

            for i in 0..count {
                let r = refs.nth(i as i32);
                let label = r
                    .get_attribute("data-callout-ref")
                    .await
                    .expect("ref label")
                    .unwrap_or_default();
                assert!(!label.is_empty(), "ref #{i} has empty data-callout-ref");

                let expected_href = format!("#callout-{label}");
                expect(r.clone())
                    .to_have_attribute("href", &expected_href)
                    .await
                    .unwrap_or_else(|e| panic!("ref `{label}`: href mismatch: {e:?}"));

                let target = page
                    .locator(&format!(r#"button[id="callout-{label}"]"#))
                    .await;
                expect(target.clone())
                    .to_have_count(1)
                    .await
                    .unwrap_or_else(|e| panic!("ref `{label}`: target badge missing: {e:?}"));

                let ref_ordinal = r
                    .get_attribute("data-callout-ordinal")
                    .await
                    .expect("ref ordinal")
                    .unwrap_or_default();
                expect(target.clone())
                    .to_have_attribute("data-callout-ordinal", &ref_ordinal)
                    .await
                    .unwrap_or_else(|e| {
                        panic!("ref `{label}`: ordinal mismatch (ref={ref_ordinal}): {e:?}")
                    });

                let ref_text = r
                    .text_content()
                    .await
                    .expect("ref text")
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                expect(target)
                    .to_have_text(&ref_text)
                    .await
                    .unwrap_or_else(|e| {
                        panic!("ref `{label}`: rendered text mismatch (ref=\"{ref_text}\"): {e:?}")
                    });
            }
        },
    )
    .await;
}

#[tokio::test]
async fn every_cross_refed_label_has_a_visible_badge_in_the_chapter() {
    // Regression guard, scoped to labels the author actually points at:
    // every `{{#callout LABEL}}` directive must have a corresponding
    // `button[id="callout-LABEL"]` somewhere in the rendered page.
    with_traced_chapter(
        "every_cross_refed_label_has_a_visible_badge_in_the_chapter",
        CH05,
        |page| async move {
            let refs = page.locator(locator!("a[data-callout-ref]")).await;
            let count = refs.count().await.expect("count refs");

            let mut missing: Vec<String> = Vec::new();
            for i in 0..count {
                let label = refs
                    .nth(i as i32)
                    .get_attribute("data-callout-ref")
                    .await
                    .expect("ref label")
                    .unwrap_or_default();
                if label.is_empty() {
                    continue;
                }
                let target = page
                    .locator(&format!(r#"button[id="callout-{label}"]"#))
                    .await;
                if target.count().await.expect("count target") == 0 {
                    missing.push(label);
                }
            }
            missing.sort();
            missing.dedup();

            assert!(
                missing.is_empty(),
                "the following labels are cross-refed in chapter prose but have no \
                 `button[id=\"callout-LABEL\"]` target. Broken labels: {}",
                missing.join(", "),
            );
        },
    )
    .await;
}

#[tokio::test]
async fn clicking_each_cross_ref_scrolls_target_badge_into_viewport() {
    // End-to-end click-through guard: for every prose-side
    // `a[data-callout-ref]`, click it and assert the target badge ends
    // up visible (the natural in-page anchor-jump behaviour).
    with_traced_chapter(
        "clicking_each_cross_ref_scrolls_target_badge_into_viewport",
        CH05,
        |page| async move {
            let refs = page.locator(locator!("a[data-callout-ref]")).await;
            let count = refs.count().await.expect("count refs");
            assert!(
                count > 0,
                "expected at least one cross-ref for click-through coverage"
            );

            let mut labels: Vec<String> = Vec::with_capacity(count);
            for i in 0..count {
                if let Some(label) = refs
                    .nth(i as i32)
                    .get_attribute("data-callout-ref")
                    .await
                    .expect("ref label")
                    && !label.is_empty()
                {
                    labels.push(label);
                }
            }

            let mut failures: Vec<String> = Vec::new();
            for label in &labels {
                // CALLOUT: clear-url-fragment Reset the URL hash so each click is a fresh navigation rather than a no-op when the current hash already matches. The typed `Page::clear_url_fragment()` shipped upstream as `padamson/playwright-rust@401be500` in response to padamson/playwright-rust#89 — eliminates the last JS string from the entire e2e suite.
                page.clear_url_fragment().await.expect("reset hash");

                let r = page
                    .locator(&format!(r#"a[data-callout-ref="{label}"]"#))
                    .await
                    .first();
                if let Err(e) = r.click(None).await {
                    failures.push(format!("label `{label}`: click failed: {e:?}"));
                    continue;
                }

                let target = page
                    .locator(&format!(r#"button[id="callout-{label}"]"#))
                    .await
                    .first();
                if let Err(e) = target.scroll_into_view_if_needed().await {
                    failures.push(format!("label `{label}`: scroll failed: {e:?}"));
                    continue;
                }
                if !target.is_visible().await.unwrap_or(false) {
                    failures.push(format!("label `{label}`: target not visible after click"));
                    continue;
                }

                let actual_hash: String = page
                    .evaluate_value("location.hash")
                    .await
                    .expect("read hash");
                let expected_hash = format!("#callout-{label}");
                if actual_hash != expected_hash {
                    failures.push(format!(
                        "label `{label}`: hash after click was `{actual_hash}` but expected `{expected_hash}`"
                    ));
                }
            }

            assert!(
                failures.is_empty(),
                "click-through navigation failed for {} of {} cross-ref(s):\n  - {}",
                failures.len(),
                labels.len(),
                failures.join("\n  - "),
            );
        },
    )
    .await;
}
