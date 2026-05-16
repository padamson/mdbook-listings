# Dogfooding-Driven Polish

```admonish note title="Why this chapter exists"
Chapters 2–5 shipped the v0.1.0 primitives (install, freeze, diff,
callouts). The first real downstream project to take a dependency
on those primitives — the
[t2t](https://github.com/padamson/t2t) book — surfaced a handful of
rendering and ergonomic gaps that the in-house book never exercised
hard enough to notice. This chapter collects the resulting polish
work, one slice per gap. The verify story (ch.7) is still
placeholder; it'll close the v0.1.0 loop separately.

If we identify dogfood, we eat it. New gaps that surface on later
downstream passes get appended as new acceptance criteria and new
slices — there is no "out of scope" exit door.
```

## Story

> As a downstream book author, I want the v0.1.0 primitives to feel
> finished when I write real annotated prose against them — not just
> "the happy path runs to completion," but "the rendered output is
> the output I wrote, and the CLI tells me what I need to know to
> keep going."

## Acceptance criteria

1. **Inline markdown in callout body text.** A callout body that
   contains inline markdown (backticks for code spans, `*emphasis*`,
   `**strong**`, `[text](url)`) renders as the corresponding inline
   HTML in the body popover — not as literal punctuation. Block-level
   markdown (lists, blockquotes, headings) is out of scope: callouts
   are inline annotations. Raw HTML in a callout body renders as
   escaped text, not as pass-through HTML.
2. **Bundled assets refresh on every build, not just at install
   time.** Today `install` writes `mdbook-listings.css` and
   `mdbook-listings.js` into the book source tree as a one-time
   snapshot, then the bytes drift as the binary version moves
   forward — `additional-css`/`additional-js` keep referencing the
   stale on-disk copies until the author manually re-runs `install`.
   The preprocessor — which already runs on every `mdbook build` —
   instead writes the bundled bytes into the book root, refreshing
   them automatically when the binary is upgraded. `install` keeps
   the `book.toml` registration job and adds the two asset paths to
   `.gitignore` so downstream books treat them as build artifacts
   (matches `target/`). Author override works the same way it does
   for any other mdbook stylesheet: drop `theirs.css` into the book
   directory and add `additional-css = ["./theirs.css"]` to
   `book.toml`. mdbook cascades the second `additional-css` entry
   after the first, so author rules win.
3. **Callout popover never covers the line it annotates.** The
   default opens the popover to the right of the badge (the
   un-annotated gutter), an author override switches a specific
   callout to the left for narrow viewports, and a transparent /
   `backdrop-filter: blur` fallback keeps the underlying code legible
   when overlap is unavoidable.
4. **`freeze` output closes the authoring loop.** Every successful
   `freeze` prints the frozen path AND the ready-to-paste
   `\{{#include listings/<tag>.<ext>}}` directive — the author
   shouldn't have to grep `listings.toml` to find the include path.
5. **A `list` (or `status`) subcommand prints `tag → frozen path →
   source` rows** so authors can browse the manifest as a book
   accumulates listings.
6. **`install` is idempotent.** Re-running `install` on an
   already-configured book is a no-op with a friendly "already
   installed" message; never duplicates registrations.
7. **`freeze` derives a default tag when `--tag` is omitted.** The
   default `<basename>-v<next>` removes the "invent your own scheme"
   tax on every first-time author. Already on the v0.2.0 ROADMAP;
   downstream surfaced it as a real pain point, so it lives here.

## The slice — outside-in narrative outline

| Slice | What it adds |
|---|---|
| 1 | Inline markdown in callout body text (AC 1). Downstream dogfooding noticed that backticks around an identifier in a callout body rendered as literal backtick characters rather than a `<code>` span. The fix routes the body through pulldown-cmark's inline parser before wrapping it in the `<div class="callout-body">`, strips the synthetic `<p>` wrapper, and re-applies the `{` → `&#123;` escape for cross-ref-scanner safety. Raw HTML events are remapped to text events so a body containing `<script>` still renders as `&lt;script&gt;`, not as pass-through HTML. |
| 2 | Preprocessor refreshes assets on every build (AC 2). Today `install` writes `mdbook-listings.css` and `mdbook-listings.js` into the book source tree as a one-time snapshot, then the bytes drift as the binary version moves forward — t2t Pass 3 hit this: bumping the locally-installed binary forward without re-running `install` left the rendered book mixing new HTML emission with stale CSS, producing subtle (and sometimes loud) breakage. The slice moves the asset write from `install` to the preprocessor's run hook so the bytes refresh on every build (no-op when bytes already match). `install` keeps the `book.toml` registration job and now also adds the two asset paths to `.gitignore` so downstream books treat them as build artifacts. Migration for existing books: re-run `install`, then `git rm --cached` the two old committed copies. |
| 3 | Open the popover to the right by default (AC 3, fix 1 of 3). CSS-only positioning change on the `<div class="callout-body">` so the natural reading direction (left-to-right) drops the popover into the un-annotated gutter rather than over the line it annotates. |
| 4 | Per-callout `--align` override (AC 3, fix 2 of 3). Tiny extension to the `// CALLOUT: <label>` grammar — `// CALLOUT: <label> --align=left <body>` flips a single callout when the right-side gutter isn't usable (sidebar, narrow viewport, badge near the page edge). The extension is shaped to scale to other per-callout options later (width, theme). |
| 5 | Transparent / `backdrop-filter: blur` fallback (AC 3, fix 3 of 3). Pure CSS. When the popover must cover the listing (narrow viewport, author override, very long body), a translucent background + backdrop blur keeps the underlying code legible behind it. |
| 6 | `freeze` output closes the loop (AC 4). Augments the `created: <tag>` line with the frozen path and the exact `\{{#include listings/<tag>.<ext>}}` directive to copy-paste into the chapter. |
| 7 | `mdbook-listings list` subcommand (AC 5). Prints one row per `[[listing]]` in `listings.toml`: tag, frozen path, source path. No filtering options yet — just the basic catalogue view. |
| 8 | `install` idempotency (AC 6). After slice 2 the only things `install` writes are `book.toml` registrations and the `.gitignore` entries. The first run continues to register the preprocessor + `additional-css`/`additional-js` and to add the asset paths to `.gitignore`. A second run detects everything already present and prints "already installed" with no writes. |
| 9 | Default tag derivation (AC 7). When `--tag` is omitted, derive `<basename>-v<next>` by reading existing `[[listing]]` entries for the same source path and bumping the highest `vN` suffix. Surfaces a clean error if any existing tag for the same source doesn't match the `<basename>-vN` shape (the heuristic is opinionated; an author who's invented their own scheme keeps using `--tag` explicitly). |

## Outside-in narrative

Sections appear here as slices ship. Slices 1–3 have shipped;
slices 4–9 are sketched in the table above.

### Slice 1 — inline markdown in callout body text

The symptom: a callout body whose author reached for inline
backticks — say, to call out a name like `PORT` — rendered to the
popover with the literal backtick characters intact instead of a
`<code>` span around the name. Annotated technical prose leans on
inline-code formatting to distinguish identifiers from prose; a
callout body that can't render inline code reads worse than the
surrounding chapter, which defeats the whole point of attaching
context to a specific line.

The diff is between the two frozen snapshots of `src/callout.rs`
that bracket this slice — `callout-v6` (the last freeze, made when
ch.5 wrapped) and `callout-v7` (frozen as part of this slice). It's
the full file diff: there's no freeze between them. Two earlier
commits modified `callout.rs` without refreezing, so their changes
show up here too: the e2e-harness refactor rescoped the
`splice_chapter_html_escapes_label_and_body` test assertion, and
ch.5's slice 9 added the `in_inline_backticks` check near the top
of `replace_callout_refs` plus the `// CALLOUT: html-escape`
comment and `.replace('{', "&#123;")` line on `html_escape`. This
slice's contribution is the call-site swap (line 640 of v7), the
new `render_inline_markdown` function just below `html_escape`,
and the unit tests at the bottom.

{{#diff callout-v6 callout-v7}}

Three details inside `render_inline_markdown` earn their own
callout: {{#callout raw-html-neutralisation}} guards against
untrusted HTML in source comments; {{#callout inline-only-output}}
explains the `<p>` strip and what happens if an author reaches for
block markdown anyway; {{#callout curly-brace-escape}} preserves
the cross-ref-scanner safety property the original `html_escape`
provided.

The PDF path needs no change. `render_callout_list_pdf` interpolates
the body into a markdown blockquote that typst-pdf re-parses, so
markdown in the body has always rendered correctly in print — the
gap was HTML-only.

Tests added in this slice:

- `callout_body_renders_inline_backticks_as_code_spans` — backticks
  → `<code>`.
- `callout_body_renders_strong_and_emphasis` — `**bold**` and
  `*italic*` → `<strong>` and `<em>`.
- `callout_body_renders_inline_link` — `[docs](https://example.com/)`
  → `<a href>`.
- `callout_body_curly_brace_escape_survives_inside_code_span` —
  the cross-ref-scanner safety property holds inside a `<code>` span.
- `callout_body_plain_text_passes_through_unchanged` — the synthetic
  `<p>` wrapper is stripped on plain bodies.
- The pre-existing `splice_chapter_html_escapes_label_and_body`
  guards the raw-HTML neutralisation (it asserts `<script>` →
  `&lt;script&gt;`).

A new e2e assertion in `tests/e2e_callouts.rs` —
`callout_body_renders_inline_backticks_as_code_spans` — closes the
loop end-to-end: it hovers the `snippets-intercept` badge in the
rendered ch.5 HTML and asserts that the popover contains a `<code>`
element with the expected text.

The diff between `e2e-callouts-v8` (last freeze, made when ch.5
wrapped) and `e2e-callouts-v9` (frozen as part of this slice) shows
the new test plus a couple of mechanical changes that came with
this commit's chapter renumbering — `CH04` was renamed to `CH05`
and its value bumped to `"ch05-render-inline-callouts"`. Ch.5
slice 9 also modified this file without refreezing (the
`callout_inside_a_sliced_include_renders_with_resolvable_cross_ref`
and `cross_ref_badges_in_prose_render_with_full_opacity_not_subdued`
tests), so those appear in the diff too.

{{#diff e2e-callouts-v8 e2e-callouts-v9}}

### Slice 2 — preprocessor refreshes assets on every build

The symptom: a downstream book installs `mdbook-listings` once, runs
`install` to drop the bundled CSS/JS into the book directory, and
ships fine. Some weeks later the author bumps the binary forward via
`cargo install --force` to pick up a fix. The next `mdbook build`
renders the chapter against the *new* HTML emission paired with the
*old* on-disk CSS/JS — silent visual breakage until the author
remembers to also re-run `install`. This is exactly what t2t
Pass 3 hit after we shipped slice 1's hljs-fade CSS fix.

The fix moves the asset write from "one-time at install" to "every
build, idempotent." Two reusable helpers land in `src/install.rs`:

- `ensure_assets_fresh(book_root)` reads each asset path and compares
  to the binary's bundled bytes; only writes when they differ. Returns
  `true` iff anything was written.
- `ensure_gitignore(book_root)` appends the two asset filenames to
  `<book>/.gitignore` (creating the file if missing); skips entries
  that are already present. Returns `true` iff the file was written.

`install()` is refactored to use both helpers — keeping its existing
idempotency contract while now also seeding `.gitignore`. The
preprocessor's `preprocess()` calls only `ensure_assets_fresh` (the
gitignore is one-time setup, not per-build).

{{#diff install-v8 install-v9}}

The new helpers carry a single `// CALLOUT:` marker each — the
detail that earns the WHY comment is the {{#callout
assets-on-build}} note, which lives in `main.rs` next to the
preprocessor call:

{{#diff main-v9 main-v10}}

Tests added in this slice (all in `tests/install.rs`):

- `install_writes_gitignore_entries_for_both_assets` — end-to-end
  install run produces a `.gitignore` listing both assets.
- `ensure_assets_fresh_writes_when_missing` — the bundled bytes land
  on first call.
- `ensure_assets_fresh_is_noop_when_bytes_match` — preprocessor calls
  this on every build; mtime churn would force unnecessary rebuilds.
- `ensure_assets_fresh_overwrites_stale_bytes` — proves the t2t
  Pass 3 fix: stale on-disk copies are refreshed automatically.
- `ensure_gitignore_creates_file_when_missing` — bare-tempdir case.
- `ensure_gitignore_appends_only_missing_entries` — preserves
  existing author entries; never duplicates.
- `ensure_gitignore_is_noop_when_complete` — required for AC 6
  idempotency (the future slice that adds the "already installed"
  message depends on this).

{{#diff install-tests-v4 install-tests-v5}}

Migration for an existing book (this book did exactly this in the
slice-2 commit):

1. Re-run `mdbook-listings install --book-root <book>` — writes
   `.gitignore` and refreshes the asset bytes.
2. `git rm --cached <book>/mdbook-listings.css <book>/mdbook-listings.js`
   to untrack the old committed copies.
3. `mdbook build` regenerates the assets via the preprocessor.

After migration, `cargo install --force ... mdbook-listings` is the
only step needed to upgrade — the next build picks up the new bytes
automatically.

### Slice 3 — popover opens to the right by default

The symptom: every callout popover opened to the LEFT of its badge,
landing on top of the code line it annotates. The reader couldn't
see the line the annotation referred to without dismissing the
popover first — the inline-callout primitive's whole point is that
the annotation sits beside the line, not over it.

Slice 3 flips the default. The change is CSS-only, contained in
`assets/mdbook-listings.css`:

- `.callout-body` switches anchoring from `right: 2em` (right-edge
  anchored, body extends leftward over the listing) to `left: 100%`
  (left-edge anchored to the badge's right edge, body extends
  rightward into the un-annotated gutter).
- The `::after` / `::before` arrow pseudos move from the body's
  right edge to its left edge, and the triangle direction flips
  from right-pointing to left-pointing — so it still points back
  at the badge it belongs to.
- The OUT-state `clip-path` flips its left/right insets so the
  collapsed sliver tucks against the badge on the left rather than
  the right; the transition then expands rightward.

{{#diff listings-css-v2 listings-css-v3}}

The `CSS_ASSET_SENTINEL` and `JS_ASSET_SENTINEL` constants in
`src/install.rs` both bump (CSS v3→v5, JS v1→v5; the iteration
during this slice's debug cycle accounts for the multi-step
versioning) so the bundled-asset check catches the new shape.

{{#diff install-v9 install-v10}}

#### Viewport-aware widening into the gutter

A naïve "always open right" default has the opposite failure mode
of the pre-slice behavior: on a narrow viewport (mobile, sidebar
open, or a callout near the rightmost edge of the chapter column),
the popover can extend off the right side of the viewport and be
unreadable. Slice 3 includes a runtime layout adjustment in
`assets/mdbook-listings.js` that picks the right side at the right
size:

- **Wide gutter** (≥ 28em ≈ 448px between the listing's right
  edge and the viewport's right edge): open right, full
  `max-width: 28em`. Default-comfortable case.
- **Mid gutter** (between the threshold and the default
  max-width): open right, but clamp `max-width` to
  `(availableRight − 2em)` so `body.right` stays inside the
  viewport. The clamp is applied as a direct inline-style write
  (`body.style.maxWidth = '278px'`).
- **Narrow gutter** (< 16em ≈ 230px): flip the popover back to
  left-opening (over the listing). The JS writes
  `body.style.left = 'auto'` + `body.style.right = '2em'` directly
  and adds a `callout-entry--left-popover` class on the entry to
  drive the arrow pseudo-element overrides (pseudo-elements can't
  take inline styles, so the arrow direction still needs a class
  hook). The fallback layout matches the pre-slice behavior.

Four gotchas had to land for the math to match reality:

1. **The scrollbar isn't on the document.** mdbook puts the
   vertical scrollbar on `.content` (`overflow-y: auto`), not on
   `<html>`. `documentElement.clientWidth` returns the full
   viewport width — a popover sized against it gets its right edge
   tucked under `.content`'s scrollbar. The JS walks up from the
   entry to find the nearest scrolling ancestor and uses
   `(container.left + container.clientWidth)` as the right limit.

2. **`em` resolves against the element's own font-size.** The
   popover has `font-size: 0.9em` and mdbook uses the
   `html { font-size: 62.5% }` trick — so `28em` on the popover
   resolves to ~403px (28 × 14.4px popover-em), but
   `28 × documentElement.fontSize` resolves to 280px (28 × 10px
   root-em). The JS reads the popover body's font-size via
   `getComputedStyle(body).fontSize` so the em conversion matches
   what the CSS rule resolves to.

3. **Direct inline-style writes drive the clamp.** An earlier
   attempt that toggled `--callout-body-max-width` silently no-op'd
   in some browser contexts — `setProperty` returned without
   throwing, but the immediate `getPropertyValue` read back empty,
   identical to "JS never ran." Direct property writes on `style`
   (`body.style.maxWidth = '278px'`) are unconditional.

4. **`max-width` is the CONTENT box by default.** mdbook doesn't
   set a global `box-sizing: border-box`, so the default
   `max-width: 28em` on `.callout-body` caps the *content* width.
   The visible popover is content + padding (`0.75em` each side) +
   border (`1px` each side), so the border-box is ~22px wider than
   the JS expects. Setting `.callout-body { box-sizing: border-box }`
   makes `max-width` constrain the visible extent directly.

The JS recalcs on `DOMContentLoaded` and on `requestAnimationFrame`
after every `resize` event, so dragging the window edge updates
the side/clamp choice live.

The full JS file (frozen as `listings-js-v1`):

```js
{{#include listings/listings-js-v1.js}}
```

#### Tests

Three e2e regressions in `tests/e2e_callouts.rs`, one per gutter
band, all using `page.set_viewport_size(...)` to drive each branch:

- `callout_body_opens_to_the_right_of_its_badge_on_wide_viewports`
  — 1800×800, asserts `body.left >= badge.right` (right-opening at
  full max-width).
- `callout_body_never_overflows_the_viewport_horizontally` —
  1024×800, asserts `body.right <= clientWidth` (mid-gutter clamp
  keeps the popover inside the viewport AND off the scrollbar).
- `callout_body_falls_back_to_left_opening_when_right_gutter_is_too_narrow`
  — 900×800, asserts `body.right <= badge.left` (narrow-gutter
  flip).

A small helper `wait_for_layout_recalc(page)` awaits two
`requestAnimationFrame` ticks so each test measures after the JS
has reacted to the viewport change.

{{#diff e2e-callouts-v9 e2e-callouts-v10}}

#### What slice 3 does NOT fix

The narrow-gutter fallback still covers the listing on the left —
that's the lesser evil compared to letting the popover spill
off-screen, but it's not invisible. Slice 4 will add a per-callout
`--align=left` override so an author can pin one side explicitly;
slice 5 will add a translucent / `backdrop-filter: blur` background
so even an unavoidable overlap leaves the underlying code legible.
The three slices together close AC 3.
