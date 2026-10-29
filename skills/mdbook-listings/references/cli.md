# mdbook-listings CLI reference

The binary is `mdbook-listings`. Author-facing subcommands are `install`,
`freeze`, `verify`, and `list`. (`supports` exists too but is the internal
mdBook preprocessor protocol — you won't call it directly.)

Every author command accepts `--book-root <DIR>` and defaults it to the current
directory, so run them from the book root (where `book.toml` lives) and you can
omit the flag.

## `install`

```
mdbook-listings install [--book-root <BOOK_ROOT>]
```

Registers the preprocessor and installs runtime assets. It:

- Adds a `[preprocessor.listings]` table to `book.toml`.
- Copies `mdbook-listings.css` and `mdbook-listings.js` into the book.
- Adds those files to `[output.html]` via `additional-css` / `additional-js`:

  ```toml
  [preprocessor.listings]

  [output.html]
  additional-css = ["mdbook-listings.css"]
  additional-js  = ["mdbook-listings.js"]
  ```

Run once per book. Re-running is safe.

## `freeze`

```
mdbook-listings freeze [OPTIONS] <SOURCE>
```

Copies `<SOURCE>` into the book's `listings/` directory and records it in
`listings.toml` with a SHA-256 hash.

- `<SOURCE>` — path to the source file, relative to the book root.
- `--tag <TAG>` — the frozen filename stem and manifest key; must be unique
  within the book. When omitted, it is derived from the source basename:
  `<basename>-v1` for the first freeze, and the version suffix is bumped
  (`-v2`, `-ver2`, `-rev2`, `-version2`, matching the existing series style)
  on subsequent freezes of the same basename.
- `--force` — overwrite an existing frozen copy that has the **same tag but
  different bytes**. Without it, a conflicting re-freeze is rejected.
- `--source-url <URL>` — record where a reader can see this snapshot, as
  `url` in `listings.toml`. Use the file's URL **at the commit it was frozen
  from** (a GitHub `blob/<sha>/…` link), not a branch, which moves on while
  the listing stays put. With `show-listing-provenance` on, the provenance
  line links the path to it, and for a source outside the book's repository
  (a sibling checkout) it is the only way to show a path at all. Only
  `http://` and `https://` URLs with a host are accepted, and none holding
  whitespace, `<` or `>`. The same check runs whenever `listings.toml` loads,
  so a hand-edited `url` that fails it stops the build.

  ```
  mdbook-listings freeze ../../ontology-authoring-template/data/wine.yaml \
    --tag wine-v1 \
    --source-url https://github.com/padamson/ontology-authoring-template/blob/20cac5f/data/wine.yaml
  ```
- `--allow-long-lines` — freeze a source with lines wider than the book's
  `max-line-width` (see [Line-width budget](#line-width-budget)), recording
  `allow-long-lines = true` in `listings.toml` so `verify` accepts it. Meant
  for content the book can't reflow, such as a file from another project.

### Identity model

- **tag = human identity, sha256 = integrity.**
- Re-freezing **identical** bytes under the same tag is a **no-op**, except
  that a `--source-url` given on that run sets or replaces the recorded URL.
  This is how to add a URL to a listing frozen without one.
- Re-freezing **different** bytes under an existing tag is **rejected** unless
  `--force` is passed. A forced re-freeze drops the recorded URL, since it
  pointed at the old bytes, and warns; pass `--source-url` again to record
  the new one.
- `allow-long-lines` follows the same rule as the URL. An unchanged re-freeze
  keeps it, and one with `--allow-long-lines` records it (the summary says
  `long lines allowed`). A forced re-freeze of different bytes drops it and
  checks the new bytes against the budget, unless `--allow-long-lines` is
  passed again. The flag records the exemption only for a source with a line
  over the budget; on one that fits, it does nothing.

### Line-width budget

```toml
[preprocessor.listings]
max-line-width = 72
```

With a budget set, `freeze` refuses a source with a line wider than it,
naming each line and its width, and writes nothing. That includes an
unchanged re-freeze of a listing frozen before the budget was set. `verify` reports the
same thing for every frozen listing, which catches the ones frozen before
the budget was set. A line is measured as the listing renders it:

- `CALLOUT:` marker lines are left out, since the renderer strips them. The
  comment prefix comes from the frozen file's extension, as for callouts.
- Width is display columns, not bytes or characters: an em dash is one
  column, a CJK character two, a combining accent none, and a tab advances
  to the next multiple of 8 (the browser default, which mdBook's theme
  keeps).
- Only the listing's own lines count. A `{{#diff}}` adds a one-column
  `+`/`-` marker and a sliced include adds a two-line header, neither of
  which is measured.

A listing over the budget gets an exemption, `allow-long-lines = true` on
its record in `listings.toml`. Pass `--allow-long-lines` to `freeze`, or,
for a listing already frozen whose source has moved on, add the line by
hand. A budget that isn't a positive integer fails both commands.

This is what lets the book detect drift: the manifest's recorded hash must
match the frozen file's actual hash.

## `verify`

```
mdbook-listings verify [--book-root <BOOK_ROOT>]
```

Errors (any one exits non-zero):

1. Each frozen listing still matches its recorded SHA-256 in `listings.toml`.
2. Every `{{#include}}` path and `{{#diff}}` tag operand in the book's
   markdown resolves to a manifest record.
3. Every `<tag>.callouts.toml` sidecar names a frozen listing.
4. No chapter holds an unescaped `{{#include listings/…}}` or
   `{{#include snippets/…}}` that the preprocessor skips (in inline code, say,
   or written as `{{ #include …}}`) but mdBook's `links` preprocessor would
   still expand. The build fails on the same thing. Escape it as
   `\{{#include …}}`, on one line, to show the directive literally.
5. No `{{#include listings/…}}` or `{{#include snippets/…}}` outside a fence
   shares its line with other text. The build rejects the same thing, since
   the code block the include expands to has to start a line.
6. With `max-line-width` set, every frozen listing whose record doesn't
   allow long lines fits it (see [Line-width budget](#line-width-budget)).

Warnings (reported on stderr, exit stays 0):

- A frozen file under `src/listings/` that no manifest record claims.
- A `live:` diff operand — that spot tracks moving source by choice, and the
  audit lists where.
- A `CALLOUT:` marker whose badge renders in some chapter but that no
  `{{#callout}}` directive references. Only rendered markers count: a marker
  in a frozen version no chapter shows, on a line outside every include's
  slice, or on a context line of a diff produces no badge and is not
  reported.
- A `caption=`, `label=` or `show-provenance=` argument on a `snippets/`
  include. Snippets emit no locator anchor, so they are never numbered and
  none of the three renders. A label is the worst of them: nothing shows it,
  and a `{{#listing-ref}}` aimed at it fails the build from another file.
- A sliced include whose end line is a `CALLOUT:` marker. The marker
  annotates the line after it, which the slice excludes, so the badge
  attaches to nothing. Applies to `listings/` and `snippets/` includes; a
  marker on the file's last line is exempt (its badge clamps the same way in
  every rendering).

**Run it in CI** to catch drift before readers see stale code.

## `list`

```
mdbook-listings list [--book-root <BOOK_ROOT>]
```

Prints one tab-separated row per manifest entry:

```
<tag>\t<frozen-path>\t<source-path>
```

Order matches manifest insertion order. Useful for scripting or for discovering
existing tags before choosing a new one.

## The `listings.toml` manifest

The CLI owns this file — **do not hand-edit it.** Shape:

```toml
version = 1

[[listing]]
tag = "main-v1"
source = "../src/main.rs"
frozen = "src/listings/main-v1.rs"
sha256 = "d61bebc89bb132ae602d25b487d2016609a1ef978d8169ecea3eadae19f0a471"
```

- `tag` — unique key (see identity model).
- `source` — the original file the snapshot was taken from (relative to the
  manifest).
- `frozen` — the committed snapshot the book embeds.
- `sha256` — integrity hash that `verify` enforces.
