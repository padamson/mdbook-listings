# mdbook-listings

[![crates.io](https://img.shields.io/crates/v/mdbook-listings.svg)](https://crates.io/crates/mdbook-listings)
[![CI](https://github.com/padamson/mdbook-listings/actions/workflows/ci.yml/badge.svg)](https://github.com/padamson/mdbook-listings/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![skills.sh](https://skills.sh/b/padamson/mdbook-listings)](https://skills.sh/padamson/mdbook-listings)

Managed code listings for mdbook: freeze real source into your book, diff
and annotate it, and verify it stays honest.

A book that embeds code drifts: the snapshot in chapter 3 quietly stops
matching the file it came from. mdbook-listings keeps the embedded code
real and the prose keyed to it:

- **Freeze** a source file under a tag and embed it with mdbook's
  `{{#include}}`, so a chapter shows a stable snapshot even as the code
  evolves, inside a list item as readily as between paragraphs.
- **Diff** two frozen tags inline with `{{#diff a b}}` to show how a
  listing changed between slices, and annotate the diff from a sidecar
  without editing either file.
- **Callouts** — `// CALLOUT: <label>` markers, or a sidecar TOML for code
  you don't own, turn into numbered badges with hover annotations (click
  one to pin it open) and prose cross-references, so explanations stay
  attached to specific lines instead of fragile line numbers.
- **Fit the page** — `width="wide"` lets a listing grow out of the text
  column, `wrap="true"` wraps its long lines instead of scrolling, and
  `max-line-width` caps how wide a frozen line may be, so `freeze` refuses
  a source over it and `verify` catches one frozen before the budget was
  set. Each has a book-wide default under `[preprocessor.listings]` and a
  per-listing override.
- **Number and reference** — opt-in `Listing N.M` numbers (letters in
  appendices) with per-listing captions, a book-wide List of Listings as
  a page or in the sidebar, and `label=` + `{{#listing-ref}}`
  cross-references that resolve to the current number so they never go
  stale.
- **Show where a listing came from** — opt-in, a muted line under each
  caption names the file the snapshot was taken from and the tag it was
  frozen under, so a book that freezes one file repeatedly doesn't have to
  spend its captions saying which version you're looking at. With
  `freeze --source-url` the path links to the file at the commit it was
  frozen from.
- **Verify** in CI — `mdbook-listings verify` fails the build if a frozen
  snapshot was tampered with, a reference doesn't resolve, or a directive
  sits where its code block can't render, and warns on the quieter slips:
  an annotation no prose picks up, a slice that cuts off the line it
  annotates, an argument that will never render.

## Installation

```bash
cargo install mdbook-listings
```

## Usage

```bash
mdbook-listings --help
```

Full documentation at <https://padamson.github.io/mdbook-listings/>.

What's planned beyond what's shipped: see
[Future Work](https://padamson.github.io/mdbook-listings/ch09-future-work.html).

## Agent skill

A skill that teaches a coding agent the CLI and directive syntax ships in
this repo. Install it into a book repo with the skills CLI:

```bash
npx skills add padamson/mdbook-listings -s mdbook-listings -a claude-code -y
```

## Development

See [CLAUDE.md](CLAUDE.md) for development commands.

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (MSRV: 1.88)
- [prek](https://github.com/j178/prek) for pre-commit hooks: `cargo install prek && prek install`

### Build and test

```bash
cargo build
cargo nextest run
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
