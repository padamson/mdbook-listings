use playwright_rs::protocol::{BoundingBox, Locator, Viewport};
use playwright_rs::{expect, locator};

mod common;
// CALLOUT: harness-import Pulls in the shared per-test e2e harness (tests/common/e2e_harness.rs) — every test in this file goes through `with_traced_chapter`, so per-test Playwright launch + trace recording + tracing_subscriber init all live in one place.
use common::e2e_harness::with_traced_chapter;

const CH05: &str = "ch05-render-inline-callouts";
const CH06: &str = "ch06-dogfooding-polish";

/// The rendered box of a locator's first match. `Locator::bounding_box` is
/// a plain query with no auto-wait, and it answers `None` both for a
/// selector that matches nothing and for an element that is not rendered;
/// a geometry test can measure neither, and the two need different fixes,
/// so the panic says which it was.
async fn bounding_box(locator: &Locator) -> BoundingBox {
    match locator.bounding_box().await.expect("bounding box") {
        Some(rendered) => rendered,
        None => {
            let matches = locator.count().await.expect("count matches");
            panic!(
                "cannot measure {locator:?}: {}",
                if matches == 0 {
                    "no element matches".to_string()
                } else {
                    format!("{matches} match(es), none rendered")
                }
            )
        }
    }
}

/// `raf2()`: a promise for two animation frames, the one settle every
/// layout wait in this file uses. The first frame lets a `resize` or event
/// listener run and schedule the badge recalc; the second lets the recalc's
/// class and CSS-variable changes land in layout. `recalc` itself runs
/// synchronously inside one frame, so two is the whole wait.
const RAF2_JS: &str = r#"
    const raf2 = () => new Promise(r =>
      requestAnimationFrame(() => requestAnimationFrame(r)));
"#;

/// `lineRect(pre, line)`: the box of a listing line's first character,
/// found by walking the pre's text nodes and counting newlines. The ground
/// truth for where a line sits, wrapped or not, shared by the alignment
/// sweeps that inject it.
const LINE_RECT_JS: &str = r#"
    function lineRect(pre, line) {
      const walker = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
      let remaining = line - 1;
      let pending = false; // line starts at next non-empty node
      let node;
      while ((node = walker.nextNode())) {
        const text = node.nodeValue;
        let idx = 0;
        if (pending) {
          if (text.length === 0) continue;
          pending = false;
        } else {
          while (remaining > 0) {
            const nl = text.indexOf('\n', idx);
            if (nl === -1) break;
            idx = nl + 1;
            remaining--;
          }
          if (remaining > 0) continue;
          if (idx >= text.length) { pending = true; continue; }
        }
        const r = document.createRange();
        r.setStart(node, idx);
        r.setEnd(node, Math.min(idx + 1, text.length));
        return r.getBoundingClientRect();
      }
      return null;
    }
"#;

/// `survey(lineRectFn)`: every badge against the line it annotates,
/// measured by `lineRectFn(pre, line)`. A badge's vertical centre must fall
/// inside its line's box, 2px tolerance. Every entry counts: an overlay
/// with no pre, an entry with no line or badge, and a line the measurer
/// cannot find are all reported, not stepped over, so a regression in the
/// emitter cannot shrink the sweep into a pass.
const SURVEY_JS: &str = r#"
    function survey(lineRectFn) {
      const bad = [];
      let checked = 0;
      document.querySelectorAll('.callout-overlay').forEach((ov, i) => {
        const pre = ov.previousElementSibling;
        if (!pre || pre.tagName !== 'PRE') {
          bad.push('overlay#' + i + ': no <pre> sibling');
          return;
        }
        ov.querySelectorAll('.callout-entry').forEach(e => {
          const line = parseInt(e.dataset.calloutLine, 10);
          const badge = e.querySelector('.callout-badge');
          checked++;
          if (!line || !badge) {
            bad.push('overlay#' + i + ' entry ' + (e.dataset.calloutLine || '?') +
              ': ' + (badge ? 'no data-callout-line' : 'no badge'));
            return;
          }
          const lr = lineRectFn(pre, line);
          if (!lr || lr.height === 0) {
            bad.push(badge.id + ': line ' + line + ' not found in its <pre>');
            return;
          }
          const br = badge.getBoundingClientRect();
          const c = (br.top + br.bottom) / 2;
          if (c < lr.top - 2 || c > lr.bottom + 2) {
            bad.push(badge.id + ': center=' + Math.round(c) +
              ' line=[' + Math.round(lr.top) + ',' + Math.round(lr.bottom) + ']');
          }
        });
      });
      return { checked, bad };
    }
"#;

/// Wait for the page to lay out and the popover-positioning JS to re-run
/// after a viewport change; see [`RAF2_JS`] for why two frames.
async fn wait_for_layout_recalc(page: &playwright_rs::protocol::Page) {
    let script = [
        "(async () => {",
        RAF2_JS,
        "await raf2(); return 'done'; })()",
    ]
    .concat();
    let _: String = page
        .evaluate_value(&script)
        .await
        .expect("wait for layout recalc");
}

#[tokio::test]
async fn label_only_callout_renders_badge_without_following_body() {
    // CALLOUT: harness-call Canonical call shape. The harness opens a per-test BrowserContext, navigates to the chapter HTML, starts a Playwright trace, runs the closure body with the resulting Page, and on panic saves the trace to target/playwright-traces/<name>.zip + prints a failed-action summary parsed via playwright-rs-trace.
    with_traced_chapter(
        "label_only_callout_renders_badge_without_following_body",
        CH05,
        |page| async move {
            let badge = page.locator(locator!("button#callout-cli-parse"));
            expect(badge)
                .to_have_count(1)
                .await
                .expect("label-only badge button must exist");
            let body = page.locator(locator!("#callout-body-cli-parse"));
            expect(body)
                .to_have_count(0)
                .await
                .expect("label-only callout must not have a body popover");
        },
    )
    .await;
}

#[tokio::test]
async fn callout_badge_renders_with_data_attribute_in_ch05() {
    with_traced_chapter(
        "callout_badge_renders_with_data_attribute_in_ch05",
        CH05,
        |page| async move {
            let badges = page.locator(locator!("[data-callout-badge]"));
            let count = badges.count().await.expect("count badges");
            assert!(
                count > 0,
                "expected at least one [data-callout-badge]; got 0"
            );
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
            let cross_ref = page.locator(locator!(r#"a[data-callout-ref="cross-ref-emit"]"#));
            expect(cross_ref)
                .to_have_attribute("href", "#callout-cross-ref-emit")
                .await
                .expect("cross-ref href must point at listing badge anchor");
            let target = page.locator(locator!("button#callout-cross-ref-emit"));
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
            let pre = page.locator(locator!(
                r#"xpath=//pre[following-sibling::div[1][.//button[@id="callout-cross-ref-emit"]]]"#
            ));
            expect(pre.clone())
                .not()
                .to_contain_text("CALLOUT: cross-ref-emit")
                .await
                .expect("marker comment line must be stripped from the include's <pre>");

            // Body popover starts hidden and becomes visible after hovering its
            // triggering badge.
            let badge = page.locator(locator!("button#callout-cross-ref-emit"));
            badge.hover(None).await.expect("hover badge");
            let body = page.locator(locator!("#callout-body-cross-ref-emit"));
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
            let refs = page.locator(locator!("a[data-callout-ref]"));
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
                    .unwrap_or_else(|| panic!("ref #{i} has no data-callout-ref"));
                assert!(!label.is_empty(), "ref #{i} has empty data-callout-ref");

                let expected_href = format!("#callout-{label}");
                expect(r.clone())
                    .to_have_attribute("href", &expected_href)
                    .await
                    .unwrap_or_else(|e| panic!("ref `{label}`: href mismatch: {e:?}"));

                let target = page.locator(format!(r#"button[id="callout-{label}"]"#));
                expect(target.clone())
                    .to_have_count(1)
                    .await
                    .unwrap_or_else(|e| panic!("ref `{label}`: target badge missing: {e:?}"));

                let ref_ordinal = r
                    .get_attribute("data-callout-ordinal")
                    .await
                    .expect("ref ordinal")
                    .unwrap_or_else(|| panic!("ref `{label}` has no data-callout-ordinal"));
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
                    .unwrap_or_else(|| panic!("ref `{label}` has no text"))
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
            let refs = page.locator(locator!("a[data-callout-ref]"));
            let count = refs.count().await.expect("count refs");

            let mut missing: Vec<String> = Vec::new();
            for i in 0..count {
                let label = refs
                    .nth(i as i32)
                    .get_attribute("data-callout-ref")
                    .await
                    .expect("ref label")
                    .unwrap_or_else(|| panic!("ref #{i} has no data-callout-ref"));
                assert!(!label.is_empty(), "ref #{i} has empty data-callout-ref");
                let target = page.locator(format!(r#"button[id="callout-{label}"]"#));
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
            let refs = page.locator(locator!("a[data-callout-ref]"));
            let count = refs.count().await.expect("count refs");
            assert!(
                count > 0,
                "expected at least one cross-ref for click-through coverage"
            );

            let mut labels: Vec<String> = Vec::with_capacity(count);
            for i in 0..count {
                let label = refs
                    .nth(i as i32)
                    .get_attribute("data-callout-ref")
                    .await
                    .expect("ref label")
                    .unwrap_or_else(|| panic!("ref #{i} has no data-callout-ref"));
                assert!(!label.is_empty(), "ref #{i} has empty data-callout-ref");
                labels.push(label);
            }

            let mut failures: Vec<String> = Vec::new();
            for label in &labels {
                // CALLOUT: clear-url-fragment Reset the URL hash so each click is a fresh navigation rather than a no-op when the current hash already matches. The typed `Page::clear_url_fragment()` shipped upstream as `padamson/playwright-rust@401be500` in response to padamson/playwright-rust#89 — eliminates the last JS string from the entire e2e suite.
                page.clear_url_fragment().await.expect("reset hash");

                let r = page
                    .locator(format!(r#"a[data-callout-ref="{label}"]"#))
                    .first();
                if let Err(e) = r.click(None).await {
                    failures.push(format!("label `{label}`: click failed: {e:?}"));
                    continue;
                }

                let target = page
                    .locator(format!(r#"button[id="callout-{label}"]"#))
                    .first();
                if let Err(e) = target.scroll_into_view_if_needed().await {
                    failures.push(format!("label `{label}`: scroll failed: {e:?}"));
                    continue;
                }
                // A point-in-time check on purpose: the jump has happened by
                // now, and a target that only turns up after a polling wait
                // would be the bug.
                match target.is_visible().await {
                    Ok(true) => {}
                    Ok(false) => {
                        failures.push(format!("label `{label}`: target not visible after click"));
                        continue;
                    }
                    Err(e) => {
                        failures.push(format!("label `{label}`: visibility check failed: {e:?}"));
                        continue;
                    }
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

#[tokio::test]
async fn cross_ref_badges_in_prose_render_with_full_opacity_not_subdued() {
    // Regression guard: a bare-anchor listing badge (label-only marker
    // with no body popover) is styled muted/dashed via
    // `.callout-entry .callout-badge:only-child`. Pre-fix that rule was
    // unscoped (`.callout-badge:only-child`) and matched every cross-ref
    // <a> in chapter prose — they're typically the only ELEMENT child
    // of their <p> parent (text nodes don't count for :only-child), so
    // every inline cross-ref ended up muted/dashed. The scoping fix
    // requires the badge to live inside a `.callout-entry` overlay
    // before muting kicks in.
    with_traced_chapter(
        "cross_ref_badges_in_prose_render_with_full_opacity_not_subdued",
        CH05,
        |page| async move {
            let cross_ref = page
                .locator(locator!("a.callout-badge.callout-ref"))
                .first();
            expect(cross_ref).to_have_css("opacity", "1").await.expect(
                "cross-ref badge in prose should have full opacity; subdued styling means \
                     the .callout-entry scope on `:only-child` regressed",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn callout_inside_a_sliced_include_renders_with_resolvable_cross_ref() {
    // Slice 9 demo: the chapter slices `include-line-ranges-v1.rs:73:96`
    // and the slice carries a `// CALLOUT: include-range-cross-ref-resolves`
    // marker. Verify the full pipeline end-to-end: the badge button has
    // the expected id, and the prose-side `{{#callout ...}}` cross-ref
    // resolves to that id.
    with_traced_chapter(
        "callout_inside_a_sliced_include_renders_with_resolvable_cross_ref",
        CH05,
        |page| async move {
            let badge = page.locator(locator!("button#callout-include-range-cross-ref-resolves"));
            expect(badge)
                .to_have_count(1)
                .await
                .expect("badge for callout inside sliced include must exist");
            let cross_ref = page.locator(locator!(
                r#"a[data-callout-ref="include-range-cross-ref-resolves"]"#
            ));
            expect(cross_ref)
                .to_have_attribute("href", "#callout-include-range-cross-ref-resolves")
                .await
                .expect("cross-ref href must point at the badge anchor");
        },
    )
    .await;
}

#[tokio::test]
async fn every_badge_renders_inside_its_owning_pre() {
    // Regression guard for the long-diff badge mispositioning bug:
    // each callout badge must visually land within the y-range of the
    // <pre> it belongs to (the one immediately preceding its
    // .callout-overlay parent). Pre-fix, badges in long diffs drifted
    // ~3px per line above their intended row because the overlay's
    // assumed line-height (1.5em at 0.875em font = 21px) didn't match
    // the pre's rendered line-height (`normal` ~ 18px for monospace).
    // For a 600-line diff that compounds to ~1800px, landing badges
    // inside the wrong sibling pre.
    with_traced_chapter(
        "every_badge_renders_inside_its_owning_pre",
        CH05,
        |page| async move {
            // For each .callout-overlay, its sibling <pre> and every
            // .callout-badge inside: each badge's y sits within the pre's
            // y-range. Both boxes are viewport-relative, so they compare.
            let overlays = page.locator(locator!(".callout-overlay"));
            let overlay_count = overlays.count().await.expect("count overlays");
            assert!(overlay_count > 0, "expected at least one .callout-overlay");
            let mut failures: Vec<String> = Vec::new();
            for i in 0..overlay_count {
                let overlay = overlays.nth(i as i32);
                let pre = overlay.locator("xpath=preceding-sibling::*[1][self::pre]");
                let Some(pre_box) = pre.bounding_box().await.expect("pre box") else {
                    failures.push(format!("overlay #{i}: no rendered <pre> sibling"));
                    continue;
                };
                let (top, bottom) = (pre_box.y, pre_box.y + pre_box.height);
                let badges = overlay.locator(".callout-badge");
                let badge_count = badges.count().await.expect("count badges");
                for j in 0..badge_count {
                    let badge = badges.nth(j as i32);
                    let verdict = match badge.bounding_box().await.expect("badge box") {
                        None => Some("not rendered".to_string()),
                        Some(b) if b.y < top - 2.0 || b.y > bottom + 2.0 => {
                            Some(format!("y={:.0} pre=[{top:.0}..{bottom:.0}]", b.y))
                        }
                        Some(_) => None,
                    };
                    if let Some(problem) = verdict {
                        let id = badge
                            .get_attribute("id")
                            .await
                            .expect("badge id")
                            .unwrap_or_default();
                        failures.push(format!("overlay #{i} badge `{id}`: {problem}"));
                    }
                }
            }
            assert!(
                failures.is_empty(),
                "badges rendered outside their owning <pre>:\n{}",
                failures.join("\n")
            );
        },
    )
    .await;
}

#[tokio::test]
async fn callout_body_renders_inline_backticks_as_code_spans() {
    // ch.6 slice 1: a callout body that contains inline backticks must
    // render the wrapped span as <code>, not as literal punctuation.
    // The `snippets-intercept` callout in listings/include-v1.rs has
    // four backtick spans (`listings/`, `snippets/`, `CALLOUT:`,
    // `links`); asserting one <code> with the right text is enough to
    // confirm the inline-markdown render path is wired up end-to-end.
    // The body popover starts hidden — `to_have_text` uses innerText,
    // which respects visibility, so we hover the badge first.
    with_traced_chapter(
        "callout_body_renders_inline_backticks_as_code_spans",
        CH05,
        |page| async move {
            let badge = page.locator(locator!("button#callout-snippets-intercept"));
            badge.hover(None).await.expect("hover badge to reveal body");
            let body = page.locator(locator!("#callout-body-snippets-intercept"));
            expect(body.clone())
                .to_be_visible()
                .await
                .expect("body popover must be visible after hover");
            let code = body.locator("code").first();
            expect(code)
                .to_have_text("listings/")
                .await
                .expect("first <code> in body must be the rendered `listings/` backtick span");
        },
    )
    .await;
}

#[tokio::test]
async fn callout_body_opens_to_the_right_of_its_badge_on_wide_viewports() {
    // ch.6 slice 3: on a viewport wide enough to leave a usable right
    // gutter (≥ the JS threshold, 16em ≈ 256px), the popover defaults
    // to opening into the un-annotated gutter on the RIGHT of the badge
    // — never covering the line it annotates. Pre-slice it opened left
    // and sat on top of the listing — defeating the inline-callout
    // point. The contract is a layout assertion: body.left >= badge.right
    // (modulo a 1px tolerance for subpixel rounding). Narrow-viewport
    // fallback (flip to left when the gutter is too narrow) and the
    // mid-viewport max-width clamp have their own tests below.
    with_traced_chapter(
        "callout_body_opens_to_the_right_of_its_badge_on_wide_viewports",
        CH05,
        |page| async move {
            // 1800x800: comfortably above the threshold for the right
            // gutter to host the popover at its full max-width.
            page.set_viewport_size(Viewport {
                width: 1800,
                height: 800,
            })
            .await
            .expect("set wide viewport");
            wait_for_layout_recalc(&page).await;
            let badge = page.locator(locator!("button#callout-snippets-intercept"));
            badge.hover(None).await.expect("hover badge to reveal body");
            // Confirm the body is laid out before measuring (clip-path
            // animation has finished and the box has its target width).
            let body = page.locator(locator!("#callout-body-snippets-intercept"));
            expect(body.clone())
                .to_be_visible()
                .await
                .expect("body popover must be visible after hover");

            let badge_box = bounding_box(&badge).await;
            let body_box = bounding_box(&body).await;
            let (body_left, badge_right) = (body_box.x, badge_box.x + badge_box.width);
            assert!(
                body_left + 1.0 >= badge_right,
                "popover must open to the right of its badge, not over the line it annotates; \
                 body.left={body_left:.1} badge.right={badge_right:.1}",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn callout_body_falls_back_to_left_opening_when_right_gutter_is_too_narrow() {
    // ch.6 slice 3 viewport-aware behavior: when the right-side gutter
    // between the listing's right edge and the viewport's right edge
    // is too narrow to host even a usable popover (below the JS
    // threshold, currently 16em ≈ 256px), the JS flips the popover
    // back to the LEFT side. The reader sees the popover cover the
    // listing — accepted as the lesser evil vs. a popover that spills
    // off the viewport entirely and can't be read.
    with_traced_chapter(
        "callout_body_falls_back_to_left_opening_when_right_gutter_is_too_narrow",
        CH05,
        |page| async move {
            // 900x800: chapter content fills most of the viewport, the
            // right gutter shrinks below the JS threshold (16em ≈ 256px)
            // but stays above the mobile-layout breakpoint where the
            // sidebar would slide off and the badge would become
            // unreachable to a hover.
            page.set_viewport_size(Viewport {
                width: 900,
                height: 800,
            })
            .await
            .expect("set narrow viewport");
            wait_for_layout_recalc(&page).await;
            let badge = page.locator(locator!("button#callout-snippets-intercept"));
            badge
                .scroll_into_view_if_needed()
                .await
                .expect("scroll badge into view");
            badge.hover(None).await.expect("hover badge");
            let body = page.locator(locator!("#callout-body-snippets-intercept"));
            expect(body.clone())
                .to_be_visible()
                .await
                .expect("body must be visible after hover");

            let badge_box = bounding_box(&badge).await;
            let body_box = bounding_box(&body).await;
            let (body_right, badge_left) = (body_box.x + body_box.width, badge_box.x);
            assert!(
                body_right <= badge_left + 1.0,
                "popover must fall back to opening left when the right gutter is too narrow; \
                 body.right={body_right:.1} badge.left={badge_left:.1}",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn callout_body_never_overflows_the_viewport_horizontally() {
    // ch.6 slice 3 viewport-aware behavior: at intermediate viewport
    // widths (right gutter exists but is smaller than the popover's
    // default `max-width: 28em`), the JS clamps `max-width` so the
    // popover's right edge stays inside the viewport rather than
    // spilling off-screen. The contract is universal: regardless of
    // which side the popover opens on, body.right must never exceed
    // window.innerWidth.
    with_traced_chapter(
        "callout_body_never_overflows_the_viewport_horizontally",
        CH05,
        |page| async move {
            // 1024x800: a typical mid-size viewport. mdbook content
            // takes most of the column; the right gutter is small but
            // non-zero. Either the JS clamps to fit OR flips left —
            // either way, body must stay on-screen.
            page.set_viewport_size(Viewport {
                width: 1024,
                height: 800,
            })
            .await
            .expect("set mid viewport");
            wait_for_layout_recalc(&page).await;
            let badge = page.locator(locator!("button#callout-snippets-intercept"));
            badge.hover(None).await.expect("hover badge");
            let body = page.locator(locator!("#callout-body-snippets-intercept"));
            expect(body.clone())
                .to_be_visible()
                .await
                .expect("body must be visible after hover");

            // The right edge the popover must stay inside is the scroll
            // container's VISIBLE width, excluding its scrollbar: mdbook's
            // scrollbar lives on `.content`, not on the document, and
            // measuring against `window.innerWidth` lets the popover hide
            // under it. Only that edge is computed in the page.
            let usable_right: f64 = page
                .evaluate_value(
                    r#"(() => {
                      const body = document.querySelector('#callout-body-snippets-intercept');
                      let p = body.parentElement;
                      while (p && p !== document.body) {
                        const oy = getComputedStyle(p).overflowY;
                        if (oy === 'auto' || oy === 'scroll') break;
                        p = p.parentElement;
                      }
                      const container = (p && p !== document.body) ? p : document.documentElement;
                      return String(container.getBoundingClientRect().left + container.clientWidth);
                    })()"#,
                )
                .await
                .expect("measure the scroll container's visible right edge")
                .parse()
                .expect("a pixel value");
            let body_box = bounding_box(&body).await;
            let body_right = body_box.x + body_box.width;
            assert!(
                body_right <= usable_right + 1.0,
                "popover overflows the visible area or sits under the scrollbar (clamp or flip \
                 not applied); body.right={body_right:.1} usableRight={usable_right:.1}",
            );
            assert!(
                body_box.x >= -1.0,
                "popover overflows the left viewport edge; body.left={:.1}",
                body_box.x
            );
        },
    )
    .await;
}

#[tokio::test]
async fn callout_with_align_left_option_pins_popover_left_even_on_wide_viewport() {
    // ch.6 slice 4: the `--align=left` per-callout option in the
    // CALLOUT marker (e.g. `// CALLOUT: lbl --align=left Body.`)
    // surfaces as `data-callout-align="left"` on the entry. The JS
    // short-circuits viewport-aware detection and pins the popover
    // to the left (over the listing) regardless of available right
    // gutter. The fixture is a snippet in ch.6's narrative carrying
    // a marker that uses the option.
    with_traced_chapter(
        "callout_with_align_left_option_pins_popover_left_even_on_wide_viewport",
        CH06,
        |page| async move {
            // 1800×800: viewport plenty wide enough for right-opening
            // at full max-width. The override must beat the default.
            page.set_viewport_size(Viewport {
                width: 1800,
                height: 800,
            })
            .await
            .expect("set wide viewport");
            wait_for_layout_recalc(&page).await;

            let badge = page.locator(locator!("button#callout-align-left-demo"));
            badge
                .scroll_into_view_if_needed()
                .await
                .expect("scroll badge into view");
            badge.hover(None).await.expect("hover badge");
            let body = page.locator(locator!("#callout-body-align-left-demo"));
            expect(body.clone())
                .to_be_visible()
                .await
                .expect("body popover must be visible after hover");

            let aligned_entry = page.locator(locator!(
                r#".callout-entry[data-callout-align="left"] button#callout-align-left-demo"#
            ));
            expect(aligned_entry)
                .to_have_count(1)
                .await
                .expect("the --align=left option must surface as data-callout-align on the entry");
            let badge_box = bounding_box(&badge).await;
            let body_box = bounding_box(&body).await;
            let (body_right, badge_left) = (body_box.x + body_box.width, badge_box.x);
            assert!(
                body_right <= badge_left + 1.0,
                "popover must pin left despite the wide viewport; \
                 body.right={body_right:.1} badge.left={badge_left:.1}",
            );
        },
    )
    .await;
}

#[tokio::test]
async fn sidecar_callout_renders_alongside_inline_marker_in_same_listing() {
    // ch.6 slice 9: the `callout-v9` listing in ch.6 carries an inline
    // `// CALLOUT: parse-entry` marker AND two sidecar entries
    // (`parse-line-entry`, `label-validity-check`) attached via
    // `book/src/listings/callout-v9.callouts.toml`. All three badges
    // must render against the same listing's overlay — proves that
    // inline + sidecar callouts compose end-to-end through the chapter
    // pipeline (include splicer → callout splicer → HTML renderer).
    with_traced_chapter(
        "sidecar_callout_renders_alongside_inline_marker_in_same_listing",
        CH06,
        |page| async move {
            // Each badge label rendered separately so a failure
            // diagnostic names the specific missing badge. Selector and
            // panic message use positional `{}` rather than named
            // `{label}` interpolation so the typst-pdf markdown→typst
            // converter doesn't misparse the raw-string `{...}` shape
            // when this test file gets included as a `{{#diff}}` in the
            // chapter narrative.
            let labels = ["parse-entry", "parse-line-entry", "label-validity-check"];
            for label in labels {
                let selector = format!("button[data-callout-badge=\"{}\"]", label);
                let badge = page.locator(&selector);
                expect(badge).to_have_count(1).await.unwrap_or_else(|_| {
                    panic!(
                        "badge with label '{}' must render exactly once in ch.6",
                        label
                    )
                });
            }
        },
    )
    .await;
}

// List of Listings — sidebar (nested). The book dogfoods
// `list-of-listings-sidebar = "nested"`, so mdbook-listings.js pairs each of
// the current page's listing anchors with its enclosing heading and hangs it
// under that heading's node in mdbook's per-page header tree. Driven against
// the built book like every callout test above.
#[tokio::test]
async fn list_of_listings_sidebar_nests_entries_under_page_headings() {
    with_traced_chapter(
        "list_of_listings_sidebar_nests_entries_under_page_headings",
        CH05,
        |page| async move {
            // mdbook builds the active page's header tree at runtime and our
            // observer nests once it appears; `to_be_visible` auto-waits. A
            // listing entry must sit inside a header node (`.header-item`) of
            // the real nav tree — i.e. under its section, not flat under the
            // chapter.
            let under_heading = page
                .locator(locator!(
                    r##".header-item .mdbook-listings-nav-item a[href^="#listing-"]"##
                ))
                .first();
            expect(under_heading)
                .to_be_visible()
                .await
                .expect("a listing entry must nest under a page heading in the nav tree");

            // Page-local: the sidebar lists this page's listings only, not the
            // whole book. ch05's listing count in the nav matches the page's
            // own `listing-N-M` caption anchors.
            let nav_entries = page.locator(locator!(".mdbook-listings-nav-item"));
            let nav_count = nav_entries.count().await.expect("count nav entries");
            let page_listings = page
                .locator(locator!(r#"main .listing-caption[id^="listing-"]"#))
                .count()
                .await
                .expect("count page listing anchors");
            assert_eq!(
                nav_count, page_listings,
                "sidebar should nest exactly this page's listings",
            );

            // Nested mode does not build the standalone append block.
            let append_block = page.locator(locator!("#mdbook-listings-sidebar"));
            expect(append_block)
                .to_have_count(0)
                .await
                .expect("nested mode must not also render the append section");
        },
    )
    .await;
}

// The append rung, exercised on a page the book builds in "nested" mode by
// rewriting the marker and rebuilding through the script's test seam. A book
// is built in one sidebar mode, so without this the un-dogfooded rung has no
// page to run on — which is how it shipped rendering as an overlay across
// mdbook's table of contents.
#[tokio::test]
async fn list_of_listings_sidebar_append_flows_below_the_nav_tree() {
    with_traced_chapter(
        "list_of_listings_sidebar_append_flows_below_the_nav_tree",
        CH05,
        |page| async move {
            // Swap the marker to append mode, carrying a manifest built from
            // the page's own listings, then rebuild.
            let built: String = page
                .evaluate_value(
                    r#"(() => {
                        const el = document.getElementById('mdbook-listings-manifest');
                        if (!el) return 'no-marker';
                        const items = [...document.querySelectorAll('main .listing-caption[id^="listing-"]')]
                            .map(d => ({ number: d.id.replace('listing-', '').replace('-', '.'),
                                         caption: null, id: d.id }));
                        if (!items.length) return 'no-listings';
                        document.querySelectorAll('.mdbook-listings-nav').forEach(n => n.remove());
                        el.dataset.sidebar = 'append';
                        el.textContent = JSON.stringify([
                            { name: 'Ch', path: location.pathname.split('/').pop(), listings: items }
                        ]);
                        window.__mdbookListingsSidebarBuild();
                        return String(items.length);
                    })()"#,
                )
                .await
                .expect("rebuild sidebar in append mode");
            assert!(
                built.parse::<u32>().is_ok(),
                "expected a listing count, got `{built}`"
            );

            let section = page.locator(locator!("#mdbook-listings-sidebar"));
            expect(section.clone())
                .to_be_visible()
                .await
                .expect("append section must render");

            // The defect this guards: the section used to be a normal-flow
            // sibling of the absolutely-positioned `.sidebar-scrollbox`, so it
            // painted on top of the nav tree instead of after it. Compare
            // rendered boxes rather than DOM position; the DOM looked fine
            // while the layout was broken.
            let section_box = bounding_box(&section).await;
            let tree_box = bounding_box(&page.locator(locator!("#mdbook-sidebar ol.chapter"))).await;
            assert!(
                section_box.height > 0.0 && section_box.width > 0.0,
                "append section collapsed to {}x{}",
                section_box.width,
                section_box.height
            );
            let (section_top, tree_bottom) = (section_box.y, tree_box.y + tree_box.height);
            assert!(
                section_top >= tree_bottom - 1.0,
                "append section must flow below the nav tree, not over it; \
                 section.top={section_top:.0} tree.bottom={tree_bottom:.0}",
            );
        },
    )
    .await;
}

// Hover recolours the entry's text; it must not paint a block behind it (and
// must never make the text match its own background, which rendered the entry
// as a solid unreadable bar).
#[tokio::test]
async fn list_of_listings_sidebar_hover_recolours_text_without_a_block() {
    with_traced_chapter(
        "list_of_listings_sidebar_hover_recolours_text_without_a_block",
        CH05,
        |page| async move {
            let entry = page
                .locator(locator!(".mdbook-listings-nav-item a"))
                .first();
            expect(entry.clone())
                .to_be_visible()
                .await
                .expect("a nested listing entry must be present");
            entry.hover(None).await.expect("hover the entry");

            // Chromium reports `transparent` as rgba(0, 0, 0, 0).
            expect(entry.clone())
                .to_have_css("background-color", "rgba(0, 0, 0, 0)")
                .await
                .expect("hover must recolour the text only, with no background block");
            expect(entry)
                .not()
                .to_have_css("color", "rgba(0, 0, 0, 0)")
                .await
                .expect("hovered text must stay visible, not match its transparent background");
        },
    )
    .await;
}

// Soft-wrap safety: badge placement must track the real line boxes, not
// `index × average_row_height`. The average-math approach desyncs every
// badge below a wrapped line (the average inflates), which is what forced
// downstream books to choose between horizontal scroll and hand-folding
// their sources. This test turns on `pre-wrap` at a width that wraps many
// lines, then checks each badge sits on the line it annotates.
#[tokio::test]
async fn callout_badges_stay_on_their_lines_when_listing_soft_wraps() {
    with_traced_chapter(
        "callout_badges_stay_on_their_lines_when_listing_soft_wraps",
        CH05,
        |page| async move {
            let _: String = page
                .evaluate_value(
                    r#"(() => {
                        const style = document.createElement('style');
                        style.textContent =
                          'main pre, main pre code { white-space: pre-wrap !important; word-break: break-all; } ' +
                          'main pre { max-width: 60ch; }';
                        document.head.appendChild(style);
                        window.dispatchEvent(new Event('resize'));
                        return 'ok';
                    })()"#,
                )
                .await
                .expect("inject wrap css");
            wait_for_layout_recalc(&page).await;

            // A wrapped target line anchors to its first visual row, which
            // is where `lineRect` measures.
            let script = [
                "(() => {",
                LINE_RECT_JS,
                SURVEY_JS,
                r#"
                    const s = survey(lineRect);
                    if (s.checked === 0) return 'no-entries-checked';
                    return s.bad.length
                      ? 'MISALIGNED(' + s.bad.length + '/' + s.checked + ') ' + s.bad.slice(0, 4).join('; ')
                      : 'aligned:' + s.checked;
                })()"#,
            ]
            .concat();
            let report: String = page
                .evaluate_value(&script)
                .await
                .expect("measure badge-vs-line alignment");
            assert!(
                report.starts_with("aligned:"),
                "every badge must sit on its target line under soft-wrap; got: {report}"
            );
        },
    )
    .await;
}

// Font-timing safety: badge positions are pixel-pinned from measurements, so
// anything that reflows the pre after the initial placement — above all the
// async code-font swap on a cold cache — must trigger a re-measure. This
// simulates the swap by shifting the pre's font metrics without firing a
// resize (exactly what @font-face activation does), then fires the
// FontFaceSet's `loadingdone` event and expects the badges to re-anchor.
#[tokio::test]
async fn callout_badges_realign_after_late_font_load() {
    with_traced_chapter(
        "callout_badges_realign_after_late_font_load",
        CH05,
        |page| async move {
            let script = [
                "(async () => {",
                RAF2_JS,
                LINE_RECT_JS,
                SURVEY_JS,
                r#"
                    // Reflow the pres the way a late font activation does:
                    // metrics change, no resize event.
                    const style = document.createElement('style');
                    style.textContent =
                      'main pre, main pre code { font-size: 19px !important; }';
                    document.head.appendChild(style);
                    await raf2();
                    const before = survey(lineRect);
                    if (before.checked === 0) return 'no-entries';
                    if (before.bad.length === 0) return 'perturbation-had-no-effect';

                    // The production signal that fonts finished loading.
                    document.fonts.dispatchEvent(new Event('loadingdone'));
                    await raf2();
                    const after = survey(lineRect);
                    return after.bad.length === 0
                      ? 'realigned:' + after.checked
                      : 'STILL-STALE(' + after.bad.length + '/' + after.checked + ') ' + after.bad.slice(0, 4).join('; ');
                })()"#,
            ]
            .concat();
            let outcome: String = page
                .evaluate_value(&script)
                .await
                .expect("perturb, signal loadingdone, survey alignment");
            assert!(
                outcome.starts_with("realigned:"),
                "badges must re-anchor when the font set finishes loading; got: {outcome}"
            );
        },
    )
    .await;
}

// Engine-tolerance: release Safari returns a two-line union rect for a Range
// over the whitespace character at a line boundary in `white-space: pre`
// content (its top is the PREVIOUS line's top, height two rows), where
// Chromium and newer WebKit return the plain one-row glyph box. Anchoring to
// that rect put every badge one line high in Safari — uniformly, since YAML
// and Rust lines all start with indentation. This test emulates Safari's
// semantics in Chromium by patching Range.getBoundingClientRect for
// boundary-whitespace ranges (the survey keeps the unpatched original), then
// expects badges to land correctly anyway — which requires the placement to
// measure a visible glyph, not boundary whitespace.
#[tokio::test]
async fn callout_badges_place_correctly_under_safari_boundary_rect_semantics() {
    with_traced_chapter(
        "callout_badges_place_correctly_under_safari_boundary_rect_semantics",
        CH05,
        |page| async move {
            let script = [
                "(async () => {",
                RAF2_JS,
                SURVEY_JS,
                r#"
                        const orig = Range.prototype.getBoundingClientRect;
                        Range.prototype.getBoundingClientRect = function () {
                          const r = orig.call(this);
                          const s = this.toString();
                          if (s === ' ' || s === '\t') {
                            const c = this.startContainer, o = this.startOffset;
                            const boundary = o > 0
                              ? c.nodeValue.charAt(o - 1) === '\n'
                              : true; // cross-node line starts
                            if (boundary) {
                              return new DOMRect(r.x, r.y - r.height, r.width, r.height * 2);
                            }
                          }
                          return r;
                        };

                        window.dispatchEvent(new Event('resize'));
                        await raf2();

                        // Like lineRect, but measures the line's first VISIBLE
                        // glyph with the UNPATCHED rect, so the survey's ground
                        // truth is not itself subject to the emulated bug.
                        function lineRectTrue(pre, line) {
                          const walker = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
                          let remaining = line - 1, pending = false, node;
                          while ((node = walker.nextNode())) {
                            let text = node.nodeValue, idx = 0;
                            if (pending) { if (!text.length) continue; pending = false; }
                            else {
                              while (remaining > 0) {
                                const nl = text.indexOf('\n', idx);
                                if (nl === -1) break;
                                idx = nl + 1; remaining--;
                              }
                              if (remaining > 0) continue;
                              if (idx >= text.length) { pending = true; continue; }
                            }
                            // survey against the line's first visible glyph,
                            // measured with the UNPATCHED rect.
                            while (true) {
                              while (idx < text.length && (text[idx] === ' ' || text[idx] === '\t')) idx++;
                              if (idx < text.length) break;
                              node = walker.nextNode(); if (!node) return null;
                              text = node.nodeValue; idx = 0;
                            }
                            if (text[idx] === '\n') return null;
                            const r = document.createRange();
                            r.setStart(node, idx); r.setEnd(node, idx + 1);
                            return orig.call(r);
                          }
                          return null;
                        }
                        const s = survey(lineRectTrue);
                        Range.prototype.getBoundingClientRect = orig;
                        if (s.checked === 0) return 'no-entries';
                        return s.bad.length === 0
                          ? 'aligned:' + s.checked
                          : 'MISALIGNED(' + s.bad.length + '/' + s.checked + ') ' + s.bad.slice(0, 4).join('; ');
                })()"#,
            ]
            .concat();
            let outcome: String = page
                .evaluate_value(&script)
                .await
                .expect("emulate safari rects, survey alignment");
            assert!(
                outcome.starts_with("aligned:"),
                "badges must place correctly under Safari's boundary-whitespace rect semantics; got: {outcome}"
            );
        },
    )
    .await;
}

// Stable listing cross-references: the errata page carries a live
// `listing-ref` to the labelled acceptance-tests listing in ch03. The
// rendered artifact must be a link whose text is the target's *current*
// number and whose href points at the matching caption anchor — the pair is
// derived, so this stays green when ch03's numbering shifts.
#[tokio::test]
async fn listing_ref_renders_as_link_to_current_number() {
    with_traced_chapter(
        "listing_ref_renders_as_link_to_current_number",
        "reading-this-book",
        |page| async move {
            let link = page
                .locator(locator!(
                    r#"main a[href*="ch03-freeze-a-listing.html#listing-"]"#
                ))
                .first();
            let href = link
                .get_attribute("href")
                .await
                .expect("href")
                .expect("a listing-ref link carries an href");
            let id = href
                .split('#')
                .nth(1)
                .unwrap_or_else(|| panic!("listing-ref href has no fragment: {href}"));
            // `listing-3-2` names `Listing 3.2`: the first dash separates the
            // chapter from the ordinal.
            let expected = format!(
                "Listing {}",
                id.trim_start_matches("listing-").replacen('-', ".", 1)
            );
            expect(link)
                .to_have_text(&expected)
                .await
                .expect("listing-ref must render the target's current number as its link text");
        },
    )
    .await;
}
