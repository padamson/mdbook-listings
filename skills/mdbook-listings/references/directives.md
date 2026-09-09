# mdbook-listings directive reference

There are five directives — `{{#include}}`, `{{#diff}}`, `{{#callout}}`,
`{{#listing-ref}}`, and `{{#list-of-listings}}` — plus the `// CALLOUT:`
source-marker syntax. They are
written by hand in chapter markdown and expanded by the preprocessor at build
time. Code samples below use four-backtick fences so the inner three-backtick
block is shown literally.

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

Point it at the **frozen** copy in `listings/` (relative to the chapter's
`src/` directory), on a line of its own:

````markdown
{{#include listings/main-v1.rs}}
````

Readers see the frozen snapshot; you maintain the original source and re-freeze
when it changes.

The directive renders the whole code block, so no surrounding fence is needed —
same as `{{#diff}}`. The highlight language comes from the file extension:
`.rs` opens a `rust` block, `.yml` a `yaml` one, and an extension that is
already a language name (`toml`, `sql`, `go`) is used as written.

### Language override

When the extension doesn't name the highlighter you want, set it explicitly
with `lang="..."`:

````markdown
{{#include listings/shapes.txt lang="turtle"}}
````

### Wrapping it in a fence still works

The older form, with the author supplying the fence, renders identically:

````markdown
```rust
{{#include listings/main-v1.rs}}
```
````

Both produce byte-identical output, so there is nothing to migrate in an
existing book. Inside a fence the fence's own info string sets the language and
`lang="..."` is ignored.

One case is an error either way: a directive that shares its line with other
text. A code block has to start a line, so there is no rendering for
`see {{#include listings/foo.rs}} above`.

### Line ranges

A trailing `:start:end` suffix embeds only part of the file. Endpoints are
**inclusive and 1-based**; empty endpoints mean "to end" / "from start":

| Directive | Embeds |
|---|---|
| `{{#include listings/foo.rs}}` | the whole file |
| `{{#include listings/foo.rs:1:30}}` | lines 1–30 |
| `{{#include listings/foo.rs:200:}}` | line 200 to EOF |
| `{{#include listings/foo.rs::100}}` | the start through line 100 |

A sliced include is prefixed with a two-line, language-aware locator banner
(e.g. `// basename` then `// @@ start,end @@`) so readers can tell it's a
fragment and which file it came from. Out-of-range endpoints clamp silently.

A slice whose **end** line is a `CALLOUT:` marker draws a `verify` warning:
the marker annotates the line after it, which the slice excludes, so the
badge would attach to nothing. Line numbers shift every refreeze, so a range
that ended cleanly before a marker can end on top of one later.

Includes of `snippets/...` (hand-curated excerpts, not frozen tags) are also
processed, so any `// CALLOUT:` markers in them render — but their bytes are
*not* integrity-checked by `mdbook-listings verify` (the slice-end warning
does apply to them). Use `listings/` for byte-exact frozen mirrors and
`snippets/` only for curated excerpts.

### Caption

An optional `caption="..."` argument renders a caption line above the listing.
The value is double-quoted; the first `"` ends it (there is no escape). It can
sit anywhere among the arguments — the parser lifts it out before reading the
path:

````markdown
{{#include listings/foo.rs caption="The reuse manifest"}}
````

With `number-listings` on (see [Numbering](#numbering-and-the-list-of-listings)
below), the caption renders as `Listing N.M — caption`; with numbering off, a
captioned listing still gets its caption line, just unnumbered.

Inline markdown renders inside a caption — backticks become code, and
emphasis and links work — in the caption line, the List of Listings, and the
PDF. Naming an identifier in code font is the common case:

````markdown
{{#include listings/foo.rs caption="Adding `toml_edit` as a runtime dependency"}}
````

`caption=`, `label=` and `show-provenance=` only work on `listings/`
includes and on `{{#diff}}`. A `snippets/` include is never numbered, so
none of them renders — `verify` warns when it finds one. `lang=` does work
on a snippet, since it sets the fence the directive emits.

A caption is inline content only: one that happens to open with a block
marker (`1.`, `- `, `# `, `> `) stays literal text rather than turning into a
list or heading. Raw HTML is escaped to visible text rather than passed
through. The value is still delimited by the first `"`, so a caption cannot
contain a double quote.

That covers the mechanics. For what to write in one — length, head word,
the anti-patterns, and the house choices a book has to make once — see
[captions.md](captions.md).

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

### Caption and context radius

`{{#diff}}` accepts the same `caption="..."` argument as `{{#include}}`, plus
`context=N` to set the unified-diff context radius (default 3, matching
`diff -U3`) — useful when a hunk needs more surrounding lines to place a
change:

````markdown
{{#diff add-v1 add-v2 caption="Fence-aware scanning" context=8}}
````

A malformed value (`context=x`) falls back to the default rather than dropping
the directive.

`{{#diff}}` also accepts `show-provenance="true|false"`, as `{{#include}}`
does — see [`show-listing-provenance`](#show-listing-provenance).

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
reset between listings; with `number-listings` on, badges are scoped to their
listing's number instead (`5.3.1` — listing `5.3`, badge `1`), in the listing
and in prose cross-references, so a badge says which listing it belongs to.

Example source file, frozen and then included:

````markdown
{{#include listings/greeting-v1.rs}}
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

The reverse slip — a marker whose badge renders but that no `{{#callout}}`
anywhere picks up — builds clean and is reported by `verify` as a warning
naming the frozen file and line. Annotation without prose is allowed (the
hover text stands on its own), but the warning is the reminder that the prose
was never written.

## Numbering and the List of Listings

All opt-in via `[preprocessor.listings]` in `book.toml`; with everything off,
output is unchanged.

### `number-listings`

```toml
[preprocessor.listings]
number-listings = true
```

Every listing (include or diff) renders a `Listing N.M` label above it — `N`
the chapter's dotted section number, `M` the listing's 1-based order of
appearance in the chapter. The label line carries a stable `id="listing-N-M"`
anchor, so prose (and the index below) can link to a listing directly. Callout
badges scope to the listing number, as described above.

**Appendices:** a suffix chapter titled `Appendix A …` numbers its listings
`Listing A.1`, `A.2` — the letter is derived from the chapter title, since
mdbook hands suffix chapters no section number. Badges scope to `A.1.1`,
and the index and `{{#listing-ref}}` pick the letter up. Other suffix
chapters (Introduction, this index page) stay unnumbered.

### `show-listing-provenance`

```toml
[preprocessor.listings]
show-listing-provenance = true
```

Renders a muted line beneath each listing's caption naming where its bytes
**came from** and what the listing **is**:

```
Listing 2.8 — Writing the CSS asset and registering it under `additional-css`
../src/install.rs  (install-v4)
```

The path comes first, and is the manifest's `source` field — the file a
reader can go and open, not the frozen copy under `src/listings/`, which is
freezing's bookkeeping. Leading with it follows every comparable toolchain:
the Rust Book prints `Filename: src/main.rs` above the code, and Docusaurus
and Material for MkDocs put the filename in the code block's title bar.

The tag follows, rendered as a pill. It is the manifest's primary key and the
tool's own derived name (`<basename>-v<N>`), so it carries the artifact *and*
which iteration this is. No documentation toolchain has a precedent for
showing a snapshot revision beside a listing, so it is styled as metadata
about the path rather than as a second path.

Without this, the caption is the only place a listing's identity can live, so
a book that freezes one file once per authoring step ends up spending its
captions on saying which artifact each listing is. See
[captions.md](captions.md) for what that frees the caption to do instead.

A `{{#diff}}` renders both operands as two pills (`(b-v1) → (b-v2)`) and
collapses the path to one when both tags were frozen from the same file,
which is the usual case:

```
../src/callout.rs  (callout-v9) → (callout-v10)
```

A `live:<path>` operand contributes the live path itself. A tag the manifest
does not carry contributes no path, and the line shows the tag alone.

The PDF backend has no pill, so it parenthesises the tag instead:
`` `../src/install.rs` (`install-v4`) ``.

The List-of-Listings index stays caption-only — its entries are the one place
a caption has to stand alone, and paths there would bury it.

**Per-directive override.** `show-provenance="false"` suppresses the line for
one listing in a book that shows them; `show-provenance="true"` turns it on
for one listing in a book that does not:

````markdown
{{#include listings/install-css-v1.css show-provenance="false"}}
{{#diff install-v3 install-v4 show-provenance="true"}}
````

A value that is neither `true` nor `false` is ignored, leaving the book-level
setting to decide.

### `{{#list-of-listings}}` — book-wide index

```toml
[preprocessor.listings]
number-listings = true
list-of-listings = true
```

The marker is replaced with an index of every numbered listing in the book,
grouped under a `##` heading per chapter, each entry a
`Listing N.M — caption` link to the listing's anchor. It takes no arguments.
The usual home is a dedicated back-matter page linked from `SUMMARY.md`:

````markdown
# List of Listings

{{#list-of-listings}}
````

When the feature is off the marker is stripped, not leaked. Inside a fenced
block it is left alone, so a chapter can show the directive verbatim.

### `list-of-listings-sidebar`

```toml
[preprocessor.listings]
number-listings = true
list-of-listings-sidebar = "nested"   # "off" (default) | "append" | "nested"
```

A browser-built sidebar view of the numbered listings (HTML renderer only; the
inline index above is the floor for PDF):

- `"nested"` — each of the **current page's** listings is placed in the
  sidebar's per-page header tree, under the heading it lives beneath, and
  folds with that heading. Couples to mdBook's default-theme sidebar DOM.
- `"append"` — a self-contained, book-wide "Listings" section below the table
  of contents, independent of the theme's navigation tree. The
  theme-independent fallback when `"nested"` doesn't match your theme.

Both link each entry to its listing anchor. Independent of `list-of-listings`
— a book can have the page index, the sidebar, or both.

### `label=` and `{{#listing-ref}}` — stable listing cross-references

Listing numbers are assigned by order of appearance, so a hand-written "see
Listing 5.4" silently goes stale when a listing is inserted above it. Name
the listing instead, and reference it by name:

````markdown
```rust
{{#include listings/freeze-tests-v1.rs label="freeze-acceptance-tests" caption="Rejecting conflicting content and duplicate tags, which the book cannot exercise"}}
```
````

Anywhere in the book (cross-chapter included):

````markdown
The two rejection cases are pinned in {{#listing-ref freeze-acceptance-tests}}.
````

renders as the listing's *current* number — `Listing 3.1`, hyperlinked to the
listing — and keeps tracking it as numbers shift. Mirrors what
`{{#callout <label>}}` does for badges, one level up.

- `label="..."` works on `{{#include}}` and `{{#diff}}`, combines with
  `caption=` in either order, and requires the listing to be numbered
  (`number-listings` on, numbered chapter) — refs resolve to numbers.
- An **unknown label fails the build** with the chapter and line; a **label
  defined twice fails the build** (a ref must have exactly one target).
- In the typst-pdf renderer the ref renders as plain `Listing N.M` text.
- Inside a fenced block the directive is left verbatim, so you can quote it.

## Gotcha: don't write a bare two-arg `{{#diff a b}}` in inline prose

The preprocessor skips directives inside fenced code blocks **and** inline code
spans (`` `…` ``). But a `{{#diff old new}}` written as *plain prose* (not in
backticks, not in a fence) is treated as a live directive and will try to
resolve its operands. When you want to *mention* the directive rather than
invoke it, wrap it in inline backticks or a fenced block, or use a placeholder
like `{{#diff …}}`. Backslash-escaping (`\{{#diff …}}`) is **not** reliable —
mdBook's `links` preprocessor strips the leading `\` before mdbook-listings
runs. Only write a live `{{#diff a b}}` where you actually intend it to render.
