# Dogfooding-Driven Polish

```admonish note title="Why this chapter exists"
Chapters 2–5 shipped the v0.1.0 primitives (install, freeze, diff,
callouts). The first real downstream project to take a dependency
on those primitives — the
[t2t](https://github.com/padamson/t2t) book — surfaced a handful of
rendering and ergonomic gaps that this book never exercised
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
3. **Callout popover never covers the line it annotates** in the
   common case. The default opens the popover to the right of the
   badge (the un-annotated gutter), and an author override switches
   a specific callout to the left when the right-side gutter isn't
   usable. Some overlap is unavoidable on narrow viewports — the
   fallback there is to live with it. A planned third fix
   (translucent background + `backdrop-filter: blur`) was
   prototyped and dropped: the in-browser effect was too subtle to
   read as translucent across mdbook's themes, where
   `--theme-popup-bg` sits very close to the listing's pre bg.
4. **`freeze` output closes the authoring loop.** Every successful
   `freeze` prints the frozen path AND the ready-to-paste
   `\{{#include listings/<tag>.<ext>}}` directive — the author
   shouldn't have to grep `listings.toml` to find the include path.
   When a prior listing exists in the manifest for the same source
   path, the output also prints the matching
   `\{{#diff <prev-tag> <new-tag>}}` directive so the author can
   paste both lines without a second lookup.
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
8. **Sidecar TOML callouts.** Some listings can't carry inline
   `// CALLOUT:` markers — code the author doesn't own (third-party
   crates, vendored snippets, generated code), or languages without
   a recognized single-line comment syntax (CSS, plain Markdown).
   For those cases, callouts can live in a sibling TOML file
   alongside the frozen listing (`book/src/listings/<tag>.callouts
   .toml`). The splicer loads the sidecar when present, merges its
   entries with any inline markers, and emits one combined overlay
   per fenced block. Inline + sidecar callouts compose cleanly;
   label collisions across the two sources fail the build with a
   diagnostic naming the duplicate label and both source locations.
9. **Diff callouts render on added or changed lines only.** In a `\{{#diff}}` block, a
   callout badge renders only on an **added** or **changed** line, including the added
   side of a modification (the `+` line of a `-`/`+` pair). Context
   (unchanged) and removed lines carry no badge. Because a callout marker
   is always its own dedicated comment line, a *changed* callout (a new or
   edited marker) is itself an added marker line, so it still badges and
   lands on the line it annotates. (Only a genuinely unchanged callout is
   suppressed.) This refines the diff clause of ch.5 AC 1 (which rendered
   badges on added *and* context lines). `\{{#include}}` is unchanged and
   still renders every callout.
10. **One directive grammar across the three passes.** The include, diff,
    and callout cross-ref passes agree on what counts as a directive
    occurrence: backslash-escaped forms stay literal, occurrences inside
    inline code spans or fenced code blocks are left alone, and fence
    tracking follows CommonMark — a shorter same-character fence line
    inside an outer fence is literal text, not a closer. Two consequences
    are author-visible: a literal `\{{#diff}}` example inside a 4-backtick
    fence no longer parses, and `\{{#callout}}` honours the backslash
    escape the other two directives already did.

## The slice — outside-in narrative outline

| Slice | What it adds |
|---|---|
| 1 | Inline markdown in callout body text (AC 1). Downstream dogfooding noticed that backticks around an identifier in a callout body rendered as literal backtick characters rather than a `<code>` span. The fix routes the body through pulldown-cmark's inline parser before wrapping it in the `<div class="callout-body">`, strips the synthetic `<p>` wrapper, and re-applies the `{` → `&#123;` escape for cross-ref-scanner safety. Raw HTML events are remapped to text events so a body containing `<script>` still renders as `&lt;script&gt;`, not as pass-through HTML. |
| 2 | Preprocessor refreshes assets on every build (AC 2). Today `install` writes `mdbook-listings.css` and `mdbook-listings.js` into the book source tree as a one-time snapshot, then the bytes drift as the binary version moves forward — t2t Pass 3 hit this: bumping the locally-installed binary forward without re-running `install` left the rendered book mixing new HTML emission with stale CSS, producing subtle (and sometimes loud) breakage. The slice moves the asset write from `install` to the preprocessor's run hook so the bytes refresh on every build (no-op when bytes already match). `install` keeps the `book.toml` registration job and now also adds the two asset paths to `.gitignore` so downstream books treat them as build artifacts. Migration for existing books: re-run `install`, then `git rm --cached` the two old committed copies. |
| 3 | Open the popover to the right by default (AC 3, fix 1 of 2). CSS-only positioning change on the `<div class="callout-body">` so the natural reading direction (left-to-right) drops the popover into the un-annotated gutter rather than over the line it annotates. |
| 4 | Per-callout `--align` override (AC 3, fix 2 of 2). Tiny extension to the `// CALLOUT: <label>` grammar — `// CALLOUT: <label> --align=left <body>` flips a single callout when the right-side gutter isn't usable (sidebar, narrow viewport, badge near the page edge). The extension is shaped to scale to other per-callout options later (width, theme). |
| 5 | `freeze` output closes the loop (AC 4). Augments the `created: <tag>` line with the frozen path, the exact `\{{#include listings/<tag>.<ext>}}` directive, and — when a prior listing exists in the manifest for the same source — the matching `\{{#diff <prev-tag> <new-tag>}}` directive. The prior-tag lookup is source-based (most-recent manifest entry with the same `source = ...`), not tag-convention-based. |
| 6 | `mdbook-listings list` subcommand (AC 5). Prints one row per `[[listing]]` in `listings.toml`: tag, frozen path, source path. No filtering options yet — just the basic catalogue view. |
| 7 | `install` idempotency (AC 6). After slice 2 the only things `install` writes are `book.toml` registrations and the `.gitignore` entries. The first run continues to register the preprocessor + `additional-css`/`additional-js` and to add the asset paths to `.gitignore`. A second run detects everything already present and prints "already installed" with no writes. |
| 8 | Default tag derivation (AC 7). When `--tag` is omitted, derive `<basename>-v<next>` by reading existing `[[listing]]` entries for the same source path and bumping the highest `vN` suffix. Surfaces a clean error if any existing tag for the same source doesn't match the `<basename>-vN` shape (the heuristic is opinionated; an author who's invented their own scheme keeps using `--tag` explicitly). |
| 9 | Sidecar TOML callouts (AC 8). Listings that can't carry inline markers (generated code, no-comment languages) attach callouts via a sibling `<tag>.callouts.toml` file. Splicer loads it alongside the frozen listing, merges with any inline markers, errors on cross-source label collisions. |
| 10 | Diff callouts render on added and changed lines only (AC 9). A downstream pass noticed a `\{{#diff}}` rendering a badge on an unchanged context line, which is noise in a view about change and a duplicate of the badge the same callout already gets on its first inclusion in a listing. The splicer now badges only added (`+`) marker lines; context (` `) and removed (`-`) markers are stripped from the rendered diff but earn no badge. A changed or new callout is an added marker line, so it still surfaces. This is a splicer-level change only; there is no asset or grammar change. |
| 11 | One directive grammar across the three passes (AC 10). A review pass over the splicer pipeline found the include, diff, and callout-ref parsers each hand-rolling the same `\{{#…}}` scan, and the copies had drifted: the diff parser tracked fences with a toggle that a shorter inner fence line could flip, consuming a literal directive example and missing the real one after the fence. Two new modules — `src/fence.rs` (a CommonMark fence iterator) and `src/directive.rs` (a shared occurrence scanner) — now own the grammar; the three passes keep only argument parsing and policy. |

## Outside-in narrative

Sections appear here as slices ship. All eleven slices have shipped.

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
hit after we shipped slice 1's `hljs`-fade CSS fix.

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
- `ensure_assets_fresh_overwrites_stale_bytes` — proves the fix: stale
  on-disk copies are refreshed automatically.
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
off-screen, but it's not invisible. Slice 4 adds a per-callout
`--align=left|right` override so an author can pin one side
explicitly; a third planned fix (translucent +
`backdrop-filter: blur`) was prototyped and dropped because the
in-browser effect was too subtle to read as translucent across
mdbook's themes. Slices 3+4 are what closes AC 3 in practice.

### Slice 4 — per-callout `--align` override

The symptom: slice 3's viewport-aware fallback decides the popover
side by measuring the available right-side gutter at hover time.
That's the right default for most callouts, but it has no notion
of intent — an author who *knows* a specific callout sits next
to a wide right-gutter element they don't want covered (a sidebar,
an image, a floated note) has no way to say so. Conversely, an
author on a wide viewport who knows a particular body is short
enough to read fine over the listing can't pin it left. The
runtime makes the call; the author can't override it.

Slice 4 extends the `// CALLOUT:` grammar with `--key=value`
options between the label and the body, and ships the first such
option: `--align=left|right`. The marker shape becomes:

```text
// CALLOUT: <label> [--align=left|--align=right] <body>
```

When `--align=left` is present, the splicer emits
`data-callout-align="left"` on the entry; the runtime JS sees the
attribute and pins the popover to the left, short-circuiting the
viewport-aware path. `--align=right` is symmetric (pins right
regardless of available gutter). Anything else falls through to
slice 3's default behaviour.

The option grammar is deliberately a tiny generalisation rather
than a one-off flag: a future slice that wants per-callout
`--width=...` or `--theme=...` will not need to re-touch the
parser. Tokens that don't match `--key=value` end option parsing
and become the start of the body, so the existing bodyless and
body-with-no-options forms keep parsing unchanged.

Here's the demo fixture — a snippet with one `--align=left` marker.
The same file is included into the e2e test as the on-page fixture
the regression hovers; the rendered badge sits directly below the
fenced block:

```rust
{{#include snippets/callout-align-snippet-v1.rs}}
```

The production-code change is in `src/callout.rs`: the `Callout`
struct grows a `pub options: HashMap<String, String>` field, a
new `parse_options` helper pulls `--key=value` tokens off the
front of the rest-of-line, and `render_callout_overlay_html`
emits `data-callout-align="<value>"` on the entry when the option
is set:

{{#diff callout-v7 callout-v8}}

The runtime change is in `assets/mdbook-listings.js`: the
`adjustPopoverPositioning` loop reads `entry.dataset.calloutAlign`
at the top of each iteration and short-circuits to the pinned
side before the gutter math runs. The shipped behaviour for
`--align=left` matches slice 3's narrow-gutter fallback (`body.left
= 'auto'`, `body.right = '2em'`, `callout-entry--left-popover`
class on the entry to drive the arrow-pseudo overrides); for
`--align=right`, the script clears any prior inline overrides so
the CSS default takes over. A `data-callout-popover-decision`
marker (`author-left` / `author-right`) is written on the entry
for devtools diagnostics, matching the scheme slice 3 introduced
for the viewport-aware decisions:

{{#diff listings-js-v1 listings-js-v2}}

The `JS_ASSET_SENTINEL` constant in `src/install.rs` bumps
v5→v6 so the bundled-asset check catches the new shape:

{{#diff install-v10 install-v11}}

Tests added in this slice:

- Six new lib tests in `src/callout.rs` cover the parser:
  `parse_callout_marker_parses_align_left_option`,
  `parse_callout_marker_parses_align_right_option`,
  `parse_callout_marker_no_options_leaves_map_empty`,
  `parse_callout_marker_unknown_option_is_passed_through`,
  `parse_callout_marker_token_without_equals_ends_option_parsing`,
  `parse_callout_marker_option_with_no_body_keeps_body_none`.
- Two new lib tests cover the HTML emission:
  `render_callout_overlay_html_emits_data_callout_align_when_align_option_set`
  and `..._omits_data_callout_align_when_no_option`.
- One new e2e test in `tests/e2e_callouts.rs`:
  `callout_with_align_left_option_pins_popover_left_even_on_wide_viewport`
  drives the end-to-end path. The viewport is set to 1800×800
  (wide enough that slice 3's default would open right), the
  badge from the snippet above is hovered, and the assertion
  checks both `entry.dataset.calloutAlign === 'left'` and
  `body.right <= badge.left + 1` — proving the author override
  beats viewport-aware auto-detection.

{{#diff e2e-callouts-v10 e2e-callouts-v11}}

### Slice 5 — `freeze` output closes the authoring loop

The symptom: every successful `mdbook-listings freeze` printed a
single line — `created: <tag>` (or `unchanged`, or `replaced`) —
and then went silent. To actually USE the frozen listing the
author then had to either remember the include directive's exact
shape (`\{{#include listings/<tag>.<ext>}}`) or grep
`listings.toml` for the path. AND, since most freezes in this
book are *versioned* (`callout-v6`, `callout-v7`, `callout-v8`
…), almost every freeze in a slice is paired with a
`\{{#diff <prev> <new>}}` directive that the author had to
likewise remember or grep for. Per-freeze friction × two;
surfaced on every chapter slice this book wrote.

Slice 5 makes `freeze` print all three on every successful
outcome — verb + tag (as before), the frozen path, the
ready-to-paste `\{{#include …}}` directive, and (when a prior
listing exists for the same source path) the matching
`\{{#diff …}}` directive. The new output:

```text
$ mdbook-listings freeze --tag callout-v8 ../src/callout.rs
created: callout-v8
  frozen:  src/listings/callout-v8.rs
  include: \{{#include listings/callout-v8.rs}}
  diff:    \{{#diff callout-v7 callout-v8}}
```

The first freeze of a source path skips the `diff:` line (there
is no prior). Re-running freeze on an unchanged source prints
all available lines (`unchanged: <tag>` + path + include + diff
if a prior exists) — re-runs are a real "give me the directives
again" workflow, and there's no reason to make the author repeat
the freeze invocation to see them.

Two implementation details earn a note:

- The include path drops the `src/` prefix that the on-disk path
  carries: mdbook resolves `\{{#include …}}` relative to the
  chapter file, which already lives under `src/`. So
  `src/listings/demo.rs` on disk becomes `listings/demo.rs`
  inside the directive.
- The "prior listing" lookup is *source-based*, not tag-based:
  walk the manifest in reverse insertion order and find the
  most-recent listing whose `source = ...` matches and whose tag
  isn't the just-frozen one. No tag-convention parsing, no
  `<basename>-v<N>` heuristic — that means the suggestion works
  for any naming scheme an author uses (and stays quiet when the
  manifest has no candidate). The trade-off: if the author has
  frozen the same source under unrelated tag names (`first-cut`
  → `second-attempt` → `final`), the diff target might surprise
  them. The escape valve is just to ignore the suggestion.

The production-code change is in `src/freeze.rs`
(`frozen_relative_path` is made `pub` so the CLI can recover the
disk path; `freeze` now returns a `FreezeReport` struct carrying
the outcome plus an optional `previous_tag`; new
`previous_listing_for_source` helper does the reverse-iteration
manifest lookup) and `src/main.rs` (the three new `println!`
lines + the strip-`src/` derivation + the conditional diff
line):

{{#diff freeze-v1 freeze-v2}}

{{#diff main-v10 main-v11}}

Tests added in this slice:

- Five new lib tests in `src/freeze.rs` cover
  `previous_listing_for_source`: empty manifest, no matching
  source, only-match-is-current-tag, picking the most-recent
  prior, and skipping the current tag when multiple matches
  exist.
- Five new CLI integration tests in `tests/freeze.rs` cover the
  end-to-end output shape across all three `FreezeOutcome`
  variants plus both diff-suggestion cases (prior exists, no
  prior). The pre-existing `freeze_rejects_*` tests still pass
  — failures only ever wrote to stderr, so the new stdout lines
  don't affect them.

{{#diff freeze-tests-v1 freeze-tests-v2}}

### Slice 6 — `mdbook-listings list` subcommand

The symptom: a book accumulates `[[listing]]` entries over time —
this book has 90+ as of slice 6. The author had to `cat` (or grep)
`listings.toml` to answer basic questions like "what tags exist
for this source file?" or "which freeze versions have I created?"
The manifest is TOML, which is fine for editing but noisy to scan:
every entry is four lines (`[[listing]]`, `tag`, `source`,
`frozen`, `sha256`), most of which is repeated boilerplate.

Slice 6 adds a `list` subcommand that prints one tab-separated
row per listing:

```text
$ mdbook-listings list
callout-v6      src/listings/callout-v6.rs      ../src/callout.rs
callout-v7      src/listings/callout-v7.rs      ../src/callout.rs
callout-v8      src/listings/callout-v8.rs      ../src/callout.rs
e2e-callouts-v9 src/listings/e2e-callouts-v9.rs ../tests/e2e_callouts.rs
...
```

Three columns: tag, frozen-path (book-root-relative), source-path
(book-root-relative as recorded by the most recent freeze).
Order matches manifest insertion order — most recently added at
the bottom, giving chronological awareness without a separate
timestamp column. No filtering, sorting, or formatting options
yet; the basic catalogue view is enough for the workflows that
surfaced the gap, and `awk` / `grep` / `column -t` handle the
rest from a tab-separated stream.

Design choices that earn a note:

- **Tab-separated, no header.** Pipe-friendly by default; an author
  who wants headers can pipe through `column -t -N tag,frozen,
  source`. Adding a header here would force every script consumer
  to skip line 1.
- **Empty manifest prints nothing.** No "no listings recorded"
  banner. Stays quiet and predictable for scripts that test
  command exit status + line count.
- **Insertion order, not alphabetical.** Most-recent-at-bottom
  matches the visual rhythm of `git log` and `tail -f` — the
  reader's eye trains on the bottom as "what just happened."
  Alphabetical sort would scatter `v1`/`v2`/`v3` if the author
  re-runs freeze months apart for unrelated source files.

The production-code change is in `src/main.rs`: a new
`Command::List` variant on the enum plus a four-line handler
that loads the manifest and iterates its `listings` vector:

{{#diff main-v12 main-v13}}

Tests added in this slice (all in `tests/list.rs`, a new file):

- `list_prints_nothing_when_manifest_is_empty` — empty-manifest
  contract: stdout is empty, exit success.
- `list_prints_one_tab_separated_row_per_listing_in_insertion_order`
  — happy path: two freezes, two rows, in the order they were
  inserted.
- `list_source_column_matches_manifest_normalised_path` — the
  source column is the same forward-slash-normalised string the
  manifest records, not a re-stringified `Path` (which would
  re-introduce the Windows backslash bug fixed in the slice 5
  follow-up commit).

```rust
{{#include listings/list-tests-v1.rs}}
```

### Slice 7 — `install` idempotency

The symptom: re-running `mdbook-listings install` against an
already-configured book LOOKED idempotent — the same registrations
were already in `book.toml`, the same asset bytes were on disk —
but the contract had never been pinned. An author who'd run
install once and was about to run it again would reasonably
wonder: "will this duplicate the `additional-css` entry? will it
clobber my hand-edited book.toml ordering? is there a flag I'm
supposed to pass for re-installs?" The CLI gave no signal either
way.

The slice 2 refactor (preprocessor refreshes assets on every
build) had already made the IMPLEMENTATION idempotent as a
precondition for per-build refresh — `ensure_assets_fresh` and
`ensure_gitignore` both short-circuit when bytes already match,
and the toml_edit-based `register_listings_*` methods don't
append duplicates. What slice 7 adds is the contract: a pinned
test that a second `install` returns `InstallOutcome::Unchanged`
and writes nothing, and a CLI integration test that the
"already installed; nothing changed" message lands on stdout.

The CLI output:

```text
$ mdbook-listings install --book-root book
installed mdbook-listings into book

$ mdbook-listings install --book-root book
mdbook-listings already installed in book; nothing changed
```

Production-code change in this slice: none. The `InstallOutcome`
enum, the `install()` function's three-way OR over toml/asset/
gitignore changes, and the main.rs match-arm that selects the
"already installed" message were all in place after slice 2.
What was missing was the *contract pin*: tests that lock the
behaviour in place so a future refactor that accidentally
re-enables duplicate registration would fail loudly.

Two new tests in `tests/install.rs`:

- `install_on_fully_configured_book_is_noop_and_returns_unchanged`
  — lib-level: first install returns `Installed`, second returns
  `Unchanged`, and both `book.toml` and `.gitignore` are byte-
  identical between calls. The byte-equality check catches a
  whole class of "almost-idempotent" regressions (e.g. a future
  TOML re-serialiser that normalises whitespace would change
  bytes silently; this test would fail and force the contract
  to be reconsidered).
- `install_command_prints_already_installed_on_second_run` —
  CLI-level: the friendly message reaches stdout, both
  invocations exit success. Pins the downstream signal.

{{#diff install-tests-v5 install-tests-v6}}

### Slice 8 — default `--tag` derivation

The symptom: every `mdbook-listings freeze` invocation required
the author to invent and type out a `--tag`. For a book that
freezes the same source file repeatedly across slices
(`callout-v6`, `callout-v7`, `callout-v8`, …), the v-suffix
schema is so mechanical that "what's the next tag?" is a
question with a deterministic answer the tool should just
compute. Forcing the human to compute it per-freeze is per-
freeze friction that adds up.

Slice 8 makes `--tag` optional. When omitted, freeze derives a
default from the source basename + the manifest's existing
entries for the same source:

```text
$ mdbook-listings freeze ../src/callout.rs
created: callout-v8
  frozen:  src/listings/callout-v8.rs
  include: \{{#include listings/callout-v8.rs}}
  diff:    \{{#diff callout-v7 callout-v8}}
```

The derivation rule is intentionally narrow:

- **First freeze of a source** (no prior listings): default to
  `<basename>-v1`. `v` is the canonical Rust convention; first
  author to freeze a given source establishes it without per-
  source configuration.
- **Prior listings exist with `<basename>-<prefix><N>` shape**
  where `<prefix>` is one of `v`, `ver`, `rev`, `version`:
  default to `<basename>-<prefix>(maxN + 1)`. The prefix is
  taken from the most-recently-inserted matching listing, so a
  mid-stream convention switch (started with `v1`, then moved
  to `rev1`/`rev2`) sticks with the new convention rather than
  silently flipping back.
- **Prior listings exist but NONE match the allowlist**
  (the motivating case: t2t's `<basename>-ch<NN>-phase<N>`):
  return an actionable error naming the existing scheme and
  directing the author to pass `--tag` explicitly. The CLI
  never silently picks a name that might conflict with the
  author's own scheme.

Two non-obvious design choices:

- **Hyphen-separated allowlist, not "any trailing digits."** The
  prefix has to be one of `v`/`ver`/`rev`/`version` AND there has
  to be a hyphen between basename and prefix. A name like
  `compose3` could be a typo for `compose-v3`, a deliberate name,
  or "compose for Postgres 3" — autopilot is the wrong call.
  Restricting to a known allowlist with a separator rules out the
  ambiguous cases.
- **Most-recent-prefix wins on mixed conventions.** When the
  manifest has `foo-v1`, `foo-v2`, `foo-rev3` (the author
  switched mid-stream), the next default is `foo-rev4`, not
  `foo-v3`. The author's most recent choice is the better signal
  of present intent than max-N alone.

Production-code change in `src/freeze.rs`: new
`derive_default_tag` function, supporting `parse_version_suffix`
helper, `VERSION_PREFIXES` constant, and `TagDerivationError`
enum with two variants (`UnusableSourceName`,
`UnrecognisedConvention`).

{{#diff freeze-v3 freeze-v4}}

CLI wiring in `src/main.rs`: `Command::Freeze::tag` becomes
`Option<String>`; the handler calls `derive_default_tag` when
`None`, wraps the `TagDerivationError` in `anyhow::Error` so the
CLI surfaces the actionable message on stderr with exit 1.

{{#diff main-v13 main-v14}}

Tests added in this slice:

- Eight new lib tests in `src/freeze.rs` covering the derivation
  logic: empty-manifest first-freeze, single-prior bump, max-N
  vs count, each allowlist prefix, mixed-prefix
  most-recent-wins, unrecognised-convention error, and
  cross-source isolation (other-source listings don't pollute).
- Three new CLI integration tests in `tests/freeze.rs` covering
  the end-to-end path: `--tag` omitted on first freeze derives
  v1, `--tag` omitted bumps an existing v-series, and `--tag`
  omitted on an unrecognised-convention prior errors with the
  actionable message.

{{#diff freeze-tests-v2 freeze-tests-v3}}

### Slice 9 — sidecar TOML callouts

The symptom: every callout this book has shipped attaches via
an inline `// CALLOUT:` marker that the splicer parses out of
the frozen listing's source bytes. That model breaks for code
the author doesn't own (third-party crates, vendored snippets,
generated code) and for languages without a recognized
single-line comment syntax (CSS, plain Markdown, plain text).
For both cases, no comment-style marker is possible.

Slice 9 adds a parallel attachment mechanism — a sidecar TOML
file alongside the frozen listing.

Here's the shape, dogfooded against this very book —
`book/src/listings/callout-v9.callouts.toml` sits next to the
`callout-v9` frozen listing (i.e. `book/src/listings/callout-v9.rs`,
the post-slice-9 freeze of `src/callout.rs`) and attaches two
callouts at source lines that don't carry inline markers:

```toml
{{#include listings/callout-v9.callouts.toml}}
```

The naming convention is `<tag>.callouts.toml` next to
`<tag>.<ext>`; the splicer scans the listings directory at the
start of each chapter pass and keys the parsed entries by tag.
Per fenced block, it looks up the `<div data-listing-tag>` anchor
the include splicer already emits (the same anchor that makes
locator-anchor screenshots work) and merges any sidecar entries
for that tag with the inline markers parsed from the block. One
entry per `[[callout]]` table — required `line` (source-file line
in the frozen listing) + `label`, optional `body`.

The rendered effect, on a small slice of the `callout-v9` listing
that ALSO carries the inline `// CALLOUT: parse-entry` marker at
source line 28 — three badges total, one inline, two sidecar:

```rust
{{#include listings/callout-v9.rs:28:50}}
```

#### Three correctness details earned their own test

1. **Source-line → post-strip translation.** The sidecar `line`
   field is the line number in the FROZEN LISTING SOURCE, not a
   line in the rendered chapter. The splicer translates: for a
   ranged `\{{#include listings/<tag>.<ext>:A:B}}`, the include
   splicer prepends 2 header lines (basename + `@@ A,B @@`) to the
   block, so source line N appears at block-text line
   `(N − A + 1) + 2`. Inline marker stripping then shifts every
   subsequent line up by the count of stripped markers before it.
   The render path applies both translations and asserts on the
   resulting post-strip position.
2. **Sidecar pointing at an inline-marker line errors.** If a
   sidecar entry's `line` happens to be the source line of an
   inline `// CALLOUT:` marker (which the strip pass removes from
   the rendered listing), the badge would have nowhere to land.
   The splicer raises `SpliceError::SidecarLineOnStrippedMarker`
   naming the label, listing tag, source line, and sidecar path.
3. **Cross-source label collisions error.** Same label appearing
   as BOTH an inline marker AND a sidecar entry would silently
   shadow one of the rendered badges. The splicer raises
   `SpliceError::LabelCollision` naming the label and both source
   locations. Same-source duplicates (two sidecar entries with the
   same label in one TOML) are caught at load time with
   `SidecarLoadError::DuplicateLabel`.

Production code change in `src/callout.rs`: new `SidecarCallouts`
type with `load(listings_dir)` constructor, new `SidecarFile` +
`SidecarEntry` deserialisable shapes, new `ListingAnchor` extracted
from the `<div data-listing-tag …>` element (including the optional
`data-listing-tag-range` attribute), new
`source_line_to_block_line` + `translate_sidecar_line_to_post_strip`
helpers. The `splice_chapter` signature gains a third parameter
for the sidecar map; `SpliceError` gains two new variants;
`SidecarLoadError` is a separate enum surfaced at load time.

`strip_marker_lines` and `strip_marker_lines_diff` refactored from
returning a 3-tuple to returning a `StripResult` struct with a
new `stripped_source_lines: Vec<usize>` field — the per-block
source-line numbers of stripped inline markers, which the
sidecar translation step needs.

{{#diff callout-v8 callout-v9}}

CLI wiring in `src/main.rs`: load the sidecar map once per
preprocessor invocation and pass `&sidecars` to every
`splice_callouts(...)` call.

{{#diff main-v14 main-v15}}

#### Quieting chronic build noise: escape `{{` in substituted content

While verifying the slice 9 PDF render, a chronic source of
include-directive resolution errors surfaced in the build log.
Root cause: the include and diff splicers substitute frozen
source-code bytes into the chapter buffer; some of those frozen
files contain literal `\{{#include …}}` strings as test fixtures
(test code asserting on splicer behaviour) or doc-comment
examples. Once substituted, mdbook's built-in `links`
preprocessor scans the chapter buffer and tries to resolve those
literals as real directives, failing because the referenced
files don't exist. Build keeps going (errors are non-fatal),
but every build prints a screenful of confused noise.

The fix: both splicers escape `{{` → `\{{` as they substitute.
mdbook's resolver sees the escape and leaves the literal alone;
the rendered HTML still shows `{{...}}` visually (the `\` is
consumed as the escape sigil). Safe because every file
mdbook-listings freezes is source code (Rust, YAML, TOML, JS,
CSS) — never Markdown — so `{{...}}` in the body is always
literal text, never an authored directive.

{{#diff include-v2 include-v3}}

{{#diff diff-v9 diff-v10}}

Tests added in this slice:

- 18 new lib tests in `src/callout.rs`:
  - 4 cover `SidecarCallouts::load` (missing dir, well-formed
    file, invalid-label rejection, ignored-extension files,
    same-source duplicate-label rejection).
  - 2 cover `listing_anchor_after_fence` (with + without range
    attribute).
  - 2 cover `source_line_to_block_line` (identity for full-file,
    offset for ranged).
  - 3 cover `translate_sidecar_line_to_post_strip` (no-strip
    identity, shift by stripped count, error on stripped-line
    collision).
  - 4 cover `splice_chapter` end-to-end (sidecar-only merge,
    inline+sidecar compose in line order, label-collision error,
    sidecar-line-on-stripped-marker error).
- The 30 pre-existing `splice_chapter` tests all updated to pass
  `&SidecarCallouts::empty()` as the new third parameter; their
  behavior is unchanged.
- 1 new e2e test in `tests/e2e_callouts.rs`:
  `sidecar_callout_renders_alongside_inline_marker_in_same_listing`
  asserts that all three badges (`parse-entry` inline,
  `parse-line-entry` + `label-validity-check` sidecar) render
  exactly once each in the rendered ch.6 HTML.
- 1 new lib test in `src/include.rs`:
  `splice_chapter_escapes_double_braces_in_included_body` pins
  the `{{` → `\{{` substitution contract above.

{{#diff e2e-callouts-v11 e2e-callouts-v12}}

Alongside the splicer escapes, a sweep of earlier chapters
fixed unescaped illustrative `{{#…}}` references inside inline
backticks (mdbook's built-in `links` preprocessor doesn't
respect inline-backtick context as a directive-skip zone, so
those mentions raised the same noise). One multi-line example
in ch.5 was rewritten as plain prose because no backslash
position avoided the line-wrap parsing issue cleanly.

### Slice 10 — diff callouts render on changed or added lines only

The symptom: a downstream book embedded a `\{{#diff}}` of two versions of
a listing whose only real change was one added line, and the rendered diff
showed two callout badges: one on the added line, one on an unchanged
context line above it. The diff is about what changed, so the second badge
is just noise. It is also redundant, since that callout already shows up
wherever the listing appears in full.

Ch.5's AC 1 made this deliberate: it badged "added or context lines, but
not removed lines." Dogfooding reconsidered the context half. The refined
rule (AC 9 here) is that a diff badges only the lines it changed, meaning
`+` lines, including the `+` side of a `-`/`+` pair.

This follows from how a marker is written. A callout marker is always its
own comment line (`parse_line` wants the comment prefix as the first
non-whitespace content, and there is no trailing-comment form). So editing
or adding a callout changes its marker line, which the unified diff emits
as a `+` line:

- Edit a callout's body on an otherwise-unchanged code line: the marker
  becomes a `-old`/`+new` pair, the `+` side badges, and the badge lands on
  the unchanged code line. The changed callout isn't lost.
- Add a callout above unchanged code: its marker is a `+` line, so it badges.
- Remove a callout: its marker is a `-` line, so no badge.
- A marker that is byte-identical on both sides is a pure context line, and
  gets no badge.

The change stays in the splicer: no asset or grammar change. Two functions
in `src/callout.rs` already sorted diff lines by their `+`/` `/`-` prefix,
and this slice narrows both so a context (` `) line is treated like a
removed one. `callouts_from_diff_block` parses callouts from `+` lines
only. The HTML emitter, the PDF emitter, and the ordinal pass all reach it
through the shared `callouts_for_block` dispatch, so the one change covers
every path, and badge numbers renumber to count only what a diff renders.
`strip_marker_lines_diff` records a post-strip badge position for `+`
markers (callout {{#callout strip-diff}}); context and removed markers fall
through with no badge (callout {{#callout strip-diff-skip}}).

The diff below is this slice's own change. It badges two callouts,
`strip-diff` and `strip-diff-skip`; both are `+` lines, so the diff is
itself an instance of the rule it documents.

{{#diff callout-v9 callout-v10}}

### Slice 11 — one directive grammar across the three passes

Three passes parse `\{{#…}}` directives out of chapter markdown: include
(ch.5 slice 8), diff (ch.4), and callout cross-refs (ch.5 slice 5). Each
had grown its own scanner, and the copies had drifted. A review pass over
the pipeline found the visible casualty in the diff parser's fence
tracking: it flipped a boolean on every ```` ``` ````/`~~~` line without
recording the opener's character or length. CommonMark says a fence
closes only on a same-character fence at least as long as the opener, so
a 3-backtick line inside a 4-backtick fence is literal text. The toggle
treated it as a closer — a literal `\{{#diff}}` example written inside
such a fence got consumed as a real directive, and the real directive
after the fence was missed. The callout pass already tracked fences
correctly and had the regression test to prove it; the diff parser had
neither.

The drift ran further than the bug. The backslash-escape check and the
inline-backtick check existed as three near-identical copies, the
`line_number` diagnostic helper as two byte-identical ones, and the
callout cross-ref pass had no escape check at all — `\{{#callout label}}`
with a known label resolved anyway, stranding the backslash.

The fix lands in two layers. First, fence walking moves out of
`callout.rs` into its own module as an iterator. The walker logic is
unchanged; the shape change retires the error-smuggling dance its three
fallible callers had to do (declare an `Option<SpliceError>` outside an
infallible closure, assign into it, check it on every later iteration).
Callers now loop and use `?` (callout {{#callout fence-iterator}}); the
closer rule the diff parser got wrong is pinned by the walker's first
direct unit tests (callout {{#callout fence-closer-rule}}):

```rust
{{#include listings/fence-v1.rs:1:154}}
```

Second, a shared scanner owns the occurrence grammar — find the prefix,
skip escaped and inline-code forms, find the closing braces, classify
fence membership via the iterator above. The three passes keep what
actually differs between them: argument parsing and fence policy
(callout {{#callout fence-policy}}). The duplicated diagnostic helper
consolidates here too (callout {{#callout shared-line-number}}):

```rust
{{#include listings/directive-v1.rs:1:111}}
```

Neither new module carries inline `// CALLOUT:` markers; the four badges
above attach via sidecar TOML files next to the frozen listings — the
slice 9 mechanism doing the job it was built for.

The three parser rewires, each reduced to a loop over the scanner's
occurrences. The diff parser's rewrite includes the regression test that
failed against the old toggle
(`parse_directives_does_not_close_outer_fence_on_shorter_inner_fence`):

{{#diff diff-v10 diff-v11}}

The include parser keeps its path-prefix interception and range-suffix
parsing, and drops everything else. Its entry-point marker also gets a
rename: include.rs and callout.rs both carried a `parse-entry` label —
one more drift artifact — and the e2e suite caught the duplicate the
moment this diff first rendered, because ch.6 pins `parse-entry` to
exactly one badge. The renamed marker is an edited `+` line, so it
badges here under slice 10's rule:

{{#diff include-v3 include-v4}}

The callout pass loses its local fence and backtick machinery, gains the
escape check, and picks up a pin test for it
(`replace_callout_refs_leaves_backslash_escaped_directive_literal`).
The fence walker's departure to its own module is most of this diff's
bulk:

{{#diff callout-v10 callout-v11}}

To confirm the refactor changed nothing it shouldn't, this book was
built twice — once with the pre-slice binary, once with this one — and
the rendered HTML compared byte-for-byte. Every chapter matched except
ch.4, whose `live:` diff block re-renders the current `src/diff.rs` by
design.

## What this story does not solve

- **`verify`** still bails with `not yet implemented`. The
  chapter that wires it up (ch.7) is placeholder.
- **Sidecar `line` for ranged includes with inline markers**
  works correctly, but the translation is purely positional —
  there's no "anchor by label" mechanism that decouples a
  sidecar entry from its source-line position. A refactor that
  moved code around would silently shift the sidecar's badge.
- **Sidecar entries don't support `--align`-style options.**
  The inline `// CALLOUT:` grammar accepts `--key=value` tokens
  (slice 4); the sidecar TOML schema doesn't. Adding it is
  straightforward but no downstream has asked for it yet.
- **PDF-side sidecar rendering** works (the PDF splicer's
  blockquote emit gets the merged callouts) but isn't exercised
  by an e2e test the way HTML is. PDF coverage on sidecar is
  whatever the HTML coverage proves transitively.
