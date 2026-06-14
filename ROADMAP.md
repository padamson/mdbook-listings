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
  tag, embed it via mdbook's existing `{{#include}}` machinery.
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

## v0.1.1 — listing numbers and captions *(on main, unreleased)*

A non-breaking follow-up surfaced by dogfooding a content-heavy chapter
elsewhere. Landed on `main` and live in the book; the release awaits
further downstream validation. Numbering and scoped badges are opt-in via
`[preprocessor.listings] number-listings`; captions are per-directive.

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

## v0.2.0 — power-user ergonomics

- Auto-tag derivation for `freeze` (no `--tag` required;
  `<basename>-v<next>`).
- `mdbook-listings unfreeze <tag>` for orphan cleanup.
- `verify --prune` for interactive orphan removal.
- Per-chapter tag namespacing under `book/src/listings/<chapter>/`.

## v0.3.0 — richer rendering

- Syntax-highlighted diffs (currently plain unified-diff text).
- Multi-paragraph callout bodies, inline code in callouts.
- Callouts overlaid on diff output.

## v0.4.0 — language reach + workflow

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
