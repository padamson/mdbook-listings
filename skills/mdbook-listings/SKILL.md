---
name: mdbook-listings
description: >-
  Author and maintain managed code listings in an mdBook book that uses the
  mdbook-listings preprocessor. Use when editing book chapters (book/src/*.md),
  the listings.toml manifest, or book.toml, or when running the mdbook-listings
  CLI. Covers freezing source files into verifiable snapshots, embedding them
  with {{#include}}, annotating lines with {{#callout}} badges, showing
  {{#diff}} between slices, numbering and captioning listings, rendering a
  book-wide List of Listings index, and verifying that the book stays in sync
  with its sources.
license: MIT OR Apache-2.0
metadata:
  version: "0.1.25"
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

You never hand-edit frozen files or `listings.toml` — the CLI owns them
(the one exception is adding `allow-long-lines = true` to a record). You
*do* hand-write the `{{#include}}` / `{{#listing}}` / `{{#callout}}` /
`{{#diff}}` directives in chapter prose, and any `<tag>.callouts.toml`
sidecar.

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

3. **Embed** the frozen snapshot in chapter prose with `{{#include}}` on a
   line of its own. It renders the code block for you, taking the highlight
   language from the file extension; the path points at the *frozen* copy,
   not the original:

   ````markdown
   {{#include listings/main-v1.rs}}
   ````

4. **(Optional) Annotate** lines by adding `// CALLOUT: <label> <body>` marker
   comments *in the source file* before you freeze it, then reference them from
   prose with `{{#callout <label>}}`. For code you can't edit, put the
   callouts in a `listings/<tag>.callouts.toml` sidecar instead. Or **show a
   diff** between two frozen slices with `{{#diff <old-tag> <new-tag>}}`.
   Full syntax in
   [references/directives.md](references/directives.md).

5. **Verify** that every frozen listing still matches its hash and every
   `{{#include}}` resolves. Run this in CI:

   ```bash
   mdbook-listings verify
   ```

## Quick directive reference

Authored in markdown; the preprocessor expands them at build time. Details and
edge cases live in [references/directives.md](references/directives.md).

- `{{#include listings/<tag>.<ext>}}` — embed a frozen listing. Put it on a
  line of its own; it renders the code block and takes the highlight language
  from the extension, which `lang="..."` overrides. Wrapping it in your own
  fence still works and renders identically. Optional `:start:end` suffix
  embeds only a line range: `{{#include listings/<tag>.<ext>:1:30}}`. Optional
  `caption="..."` renders a caption line above the listing.
- `{{#diff <old-tag> <new-tag>}}` — render the line-by-line diff between two
  frozen listings, on a line of its own. Both tags must exist in
  `listings.toml`. Optional per-side line ranges:
  `{{#diff a b 1:30 1:30}}`. Either operand may be `live:<path>` to diff against
  a live file (chapter-relative path). Also accepts `caption="..."` and
  `context=N` (unified-diff context radius, default 3).
- `{{#list-of-listings}}` — replaced with a book-wide, chapter-grouped index of
  every numbered listing, each entry linked to its listing. Requires the
  `list-of-listings` opt-in; typically placed on its own back-matter page.
- `label="..."` on `{{#include}}` / `{{#diff}}` — names a listing;
  `{{#listing-ref <label>}}` in prose then renders the listing's *current*
  `Listing N.M`, hyperlinked, so "see Listing 5.4" can't go stale when
  numbers shift. Unknown or duplicated labels fail the build.

## Numbering, captions, and the List of Listings

Opt-in features, configured under `[preprocessor.listings]` in `book.toml`.
With none of them on, output is unchanged — existing books are unaffected.

```toml
[preprocessor.listings]
number-listings = true            # "Listing N.M" labels + scoped badges
list-of-listings = true           # enables the {{#list-of-listings}} marker
list-of-listings-sidebar = "nested"  # "off" (default) | "append" | "nested"
show-listing-provenance = true    # muted "<source> (<tag>)" line under the caption
max-line-width = 72               # freeze refuses, verify fails, wider lines
listing-width = "wide"            # "normal" (default) | "wide": grow past the column
```

- **`number-listings`** — every listing gets a `Listing N.M` label (`N` =
  chapter section number, `M` = order of appearance), and callout badges
  scope to it: a badge reads `5.3.1` instead of a bare `1`, in the listing
  and in prose cross-references.
- **Captions** — `caption="..."` on `{{#include}}` or `{{#diff}}` renders as
  `Listing N.M — caption` (or just the caption when numbering is off). The
  caption is the only text the List of Listings carries, so a weak one makes
  that index useless. [references/captions.md](references/captions.md) covers
  what to write.
- **`show-listing-provenance`** — each listing gains a muted line beneath its
  caption naming the `source` path from `listings.toml` (shown relative to
  the repository root, linked when frozen with `--source-url`, and left out
  when the source lies outside the repo with no URL) followed by its tag as
  a pill, so the caption need not — and should not — spend words on which
  artifact this is. A `{{#diff}}` shows
  both tags and collapses the path to one when its operands were frozen from
  the same file. The List-of-Listings index stays caption-only.
  `show-provenance="true"` / `show-provenance="false"` on an individual
  `{{#include}}` or `{{#diff}}` overrides it either way.
- **`max-line-width`** — a budget for the lines a listing renders (marker
  lines left out, measured in display columns). `freeze` refuses a source
  over it and `verify` fails on a frozen listing over it. For content the
  book can't reflow, `freeze --allow-long-lines` records an exemption,
  `allow-long-lines = true` in `listings.toml`; add that line by hand for a
  listing already frozen. Details in
  [references/cli.md](references/cli.md#line-width-budget).
- **`listing-width`** — `"wide"` lets a listing whose lines overflow the
  text column grow past it in HTML, evenly on both sides and only as far as
  it needs and the page has room for. `width="wide"` / `width="normal"` on
  an individual `{{#include}}` or `{{#diff}}` overrides it either way. For
  code the book can't reflow; details in
  [references/directives.md](references/directives.md#wide-listings-width-and-listing-width).
- **`list-of-listings`** — the `{{#list-of-listings}}` marker renders the
  index; link the hosting page from `SUMMARY.md`.
- **`list-of-listings-sidebar`** — a browser-built sidebar view (HTML only).
  `"nested"` places each of the current page's listings under its heading in
  the sidebar's per-page header tree, folding with it; `"append"` adds a
  self-contained book-wide "Listings" section below the table of contents.
- **Callout markers** — `// CALLOUT: <label> <body>` comment lines written *in
  the source* (comment prefix is language-specific). On render the marker line
  is stripped and replaced with a numbered badge; the body shows on hover, on
  keyboard focus, or pinned open by a click (HTML), or as a note (PDF). A
  label-only marker is a bare cross-reference target.
- **Sidecar callouts** — `listings/<tag>.callouts.toml`, hand-written beside
  the frozen file, attaches callouts without editing the source: one
  `[[callout]]` table per badge with `line` (a line of the frozen file),
  `label`, and an optional `body`. Use it for code you don't own, generated
  code, and languages with no line comment. It applies to `{{#include}}`
  only, not to `{{#diff}}`, and a sliced include shows only the entries
  inside its slice.
  See [references/directives.md](references/directives.md#sidecar-callouts-tagcalloutstoml).
- `{{#callout <label>}}` — *prose-side* reference to a marker by label, rendered
  as the same numbered badge, hyperlinked to the listing. Unknown label fails
  the build.

## Authoritative sources

This skill captures the **stable interface**. The installed binary may be
older or newer than the plugin — directive and config availability can
differ by version — so when this doc and the tool disagree, the tool wins.
Its own help never drifts, and `--version` identifies the exact build:

```bash
mdbook-listings --version
mdbook-listings --help
mdbook-listings <subcommand> --help
```

For depth beyond what's here, read [references/cli.md](references/cli.md),
[references/directives.md](references/directives.md), and
[references/captions.md](references/captions.md).
