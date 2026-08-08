# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- **`verify` warns on a callout marker no prose picks up.** A `CALLOUT:`
  marker whose badge renders in some chapter, but that no `{{#callout}}`
  directive anywhere references, is reported as a warning naming the
  frozen file and line. The reverse direction (a reference to a missing
  marker) already failed the build; this covers the quieter slip, where
  the marker is added while editing the source and the prose never gets
  written. Only rendered markers are reported — a marker in a frozen
  version no chapter shows, on a line outside every include's slice, or
  on a context line of a diff produces no badge and stays silent. The
  exit code is unchanged: annotation without prose still builds.
- **`verify` warns when a slice ends on a callout marker.** A sliced
  `{{#include}}` whose end line is a `CALLOUT:` marker renders the badge
  while excluding the line it annotates, so the badge attaches to
  nothing. Slice bounds are line numbers and shift every refreeze, which
  makes this easy to hit without noticing. The warning names the chapter
  and line of the directive, the range, the label, and the annotated
  line the slice excludes. A marker on the file's last line is exempt
  (its badge clamps to the last visible line in every rendering), and
  `snippets/` includes are covered as well as `listings/` ones.

## [0.2.0] - 2026-08-04

One authoring idiom for both directives, plus the dual license and a
library surface cut back to what the binary and its tests actually use.
Nothing here changes what an existing book renders: the fenced
`{{#include}}` form is byte-identical to before, so books upgrade without
edits. The major bump is for the Rust API, which no published crate
depends on.

### Added
- **`{{#include}}` no longer needs a surrounding fence.** A directive on
  a line of its own renders the whole code block, matching how
  `{{#diff}}` has always worked. The highlight language comes from the
  file extension (`.rs` opens a `rust` fence, `.yml` a `yaml` one), and
  an extension the mapping doesn't name is used as written.
- **`lang="..."` on `{{#include}}`** overrides the language the file
  extension implies, for listings whose extension doesn't name their
  highlighter.

### Changed
- An `{{#include}}` still wrapped in a fence behaves exactly as before,
  and both forms produce byte-identical output, so existing books need
  no edits.
- The error for an unfenced include is replaced by a narrower one. It
  now fires only when the directive shares its line with other text,
  where no code block can be rendered at all.

- **Dual licensed as `MIT OR Apache-2.0`** (was MIT). Apache-2.0 adds a
  patent grant; keeping MIT alongside it means nothing that relied on the
  0.1.x terms loses them. `LICENSE` is now `LICENSE-MIT`, beside a new
  `LICENSE-APACHE`.

### Fixed
- **`{{#diff}}` no longer breaks out of its own code block** when the
  listings being compared contain a fence of their own. Both directives
  now size the fence they emit to the content it wraps.

### Removed
- **The Rust library surface is no longer public.** `lib.rs` exposed
  fourteen modules that nothing outside the crate consumed: crates.io
  reports zero reverse dependencies, and in-repo only `tests/install.rs`
  builds against the library. All but `install` are now crate-private,
  and the CLI moved from `src/main.rs` into `cli::run` so it no longer
  needs them public either. This is the breaking change behind the major
  bump, and it exists so that future features stop forcing one:
  adding an error variant or a directive field is no longer a semver
  event. `InstallOutcome`, the last publicly-reachable enum, is now
  `#[non_exhaustive]` for the same reason.

## [0.1.1] - 2026-08-01

Non-breaking follow-up: opt-in listing numbers, optional captions, and
listing-scoped callout badges. Existing books are unchanged unless they
opt in; with numbering off and no captions the preprocessor output is
byte-identical to 0.1.0.

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

[0.1.1]: https://github.com/padamson/mdbook-listings/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/padamson/mdbook-listings/releases/tag/v0.1.0
