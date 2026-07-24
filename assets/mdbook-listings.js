/* mdbook-listings — runtime layout helpers for the callout overlay.
 *
 * Two things this script does:
 *
 * 1. Calibrate `--callout-line-px` on every overlay so the badge sits
 *    on the line that previously held its `// CALLOUT:` marker.
 *    mdbook's pre uses `line-height: normal` (~18px for monospace at
 *    16px); the overlay's em-based CSS fallback computes ~21px and
 *    drifts badges 3px per line above their intended row. For a
 *    600-line diff the cumulative drift pulls badges ~1800px above
 *    where they should be — landing inside a sibling pre. Measuring
 *    the pre's actual per-line height once and writing it as a CSS
 *    custom property on the overlay keeps every badge in place
 *    regardless of theme or font.
 *
 * 2. Pick a popover side (left vs right) and clamp its max-width to
 *    fit the available right-side gutter. The CSS defaults to opening
 *    the popover into the un-annotated gutter on the RIGHT of the
 *    listing (ch.6 slice 3). On narrow viewports that gutter can be
 *    too small to host even a usable popover — instead of spilling
 *    off the viewport's right edge (or under the scroll container's
 *    scrollbar), this script flips the popover back to the LEFT
 *    (over the listing) when the gutter is below the threshold.
 *    Between the threshold and the default max-width, the script
 *    clamps max-width so the popover's right edge stays inside the
 *    visible area.
 *
 *    The "visible area" is bounded by the popover's nearest scrolling
 *    ancestor — in mdbook's default theme that's `.content`
 *    (`overflow-y: auto`), NOT `<html>`. `documentElement.clientWidth`
 *    returns the full viewport width because the document doesn't
 *    scroll, so a popover sized against it gets its right edge tucked
 *    under `.content`'s scrollbar. Walking up to the scroll container
 *    and using `(container.left + container.clientWidth)` gets the
 *    right edge of the visible area in viewport coords.
 *
 *    Width / side decisions are applied as DIRECT inline styles on
 *    the body element (`body.style.maxWidth`, `body.style.left`, etc.)
 *    — no CSS-variable or class-toggle indirection. The earlier
 *    var-based approach silently failed in some browser contexts
 *    (setProperty without throwing, getPropertyValue returning empty)
 *    and the symptom was identical to "JS never ran." Direct
 *    inline-style writes are unconditional.
 *
 *    Tunables:
 *      - LEFT_FALLBACK_THRESHOLD_EM (16em ≈ 256px): below this
 *        available-gutter value, flip to left-opening.
 *      - DEFAULT_MAX_WIDTH_EM (28em ≈ 448px): the CSS max-width.
 *        Clamped to `availableRight - GUTTER_BUFFER_EM` when the
 *        gutter is between the threshold and this value.
 *      - GUTTER_BUFFER_EM (1em): margin between the clamped popover's
 *        right edge and the scroll container's right edge.
 *
 *    Runs on DOMContentLoaded and on `requestAnimationFrame` after
 *    every resize event, so dragging the window edge updates the
 *    side/clamp choice live.
 *
 * Sentinel string used by unit tests to confirm the bundled bytes
 * are the expected build-time asset: mdbook-listings-js-v8
 */
(function () {
  var LEFT_FALLBACK_THRESHOLD_EM = 16;
  var DEFAULT_MAX_WIDTH_EM = 28;
  // 2em buffer between the clamped popover's right edge and the
  // scroll container's right edge. 1em wasn't enough on all OS /
  // browser scrollbar widths — the popover sat right against the
  // scrollbar and its own right border / box-shadow visually merged
  // with it.
  var GUTTER_BUFFER_EM = 2;

  function calibrateLineHeights() {
    document.querySelectorAll('.callout-overlay').forEach(function (overlay) {
      var pre = overlay.previousElementSibling;
      if (!pre || pre.tagName !== 'PRE') return;
      var entry = overlay.querySelector('.callout-entry');
      if (!entry) return;
      var lines = parseInt(
        entry.style.getPropertyValue('--callout-listing-lines') || '0',
        10
      );
      if (lines <= 0) return;
      var perLine = pre.getBoundingClientRect().height / lines;
      overlay.style.setProperty('--callout-line-px', perLine + 'px');
    });
  }

  // Walk up to the nearest scrolling ancestor. mdbook's scrollbar
  // is on `.content` (`overflow-y: auto`), not on `<html>`, so
  // `documentElement.clientWidth` would return the full viewport
  // width — a popover sized against it would tuck its right edge
  // under `.content`'s scrollbar.
  function findScrollContainer(elem) {
    var parent = elem.parentElement;
    while (parent && parent !== document.body) {
      var overflowY = getComputedStyle(parent).overflowY;
      if (overflowY === 'auto' || overflowY === 'scroll') {
        return parent;
      }
      parent = parent.parentElement;
    }
    return document.documentElement;
  }

  function adjustPopoverPositioning() {
    document.querySelectorAll('.callout-entry').forEach(function (entry) {
      var body = entry.querySelector('.callout-body');
      if (!body) return;

      // `em` for non-font properties resolves against the ELEMENT'S OWN
      // font-size. The popover has `font-size: 0.9em` and mdbook uses
      // `html { font-size: 62.5% }`, so the popover's resolved font-size
      // (~14.4px) differs from documentElement's (~10px). Use the popover's
      // em so the threshold/max-width/offset values match the CSS.
      var bodyEmPx = parseFloat(getComputedStyle(body).fontSize) || 16;
      // When the popover opens LEFT it must clear the badge. The badge is
      // pinned to the entry's right edge, so the body's `right` offset has to
      // exceed the badge's own width plus a small gap — a fixed inset would
      // overlap a wide listing-scoped badge (e.g. `5.18.2`) even though it
      // cleared a bare ordinal. Measured here so it tracks the actual badge.
      var badgeEl = entry.querySelector('.callout-badge');
      var badgeWidth = badgeEl ? badgeEl.getBoundingClientRect().width : 0;
      var leftOpenRightPx = badgeWidth + 0.5 * bodyEmPx;

      // Per-callout author override (ch.6 slice 4): a `--align=left`
      // option on the CALLOUT marker surfaces as `data-callout-align`
      // on the entry. Pin the popover to that side regardless of
      // available gutter — the author has signalled the right gutter
      // isn't usable for THIS specific callout (sidebar, narrow
      // viewport, badge near the page edge, etc.).
      var authorAlign = entry.dataset.calloutAlign;
      if (authorAlign === 'left') {
        body.style.left = 'auto';
        body.style.right = leftOpenRightPx + 'px';
        body.style.maxWidth = '';
        entry.classList.add('callout-entry--left-popover');
        entry.dataset.calloutPopoverDecision = 'author-left';
        return;
      }
      if (authorAlign === 'right') {
        entry.classList.remove('callout-entry--left-popover');
        body.style.left = '';
        body.style.right = '';
        body.style.maxWidth = '';
        entry.dataset.calloutPopoverDecision = 'author-right';
        return;
      }

      var thresholdPx = LEFT_FALLBACK_THRESHOLD_EM * bodyEmPx;
      var maxWidthPx = DEFAULT_MAX_WIDTH_EM * bodyEmPx;
      var bufferPx = GUTTER_BUFFER_EM * bodyEmPx;

      var entryRect = entry.getBoundingClientRect();
      var container = findScrollContainer(entry);
      var containerRect = container.getBoundingClientRect();
      // Right edge of the scroll container's visible area (excludes
      // the scrollbar). For mdbook this is `.content`'s inner right.
      var usableRight = containerRect.left + container.clientWidth;
      var availableRight = usableRight - entryRect.right;

      // Observable per-entry marker for devtools diagnostics.
      var decision;
      if (availableRight < thresholdPx) {
        decision = 'flip-left';
        // Drive the clamp / flip via direct inline-style writes on
        // `.style.maxWidth`, `.left`, `.right` — not via CSS custom
        // properties. An earlier attempt that toggled
        // `--callout-body-max-width` silently no-op'd in some browser
        // contexts (the `setProperty` call returned without throwing,
        // but immediate `getPropertyValue` read back empty), looking
        // identical to "JS never ran." Direct property writes on the
        // element's `style` object are unconditional.
        body.style.left = 'auto';
        body.style.right = leftOpenRightPx + 'px';
        body.style.maxWidth = '';
        entry.classList.add('callout-entry--left-popover');
      } else {
        entry.classList.remove('callout-entry--left-popover');
        body.style.left = '';
        body.style.right = '';
        if (availableRight - bufferPx < maxWidthPx) {
          decision = 'clamp-' + Math.round(availableRight - bufferPx) + 'px';
          body.style.maxWidth = (availableRight - bufferPx) + 'px';
        } else {
          decision = 'wide';
          body.style.maxWidth = '';
        }
      }
      entry.dataset.calloutPopoverDecision = decision;
    });
  }

  function recalc() {
    // Observable marker — bumps every time recalc fires. Devtools
    // diagnostic can read `window.__mdbookListingsRecalcs` to confirm
    // the script ran (and how many times). Without this marker, a
    // failed recalc looks identical to "the script didn't load."
    window.__mdbookListingsRecalcs = (window.__mdbookListingsRecalcs || 0) + 1;
    calibrateLineHeights();
    adjustPopoverPositioning();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', recalc);
  } else {
    recalc();
  }

  // requestAnimationFrame-debounced resize handler: coalesces rapid
  // resize events (e.g., during a window drag) into one recalc per
  // animation frame, but fires by the next frame instead of waiting
  // a fixed timeout. The frame-based pacing also makes the recalc
  // visible to e2e tests that hover immediately after set_viewport_size
  // (one rAF cycle is much shorter than a setTimeout poll).
  var rafScheduled = false;
  window.addEventListener('resize', function () {
    if (rafScheduled) return;
    rafScheduled = true;
    requestAnimationFrame(function () {
      rafScheduled = false;
      recalc();
    });
  });
})();

/* mdbook-listings — sidebar "List of Listings" (Phase 2: append).
 *
 * Reads the inline JSON manifest the preprocessor emits on every page
 * (`<script id="mdbook-listings-manifest" data-sidebar="…">`) and, in
 * "append" mode, adds a self-contained "Listings" section to the sidebar
 * nav below mdbook's own table of contents. Each entry links to a listing's
 * caption anchor (`path#id`).
 *
 * The section is inserted as a sibling of `<mdbook-sidebar-scrollbox>`, NOT
 * inside it: mdbook's `toc.js` populates the scrollbox by assigning its
 * `innerHTML` in a custom-element `connectedCallback`, which would wipe
 * anything appended inside it depending on script order. A sibling under
 * `<nav id="mdbook-sidebar">` (before the resize handle) survives that and
 * needs no knowledge of the nav tree — that independence is the whole point
 * of the "append" rung. The richer "nested" mode (Phase 3) is a separate
 * path that does match the nav tree.
 *
 * Sentinel (paired with the -js-v8 bump above): mdbook-listings-sidebar-v8
 */
(function () {
  var SECTION_ID = 'mdbook-listings-sidebar';

  function readManifest() {
    var el = document.getElementById('mdbook-listings-manifest');
    if (!el) return null;
    var mode = el.dataset.sidebar || 'off';
    if (mode === 'off') return null;
    var data;
    try {
      data = JSON.parse(el.textContent);
    } catch (e) {
      return null;
    }
    if (!Array.isArray(data) || data.length === 0) return null;
    return { mode: mode, chapters: data };
  }

  function buildSection(chapters) {
    var section = document.createElement('section');
    section.id = SECTION_ID;
    section.className = 'mdbook-listings-sidebar';
    var heading = document.createElement('div');
    heading.className = 'mdbook-listings-sidebar-heading';
    heading.textContent = 'Listings';
    section.appendChild(heading);
    var ol = document.createElement('ol');
    ol.className = 'mdbook-listings-sidebar-list';
    chapters.forEach(function (ch) {
      (ch.listings || []).forEach(function (l) {
        var li = document.createElement('li');
        var a = document.createElement('a');
        a.href = ch.path + '#' + l.id;
        // The number is plain digits/dots; the caption is already
        // HTML-escaped upstream, so innerHTML restores its intended text
        // (e.g. `&amp;` -> `&`) rather than showing the entities.
        var label = 'Listing ' + l.number;
        if (l.caption) label += ' — ' + l.caption;
        a.innerHTML = label;
        li.appendChild(a);
        ol.appendChild(li);
      });
    });
    section.appendChild(ol);
    return section;
  }

  function appendSidebar() {
    if (document.getElementById(SECTION_ID)) return; // idempotent
    var manifest = readManifest();
    if (!manifest) return;
    // Phase 2 owns "append"; "nested" is Phase 3's job.
    if (manifest.mode !== 'append') return;
    var nav = document.getElementById('mdbook-sidebar');
    if (!nav) return;
    var section = buildSection(manifest.chapters);
    var resize = document.getElementById('mdbook-sidebar-resize-handle');
    if (resize && resize.parentNode === nav) {
      nav.insertBefore(section, resize);
    } else {
      nav.appendChild(section);
    }
    // Observable marker: e2e/devtools can read `window.__mdbookListingsSidebar`
    // to confirm the block was built (and in which mode).
    window.__mdbookListingsSidebar = manifest.mode;
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', appendSidebar);
  } else {
    appendSidebar();
  }
})();
