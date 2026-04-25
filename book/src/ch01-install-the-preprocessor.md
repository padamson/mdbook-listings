# Install the Preprocessor

```admonish note title="This chapter is mid-flight"
The Story, Acceptance criteria, and slice list below describe what
this chapter will deliver once its slices land. Slice 1 has shipped
(see the Outside-in narrative below); slices 2–8 and the Final
state section are still pending.
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

<!--
The sections below are scaffold for the writer of the slices. They get
moved out of this HTML comment as the corresponding work lands.

Slice-by-slice promotion plan (what comes out of this comment when):

  * slice 1 lands: DONE — narrative section now lives above this
    HTML comment block.
  * slices 2–8: each adds one sub-section to `## Outside-in
    narrative` describing what changed and what tests passed.
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
    story adds a new module (`src/install.rs`) and modifies
    `src/lib.rs`, `src/main.rs`, and `tests/integration.rs` — files
    that are *already* frozen by ch. 2 under `-v1` tags. This story's
    Final state section will freeze those same files under `-v2` tags
    (`lib-v2`, `main-v2`, `integration-tests-v2`) plus a new
    `install-v1` for the install module itself. Until **Show Diffs
    Between Slices** ships (ch. 3), the chapter will embed the full
    `-v2` listings rather than diffs against `-v1`. Readers of ch. 1
    and ch. 2 in sequence will see the freeze-related code twice; the
    duplication is the cost of shipping before the diff primitive
    exists, and goes away as a one-line cleanup once diffs are
    available.

## What this slice will not solve (anticipated)

  * No uninstall command. Authors who want to remove mdbook-listings
    edit `book.toml` by hand.
  * No upgrade flow. When the bundled CSS asset version bumps, authors
    re-run install, which overwrites the asset.
  * No detection of pre-existing conflicting configurations. If the
    book already has a different preprocessor named `listings`, install
    refuses (a stronger AC for slice 5).
-->
