---
name: mdbook-listings
description: >-
  Author and maintain managed code listings in an mdBook book that uses the
  mdbook-listings preprocessor. Use when editing book chapters (book/src/*.md),
  the listings.toml manifest, or book.toml, or when running the mdbook-listings
  CLI. Covers freezing source files into verifiable snapshots, embedding them
  with {{#include}}, annotating lines with {{#callout}} badges, showing
  {{#diff}} between slices, and verifying that the book stays in sync with its
  sources.
---

# Authoring with mdbook-listings

`mdbook-listings` is an mdBook preprocessor that turns ordinary fenced code
blocks into **managed, verifiable listings**. Instead of pasting code into
prose by hand (where it silently drifts from the real source), you *freeze* a
source file into a hashed snapshot, embed the snapshot, and let `verify` catch
drift in CI.

Apply this skill whenever the current repo is an mdBook book whose `book.toml`
contains `[preprocessor.listings]`, or when the user asks to add/update a code
listing, a diff between two versions, or inline line annotations.

## The mental model

Two identities govern every listing:

- **tag** — the human-facing name (e.g. `main-v1`). It is both the frozen
  filename stem and the manifest key, so it must be unique within the book.
- **sha256** — the integrity check. Re-freezing identical bytes under the same
  tag is a no-op; re-freezing *different* bytes under an existing tag is
  **rejected** unless you pass `--force`.

You never hand-edit frozen files or `listings.toml` — the CLI owns them. You
*do* hand-write the `{{#include}}` / `{{#listing}}` / `{{#callout}}` /
`{{#diff}}` directives in chapter prose.

## The core workflow

All CLI commands are run **from the book root** (the directory containing
`book.toml`). Source paths passed to `freeze` are relative to that root.

1. **One-time setup** (if the preprocessor isn't registered yet):

   ```bash
   mdbook-listings install
   ```

   Adds `[preprocessor.listings]` to `book.toml` and copies the runtime assets
   (`mdbook-listings.css`, `mdbook-listings.js`), wiring them into
   `[output.html]`. See [references/cli.md](references/cli.md).

2. **Freeze a source file** into a hashed snapshot:

   ```bash
   mdbook-listings freeze ../src/main.rs --tag main-v1
   ```

   Copies the file to `listings/main-v1.rs` and records its hash in
   `listings.toml`. Omit `--tag` to auto-derive one from the basename
   (`main.rs` → `main-v1`, then `main-v2` on the next freeze of the same
   basename).

3. **Embed** the frozen snapshot in chapter prose with mdBook's native
   `{{#include}}` inside a fenced block. The fence language controls
   highlighting; the path points at the *frozen* copy, not the original:

   ````markdown
   ```rust
   {{#include listings/main-v1.rs}}
   ```
   ````

4. **(Optional) Annotate** lines by adding `// CALLOUT: <label> <body>` marker
   comments *in the source file* before you freeze it, then reference them from
   prose with `{{#callout <label>}}`. Or **show a diff** between two frozen
   slices with `{{#diff <old-tag> <new-tag>}}`. Full syntax in
   [references/directives.md](references/directives.md).

5. **Verify** that every frozen listing still matches its hash and every
   `{{#include}}` resolves. Run this in CI:

   ```bash
   mdbook-listings verify
   ```

## Quick directive reference

Authored in markdown; the preprocessor expands them at build time. Details and
edge cases live in [references/directives.md](references/directives.md).

- `{{#include listings/<tag>.<ext>}}` — embed a frozen listing (inside a fenced
  ```` ```lang ```` block). Optional `:start:end` suffix embeds only a line
  range: `{{#include listings/<tag>.<ext>:1:30}}`.
- `{{#diff <old-tag> <new-tag>}}` — render the line-by-line diff between two
  frozen listings (inside a ```` ```diff ```` -less fenced block, or on its own
  line). Both tags must exist in `listings.toml`. Optional per-side line ranges:
  `{{#diff a b 1:30 1:30}}`. Either operand may be `live:<path>` to diff against
  a live file (chapter-relative path).
- **Callout markers** — `// CALLOUT: <label> <body>` comment lines written *in
  the source* (comment prefix is language-specific). On render the marker line
  is stripped and replaced with a numbered badge; the body shows on hover (HTML)
  or as a note (PDF). A label-only marker is a bare cross-reference target.
- `{{#callout <label>}}` — *prose-side* reference to a marker by label, rendered
  as the same numbered badge, hyperlinked to the listing. Unknown label fails
  the build.

## Authoritative sources

This skill captures the **stable interface**. When in doubt, the binary's own
help never drifts:

```bash
mdbook-listings --help
mdbook-listings <subcommand> --help
```

For depth beyond what's here, read [references/cli.md](references/cli.md) and
[references/directives.md](references/directives.md).
