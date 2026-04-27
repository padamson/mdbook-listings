# Show Diffs Between Slices

```admonish note title="This chapter is in progress"
The story is being built outside-in. Each slice ships as one
commit; the **Outside-in narrative** sub-section grows by one
sub-section per slice. The chapter is read top-to-bottom for
the methodology view; the sub-sections embed each frozen tag
at the slice that introduced it.
```

## Story

> As a book author, I want to render a unified diff between two frozen
> listings of the same file in a chapter, so that I can walk the
> reader through slice-by-slice evolution without repeating the full
> file contents on every slice.

## Acceptance criteria

1. An author can request a diff between two frozen listings (by tag)
   inline in a chapter. The directive renders to a fenced diff block
   at the point of request.
2. Diff bytes are computed from the *frozen* listings on disk, not
   from any current source file. Once a chapter is built, later edits
   to the original sources do not retroactively change the rendered
   diff.
3. A diff request that names a tag not present in `listings.toml`
   (or whose frozen file is missing on disk) fails the build with a
   diagnostic identifying the missing tag, the chapter source path,
   and the 1-based line number of the offending directive within
   that chapter — enough for the author to jump straight to the bad
   directive without grep.
4. A diff between byte-identical listings renders a clear "no
   changes" notice rather than an empty diff block.
5. Adding a `{{#diff}}` directive to a chapter does not change any
   other content in the chapter (the preprocessor is a precise
   in-place splice).
6. Literal `{{#diff ...}}` text in prose can be escaped with a
   leading backslash (`\{{#diff ...}}`), matching mdbook's existing
   `{{#include}}` convention, so the chapter that *documents* the
   directive can show its own syntax verbatim.
7. An author can opt in to a diff against a live source file via
   `live:<path>` in either operand. Doing so defeats the freeze
   stability guarantee for that diff and is flagged later by the
   *Verify Sync with Source* story (ch. 5).

## The slice — outside-in narrative outline

The story ships as six slices plus an optional refactor and a
wrap-up chore:

| Slice | What it adds |
|---|---|
| 1/6 | Failing integration test asserting AC 1 against a tempdir fixture book whose `book.toml` declares the listings preprocessor and whose `book/listings.toml` records two frozen tags. The test pipes a hand-built `(PreprocessorContext, Book)` envelope to the binary's no-subcommand arm and asserts on a ` ```diff ` fence in the round-tripped chapter content. The arm becomes a no-op pass-through that round-trips the book unchanged, so the assertion fails — the test is `#[ignore]`'d to keep the green-build pre-commit chain passing while later slices grow the directive parser, tag resolver, diff renderer, and splicer. ACs 4 and 5 get their own assertions in the slice that gives them something to assert against (slice 5). |
| 2/6 | Directive parser as a pure unit. New `src/diff.rs` exposes `parse_directives` returning byte-span-tagged `DiffDirective`s; respects `\{{#diff ...}}` escapes (AC 6). Unit-tested in isolation; not yet wired into the preprocessor. |
| 3/6 | Tag resolution. `diff::resolve` looks each operand up in `Manifest` (re-using `Manifest::find` from ch. 2) and produces a structured error for missing tags carrying the chapter source path and 1-based line number derived from the directive's byte span (AC 3). Unit-tested. |
| 4/6 | Unified diff computation via the `similar` crate. `diff::render` takes the resolved bytes plus labels and produces unified-diff text; identical bytes produce a "no changes" notice rather than an empty block (AC 4). Unit-tested with synthetic byte pairs. |
| 5/6 | Splicer wires slices 2–4 into the no-op preprocessor. Each directive span is replaced with a fenced ` ```diff ` block carrying the rendered output. The HTML renderer's bundled highlight.js colors `+`/`−`/`@@` lines automatically because of the `diff` info-string. Slice 1's integration test goes green; AC 5 gets its own integration test pinning surrounding-content invariance. |
| 6/6 | `live:<path>` operand (AC 7). Recognised in either operand position; the resolver reads the live file from disk relative to `book_root`. |
| refactor | Optional. |
| wrap-up | Register `[preprocessor.listings]` in our own `book/book.toml` via `cargo run -- install --book-root book` (the install handler from ch. 1 is idempotent). The book itself now exercises the diff primitive end-to-end on every CI build. |

## Outside-in narrative

### Slice 1 — failing integration test + no-op pass-through

The first slice introduces a CLI-level integration test that pipes a
preprocessor envelope to `mdbook-listings`'s no-subcommand arm and
asserts on the round-tripped chapter content. The arm itself becomes
a no-op pass-through — the smallest possible body that still satisfies
the wire format mdbook expects. The test fails because pass-through
doesn't add a diff fence, and is `#[ignore]`'d so the green-build
chain stays passing while slices 2–4 grow the pieces it needs.

`Cargo.toml` gains two runtime deps: `mdbook-preprocessor` (for the
`PreprocessorContext` and `Book` types plus the `parse_input` helper)
and `serde_json` (for the round-trip serialisation that
`parse_input`'s counterpart writes back to stdout). **What's new in
`cargo-toml-v2` compared to `cargo-toml-v1`:** the
`mdbook-preprocessor = "0.5"` and `serde_json = "1"` lines added
inside `[dependencies]` in alphabetical position. Everything else is
unchanged.

```toml
{{#include listings/cargo-toml-v2.toml}}
```

`src/main.rs`'s `preprocess` function used to bail with `not yet
implemented`; it now reads the JSON envelope from stdin via
`mdbook_preprocessor::parse_input`, discards the `PreprocessorContext`
(slice 3 is the first to need it), and writes the book straight back
to stdout via `serde_json::to_writer`. Both calls are fully
qualified so no new `use` statements are needed yet. **What's new in
`main-v3` compared to `main-v2`:** the body of `preprocess` is
replaced with the `parse_input` → `to_writer` round-trip; the doc
comment on `preprocess` is unchanged. Everything else — the `clap`
derive struct, every other subcommand handler, `supports`,
`main`/`run` — is byte-identical.

```rust
{{#include listings/main-v3.rs}}
```

The integration test lives in a new `tests/diffs.rs` (per ch. 0's
"one integration-test file per story" rule). The file contains one
test plus a `MinimalDiffsBook` helper that materialises a tempdir
holding `book.toml`, `book/listings.toml`, and two stub frozen files
under `book/src/listings/`. The helper's `envelope_with_chapter`
method builds the `(PreprocessorContext, Book)` tuple from public
mdbook constructors (`PreprocessorContext::new`, `Chapter::new`,
`Book::new_with_items`) and serialises the pair as a two-element
JSON array — the exact shape mdbook itself sends a preprocessor.

```rust
{{#include listings/diffs-tests-v1.rs}}
```

`#[ignore]` (with a reason that names the slices that close it out)
keeps `cargo nextest run` green while the diff machinery is being
built. The test was confirmed to fail at the assertion, not at the
`assert().success()` step — the pass-through arm parses the envelope,
serialises the book unchanged, and exits zero, so the assertion
that the chapter content contains a ` ```diff ` fence is what's
red.

The `MinimalDiffsBook` fixture is deliberately bigger than the test
needs in slice 1 (the stub frozen files are unused while pass-through
is the whole pipeline). This pays off in slices 3 and 5 when the
resolver and splicer reach for those frozen bytes — the only test
change in slice 5 is removing `#[ignore]`, no fixture rewiring.

`MinimalDiffsBook::root` is currently `#[allow(dead_code)]` for the
same reason: slice 6's `live:<path>` test will need it to write
ad-hoc files into the tempdir. Carrying the accessor here keeps the
helper's surface stable across slices.

### Slice 2 — directive parser as a pure unit

Slice 2 stands up the first piece slice 5's splicer will need: the
parser that turns a chapter's markdown into a list of
`{{#diff <left> <right>}}` directives with byte spans. It's a pure
function — no IO, no manifest, no diff library — so its unit tests
pin its behaviour completely without touching disk.

A new `src/diff.rs` module declares `DiffDirective { left, right,
span }` and the free function `parse_directives(content) ->
Vec<DiffDirective>`:

```rust
{{#include listings/diff-v1.rs}}
```

The parser walks `content` byte-wise, looking for `{{#diff`. When it
finds one, it checks the byte before for a backslash (the escape AC
6 calls out — kept here as a *skip*, not a strip; the splicer in
slice 5 owns the rewrite that drops the leading `\` so the literal
directive renders to the reader). It then locates the next `}}`,
splits the inner text on whitespace, and only yields a directive
when there are exactly two operands. Wrong-arity directives
(`{{#diff a}}`, `{{#diff a b c}}`) are silently skipped — surfacing
"that's the wrong number of arguments" diagnostics is the resolver's
job in slice 3, where the chapter source path and line number are
already in scope.

Six unit tests pin the contract: well-formed directives parse and
their spans cover the whole `{{#diff …}}` substring; multiple
directives in one chapter all parse with correct spans; the escaped
form is skipped; whitespace around operands is tolerated;
wrong-arity directives are skipped; and arbitrary operand strings
(including the future `live:src/foo.rs` shape) are accepted at the
parse layer (the resolver decides what they mean).

`src/lib.rs` gains one line — `pub mod diff;` — so `src/main.rs`
and the integration tests can reach the new module.

**What's new in `lib-v3` compared to `lib-v2`:** the
`pub mod diff;` line, in alphabetical position. Everything else is
unchanged.

```rust
{{#include listings/lib-v3.rs}}
```

The integration test from slice 1 is still `#[ignore]`'d. The
parser is plumbing — slices 3 and 4 add the resolver and renderer
that the splicer in slice 5 wires together to make the assertion
pass.

<!--
  Scaffolding — to be materialized as a final "What this story does
  not solve" section in the wrap-up chore (placed at the end of the
  chapter, matching the ch. 2 convention). Items may be added,
  removed, or pulled into the story as later slices reveal what
  actually shipped vs deferred.

  Candidate deferrals as of slice 1:

  * Diff highlighting in HTML: NOT deferred — we get it free. Tagging
    the emitted fence as ```diff triggers highlight.js's built-in
    `diff` language, which colorizes +/−/@@ lines automatically in
    the HTML build. Slice 5 uses this fence and ships HTML diff
    coloring as part of the story.
  * Diff highlighting in typst-pdf: deferred. mdbook-typst-pdf 0.7.x
    has no `diff` language entry and emits the block as plain
    monospace. Authors building PDF see uncolored diffs until a
    later story plumbs Typst color macros around +/− lines.
  * Language-aware syntax highlighting *inside* the diff (e.g., Rust
    syntax overlaid on +/− coloring) — neither highlight.js nor
    typst-pdf does this; would need server-side rendering with
    `syntect`. Separate story.
  * Per-line callouts/anchors on diff output — covered by ch. 4
    (*Render Inline Callouts*).
  * Three-way diffs or diffs across renames — no current driver.
  * The verify-side warning when `live:<path>` is used ships with
    ch. 5 (*Verify Sync with Source*).
  * Per-chapter tag namespacing (`book/src/listings/<chapter>/...`)
    — on the backlog as a separate tiny story.
-->
