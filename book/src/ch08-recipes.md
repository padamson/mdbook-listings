# Recipes

Unlike the story chapters, this one isn't a single user story. It's a
grab-bag of short reference recipes that crystallised as the primitives
got used — most of them on this very book.

## Migrate a chapter that keyed a numbered list to a code block

The motivating case (from a downstream book): a fenced code block followed
by a numbered list whose items point at lines by position ("3. The call on
line 12 …"). Edit the code and every number is wrong, silently. Callouts
fix that — the prose keys to a label, not a line.

1. Freeze the source under a memorable tag:
   `mdbook-listings freeze --tag <tag> path/to/source.rs`.
2. Replace the live include with the frozen one:
   `\{{#include listings/<tag>.rs}}`.
3. Add `// CALLOUT: <label>` markers (inline, or via a sidecar TOML for
   code you don't own) at the lines the list pointed to.
4. Delete the numbered list; rewrite the prose to reference callouts by
   label, e.g. `\{{#callout <label>}}`.
5. Run `mdbook-listings verify` — it should exit 0.

The list can no longer drift out of sync, because there are no line
numbers left to drift.

## Manage multiple revisions of the same source

When to freeze a new tag versus `--force`-overwriting an existing one:

- **A chapter needs the old code to show a different historical moment**
  → new tag (`foo-v2`). Both revisions coexist in the manifest.
- **Fixing the frozen copy before the chapter has shipped** → `--force`.
- **Fixing it after readers have seen it** → don't `--force`; make a new
  tag and update the chapter. `--force` silently rewrites history.

A frozen snapshot is meant to be immutable, so guard that:

- Don't edit a frozen listing in place. If you must change what it shows,
  re-freeze (which updates the recorded hash) rather than hand-editing the
  file (which leaves the hash stale). `mdbook-listings verify` catches the
  stale hash, but re-freezing is the fix.
- Keep formatters and linters off `src/listings/`. A tool that rewrites a
  frozen file silently breaks its recorded hash. This book scopes its
  `typos` hook with `exclude: '^book/src/listings/'` for exactly that
  reason.

## Run `verify` in CI

Install the binary, then run verify against the book. The build fails if a
snapshot has drifted, a reference doesn't resolve, or a sidecar is
dangling:

```yaml
      - name: Verify frozen listings
        run: mdbook-listings verify --book-root book
```

This book dogfoods that step (it installs the crate from its own path
rather than crates.io, since it *is* the crate); a downstream book would
`cargo install mdbook-listings` first.

## Run `verify` as a pre-commit hook

The hook this book runs ([prek](https://github.com/j178/prek) /
pre-commit), scoped to fire when anything under `book/` is staged:

```yaml
  - repo: local
    hooks:
      - id: mdbook-listings-verify
        name: mdbook-listings verify
        description: Frozen snapshots intact, references resolve, no dangling sidecars
        entry: cargo run --quiet -- verify --book-root book
        language: system
        pass_filenames: false
        files: ^book/
```

It uses `cargo run` because this repo *is* the crate, so the hook
exercises the just-built binary. A downstream book that installed
`mdbook-listings` swaps the entry for the binary directly:
`entry: mdbook-listings verify --book-root book`.

## Order `mdbook-listings` before other preprocessors

The include splicer must run before mdbook's built-in `links` preprocessor,
or `links` expands `\{{#include}}` directives to file bytes before the
callout splicer ever sees their `CALLOUT:` markers. The callouts → admonish
pipeline for PDF output also needs `mdbook-listings` to run first. Express
both in `book.toml`:

```toml
[preprocessor.listings]
before = ["admonish", "links"]
```
