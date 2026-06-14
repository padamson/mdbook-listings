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

### Changed
- Callout badges render as pills at any width, so a bare `1` and a scoped
  `5.3.1` share one shape in prose and in listings.

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
