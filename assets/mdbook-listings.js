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
 * are the expected build-time asset: mdbook-listings-js-v12
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

  // The rendered box of logical line `line` (1-based) inside `pre`: walk the
  // text nodes counting newlines to the line's first character and measure a
  // Range around it. Measuring through the text nodes makes the result exact
  // regardless of soft-wrap (a wrapped line anchors to its first visual row)
  // and of highlight.js having fragmented the code into spans. Returns null
  // when the line can't be found (empty pre, out-of-range line).
  function lineStartRect(pre, line) {
    var walker = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
    var remaining = line - 1;
    var pending = false; // line starts at the next node with characters
    var node;
    while ((node = walker.nextNode())) {
      var text = node.nodeValue;
      var idx = 0;
      if (pending) {
        if (text.length === 0) continue;
        pending = false;
      } else {
        while (remaining > 0) {
          var nl = text.indexOf('\n', idx);
          if (nl === -1) break;
          idx = nl + 1;
          remaining--;
        }
        if (remaining > 0) continue;
        if (idx >= text.length) {
          pending = true;
          continue;
        }
      }
      var range = document.createRange();
      range.setStart(node, idx);
      range.setEnd(node, Math.min(idx + 1, text.length));
      return range.getBoundingClientRect();
    }
    return null;
  }

  // Position each callout entry on the measured box of its target line, and
  // refresh each overlay's average-row-height variable as the fallback for
  // entries the measurement can't resolve (and for no-JS, via the em-based
  // CSS default — exact only while no line soft-wraps).
  //
  // Per-line anchoring is what makes soft-wrap safe: the average-height
  // scheme assumes one visual row per logical line, so the moment any line
  // wraps the average inflates and every badge below the wrap drifts onto
  // the wrong line — which made `pre-wrap` unusable on calloutted listings.
  //
  // Structured as one global read phase then one write phase: any style
  // write between two measurements invalidates layout, and per-entry
  // write→measure cycles force a full page reflow per badge — seconds of
  // jank on a long chapter. Batched, the browser lays out once.
  function calibrateLineHeights() {
    var calibrations = [];
    var placements = [];
    document.querySelectorAll('.callout-overlay').forEach(function (overlay) {
      var pre = overlay.previousElementSibling;
      if (!pre || pre.tagName !== 'PRE') return;
      var entries = overlay.querySelectorAll('.callout-entry');
      if (!entries.length) return;

      var lines = parseInt(
        entries[0].style.getPropertyValue('--callout-listing-lines') || '0',
        10
      );
      if (lines > 0) {
        calibrations.push({
          overlay: overlay,
          perLine: pre.getBoundingClientRect().height / lines,
        });
      }

      var overlayTop = overlay.getBoundingClientRect().top;
      entries.forEach(function (entry) {
        var line = parseInt(entry.dataset.calloutLine || '0', 10);
        if (!line) return;
        var rect = lineStartRect(pre, line);
        if (!rect || rect.height === 0) return; // keep the CSS fallback
        placements.push({
          entry: entry,
          top: rect.top - overlayTop,
          height: rect.height,
        });
      });
    });
    calibrations.forEach(function (c) {
      c.overlay.style.setProperty('--callout-line-px', c.perLine + 'px');
    });
    placements.forEach(function (p) {
      p.entry.style.top = p.top + 'px';
      p.entry.style.height = p.height + 'px';
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

/* mdbook-listings — sidebar "List of Listings".
 *
 * The preprocessor emits a marker on every page,
 * `<script id="mdbook-listings-manifest" data-sidebar="…">`, whose mode picks
 * one of two rungs:
 *
 *   "append"  — a self-contained "Listings" section added below mdbook's
 *               table of contents, listing every numbered listing book-wide.
 *               The marker's body carries that index as JSON. The section is
 *               inserted as a sibling of `<mdbook-sidebar-scrollbox>` (before
 *               the resize handle), NOT inside it: mdbook's `toc.js` fills the
 *               scrollbox by assigning its `innerHTML` in a custom-element
 *               `connectedCallback`, which would wipe anything appended inside
 *               it. Staying a sibling survives that and needs no knowledge of
 *               the nav tree — that independence is the point of this rung.
 *
 *   "nested"  — each listing is placed in mdbook's own per-page header tree,
 *               under the heading it lives beneath, on the current page only.
 *               mdbook's `toc.js` renders the active page's `h2`–`h6` as
 *               `<a class="header-in-summary" href="#slug">` nodes and folds
 *               them; we read the page's `.listing-caption[id^="listing-"]`
 *               anchors, pair each with its nearest preceding heading, and
 *               hang it under that heading's node. So a listing shows only
 *               when you're on its page and its section is expanded, and
 *               collapses with the heading — the marker body is empty because
 *               everything is read from the rendered page. Because the header
 *               tree is built at runtime, we observe the scrollbox until it
 *               appears. This rung is coupled to the default theme's DOM.
 *
 * Sentinel (paired with the -js-v11 bump above): mdbook-listings-sidebar-v11
 */
(function () {
  var SECTION_ID = 'mdbook-listings-sidebar';
  var NESTED_CLASS = 'mdbook-listings-nav';

  // The sidebar mode, read off the marker regardless of body (nested's body
  // is empty). Returns null when there is no marker or the sidebar is off.
  function readMode() {
    var el = document.getElementById('mdbook-listings-manifest');
    if (!el) return null;
    var mode = el.dataset.sidebar || 'off';
    if (mode !== 'append' && mode !== 'nested') return null;
    return { el: el, mode: mode };
  }

  function scrollbox() {
    return (
      document.querySelector('mdbook-sidebar-scrollbox') ||
      document.getElementById('mdbook-sidebar-scrollbox')
    );
  }

  // --- "append" rung -----------------------------------------------------

  // One book-wide listing's link. The number is plain digits/dots; the caption
  // is already HTML-escaped upstream, so innerHTML restores its intended text.
  function manifestAnchor(ch, l) {
    var a = document.createElement('a');
    a.href = ch.path + '#' + l.id;
    var label = 'Listing ' + l.number;
    if (l.caption) label += ' — ' + l.caption;
    a.innerHTML = label;
    return a;
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
        li.appendChild(manifestAnchor(ch, l));
        ol.appendChild(li);
      });
    });
    section.appendChild(ol);
    return section;
  }

  // Place the section INSIDE the scrollbox, after mdbook's nav tree.
  //
  // It used to go in as a sibling of the scrollbox, which looked right in the
  // DOM and rendered as an overlay: mdbook's theme gives `.sidebar-scrollbox`
  // `position: absolute; top/bottom/left/right: 0`, so the scrollbox is out of
  // normal flow and covers the whole nav. A normal-flow sibling after it
  // therefore starts at the nav's top-left, under the tree, and the two paint
  // over each other. Inside the scrollbox the section flows after the tree and
  // scrolls with it, which is what "below the table of contents" means.
  //
  // The reason for staying outside originally was that mdbook's `toc.js` fills
  // the scrollbox by assigning `innerHTML`, which would wipe anything already
  // in there. The observer in `startAppend` handles that: if the section is
  // wiped it is simply rebuilt on the next mutation.
  function insertSection(chapters) {
    if (document.getElementById(SECTION_ID)) return true; // idempotent
    var box = scrollbox();
    if (box) {
      box.appendChild(buildSection(chapters));
      window.__mdbookListingsSidebar = 'append';
      return true;
    }
    // No scrollbox (a theme that doesn't use one): fall back to the nav, where
    // normal flow is the whole story and a plain append is correct.
    var nav = document.getElementById('mdbook-sidebar');
    if (!nav) return false;
    var resize = document.getElementById('mdbook-sidebar-resize-handle');
    if (resize && resize.parentNode === nav) {
      nav.insertBefore(buildSection(chapters), resize);
    } else {
      nav.appendChild(buildSection(chapters));
    }
    window.__mdbookListingsSidebar = 'append';
    return true;
  }

  function startAppend(el) {
    var chapters;
    try {
      chapters = JSON.parse(el.textContent);
    } catch (e) {
      return;
    }
    if (!Array.isArray(chapters) || chapters.length === 0) return;
    insertSection(chapters);
    var box = scrollbox();
    if (!box) return;
    // Re-insert if toc.js repopulates the scrollbox and wipes the section.
    var obs = new MutationObserver(function () {
      insertSection(chapters);
    });
    obs.observe(box, { childList: true });
  }

  // --- "nested" rung -----------------------------------------------------

  // The current page's listings grouped by their enclosing heading, in
  // document order. Each group is { headingId, items:[{id, label}] };
  // headingId is null for listings that precede the first heading.
  function pageListingGroups() {
    var root = document.querySelector('main') || document.querySelector('.content');
    if (!root) return [];
    var nodes = root.querySelectorAll(
      'h2, h3, h4, h5, h6, .listing-caption[id^="listing-"]'
    );
    var groups = [];
    var current = null;
    nodes.forEach(function (n) {
      if (n.classList && n.classList.contains('listing-caption')) {
        if (!current) {
          current = { headingId: null, items: [] };
          groups.push(current);
        }
        // textContent (not innerHTML): the caption div already holds the
        // rendered "Listing N.M — caption" label as text.
        current.items.push({ id: n.id, label: (n.textContent || '').trim() });
      } else {
        current = { headingId: n.id, items: [] };
        groups.push(current);
      }
    });
    return groups.filter(function (g) {
      return g.items.length > 0;
    });
  }

  // The <li> of the header-tree node whose anchor targets `#headingId`.
  function headerItem(box, headingId) {
    var links = box.querySelectorAll('a.header-in-summary');
    var target = '#' + headingId;
    for (var i = 0; i < links.length; i++) {
      if (links[i].getAttribute('href') === target) return links[i].closest('li');
    }
    return null;
  }

  // The active chapter's <li>, for listings that precede the first heading.
  function activeChapterItem(box) {
    var active = box.querySelector('.chapter-item a.active') || box.querySelector('a.active');
    return active ? active.closest('li') : null;
  }

  function nestListingList(items) {
    var ol = document.createElement('ol');
    ol.className = NESTED_CLASS;
    items.forEach(function (it) {
      var li = document.createElement('li');
      li.className = 'mdbook-listings-nav-item';
      var a = document.createElement('a');
      a.href = '#' + it.id;
      a.textContent = it.label;
      li.appendChild(a);
      ol.appendChild(li);
    });
    return ol;
  }

  // Hang each group under its heading's node (or the active chapter for the
  // no-heading group). Idempotent per container. Returns true once at least
  // one group landed.
  function nestIntoHeaders(box, groups) {
    var landed = false;
    groups.forEach(function (g) {
      var container = g.headingId ? headerItem(box, g.headingId) : activeChapterItem(box);
      if (!container || container.dataset.mdbookListings) return;
      container.dataset.mdbookListings = '1';
      container.appendChild(nestListingList(g.items));
      landed = true;
    });
    if (landed) window.__mdbookListingsSidebar = 'nested';
    return landed;
  }

  function startNested() {
    var groups = pageListingGroups();
    if (!groups.length) return;
    var needsHeaders = groups.some(function (g) {
      return g.headingId !== null;
    });
    var box = scrollbox();
    if (!box) return;

    function attempt() {
      nestIntoHeaders(box, groups);
      // Done once the header tree exists (all heading nodes are built in one
      // pass), or when no group needs one.
      return !needsHeaders || box.querySelector('a.header-in-summary') !== null;
    }

    if (attempt()) return;
    // mdbook builds the header tree at runtime; watch until it appears.
    var obs = new MutationObserver(function () {
      if (attempt()) obs.disconnect();
    });
    obs.observe(box, { childList: true, subtree: true });
  }

  // --- bootstrap ---------------------------------------------------------

  function buildSidebar() {
    var marker = readMode();
    if (!marker) return;
    if (marker.mode === 'append') {
      startAppend(marker.el);
    } else if (marker.mode === 'nested') {
      startNested();
    }
  }

  // Test seam: a book is built in exactly one sidebar mode, so the mode it
  // does NOT dogfood has no page to exercise it on. Exposing the entry point
  // lets an e2e rewrite the marker and rebuild against a real browser and the
  // real theme CSS — which is where the append rung's layout bugs live.
  window.__mdbookListingsSidebarBuild = buildSidebar;

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', buildSidebar);
  } else {
    buildSidebar();
  }
})();
