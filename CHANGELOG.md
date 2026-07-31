# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

Non-breaking follow-up toward 0.1.1: opt-in listing numbers, optional
captions, and listing-scoped callout badges. Existing books are unchanged
unless they opt in; with numbering off and no captions the preprocessor
output is byte-identical to 0.1.0.

### Added
- **Automatic listing numbers.** Each listing renders a `Listing N.M`
  label — `N` the chapter's section number, `M` the listing's order of
  appearance across `{{#include}}` and `{{#diff}}`. Opt-in via
  `[preprocessor.listings] number-listings` (default off).
- **Listing captions.** `{{#include}}` and `{{#diff}}` accept an optional
  `caption="..."`, rendered with the number as `Listing N.M — caption`.
- **Listing-scoped callout badges.** Badges read as `5.3.1` (listing
  number plus within-listing ordinal) rather than a bare `1`, in the
  listing and in prose `{{#callout}}` cross-references.
- **`{{#diff}}` context window.** An optional `context=N` argument sets the
  unified-diff context radius (default 3, matching `diff -U3`), so a hunk
  can show enough surrounding lines to place a change. A malformed value
  falls back to the default rather than dropping the directive.
- **List of Listings index.** A `{{#list-of-listings}}` marker renders a
  book-wide index of every numbered listing, grouped by the chapter it
  appears in and linking to each one. Opt-in via
  `[preprocessor.listings] list-of-listings` (default off). Each numbered
  listing's caption gains an `id` so the index (and other prose) can link
  to it.
- **List of Listings sidebar.** With
  `[preprocessor.listings] list-of-listings-sidebar`, the bundled JS renders
  a sidebar view of the numbered listings. `"append"` adds a self-contained
  "Listings" section below mdbook's table of contents listing every listing
  book-wide, placed inside the sidebar scrollbox after the navigation tree so
  it flows below the tree and scrolls with it. `"nested"` places each of the current
  page's listings in mdbook's per-page header tree, under the heading it
  lives beneath, so a listing appears only when you are on its page and its
  section is expanded, and folds with that heading. `"off"` (default) emits
  nothing. Each entry links to a listing's anchor. HTML output only.

- **Stable listing cross-references.** `label="..."` on `{{#include}}` and
  `{{#diff}}` names a listing; `{{#listing-ref <label>}}` in prose renders
  the listing's current `Listing N.M`, hyperlinked (plain text in the
  typst-pdf renderer), so a reference can't go stale when numbers shift.
  Unknown and duplicated labels fail the build with the chapter and line,
  like an unknown `{{#callout}}`. `verify` accepts the new argument.
- **Appendix letter listing numbers.** Listings in a suffix chapter titled
  `Appendix A …` number as `Listing A.1`, `A.2` (badges `A.1.1`, index
  entries and `{{#listing-ref}}` targets included). mdbook has no appendix
  concept — suffix chapters arrive unnumbered — so the letter is derived
  from the chapter's own title; a real mdbook section number always wins,
  and non-appendix suffix chapters (Introduction, a List of Listings page)
  stay unnumbered.
- **`--version` identifies non-release builds.** A binary built from git
  reports `<version> (<short sha>)` unless HEAD sits exactly on the
  release tag, so a build from `main` is distinguishable from the release
  and "rebuild to pick up the fix" is verifiable. Installs without git
  (crates.io, source tarball) print the bare version as before.

### Changed
- Callout badges render as pills at any width, so a bare `1` and a scoped
  `5.3.1` share one shape in prose and in listings.

### Fixed
- **Callout badges survive soft-wrap.** Badge placement now measures each
  target line's rendered box instead of assuming one visual row per logical
  line, so a book can enable `white-space: pre-wrap` on listings (long
  lines wrap instead of forcing horizontal scroll) without badges below a
  wrapped line drifting onto the wrong line. The average-row-height scheme
  remains as the no-JS fallback.
- **Callout badges place correctly in Safari.** Badge placement measured
  a Range over the line's leading whitespace; release Safari returns a
  two-line union rect for a Range on the whitespace character at a line
  boundary in `white-space: pre` content (its top is the previous line's
  top), so every badge sat one line high — uniformly, since code lines
  almost all start with indentation. Placement now measures the line's
  first non-whitespace glyph, which every engine boxes identically.
- **Callout badges place correctly before the code font loads.** Badge
  positions were measured once at `DOMContentLoaded`; on a cold cache the
  async code font then swapped in, reflowed the listing, and left every
  badge several lines low until a resize or reload. Placement now re-runs
  when the font set finishes loading (`document.fonts` ready and
  `loadingdone`) and on window `load`, so first paint on a cold cache ends
  up correct without user intervention.

## [0.1.0] - 2026-06-13

First release. Managed code listings for mdbook, built around four
primitives and a verification gate (each is a user-story chapter in the
[book](https://padamson.github.io/mdbook-listings/)).

### Added
- **`install`** — one-shot setup of an existing book: registers the
  preprocessor, refreshes the bundled CSS/JS on every build, and seeds
  `.gitignore`. Idempotent.
- **`freeze`** — snapshot a source file under a tag and embed it via
  mdbook's `{{#include}}`. Derives a default tag, prints the
  ready-to-paste `{{#include}}`/`{{#diff}}` directives, and a `list`
  subcommand catalogues the manifest.
- **`{{#diff a b}}`** — render a unified diff between two frozen tags
  inline, with a `live:` operand for diffing against current source and
  `START:END` line-range support.
- **Inline callouts** — `// CALLOUT: <label>` markers (and sidecar TOML
  for code you don't own) produce numbered badges with hover bodies and
  `{{#callout}}` prose cross-references; inline-markdown bodies; badges on
  a diff's added/changed lines only.
- **`verify`** — fails the build when a frozen snapshot no longer matches
  its recorded hash, a listing reference doesn't resolve, or a sidecar is
  dangling; warns on orphan files and on `live:` operands that trade away
  freeze stability.
- Claude Code plugin (marketplace + bundled skill) giving an agent a
  current reference for the CLI and directive syntax while authoring.

[Unreleased]: https://github.com/padamson/mdbook-listings/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/padamson/mdbook-listings/releases/tag/v0.1.0
