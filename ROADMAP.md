# Roadmap

`mdbook-listings` ships in version-themed groupings rather than a fixed
feature list per release. Versions are emergent — a release happens
when its theme has shipped enough to feel useful, even if not every
bullet below has landed.

The book at <https://padamson.github.io/mdbook-listings/> is the
canonical "what's shipped" reference; this file is the canonical
"what's planned" reference.

## v0.1.0 — the four primitives

The initial release. One user-story chapter per primitive (see the
self-documenting book for the full story for each). All shipped — the
theme is complete and ready to tag.

- **Install the Preprocessor** *(shipped)* — one-shot setup of an
  existing book.
- **Freeze a Listing** *(shipped)* — snapshot a source file under a
  tag, embed it via the `{{#include}}` directive. `--tag` is optional;
  omitting it derives `<basename>-v<next>` from the manifest.
- **Show Diffs Between Slices** *(shipped)* — render a unified diff
  between two frozen tags inline in a chapter, with a `live:` escape
  hatch for diffing against current source.
- **Render Inline Callouts** *(shipped)* — attach prose to specific
  lines of a frozen listing via inline `// CALLOUT: <label>` markers,
  with stable cross-references from surrounding text. Works on any
  language with a recognised single-line comment syntax. Also ships
  line-range support (`{{#diff a b 1:30 1:30}}`,
  `{{#include foo.rs:1:30}}`) and `data-listing-tag-range` locator
  anchors for the screenshot tool. Sidecar (separate TOML) callouts
  shipped later in the dogfooding-polish pass (ch.6). PDF inline-badge
  rendering — still a design sketch — lives in
  [ch.9 (Future Work)](https://padamson.github.io/mdbook-listings/ch09-future-work.html)
  and is not in v0.1.0's scope.
- **Verify Frozen Listings** *(shipped)* — `mdbook-listings verify`
  fails the build when a frozen snapshot no longer matches its recorded
  hash, a listing file is missing, or a chapter reference (or sidecar)
  doesn't resolve; it warns on orphan files and on `live:` operands that
  trade away freeze stability. Shallow only — it checks snapshot
  integrity, not that the code still compiles (deep verify is ch.9).

## v0.1.1 — listing numbers and captions *(shipped)*

A non-breaking follow-up surfaced by dogfooding a content-heavy chapter
elsewhere, validated downstream before tagging. Numbering and scoped
badges are opt-in via `[preprocessor.listings] number-listings`; captions
are per-directive.

- **Automatic listing numbers** — `Listing N.M` labels, numbered in
  document order across includes and diffs.
- **Listing captions** — optional `caption="..."` on `{{#include}}` and
  `{{#diff}}`, rendered with the number.
- **Listing-scoped callout badges** — a badge reads as `5.3.1` rather than
  a bare `1`, in the listing and in prose cross-references, and renders as
  a pill at any width.
- **`{{#diff}}` context window** — an optional `context=N` argument sets the
  unified-diff context radius (default 3), so a hunk can show enough
  surrounding lines to place a change.
- **List of Listings index** — a `{{#list-of-listings}}` marker renders a
  book-wide, chapter-grouped index of every numbered listing, each entry
  linking to its anchor. Opt-in via `list-of-listings`.
- **Appendix letter listing numbers** — `Listing A.1` in suffix chapters
  titled `Appendix X …`, derived from the title; defers to a real mdbook
  section number if one ever exists.
- **Stable listing cross-references** — `label="..."` on a listing plus
  `{{#listing-ref <label>}}` in prose, resolving to the current number,
  linked; unknown/duplicate labels fail the build.
- **List of Listings sidebar** — a client-side variant of the index in the
  sidebar, opt-in via `list-of-listings-sidebar`. `"append"` adds a
  self-contained "Listings" section below the table of contents; `"nested"`
  places each listing under its heading in mdbook's per-page header tree,
  folding with it. HTML only; the inline index is the floor for PDF.

## v0.2.0 — one directive idiom *(shipped)*

Surfaced by a downstream author who hit the asymmetry between the two
embedding directives. Books upgrade without edits: the fenced
`{{#include}}` form renders byte-identically to 0.1.1.

- **Fence-free `{{#include}}`** — a directive on a line of its own renders
  the whole code block, the way `{{#diff}}` always has. The highlight
  language comes from the file extension, or from `lang="..."`.
- **Correctly sized fences** — both directives size the fence they emit to
  the content it wraps, so a listing or diff containing a fence of its own
  no longer breaks out of its block.
- **`MIT OR Apache-2.0`** — dual licensed, per the ecosystem license
  strategy for core tools.
- **Crate-private library** — all modules but `install` are internal, and
  the CLI lives in `cli::run`. Nothing consumed the old public API, and
  removing it means new error variants and directive fields stop forcing
  major releases.

## v0.3.0 — power-user ergonomics

- `mdbook-listings unfreeze <tag>` for orphan cleanup.
- `verify --prune` for interactive orphan removal.
- Per-chapter tag namespacing under `book/src/listings/<chapter>/`.

Internals (carried over from the 2026-06 architecture review; no
user-visible change):

- Pass structured per-chapter listing metadata between pipeline stages
  instead of round-tripping through the `<div data-listing-…>` anchor
  protocol (`src/anchor.rs` centralises the current string protocol; this
  replaces it).
- `thiserror` for the structured error enums (drop the hand-written
  `Display`/`Error` impls).
- Centralise the escaping policy scattered across `html_escape` and
  `render_inline_markdown` (moot if the structured-metadata item lands
  first). The `{{` escape the include and diff splicers each applied is
  already done: both emit through `fence::render_block`, which owns it.

## v0.4.0 — richer rendering

- Syntax-highlighted diffs (currently plain unified-diff text).
- Multi-paragraph callout bodies, inline code in callouts.
- Callouts overlaid on diff output.

## v0.5.0 — language reach + workflow

- Block-comment-only languages for inline callouts (CSS, plain
  Markdown).
- `mdbook-listings install --hook` writes a pre-commit hook that runs
  `verify` on every commit.
- Watch mode (re-freeze on source change, opt-in).

## v1.0.0 — stability + deep verify

- Manifest schema and preprocessor JSON protocol committed
  (compatibility promise across future minors).
- Deep verify: compile/run check that frozen listings still typecheck
  and pass tests against the project they were frozen from.
- Upgrade flow when the bundled CSS asset bumps versions.
- Detection of conflicting preprocessor configs at install time.
- Uninstall command.

## Notes

The theme groupings are judgement, not commitment — a v0.2.0 may ship
with two of the four bullets above if those land cleanly and the
others need more design. New ideas land on this roadmap by editing
this file in a PR; substantive shifts (e.g. adopting a feature
deferred to v1.0.0 into v0.2.0) get discussed in an issue first.
