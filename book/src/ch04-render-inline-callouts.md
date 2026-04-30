# Render Inline Callouts

```admonish note title="This chapter is in progress"
The story is being built outside-in, and the first slice is the
furthest *out* this book has reached: a real Chromium driven by
[playwright-rs](https://crates.io/crates/playwright-rs) asserts on
the rendered DOM of a callout in this very chapter. The test is
`#[ignore]`'d until later slices grow the parser, the HTML
emitter, and the actual `CALLOUT:` markers in a frozen listing
that the test asserts against. Each slice ships as one commit;
the **Outside-in narrative** sub-section grows by one sub-section
per slice.
```

## Story

> As a book author, I want to attach inline annotations and named
> reference points to specific lines of a frozen listing so that my
> prose can stay keyed to the code even when the code evolves under
> a new tag.

## Acceptance criteria

Inline form (callout markers in the source itself):

1. A frozen listing whose language has a recognised inline-marker
   syntax can carry callout markers. When the chapter is rendered
   to HTML, each marker produces a numbered badge at the marker's
   position and an expandable annotation reachable from the badge.
2. The same listing rendered to PDF produces a styled note for
   each callout, ordered to match the listing.
3. A callout marker may declare just a label, with no
   accompanying annotation. In that case a numbered badge appears
   at the marker's position but no expandable annotation is
   rendered. This form serves purely as a stable cross-reference
   target.

Out-of-band form (callouts attached to a listing without modifying
its bytes):

4. Callouts can be attached to a frozen listing without modifying
   the listing itself — i.e., authors can annotate code they do
   not own, or that they want to keep callout-free in the source.
5. Inline-form and out-of-band callouts compose: both sets render.
   Label collisions across the two sources fail the build.

Cross-reference and numbering:

6. Chapter prose can reference a callout by its label, and the
   reference renders as the same numbered badge, hyperlinked back
   to the listing occurrence.
7. Badge numbers are assigned ordinally within each listing and
   reset between listings. Adding or removing a callout above an
   existing one renumbers the badges visually but does not break
   label-based references.

Passthrough and robustness:

8. A frozen listing whose language has no recognised inline-
   marker syntax is rendered unchanged for inline-form parsing.
   Out-of-band callouts still apply — they don't depend on the
   listing's language.
9. A comment that resembles a callout marker but does not parse
   cleanly is left unchanged in the rendered output (no silent
   misparse).
10. A chapter reference to a callout label that does not exist
    fails the build with a diagnostic that names the missing
    label and the chapter.

## The slice — outside-in narrative outline

The story ships as seven slices plus a refactor and a wrap-up
chore. Slice 1 is the outermost layer — a browser-driving
acceptance test — and the inner slices fill in the layers needed
to satisfy it.

| Slice | What it adds |
|---|---|
| 1 | playwright-rs harness. A failing `#[tokio::test] #[ignore]` in `tests/e2e_callouts.rs` launches Chromium against the rendered ch. 4 HTML and asserts a `[data-callout-badge]` element exists. The test fails (no callouts in ch. 4 yet, no parser, no HTML emitter); ignore keeps the green-build chain passing while later slices grow the rest. |
| 2 | Comment-syntax table + generic `parse_callouts` parser parameterised on prefix. Pure unit tests for every prefix in the initial table; verifies body and no-body forms; ignores malformed. |
| 3 | HTML emitter — badge at line, `<details>` nearby — wires parser into preprocessor. Slice 1's `#[ignore]` comes off and the test goes green for AC 1. `SupportedRenderer` enum extracted here. |
| 4 | Label-only inline form (AC 3). Small addition to emitter; new playwright-rs test asserting the bare-anchor case. |
| 5 | Cross-reference directive `{{#callout <label>}}` (ACs 6, 10). New playwright-rs test asserting the prose-rendered badge is hyperlinked to the listing-rendered badge anchor. |
| 6 | typst-pdf emitter — admonish-note block after the code block (AC 2). Non-browser; assertion is visual or assert_cmd-on-PDF-bytes — decided in the slice. |
| 7 | Sidecar TOML loader + overlay logic (ACs 4, 5). New playwright-rs test asserting a sidecar-only callout renders correctly when the source has no marker. |
| refactor | Optional. |
| wrap-up | Update `ROADMAP.md` to mark the callouts primitive shipped, materialize "What this story does not solve". |

## Outside-in narrative

### Slice 1 — playwright-rs harness + failing E2E test

The first slice introduces the outermost-layer test that the rest
of the story races to satisfy: a Rust integration test that
launches a real Chromium via
[playwright-rs](https://crates.io/crates/playwright-rs), navigates
to the rendered `ch04-render-inline-callouts.html` on disk, and
asserts that a `[data-callout-badge]` element exists with non-empty
text content. The test fails today — there's no parser, no HTML
emitter, and no callout-marked listing in this chapter yet.
`#[ignore]` keeps `cargo test` green for the green-build chain;
the author runs `cargo test --test e2e_callouts -- --ignored` once
locally to confirm the test really does fail at the badge
assertion, then commits.

`Cargo.toml` gains two `[dev-dependencies]`: `playwright-rs` (the
Rust bindings) and `tokio` (the async runtime the test uses).

{{#diff cargo-toml-v3 cargo-toml-v4}}

The new test file is `tests/e2e_callouts.rs`. The naming
parallels the other story-scoped integration test files
(`tests/install.rs`, `tests/freeze.rs`, `tests/diffs.rs`); the
`e2e_` prefix flags the harness tier so future readers don't
expect assert_cmd-style assertions from it.

```rust
{{#include listings/e2e-callouts-v1.rs}}
```

The test file is frozen as `e2e-callouts-v1` per the per-slice
freeze discipline. Slice 3 mints `e2e-callouts-v2` when it removes
the `#[ignore]`; subsequent slices that add new tests mint
further versions.

<!--
Scaffolding for later slices — sidecar TOML format sketch,
retrospective application to earlier chapters, and the "What this
story does not solve" section. Materialized in the wrap-up chore
once the story has shipped.

Sidecar format sketch (subject to change):

    # book/src/listings/manifest-v1.callouts.toml
    [[callout]]
    line = 47
    label = "upsert-order"
    body = "Preserves insertion order on replacement."

    [[callout]]
    line = 62
    label = "empty-manifest"
    # no body field → bare anchor

Retrospective application to earlier chapters:

After this story ships, a chore-level follow-up walks back through
the listings already frozen by ch. 1 (Install), ch. 2 (Freeze),
and ch. 3 (Show Diffs) and adds callouts to them — preferentially
via sidecar files, since the source code itself doesn't need to
change. The point is to demonstrate, in place, how callouts
replace the conventional inline-comment style of code
documentation: the prose lives in the chapter, the labels make
the prose addressable from the source position, and the source
stays comment-light. This is not a new user story; it's an
application of the now-available primitive to the book's own
back-catalogue.
-->
