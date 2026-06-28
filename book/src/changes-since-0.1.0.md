# Changes since v0.1.0

This book is a record of how `mdbook-listings` reached v0.1.0. The crate
kept moving after the book closed, so this page lists the changes that
postdate the prose. The chapters themselves are left as the v0.1.0 record
and do not describe what follows.

## Unreleased — List of Listings

- **List of Listings index.** A `{{#list-of-listings}}` marker renders a
  book-wide index of every numbered listing, grouped by the chapter it
  appears in and linking to each one. Opt-in through
  `[preprocessor.listings] list-of-listings`; this book's
  [List of Listings](listings-index.md) page uses it.

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
