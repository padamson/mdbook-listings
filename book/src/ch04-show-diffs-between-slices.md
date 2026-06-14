# Show Diffs Between Slices

```admonish note title="This chapter has shipped"
The story shipped across six outside-in slices, a refactor
slice, a follow-on red-green-refactor loop (slice 8) that came
out of dogfooding the primitive on this very chapter, and a
wrap-up chore. Every Acceptance criterion is exercised by at
least one test in the suite. Read the chapter top-to-bottom for
the methodology view; the **Outside-in narrative** sub-sections
embed each frozen tag at the slice that introduced it, so the
latest version of each file is in the slice that touched it last
(slice 8 for `src/diff.rs`, `src/main.rs`, and `tests/diffs.rs`).
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
5. Adding a `\{{#diff}}` directive to a chapter does not change any
   other content in the chapter (the preprocessor is a precise
   in-place splice).
6. The chapter that *documents* the directive can show its own
   syntax verbatim by putting the literal `\{{#diff …}}` inside an
   inline code span (`` `…` ``) or a fenced code block (` ``` ` /
   `~~~`) — the preprocessor skips any directive whose start byte
   falls inside either. Backslash-escape (`\{{#diff …}}`) is *not*
   a reliable escape mechanism: mdbook's built-in `links`
   preprocessor strips the leading `\` from any `\{{#…}}` pattern
   before any custom preprocessor runs, so the `\` never reaches
   our splicer in the real pipeline.
7. An author can opt in to a diff against a live source file via
   `live:<path>` in either operand. The path is resolved relative
   to the chapter's source markdown directory — the same convention
   mdbook uses for `\{{#include}}` — so a chapter at `book/src/foo.md`
   can write `live:foo.txt` to reach `book/src/foo.txt`, or
   `live:../../src/lib.rs` to reach the crate's source. Doing so
   defeats the freeze stability guarantee for that diff and is
   flagged by `mdbook-listings verify` (the *Verify Frozen Listings*
   story, ch. 7).

## The slice — outside-in narrative outline

The story ships as eight slices (six initial outside-in slices, a
refactor, and a follow-on red-green that came out of dogfooding the
primitive) plus a wrap-up chore:

| Slice | What it adds |
|---|---|
| 1 | Failing integration test asserting AC 1 against a tempdir fixture book. The test pipes a hand-built `(PreprocessorContext, Book)` envelope to the binary's no-subcommand arm and asserts on a ` ```diff ` fence in the round-tripped chapter content. The arm becomes a no-op pass-through that round-trips the book unchanged, so the assertion fails — the test is `#[ignore]`'d to keep the green-build pre-commit chain passing while later slices grow the directive parser, tag resolver, diff renderer, and splicer. ACs 4 and 5 get their own assertions in slice 5. |
| 2 | Directive parser as a pure unit. New `src/diff.rs` exposes `parse_directives` returning byte-span-tagged `DiffDirective`s. Unit-tested in isolation; not yet wired into the preprocessor. |
| 3 | Tag resolution. `diff::resolve` looks each operand up in `Manifest` (re-using `Manifest::find` from ch. 2) and produces a structured error for missing tags carrying enough context for the splicer to format an AC-3 diagnostic. Unit-tested. |
| 4 | Unified diff computation via the `similar` crate. `diff::render` takes the resolved bytes plus labels and produces unified-diff text; identical bytes produce a "no changes" notice rather than an empty block (AC 4). Unit-tested with synthetic byte pairs. |
| 5 | Splicer wires slices 2–4 into the no-op preprocessor: every `\{{#diff …}}` directive is replaced with a fenced ` ```diff ` block, the parser learns to skip directives inside fenced code blocks (initial AC 6 — so chapters can quote literal directive examples), and `cargo run -- install --book-root book` registers `[preprocessor.listings]` in our own `book/book.toml` so the book exercises the diff primitive on every build. Slice 1's integration test goes green; AC 5 gets its own integration test pinning surrounding-content invariance. |
| 6 | `live:<path>` operand (initial AC 7). Recognised in either operand position; the resolver reads the live file from disk relative to `book_root`. |
| 7 (refactor) | Remove `parse_escapes`, the escape branch in `splice_chapter`, and the matching tests — dead code in the real mdbook pipeline. Tidy duplication that emerged across slices 2–6. |
| 8 | Tighten ACs 6 and 7 in response to dogfooding. Inline code spans (`` `…` ``) join fenced blocks as a directive-skip context (AC 6) — `\{{#diff a b}}` in inline backticks no longer crashes the build. `live:<path>` resolution moves from book-root-relative to chapter-source-relative (AC 7), matching mdbook's `\{{#include}}` convention. Both come from real friction points hit while writing this very chapter. |
| wrap-up | Update [`ROADMAP.md`](https://github.com/padamson/mdbook-listings/blob/main/ROADMAP.md) to mark the diff primitive shipped. |

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
`\{{#diff …}}` directives with byte spans. It's a pure
function — no IO, no manifest, no diff library — so its unit tests
pin its behaviour completely without touching disk.

A new `src/diff.rs` module declares `DiffDirective { left, right,
span }` and the free function `parse_directives(content) ->
Vec<DiffDirective>`:

```rust
{{#include listings/diff-v1.rs}}
```

The parser walks `content` byte-wise, looking for `\{{#diff`. When it
finds one, it checks the byte before for a backslash (the escape AC
6 calls out — kept here as a *skip*, not a strip; the splicer in
slice 5 owns the rewrite that drops the leading `\` so the literal
directive renders to the reader). It then locates the next `}}`,
splits the inner text on whitespace, and only yields a directive
when there are exactly two operands. Wrong-arity directives
(`\{{#diff a}}`, `\{{#diff a b c}}`) are silently skipped — surfacing
"that's the wrong number of arguments" diagnostics is the resolver's
job in slice 3, where the chapter source path and line number are
already in scope.

Six unit tests pin the contract: well-formed directives parse and
their spans cover the whole `\{{#diff …}}` substring; multiple
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

### Slice 3 — tag resolution + missing-tag diagnostic

Slice 3 turns each parsed `DiffDirective` into the *bytes* a diff
renderer can consume: it looks the operand up in the manifest
(re-using `Manifest::find` from ch. 2), reads the frozen file from
disk, and returns a `ResolvedDiff` carrying both halves' bytes plus
labels for the unified-diff headers. When an operand is unknown or
its frozen file is missing, the resolver returns a typed
`ResolveError` carrying the offending tag name — the splicer in
slice 5 wraps that with the chapter source path and 1-based line
number derived from the directive's byte span, which together
satisfy AC 3.

**What's new in `diff-v2` compared to `diff-v1`:** the `ResolvedDiff`
struct, the `ResolveError` / `ResolveErrorKind` types with manual
`Display` and `Error` impls, the `resolve` and `resolve_operand`
functions, the `crate::manifest::Manifest` import they need, and
four new tests covering the happy path plus the three failure
shapes (unknown left tag, unknown right tag, frozen file absent
from disk). The tests share a `fixture` helper that materialises a
tempdir with two stub frozen files plus an in-memory `Manifest`
pointing at them; building the manifest in memory rather than via
`Manifest::load` keeps the unit tests independent of the manifest
file format. The parser, its tests, and the module's existing
imports are unchanged.

```rust
{{#include listings/diff-v2.rs}}
```

The resolver stops at the first failing operand: if the left tag
is unknown, the right tag is not consulted. That keeps slice 5's
diagnostic naming a single missing tag rather than two, matching
how an author would actually fix the chapter (find the typo, fix
the typo, rebuild — the second tag's resolution happens on the
rebuild). It also means tests for the right-operand failure path
have to use a known left operand, which is what the
`resolve_returns_unknown_tag_error_for_missing_right_operand` test
does.

`live:<path>` operands (AC 7) currently fall through to the
`UnknownTag` arm — `manifest.find("live:src/foo.rs")` returns
`None`. Slice 6 inserts a prefix check before the manifest lookup
and reads the file directly, leaving this slice's resolver
unchanged for the all-frozen happy path.

The integration test from slice 1 is still `#[ignore]`'d. Slice 4
adds the renderer that turns a `ResolvedDiff` into unified-diff
text; slice 5 wires parser → resolver → renderer into the
preprocessor and removes the `#[ignore]`.

### Slice 4 — unified diff computation via `similar`

Slice 4 adds the third and final pure unit slice 5's splicer needs:
`render(left, right, left_label, right_label) -> String`, which
turns two `&str` halves into unified-diff text. The actual diff
algorithm is delegated to the [`similar`] crate
(`TextDiff::from_lines(...).unified_diff().header(a, b)`); the
function's only original behaviour is the AC-4 short-circuit —
identical inputs return a one-line `(no changes between left and
right)` notice rather than the empty string `similar` would
otherwise emit, which would render as a fence body that looks
broken to a reader.

`Cargo.toml` gains `similar = "2"`. **What's new in `cargo-toml-v3`
compared to `cargo-toml-v2`:** the single `similar = "2"` line in
alphabetical position inside `[dependencies]`. Everything else is
unchanged.

```toml
{{#include listings/cargo-toml-v3.toml}}
```

`src/diff.rs` grows the `render` function plus four unit tests
covering the four shapes that matter: differing inputs produce a
header-and-hunks unified diff; identical inputs short-circuit to
the no-changes notice; two empty inputs do the same; pure
additions render with `+` prefixes and no spurious `-` lines.
This slice also tightens the doc comments on the parser, resolver,
and error types added in slices 2–3 to drop forward references to
later slices and acceptance-criteria numbers — the chapter
narrative is the right place to talk about story structure, the
source code is the right place to talk about behaviour. The
behaviour itself is unchanged.

```rust
{{#include listings/diff-v3.rs}}
```

[`similar`]: https://docs.rs/similar

The integration test from slice 1 is still `#[ignore]`'d. All
three pure-unit pieces (parse, resolve, render) now exist in
`diff.rs`; slice 5 wires them into `preprocess` and removes the
`#[ignore]`.

### Slice 5 — splicer + book registration + slice-1 test goes green

Slice 5 wires the three pure-unit pieces from slices 2–4 into the
preprocessor and registers it in our own book so this very chapter
starts rendering with diffs from this commit forward. The
sub-section's three listings are the first in the book to be
embedded as `\{{#diff …}}` rather than full file contents.

`src/diff.rs` grows three things:

* `parse_escapes` — byte positions of `\` characters that
  immediately precede an unescaped `\{{#diff` substring; the
  splicer drops each one without touching the directive that
  follows.
* `SpliceError` — pairs the `ResolveError` from slice 3 with the
  chapter source path and 1-based line number, so a missing-tag
  diagnostic reads `src/ch99-foo.md:5: no listing with tag
  \`missing-tag\`` rather than just naming the tag.
* `splice_chapter` — gathers directive and escape edits in one
  pass, sorts by start offset, and stitches the output by copying
  the gaps verbatim. Edits anchor to byte spans of the *original*
  chapter text, so the splicer never has to think about offset
  shifts as it rewrites.

The parser also gains code-fence awareness. Without it, registering
the preprocessor in our own `book.toml` would break the build the
moment a chapter quoted a frozen test fixture: the included `.rs`
file's literal `\{{#diff …}}` strings (with real-looking tag
operands) would be parsed as real directives, and the resolver
would fail to find those tags in our manifest. The parser now tracks
`` ``` ``/`~~~` fences line-by-line and skips any directive whose
start byte falls inside an open fence — the same rule that lets
this very narrative quote `\{{#diff …}}` syntax in fenced examples
without the splicer eating them.

{{#diff diff-v3 diff-v4 caption="Fence-aware directive scanning"}}

`src/main.rs`'s `preprocess` function goes from a no-op
pass-through to the actual transformation: load the manifest from
`<ctx.root>/listings.toml`, walk every `BookItem::Chapter` via
`book.for_each_mut`, hand the chapter content to `splice_chapter`,
and write the mutated book back to stdout. `for_each_mut` doesn't
let the closure return errors, so the splicer's failures are
captured into a local `Option<anyhow::Error>` checked after the
walk.

{{#diff main-v3 main-v4}}

`tests/diffs.rs` drops the `#[ignore]` on the slice-1 acceptance
test (the splicer makes it pass) and gains two more integration
tests pinning the surrounding-content invariance and the
backslash-escape behaviour at the binary boundary. The fixture is
rebuilt to mirror a real mdbook book root: `listings.toml` at the
tempdir top, frozen files under `src/listings/`, matching what
`Manifest::load(&ctx.root)` actually reads. The slice-1 fixture
put those under a redundant `book/` subdirectory, which worked
while the preprocessor was a pass-through but doesn't now.

{{#diff diffs-tests-v1 diffs-tests-v2}}

`book/book.toml` gains `[preprocessor.listings]` (with
`before = ["admonish"]` because admonish is registered too) and
`[output.html].additional-css` picks up `mdbook-listings.css`. The
edit was made by running `cargo run -- install --book-root book`
— the install handler from ch. 1 is idempotent, so re-running it
in future builds is harmless.

The integration suite is fully green: 53 tests pass, 0 skipped.
The diff primitive is end-to-end functional and the book itself
exercises it.

The `parse_escapes` helper, the escape-handling branch in
`splice_chapter`, and the `escaped_diff_directive_is_left_literal_minus_the_backslash`
integration test are dead code in the real pipeline (mdbook's
`links` preprocessor strips the leading `\` before our binary
ever runs — see the AC 6 note above). They earn their keep only
when our binary is driven directly via stdin, which isn't a
supported use case. The refactor slice removes them and
re-freezes the affected files; until then they document the
fact-of-life by their visible presence.

### Slice 6 — `live:<path>` operand

Slice 6 closes out the initial AC 7. `resolve_operand` now
recognises the `live:` prefix and reads the named file from disk
(slice 6 resolved against `book_root`; slice 8 changed this to the
chapter's source directory, matching `\{{#include}}` semantics).
The operand's full text (including the `live:` prefix) becomes the
unified-diff header label, so a reader can tell at a glance which
side is frozen and which side is live.

A new `ResolveErrorKind::LiveFileMissing` variant carries the
absolute path that failed to read so the splicer's chapter-located
diagnostic stays specific. Two unit tests cover the happy path
and the missing-file error; one new integration test in
`tests/diffs.rs` drives a `\{{#diff …}}` whose right operand is
`live:compose-live.yaml` end-to-end through the binary and
asserts on the `+++ live:…` header and the `+`/`−` lines
reflecting the live bytes. The `MinimalDiffsBook` fixture grows
a `write_live_file` helper for the same.

{{#diff diff-v4 diff-v5}}

{{#diff diffs-tests-v2 diffs-tests-v3}}

To dogfood it, here is the chapter rendering a `live:` diff
between the `diff-v5` tag (frozen above) and the live
`src/diff.rs` on disk at build time. The path is relative to the
chapter's own source directory (`book/src/`, post-slice-8),
so `../../src/diff.rs` walks up two levels to the repo root and
back into the crate's `src/`:

{{#diff diff-v5 live:../../src/diff.rs}}

When slice 6 shipped, the diff above rendered as the "no changes"
notice — the frozen `diff-v5` was byte-identical to the live
`src/diff.rs`. Slice 7 (the refactor) put the first real drift in
it, matching the `diff-v5` → `diff-v6` listing below; every later
slice that touches `src/diff.rs` widens it, so the block above
shows the live file as of whatever build you're reading. The
chapter source never changes; only the live source on disk does.
That's the use case for `live:` in a nutshell: notice
intended-and-unintended drift, no chapter edit required.

The freeze stability guarantee that AC 7 calls out as
*defeated* by `live:` is, in this story, just words on a page —
the *Verify Frozen Listings* story (ch. 7) is what surfaces a
warning when a chapter uses `live:` operands. v0.1.0 ships the
directive; ch. 7 ships the warning, as `mdbook-listings verify`'s
`live:` audit.

### Slice 7 — refactor

With slices 1–6 in the bag and the integration suite green, the
refactor slice tidies what the outside-in walk left behind.
Three changes:

* **Dead code removed.** `parse_escapes`, the escape-stripping
  branch in `splice_chapter`, and the
  `escaped_diff_directive_is_left_literal_minus_the_backslash`
  integration test all go. They tested a code path that can't
  fire in the real mdbook pipeline (mdbook's `links` preprocessor
  strips backslash-escapes upstream of any custom preprocessor —
  see AC 6). The parser's defensive backslash-skip stays: it's
  cheap, harmless, and covers the case of someone driving the
  binary directly with a hand-built envelope.

* **`for_each_directive_position` inlined.** The fence-tracking
  helper had two callers (parse_directives + parse_escapes); with
  `parse_escapes` gone it's down to one. Inlining cuts ~25 lines
  of indirection and puts the fence logic right where it's used.

* **`splice_chapter` simplified.** Without the second edit
  source (escapes), the function no longer needs to collect
  edits, sort them, and stitch in a separate pass. `parse_directives`
  already returns directives in span-order, so the splicer just
  walks them once and copies through the gaps.

The dogfood payoff lands without any chapter-source edit: the
live: diff in the slice 6 sub-section above (the `\{{#diff …}}`
whose right operand is `live:../src/diff.rs`) no longer renders
as the "no changes" notice — it now shows the real delta between
the slice-6 freeze of `src/diff.rs` and the post-refactor source.
Same directive, different output, because the live source
drifted. That's the use case for `live:` made visible.

{{#diff diff-v5 diff-v6}}

{{#diff diffs-tests-v3 diffs-tests-v4}}

53 → 51 tests (the three `parse_escapes` unit tests, the
`splice_chapter_strips_leading_backslash_from_escaped_directives`
unit test, and the `escaped_diff_directive_is_left_literal_minus_the_backslash`
integration test are gone). All 51 still pass.

### Slice 8 — extend ACs 6 and 7 from dogfooding

Writing this very chapter surfaced two real friction points that
the original ACs 6 and 7 didn't capture, so slice 8 is a fresh
red-green-refactor loop on top of the refactor:

* **AC 6: inline code spans are now a directive-skip context too.**
  Twice while drafting ch. 3 a literal `\{{#diff a b}}` inside
  inline backticks (`` `…` ``) crashed the build — the splicer saw
  it, tried to resolve the operands, and failed. The fix is one
  block in `parse_directives`: count backticks before the
  directive's start byte on the same line; if odd, we're inside an
  inline code span — skip. AC 6's wording widens from "fenced code
  blocks" to "inline code spans or fenced code blocks".

* **AC 7: `live:<path>` resolves relative to the chapter's source
  directory, not `book_root`.** Slice 6's resolution against
  `book_root` is awkward: every `live:` reference in this very
  chapter (which lives at `book/src/ch04-…md`) had to spell out
  `live:../src/diff.rs` rather than the more natural
  `live:../../src/diff.rs` (mdbook's own `\{{#include}}` already
  uses chapter-relative paths). The fix threads a `chapter_dir`
  parameter through `splice_chapter` → `resolve` → `resolve_operand`,
  and `preprocess()` in `main.rs` computes it as
  `ctx.root.join(&ctx.config.book.src).join(<chapter source dir>)`.

Three failing tests drove the loop (two in `src/diff.rs`, one in
`tests/diffs.rs`), then the implementation, then green: 54 tests
pass.

{{#diff diff-v6 diff-v7}}

The `main.rs` change threads `chapter_dir` in two spots a few lines apart.
At the default three-line context they render as two separate hunks; widening
to `context=6` merges them into one block that shows the whole top of
`preprocess` — the function header through the threaded argument — so the diff
reads on its own.

```admonish note title="Since v0.1.0: context window"
The `context=N` argument is a v0.1.1 addition, used on the diff just below.
It sets the unified-diff context radius (default 3). See
[Changes since v0.1.0](changes-since-0.1.0.md).
```

{{#diff main-v4 main-v5 context=6}}

{{#diff diffs-tests-v4 diffs-tests-v5}}

The slice-6 sub-section's live: directive
(`live:../src/diff.rs` as it shipped in slice 6) now reads
`live:../../src/diff.rs` to match the new resolution. The change
is honest about the post-slice-8 state of the chapter; readers
building older revisions of the book would see the old form.

This slice is a worked example of the methodology working as
intended: the original outside-in walk (slices 1–6) shipped a
correct, tested primitive. *Using* the primitive on the chapter
that documents it surfaced spec gaps — gaps not visible from
inside the original ACs. Rather than retconning slice 7's
refactor, slice 8 is its own loop with new ACs, new failing
tests, new impl. The chapter is longer for it, and the lesson
lands.

## What this story does not solve

* **Diff highlighting in typst-pdf.** mdbook-typst-pdf 0.7.x has no
  `diff` language entry and emits the block as plain monospace.
  Authors building PDF see uncolored diffs until a later story
  plumbs Typst color macros around `+`/`−` lines (or upstream adds
  a `diff` language). Tracked as a separate small story.
* **Language-aware syntax highlighting *inside* the diff** (e.g.,
  Rust syntax overlaid on `+`/`−` coloring). Neither highlight.js
  nor typst-pdf does this; would need server-side rendering with
  `syntect`. Separate story; sketched on the v0.3.0 roadmap.
* **Per-line callouts and anchors on diff output.** Covered by
  ch. 4 (*Render Inline Callouts*); the diff primitive emits a
  bare ` ```diff ` fence that ch. 4 layers callouts on top of.
* **Three-way diffs or diffs across renames.** No current driver
  in the dogfood book. Would surface on demand.
* **The verify-side warning when `live:<path>` is used.** Ships
  with ch. 7 (*Verify Frozen Listings*); this chapter only ships
  the directive itself. v0.1.0 binds the two together at the
  release boundary.
* **Per-chapter tag namespacing**
  (`book/src/listings/<chapter>/...`). On the backlog as a
  separate tiny story; the global flat namespace is fine while
  the book is small and tags are short.
* **End-to-end browser-side rendering assertions.** This story's
  integration tests verify the JSON our binary emits, but nothing
  exercises the rendered HTML in a real browser. ch. 4 (*Render
  Inline Callouts*) starts there — its slice 1 stands up a
  Playwright harness and a failing spec asserting on a rendered
  callout in the browser, because the outermost layer for
  callouts is the rendered DOM. Once that harness exists,
  retrospective browser assertions for the diff primitive (e.g.,
  highlight.js applying `+`/`−` coloring) are easy follow-ons if
  desired.
