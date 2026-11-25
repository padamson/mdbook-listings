# mdbook-listings CLI reference

The binary is `mdbook-listings`. Author-facing subcommands are `install`,
`freeze`, `verify`, `remove`, and `list`. (`supports` exists too but is the internal
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
  within the book. It may contain `.` (`parse-0.13.4`), but not at its start
  or end, not two in a row, and not ending in `.callouts`; `/` and `\` are
  refused. When omitted, it is derived from the source basename:
  `<basename>-v1` for the first freeze, and the version suffix is bumped
  (`-v2`, `-ver2`, `-rev2`, `-version2`, matching the existing series style)
  on subsequent freezes of the same basename.
- `--force` — overwrite an existing frozen copy that has the **same tag but
  different bytes**. Without it, a conflicting re-freeze is rejected.
- `--update-refs` — with `--force`, rewrite the line ranges that chapters
  include from this listing to where those lines are in the new bytes (see
  [Identity model](#identity-model)).
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
- Line ranges into the listing are pinned to the old bytes too. A forced
  re-freeze diffs the old bytes against the new and finds every
  `{{#include}}` or `{{#rustdoc_include}}` range (`:A:B`, `:A:`, `::B`,
  `:N`) that reaches the frozen file. If any would move, it
  lists each as `chapter:line  path:A:B → C:D` and writes nothing. With
  `--update-refs` it rewrites them and freezes, printing `moved:` for each.
  It warns (`check the range`) about a range whose first or last line
  changed, which it snaps inward, or that gained a line just outside an
  end, which it leaves outside. A range with no line left stays as written
  (`fix it by hand`). `{{#diff}}` ranges and sidecar `line`s are not
  carried yet.
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
3. Every `<tag>.callouts.toml` sidecar names a frozen listing, and each
   operand of an `<old>..<new>.callouts.toml` diff sidecar does.
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
7. Every `{{#callout <label>}}` in a chapter `SUMMARY.md` lists names a
   callout that a listing in that chapter defines, inline or in a sidecar.
   The build looks nowhere else, so citing a label from another chapter's
   listing fails it. (Not checked in 0.3.0 and earlier, where only the
   build catches it.)

Warnings (reported on stderr, exit stays 0):

- A frozen file under `src/listings/` that no manifest record claims.
- A `live:` diff operand — that spot tracks moving source by choice, and the
  audit lists where.
- A `CALLOUT:` marker whose badge renders in some chapter but that no
  `{{#callout}}` directive references. Only rendered markers count: a marker
  in a frozen version no chapter shows, on a line outside every include's
  slice, or on a context line of a diff produces no badge and is not
  reported.
- A `caption=`, `label=`, `show-provenance=`, `width=` or `wrap=` argument
  on a `snippets/` include. Snippets emit no locator anchor, so they are
  never numbered and none of them has an effect. A label is the worst of them: nothing shows it,
  and a `{{#listing-ref}}` aimed at it fails the build from another file.
- A sliced include whose end line is a `CALLOUT:` marker. The marker
  annotates the line after it, which the slice excludes, so the badge
  attaches to nothing. Applies to `listings/` and `snippets/` includes; a
  marker on the file's last line is exempt (its badge clamps the same way in
  every rendering).

**Run it in CI** to catch drift before readers see stale code.

## `remove`

```
mdbook-listings remove <TAG> [--book-root <BOOK_ROOT>]
```

(Added after 0.3.0. On 0.3.0 itself the subcommand does not exist.)

Retires a listing the book no longer uses, such as one whose source file
was deleted. It drops the tag's `[[listing]]` record from `listings.toml`,
deletes its frozen file, and deletes any sidecar under `src/listings/`
that names the tag: its own `<tag>.callouts.toml`, and any diff sidecar
`<old>..<new>.callouts.toml` with the tag on either side. It prints
`removed: <tag>` and one `deleted: <path>` line per file.

It refuses, changing nothing, while any chapter still uses the listing:
a `{{#diff}}` with the tag as an operand, or any mdBook file directive
(`{{#include}}`, `{{#rustdoc_include}}`, `{{#playground}}`) that reads the
frozen file or one of those sidecars, by whatever path from the chapter's
directory. The error names each use as `<chapter>:<line>`. Take those out of the prose, then run `remove` again.
A frozen file already deleted by hand is not an error: `remove` drops the
record and reports nothing deleted for it.

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

The CLI owns this file — **do not hand-edit it.** To take a record out,
use `remove`. Shape:

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
