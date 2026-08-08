# Changes since v0.1.0

This book is a record of how `mdbook-listings` reached v0.1.0. The crate
kept moving after the book closed, so this page lists the changes that
postdate the prose. The chapters themselves are left as the v0.1.0 record
and do not describe what follows.

## Unreleased — verify notices annotation without prose

A `\{{#callout}}` directive that names no marker fails the build
([chapter 7](ch07-verify-sync.md) covers why). The opposite slip built
clean: add a `CALLOUT:` marker to a source file, freeze it, and nothing
reminds you that no prose ever picks the label up. The badge renders, the
hover text reads fine, and the page looks finished. A downstream author
hit it the ordinary way (marker added while editing the source, prose
deferred to a later chapter) and had to check by experiment whether
adding the marker early was safe.

`verify` now reports each such marker as a warning naming the frozen file
and line. Only markers that actually render count. This book keeps every
old version of its listings as diff history, and a marker in a version no
chapter shows, or outside every include's slice, or on an unchanged line
of a diff, produces no badge and stays silent. The first draft of the
check skipped that rule, warned on every marker in every frozen file, and
buried the signal in old versions; with it, the 21 warnings this book
reports (as of this writing) are badges rendered somewhere with no prose
pointing at them. Warnings leave the exit code at 0, so a book that wants annotation
without prose still builds.

## v0.2.0 — one authoring idiom for both directives

Every chapter in this book wraps its includes in a ` ```rust ` block,
because until now the build failed without one. `{{#diff}}` never had
that requirement, and nothing in the syntax said which directive wanted
the wrapper. A downstream author hit the asymmetry while writing a
chapter against the crate and reported it.

**`{{#include}}` no longer needs a surrounding fence.** An include on a
line of its own now renders the whole block, the way a diff always has.
The highlight language comes from the file extension, or from a new
`lang="..."` argument when the extension doesn't name it. The one case
that still fails is a directive sharing its line with prose: a code block
has to start a line, so `see \{{#include listings/foo.rs}} above` has no
rendering.

Both forms produce byte-identical output, so the fenced includes
throughout these chapters render exactly as they always did. This page is
the only place in the book that uses the new form. What follows is a bare
`\{{#include snippets/render-block-snippet-v1.rs}}` with no fence around
it, showing the code that does the work:

{{#include snippets/render-block-snippet-v1.rs}}

**`{{#diff}}` no longer breaks out of its own block** when the listings it
compares contain a fence. Its wrapper was three backticks regardless of
content, so the first ` ``` ` inside a diffed Markdown listing closed it
early and the rest of the diff spilled into the page as prose. Both
directives now size the fence they emit to what it wraps, which is what
{{#callout fence-line-initial}} computes.

## v0.1.1 — e2e suite tracks playwright-rust main

- **`tests/e2e_callouts.rs` has moved past its frozen listings.** The e2e
  suite and the screenshot tool now build against
  [`playwright-rust`](https://github.com/padamson/playwright-rust) main
  rather than the released `playwright-rs` 0.14, dogfooding unreleased
  changes there the way this book dogfoods `mdbook-listings`. On main,
  `Page::locator()` returns a `Locator` directly instead of a future, so
  the live test file drops the `.await` that the chapter 5 and 6 listings
  show after every `locator()` call. The frozen `e2e-callouts-v*` and
  `capture-screenshots-v*` listings stay as the v0.1.0 record.

## v0.1.1 — List of Listings

- **List of Listings index.** A `{{#list-of-listings}}` marker renders a
  book-wide index of every numbered listing, grouped by the chapter it
  appears in and linking to each one. Opt-in through
  `[preprocessor.listings] list-of-listings`; this book's
  [List of Listings](listings-index.md) page uses it.
- **Stable listing cross-references.** A listing can carry a
  `label="..."` on its `{{#include}}` or `{{#diff}}` directive, and prose
  anywhere in the book can point at it with a `listing-ref` directive that
  renders the listing's *current* number, hyperlinked — so "see
  {{#listing-ref freeze-acceptance-tests}}" stays correct when numbers
  shift. An unknown or duplicated label fails the build. That reference is
  live: it resolves to chapter 3's acceptance-criteria listing.
- **List of Listings in the sidebar.** With
  `[preprocessor.listings] list-of-listings-sidebar`, the numbered listings
  show in the sidebar, built in the browser. `"nested"` puts each of the page
  you're on into mdbook's own header tree, under the heading it lives beneath,
  so it appears only while you're on that page and its section is open and
  folds away with the heading. `"append"` instead adds a self-contained
  "Listings" section below the table of contents, listing the whole book
  independent of the theme's nav. This book turns on `"nested"`, so the
  sidebar here shows this page's listings under their sections.

## v0.1.1 — listing numbers and captions

- **Automatic listing numbers.** Every listing renders a `Listing N.M`
  label, where `N` is the chapter's section number and `M` is the listing's
  order of appearance. Numbering is opt-in through
  `[preprocessor.listings] number-listings`; this book turns it on.
- **Optional captions.** `{{#include}}` and `{{#diff}}` accept a
  `caption="..."` argument, rendered with the number as
  `Listing N.M — caption`.
- **Listing-scoped callout badges.** A callout badge reads as `5.3.1`
  (its listing number plus the within-listing ordinal) rather than a bare
  `1`, both in the listing and in prose cross-references, so a badge says
  which listing it belongs to.
- **Pill-shaped badges.** Badges render as pills at any width, so a bare
  `1` and a scoped `5.3.1` share one shape.
- **`{{#diff}}` context window.** An optional `context=N` argument sets the
  unified-diff context radius (default 3), so a hunk can show enough
  surrounding lines to place a change. Used in
  [Show Diffs Between Slices](ch04-show-diffs-between-slices.md).

These features are active in this rendered book, but the prose, the
listings, and the captured screenshots predate them: the screenshots show
bare ordinals, and no chapter teaches numbering or captions. For the full
entry and commit history, see the project
[CHANGELOG](https://github.com/padamson/mdbook-listings/blob/main/CHANGELOG.md).
