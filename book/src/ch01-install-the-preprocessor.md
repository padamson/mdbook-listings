# Install the Preprocessor

```admonish note title="This chapter is mid-flight"
All eight slices plus the refactor have shipped (see the
Outside-in narrative below) — every AC is exercised by at
least one test in the suite. The chapter still needs a Final
state section embedding the latest frozen tags + the wrap-up
steps (promote scaffold sections out of the HTML comment, close
the `TODO(ch01-ship)` markers in ch. 0 and ch. 2).
```

## Story

> As a book author, I want one command that wires mdbook-listings into
> my book so that I don't have to hand-edit configuration or hunt down
> assets to start using the tool.

## Acceptance criteria

1. After install runs successfully against a book, building that book
   invokes mdbook-listings as a preprocessor without further author
   intervention.
2. After install runs successfully against a book, the HTML build
   picks up the CSS asset that styles mdbook-listings's output.
3. Install is idempotent: a second run on an already-installed book
   makes no further changes and confirms to the author that nothing
   changed.
4. Install preserves the rest of the book's existing configuration —
   comments, formatting, and the order of any already-registered
   preprocessors and outputs are untouched; only entries relevant to
   mdbook-listings are added.
5. Install run in a directory without a valid book configuration is
   rejected with a diagnostic identifying what was expected and not
   found.
6. If mdbook-admonish is also registered in the book, install places
   mdbook-listings *before* it in the preprocessor chain so the
   callout → admonish-note pipeline produces correctly styled PDF
   output.

## The slice — outside-in narrative outline

Anticipated commits:

| Slice | What it adds |
|---|---|
| 1/8 | Failing integration test asserting ACs 1+2 via post-install disk state: a minimal fixture book's `book.toml` gains a `[preprocessor.listings]` entry, references the bundled CSS asset in `[output.html].additional-css`, and the asset itself is written to the book root. Fails because install is a stub. (Asserting AC 1 by actually running `mdbook build` is deferred — it would couple the test to having mdbook on PATH in CI.) |
| 2/8 | Bundle the CSS asset into the binary at compile time (`include_bytes!`). Unit test: asset is non-empty + matches an expected sentinel. CSS contents stay a placeholder until ch. 4 (Callouts) settles the badge styling. |
| 3/8 | TOML round-trip primitive (read `book.toml`, mutate, write back preserving comments + ordering, via `toml_edit`). Unit-tested on synthetic input strings — no filesystem. |
| 4/8 | Add the `[preprocessor.listings]` registration. Unit test for AC 3 (idempotency) on top of slice 3. |
| 5/8 | Copy the CSS asset to `<book-root>/mdbook-listings.css` and add it to `[output.html].additional-css`. Unit test for the additional-css addition (AC 2 in the synthetic-config form). |
| 6/8 | Wire slices 2–5 into the `install` CLI handler. Slice 1's integration test now passes for ACs 1+2. AC 3 (idempotency) is pinned by slice 4's unit test. |
| 7/8 | Reject missing book config with a diagnostic (AC 5). New integration test. |
| 8/8 | Enforce ordering relative to mdbook-admonish if present (AC 6). Unit test on synthetic configs with admonish present / absent / already-correctly-ordered. Integration test in a fixture book with admonish registered after a stub preprocessor. |
| refactor | Optional. |

## Outside-in narrative

### Slice 1 — failing integration test

The first slice introduces a CLI-level integration test that
drives `install` against a minimal fixture book. The test body
delegates setup and assertions to a `MinimalFixtureBook` helper
so it reads as the scenario rather than the mechanics:

```rust
{{#include listings/install-tests-v1.rs}}
```

The test is `#[ignore]`'d so the green-build pre-commit chain
stays passing while `install` is still a stub. It was run once
locally first and confirmed to fail at the install invocation
(`error: 'mdbook-listings install' is not yet implemented`); the
ignore reason names the condition for unskipping. A later slice
wires up the install handler and removes the ignore.

### Slice 2 — bundle the CSS asset

Slice 2 introduces the first piece of code the integration test will
eventually need: the CSS bytes that `install` will copy to the book
root. The asset is compiled into the binary via [`include_bytes!`]
so a `cargo install mdbook-listings` produces a self-contained
binary with nothing external to fetch.

A new `install` module declares the constant and a sentinel string
that unit tests assert is present in the bundled bytes (so a build
that strips or replaces the asset fails loudly):

```rust
{{#include listings/install-v1.rs}}
```

The asset itself is intentionally a placeholder — real callout
styling depends on choices the **Render Inline Callouts** story
(ch. 4) hasn't made yet. The placeholder carries only the sentinel
string the unit tests look for:

```css
{{#include listings/install-css-v1.css}}
```

`src/lib.rs` gains one line — `pub mod install;` — so the rest of
the crate can reach the new module:

```rust
{{#include listings/lib-v2.rs}}
```

The unit tests run as part of the regular suite and pass; the
integration test from slice 1 is still `#[ignore]`'d because
`install` doesn't yet do anything with the bundled asset.

### Slice 3 — TOML round-trip primitive

Slice 3 stands up the primitive that lets later slices mutate
`book.toml` while preserving its formatting: a `BookConfig`
newtype around `toml_edit::DocumentMut`. Two unit tests pin the
guarantees the wrapper has to keep — round-tripping a config
without mutation is byte-identical to the input (preserving
comments and entry ordering), and invalid TOML is rejected with
a diagnostic.

`Cargo.toml` gains `toml_edit` as a runtime dep:

```toml
{{#include listings/cargo-toml-v1.toml}}
```

The install module now declares the primitive alongside the CSS
asset bundling from slice 2. **What's new in `install-v2`
compared to `install-v1`:** the `BookConfig` struct (with
`#[derive(Debug)]` so test failures format readably), its `parse`
and `render` methods, two new tests
(`book_config_round_trip_preserves_comments_and_ordering` and
`book_config_parse_rejects_invalid_toml`), and the imports those
need (`anyhow::{Context, Result}`, `toml_edit::DocumentMut`).
Everything else — the CSS constants and their tests — is
unchanged from `install-v1`.

```rust
{{#include listings/install-v2.rs}}
```

The integration test from slice 1 is still `#[ignore]`'d.
`BookConfig` is plumbing — slice 4 wires it up to add the
`[preprocessor.listings]` registration that satisfies the test's
first assertion.

### Slice 4 — register the `[preprocessor.listings]` entry

Slice 4 adds the `BookConfig` method that satisfies the chunk of
AC 1 visible from `book.toml`: a `[preprocessor.listings]` entry
with `command = "mdbook-listings"`. Two unit tests pin (a) that
the entry is added with the right command value and (b) that the
operation is idempotent — a second call on an already-registered
config produces identical rendered output (this is the unit-test
form of AC 3).

**What's new in `install-v3` compared to `install-v2`:** the
`register_listings_preprocessor` method on `BookConfig`, the
`Item, Table` imports it needs from `toml_edit`, and two new tests
(`book_config_register_listings_preprocessor_adds_entry` and
`book_config_register_listings_preprocessor_is_idempotent`).
Everything else — the CSS constants, the `BookConfig` parse and
render methods, and their tests — is unchanged from `install-v2`.

```rust
{{#include listings/install-v3.rs}}
```

The integration test from slice 1 is still `#[ignore]`'d. The
register method handles the `[preprocessor.listings]` half of
the post-install disk state; slice 5 adds the matching
`additional-css` registration for the CSS asset, and slice 6
wires both into the install handler so the integration test
goes green.

### Slice 5 — copy the CSS asset and register it

Slice 5 covers the other half of AC 2: `[output.html]` gets
`additional-css = ["./mdbook-listings.css"]` so mdbook's HTML
build picks the asset up, and the on-disk copy of the asset
itself lands at `<book-root>/mdbook-listings.css`.

The TOML mutation is a `BookConfig::register_listings_css`
method on the same newtype as the preprocessor registration; the
file copy is a free function `write_css_asset(book_root)` that
writes [`CSS_ASSET`] (from slice 2) to the conventional
filename. A new `CSS_ASSET_FILENAME` constant ties the two
together so they can't drift out of sync. Two unit tests pin
the additional-css side: the entry is added with the right
relative path, and the operation is idempotent (no duplicate
entries on a second call).

**What's new in `install-v4` compared to `install-v3`:** the
`CSS_ASSET_FILENAME` constant, the `write_css_asset` free
function, the `register_listings_css` method on `BookConfig`,
two new tests
(`book_config_register_listings_css_adds_entry`,
`book_config_register_listings_css_is_idempotent`), and the
imports they need (`std::fs`, `std::path::Path`, plus
`toml_edit::{Array, Value}` added to the existing import
line). Everything else — the CSS constants, the parse/render
methods, the preprocessor-registration method, and their tests
— is unchanged from `install-v3`.

```rust
{{#include listings/install-v4.rs}}
```

The integration test from slice 1 is still `#[ignore]`'d.
Slice 5 finishes the building blocks; slice 6 sequences
`BookConfig::parse → register_listings_preprocessor →
register_listings_css → render → write back to book.toml`,
plus a `write_css_asset` call, behind the `install` CLI
handler — at which point the integration test passes for
ACs 1+2.

### Slice 6 — wire the install handler

Slice 6 sequences the building blocks from slices 2–5 behind the
CLI: an `install(book_root)` orchestrator function in
`src/install.rs` reads `book.toml`, parses it, calls
`register_listings_preprocessor` and `register_listings_css`,
writes the file back if anything changed, and writes the CSS
asset if the on-disk copy differs from the bundled bytes. It
returns an `InstallOutcome` enum (`Installed` or `Unchanged`)
so the CLI can confirm to the author whether the run was a
no-op (the user-visible half of AC 3).

`src/main.rs`'s `Command::Install` arm calls into the
orchestrator and prints either "installed mdbook-listings into
…" or "mdbook-listings already installed in …; nothing
changed" based on the outcome.

`tests/install.rs` drops the `#[ignore]` attribute on
`install_registers_preprocessor_and_writes_css`. The test now
runs as part of the regular suite and passes — confirming
ACs 1+2 end-to-end.

**What's new in `install-v5` compared to `install-v4`:** the
`install` orchestrator function and the `InstallOutcome` enum.
The bodies of constants, `write_css_asset`, the `BookConfig`
methods, and all the existing tests are unchanged. Doc comments
throughout were trimmed to a why-only style — restating function
names or describing the body in prose is dropped — but the code
itself is the same as `install-v4`.

```rust
{{#include listings/install-v5.rs}}
```

**What's new in `main-v2` compared to `main-v1`:** the
`Command::Install` arm now calls
`mdbook_listings::install::install`, branches on
`InstallOutcome`, and prints one of two messages. The
`use mdbook_listings::install::{InstallOutcome, install};`
import is added. Everything else — the other subcommands
(`Supports`, `Freeze`, `Verify`, the no-subcommand preprocessor
stub) and the `main`/`run`/`supports` functions — is unchanged
from `main-v1`.

```rust
{{#include listings/main-v2.rs}}
```

**What's new in `install-tests-v2` compared to `install-tests-v1`:**
the `#[ignore = "..."]` attribute is removed; the
`#[test]` attribute and the body are unchanged.

```rust
{{#include listings/install-tests-v2.rs}}
```

The integration test from slice 1 is no longer ignored. Slices
7 and 8 add the remaining ACs (missing-config diagnostic for
AC 5, mdbook-admonish ordering for AC 6).

### Slice 7 — reject missing book config

Slice 7 makes the missing-`book.toml` case fail with a helpful
diagnostic instead of a generic "reading book config" wrapper.
Before slice 7, running install in a directory with no
`book.toml` produced something like:

```
error: reading book config at /path/to/book.toml
Caused by: No such file or directory (os error 2)
```

After slice 7:

```
error: book.toml not found at /path/to/book.toml — install requires
an existing mdbook book directory; run `mdbook init` first.
```

A new integration test
`install_rejects_missing_book_config` runs install against a
fresh `TempDir` (no `book.toml`) and asserts the binary exits
non-zero with `book.toml not found` in stderr.

**What's new in `install-v6` compared to `install-v5`:** the
`fs::read_to_string` call inside `install` is now a `match`
that special-cases `io::ErrorKind::NotFound` with a tailored
diagnostic; other I/O errors still go through the existing
`with_context` path. Everything else — every other function,
constant, struct, and test — is unchanged.

```rust
{{#include listings/install-v6.rs}}
```

**What's new in `install-tests-v3` compared to
`install-tests-v2`:** the `predicates::str::contains` import and
the new `install_rejects_missing_book_config` test. The existing
test (`install_registers_preprocessor_and_writes_css`) and the
`MinimalFixtureBook` helper are unchanged.

```rust
{{#include listings/install-tests-v3.rs}}
```

The suite now runs 24 tests (10 install-related, 14 from other
modules). Slice 8 takes care of AC 6 (mdbook-admonish ordering).

### Slice 8 — order before mdbook-admonish

Slice 8 satisfies AC 6: when `[preprocessor.admonish]` is
already registered in `book.toml`, install adds
`before = ["admonish"]` to the new `[preprocessor.listings]`
entry so mdbook runs listings first. The callout → admonish-note
pipeline (which the **Render Inline Callouts** story will rely
on for PDF output) requires this ordering.

The behaviour is conditional: if admonish is absent, the
`before` field is not added — keeping the registered listings
entry minimal for books that don't use admonish.

Three new unit tests cover the synthetic-config cases (admonish
present, admonish absent, idempotent re-run with admonish
present); a new integration test
`install_orders_before_admonish_when_admonish_is_registered`
sets up a fixture book with `[preprocessor.admonish]`, runs
install, and asserts the resulting `book.toml` has the
`before = ["admonish"]` ordering plus both preprocessor entries.

**What's new in `install-v7` compared to `install-v6`:** the
`register_listings_preprocessor` method now snapshots whether
`"admonish"` is a key in the preprocessor table before adding
listings, then appends `before = ["admonish"]` to the listings
entry if so. Three new tests in the unit-test module. The
method's existing behaviour (preprocessor entry with command)
and idempotency are unchanged.

```rust
{{#include listings/install-v7.rs}}
```

**What's new in `install-tests-v4` compared to
`install-tests-v3`:** the new
`install_orders_before_admonish_when_admonish_is_registered`
integration test. The other tests and helper struct are
unchanged.

```rust
{{#include listings/install-tests-v4.rs}}
```

The suite now runs 27 tests (14 install-related, 13 from other
modules). Every Acceptance criterion has at least one test
covering it. The story is feature-complete; the optional
refactor slice that follows tidies a small repetition that
accumulated across slices 4–8, and the wrap-up chore after
that promotes the remaining HTML-comment scaffold to chapter
body and adds the **Final state** section.

### Refactor

With every test green, the refactor commit tidies a small
repetition that accumulated during slices 4–8 and was
deliberately not addressed during the red-green slices:
`register_listings_preprocessor` and `register_listings_css`
both walked into nested `[preprocessor]` and `[output.html]`
tables via the same six-line
`entry().or_insert_with(...).as_table_mut().expect(...)` chain.
Extracted into a `subtable_mut(parent, key)` helper so each
call site shrinks from six lines to one.

The chapter has this section on purpose — the methodology in
ch. 0 calls out the refactor commit as part of the outside-in
cycle, and shipping a tiny one here makes that visible. Larger
refactors are still possible later (e.g., splitting install.rs
once it grows substantially); none felt load-bearing right now.

**What's new in `install-v8` compared to `install-v7`:** the
private `subtable_mut` helper at module scope, and both
`register_*` methods rewritten to use it. No public API
change; no behaviour change. The full test suite (14 install
tests, 27 overall) passes byte-for-byte the same as before.

```rust
{{#include listings/install-v8.rs}}
```

The chapter is feature- and quality-complete. The wrap-up
chore lifts the remaining scaffold sections out of the HTML
comment and writes the **Final state** section.

<!--
The sections below are scaffold for the writer of the slices. They get
moved out of this HTML comment as the corresponding work lands.

Slice-by-slice promotion plan (what comes out of this comment when):

  * slice 1 lands: DONE — narrative section now lives above this
    HTML comment block.
  * slice 2 lands: DONE — slice 2 sub-section added.
  * slice 3 lands: DONE — slice 3 sub-section added.
  * slice 4 lands: DONE — slice 4 sub-section added.
  * slice 5 lands: DONE — slice 5 sub-section added.
  * slice 6 lands: DONE — slice 6 sub-section added; slice 1's
    integration test is no longer ignored.
  * slice 7 lands: DONE — slice 7 sub-section added.
  * slice 8 lands: DONE — slice 8 sub-section added.
  * refactor lands: DONE — Refactor sub-section added.
  * wrap-up chore (separate commit): rewrite the top-of-chapter
    admonish note to past tense; promote `## Notes for
    implementers` and `## What this slice will not solve` out
    of this comment into chapter body; populate `## Final
    state` with `\{{#include}}`s of the latest `-vN` listings
    (lib-v2, main-v2, install-v7, install-tests-v4, and the
    cargo-toml-v1 + install-css-v1 + freeze-v1 + manifest-v1
    that haven't been re-frozen since their last touch); close
    the `TODO(ch01-ship)` markers in ch. 0 and ch. 2.
  * final slice (or refactor): rewrite the top-of-chapter admonish
    note (it currently says "no slice has shipped yet"); promote
    `## Notes for implementers` and `## What this slice will not
    solve` out of this comment into chapter body; populate
    `## Final state` with `\{{#include}}`s of the `-v2` frozen
    listings (and the new `install-v1`); close the corresponding
    `TODO(ch01-ship)` markers in ch. 0 and ch. 2.

## Notes for implementers

  * `toml_edit` is the standard crate for read-modify-write of a TOML
    file while preserving comments and ordering. mdbook-admonish's own
    install is a good reference implementation; we already studied it
    while writing the Freeze chapter.
  * The CSS file content depends on the rendering chosen for inline
    callouts — coordinate with the **Render Inline Callouts** story so
    the CSS install ships matches the badges and details that story
    actually renders.
  * **Expected listing overlap with ch. 2 (Freeze a Listing).** This
    story modifies `src/lib.rs` and `src/main.rs` — files that are
    *already* frozen by ch. 2 under `-v1` tags. Per the per-slice
    freeze discipline, each slice that touches one of these files
    freezes a new `-vN` tag (`lib-v2` shipped in slice 2;
    `main-v2` will land when the install handler wires up). Until
    **Show Diffs Between Slices** ships (ch. 3), the chapter
    embeds the full `-vN` listings rather than diffs against the
    previous version. Readers of ch. 1 and ch. 2 in sequence see
    the freeze-related code twice; the duplication is the cost of
    shipping before the diff primitive exists, and goes away as a
    one-line cleanup once diffs are available.

## What this slice will not solve (anticipated)

  * No uninstall command. Authors who want to remove mdbook-listings
    edit `book.toml` by hand.
  * No upgrade flow. When the bundled CSS asset version bumps, authors
    re-run install, which overwrites the asset.
  * No detection of pre-existing conflicting configurations. If the
    book already has a different preprocessor named `listings`, install
    refuses (a stronger AC for slice 5).
-->
