# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

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
