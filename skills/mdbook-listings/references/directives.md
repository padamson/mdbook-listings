# mdbook-listings directive reference

There are three directives — `{{#include}}`, `{{#diff}}`, and `{{#callout}}` —
plus the `// CALLOUT:` source-marker syntax. They are written by hand in chapter
markdown and expanded by the preprocessor at build time. Code samples below use
four-backtick fences so the inner three-backtick block is shown literally.

## Preprocessor ordering (important)

For `{{#include}}` of frozen listings and callout stripping to work, the
`listings` preprocessor must run **before** mdBook's built-in `links`
preprocessor (and before `admonish` if you use it). `mdbook-listings install`
registers the preprocessor; if includes aren't being expanded, ensure
`book.toml` has:

```toml
[preprocessor.listings]
before = ["admonish", "links"]
```

## `{{#include}}` — embed a frozen listing

mdBook's native include directive. Point it at the **frozen** copy in
`listings/` (relative to the chapter's `src/` directory), inside a fenced block
whose language sets the highlighting:

````markdown
```rust
{{#include listings/main-v1.rs}}
```
````

Readers see the frozen snapshot; you maintain the original source and re-freeze
when it changes. The `<ext>` should match the source file's extension so
highlighting works.

### Line ranges

A trailing `:start:end` suffix embeds only part of the file. Endpoints are
**inclusive and 1-based**; empty endpoints mean "to end" / "from start":

````markdown
```rust
{{#include listings/foo.rs}}        // whole file
{{#include listings/foo.rs:1:30}}   // lines 1–30
{{#include listings/foo.rs:200:}}   // line 200 to EOF
{{#include listings/foo.rs::100}}   // start to line 100
```
````

A sliced include is prefixed with a two-line, language-aware locator banner
(e.g. `// basename` then `// @@ start,end @@`) so readers can tell it's a
fragment and which file it came from. Out-of-range endpoints clamp silently.

Includes of `snippets/...` (hand-curated excerpts, not frozen tags) are also
processed, so any `// CALLOUT:` markers in them render — but they are *not*
verified by `mdbook-listings verify`. Use `listings/` for byte-exact frozen
mirrors and `snippets/` only for curated excerpts.

## `{{#diff}}` — difference between two frozen slices

Render the line-by-line difference between an older and a newer **frozen**
listing. It can sit on its own line in prose (no surrounding fence needed):

````markdown
{{#diff add-v1 add-v2}}
````

- Both tags **must exist** in `listings.toml`, or the build fails with a
  diagnostic naming the missing tag, the chapter, and the directive's line.
- The diff is computed from the **frozen bytes**, so it stays stable as the
  original source evolves (that's the point of freezing).
- Byte-identical operands render a clear "no changes" notice, not an empty block.

### Line ranges

`{{#diff}}` takes **two** ranges (one per operand, since line numbers shift
between versions). Same `start:end` rules as `{{#include}}`:

````markdown
{{#diff a b}}              // whole files
{{#diff a b 1:50 1:60}}    // left 1–50 vs right 1–60
{{#diff a b 200: 220:}}    // each side from line N to EOF
{{#diff a b :100 :100}}    // each side from start to line 100
````

Hunk headers are rewritten to parent-listing line numbers, so a sliced diff
shows the real line positions, not slice-relative ones.

### `live:` operand

Either operand may be `live:<path>` to diff a frozen listing against a file on
disk **at build time**. The path resolves relative to the **chapter's source
directory** (same convention as `{{#include}}`):

````markdown
{{#diff diff-v5 live:../../src/diff.rs}}
````

This deliberately defeats the freeze-stability guarantee for that one diff (it
re-computes every build), which is useful for spotting drift. `verify` flags
`live:` usage.

## Callouts

Callouts are a two-part feature: **markers in the source**, and an optional
**cross-reference from prose**.

### Source markers: `// CALLOUT: <label> <body>`

Write marker comments in the source file *before freezing it*. The grammar is
strict:

```
<leading-ws><comment-prefix> CALLOUT: <label>[ <body>]
```

- exactly one space after the comment prefix, the literal `CALLOUT:`, exactly
  one space, then a `label` matching `[A-Za-z0-9_-]+`, then either end-of-line
  (label-only) or one space + the rest of the line as the body.
- The **comment prefix is language-specific**, keyed off the file extension:
  `//` for rs/c/h/cpp/js/ts/jsx/tsx, `#` for yaml/yml/toml/py/sh/bash/tf/hcl,
  `--` for sql. Block-comment-only languages (CSS, plain Markdown) have no
  inline form.
- Anything that doesn't match exactly is left untouched in the output (no silent
  misparse).

On render, the marker comment line is **stripped** from the listing and replaced
with a numbered badge on that line. In HTML the body appears in a hover popover;
in PDF it renders as a styled note after the listing. A **label-only** marker
produces a bare badge with no body — it exists purely as a stable
cross-reference target. Badges are numbered ordinally within each listing and
reset between listings.

Example source file, frozen and then included:

````markdown
```rust
{{#include listings/greeting-v1.rs}}
```
````

where `greeting-v1.rs` contains:

```rust
fn greet(name: &str) -> String {
    // CALLOUT: signature Takes a borrowed str, returns an owned String.
    format!("Hello, {name}!")
}
```

### Prose cross-reference: `{{#callout <label>}}`

In chapter prose, reference a marker by its label. It renders as the same
numbered badge, hyperlinked back to the listing occurrence:

````markdown
See callout {{#callout signature}} for why the parameter is borrowed.
````

A reference to a label that no marker in the chapter defines **fails the build**
with a diagnostic naming the missing label and the chapter. Adding or removing a
marker renumbers badges visually but does not break label-based references.

## Gotcha: don't write a bare two-arg `{{#diff a b}}` in inline prose

The preprocessor skips directives inside fenced code blocks **and** inline code
spans (`` `…` ``). But a `{{#diff old new}}` written as *plain prose* (not in
backticks, not in a fence) is treated as a live directive and will try to
resolve its operands. When you want to *mention* the directive rather than
invoke it, wrap it in inline backticks or a fenced block, or use a placeholder
like `{{#diff …}}`. Backslash-escaping (`\{{#diff …}}`) is **not** reliable —
mdBook's `links` preprocessor strips the leading `\` before mdbook-listings
runs. Only write a live `{{#diff a b}}` where you actually intend it to render.
