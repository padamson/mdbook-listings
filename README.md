# mdbook-listings

[![crates.io](https://img.shields.io/crates/v/mdbook-listings.svg)](https://crates.io/crates/mdbook-listings)
[![CI](https://github.com/padamson/mdbook-listings/actions/workflows/ci.yml/badge.svg)](https://github.com/padamson/mdbook-listings/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Managed code listings for mdbook: freeze real source into your book, diff
and annotate it, and verify it stays honest.

A book that embeds code drifts: the snapshot in chapter 3 quietly stops
matching the file it came from. mdbook-listings keeps the embedded code
real and the prose keyed to it:

- **Freeze** a source file under a tag and embed it with mdbook's
  `{{#include}}`, so a chapter shows a stable snapshot even as the code
  evolves.
- **Diff** two frozen tags inline with `{{#diff a b}}` to show how a
  listing changed between slices.
- **Callouts** — `// CALLOUT: <label>` markers turn into numbered badges
  with hover annotations and prose cross-references, so explanations stay
  attached to specific lines instead of fragile line numbers.
- **Verify** in CI — `mdbook-listings verify` fails the build if a frozen
  snapshot was tampered with or a reference doesn't resolve.

## Installation

```bash
cargo install mdbook-listings
```

## Usage

```bash
mdbook-listings --help
```

Full documentation at <https://padamson.github.io/mdbook-listings/>.

What's planned beyond what's shipped: see [ROADMAP.md](ROADMAP.md).

## Claude Code plugin

If you use [Claude Code](https://claude.com/claude-code), you can install an
authoring assistant for this preprocessor. It gives the agent a concise,
always-current reference for the CLI and directive syntax while it edits a
book, so it doesn't have to re-derive the commands each session.

The plugin lives in this repo. To install it, add the repo as a plugin source
and then install:

```text
/plugin marketplace add padamson/mdbook-listings
/plugin install mdbook-listings@mdbook-listings
```

Install at user scope (every project) or, in a book repo, at project scope
(`--scope project`) so collaborators pick it up too.

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

MIT License. See [LICENSE](LICENSE).
