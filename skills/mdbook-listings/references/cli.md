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

### Identity model

- **tag = human identity, sha256 = integrity.**
- Re-freezing **identical** bytes under the same tag is a **no-op**.
- Re-freezing **different** bytes under an existing tag is **rejected** unless
  `--force` is passed.

This is what lets the book detect drift: the manifest's recorded hash must
match the frozen file's actual hash.

## `verify`

```
mdbook-listings verify [--book-root <BOOK_ROOT>]
```

Checks consistency across three things:

1. Each frozen listing still matches its recorded SHA-256 in `listings.toml`.
2. Every `{{#include}}` reference in the book's markdown resolves.
3. The manifest and the frozen files agree.

Exits non-zero on any mismatch. **Run it in CI** to catch drift before readers
see stale code.

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
