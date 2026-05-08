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
self-documenting book for the full story for each).

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
  and PDF inline-badge rendering — features adjacent to this
  primitive — have design sketches in the book's
  [ch.7 (Future Work)](https://padamson.github.io/mdbook-listings/ch07-future-work.html);
  they are not in v0.1.0's scope.
- **Verify Sync with Source** — drift-detection check that fails CI
  when the latest frozen listing for a source file no longer matches
  the source.

## v0.2.0 — power-user ergonomics

- Auto-tag derivation for `freeze` (no `--tag` required;
  `<basename>-v<next>`).
- `mdbook-listings unfreeze <tag>` for orphan cleanup.
- `verify --prune` for interactive orphan removal.
- Per-chapter tag namespacing under `book/src/listings/<chapter>/`.

## v0.3.0 — richer rendering

- Syntax-highlighted diffs (currently plain unified-diff text).
- Listing captions ("Listing 3.1: …").
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
