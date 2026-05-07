/* mdbook-listings — calibrate `--callout-line-px` on every overlay.
 *
 * The overlay's CSS positioning formula needs the pre's actual
 * per-line pixel height to put each badge on the line that previously
 * held its `// CALLOUT:` marker. mdbook's pre uses
 * `line-height: normal` (~18px for monospace at 16px) but the
 * overlay's em-based fallback in CSS computes ~21px, drifting badges
 * 3px per line above their intended row. For a 600-line diff the
 * cumulative drift pulls badges ~1800px above where they should be —
 * landing inside whatever sibling pre happens to occupy that range.
 *
 * Measure the pre once at runtime and write the actual per-line
 * pixel value as a CSS custom property on the overlay; CSS picks it
 * up via `var(--callout-line-px, ...)`.
 *
 * Sentinel string used by unit tests to confirm the bundled bytes
 * are the expected build-time asset: mdbook-listings-js-v1
 */
(function () {
  function calibrate() {
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
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', calibrate);
  } else {
    calibrate();
  }
})();
