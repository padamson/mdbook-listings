# Render Inline Callouts

<!--
Story scaffold — populated by the work that adds CALLOUT marker
handling to the preprocessor established in the Show Diffs story.

## Story (placeholder — tighten before committing slice 1)

> As a book author, I want to attach inline annotations and named
> reference points to specific lines of a frozen listing so that my
> prose can stay keyed to the code even when the code evolves under
> a new tag.

## Acceptance criteria (placeholder — tighten before implementation)

  Inline form (callout markers in the source itself):

  1. A frozen listing whose language has a recognised inline-
     marker syntax can carry callout markers. When the chapter is
     rendered to HTML, each marker produces a numbered badge at
     the marker's position and an expandable annotation reachable
     from the badge.
  2. The same listing rendered to PDF produces a styled note for
     each callout, ordered to match the listing.
  3. A callout marker may declare just a label, with no
     accompanying annotation. In that case a numbered badge
     appears at the marker's position but no expandable
     annotation is rendered. This form serves purely as a stable
     cross-reference target.

  Out-of-band form (callouts attached to a listing without
  modifying its bytes):

  4. Callouts can be attached to a frozen listing without
     modifying the listing itself — i.e., authors can annotate
     code they do not own, or that they want to keep
     callout-free in the source.
  5. Inline-form and out-of-band callouts compose: both sets
     render. Label collisions across the two sources fail the
     build.

  Cross-reference and numbering:

  6. Chapter prose can reference a callout by its label, and the
     reference renders as the same numbered badge, hyperlinked
     back to the listing occurrence.
  7. Badge numbers are assigned ordinally within each listing and
     reset between listings. Adding or removing a callout above
     an existing one renumbers the badges visually but does not
     break label-based references.

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

Anticipated commits:

  slice 1/N: Failing integration test for AC 1 in YAML, Rust, and
             one other language using a minimal fixture book. The
             multi-language coverage is the integration test's
             whole point — a YAML-only test would pass against a
             YAML-only implementation, defeating the multi-
             language commitment.
  slice 2/N: Comment-syntax table + generic CALLOUT parser
             parameterised on prefix. Unit-tested on strings for
             every prefix in the initial table; verifies body and
             no-body forms; ignores malformed.
  slice 3/N: HTML emitter — badge at line, details nearby. Wires
             parser into preprocessor so slice 1's integration
             test passes for with-body (AC 1) across the
             initial-table languages.
  slice 4/N: Label-only inline form (AC 3). Small addition to
             emitter.
  slice 5/N: Cross-reference directive `{{#callout <label>}}`
             (ACs 6, 10).
  slice 6/N: typst-pdf emitter — admonish-note block after code
             block (AC 2). Second integration test for PDF output.
  slice 7/N: Sidecar TOML loader + overlay logic (ACs 4, 5).
             Unit tests for collision detection.
  optional refactor slice.

## Notes for implementers

  * The comment-syntax table:

      .yaml/.yml/.toml/.py/.sh/.bash/.tf/.hcl  →  "#"
      .rs/.c/.h/.cpp/.hpp/.js/.ts/.jsx/.tsx     →  "//"
      .sql                                      →  "--"

    Languages can be added later with a chore commit (one new
    table entry plus one parser test per language).

  * Badge style (①②③ vs [a][b][c] vs footnote-style superscript)
    is cosmetic; decide in slice 3 and stick with it.
  * Introduce a `SupportedRenderer` enum here. Today
    `src/main.rs::supports()` matches a string literal
    `"html" | "typst-pdf"` and the supported list lives only
    in that `matches!` arm. The preprocessor for this story
    needs to switch on renderer (HTML emits `<details>`;
    typst-pdf emits an admonish-note block) — that's when the
    enum starts paying for itself. `supports()` should be
    refactored to delegate to the same enum so the supported
    list has one home. The CLI surface stays a `String` (the
    mdbook protocol expects `mdbook-listings supports
    <whatever>` to accept any input and respond via exit
    code, not via clap parse error).
  * Numbering scope: ordinal within a single listing is the
    simple choice and matches LaTeX equation numbering. Global
    numbering would make cross-listing references visually
    distinctive but adds a second coordinate system. Stick with
    per-listing.
  * Resolve the "what does it mean for two listings to share a
    label name?" question before slice 5 lands. Current lean:
    labels are namespaced per listing; `{{#callout <label>}}`
    must be unambiguous inside the chapter it appears in (error
    if two listings in the chapter share a label).
  * Sidecar format sketch (subject to change):

      # book/src/listings/manifest-v1.callouts.toml
      [[callout]]
      line = 47
      label = "upsert-order"
      body = "Preserves insertion order on replacement."

      [[callout]]
      line = 62
      label = "empty-manifest"
      # no body field → bare anchor

  * CSS: the HTML output needs light styling — badge shape,
    details-collapsed-by-default, hover. Ship a minimal
    `mdbook-listings.css` as part of this story or as the Install
    recipe (which also exists as a chore-level commit).

## What this slice will not solve (anticipated)

  * Block-comment-only languages (CSS, plain Markdown). They use
    sidecar form for callouts. Adding inline parsing for
    block-comment syntaxes is a future story.
  * Richer callout bodies (multi-paragraph, inline code, nested
    markers). Initial bodies are single-line plain text.
  * Callouts on diff output — the diff rendering from the
    previous story emits bare unified-diff text; making diffs
    callout-aware is a later story.

## Retrospective application to earlier chapters

After this story ships, a chore-level follow-up walks back through
the listings already frozen by ch. 1 (Install) and ch. 2 (Freeze)
and adds callouts to them — preferentially via sidecar files,
since the source code itself doesn't need to change. The point is
to demonstrate, in place, how callouts replace the conventional
inline-comment style of code documentation: the prose lives in
the chapter, the labels make the prose addressable from the
source position, and the source stays comment-light. This is not
a new user story; it's an application of the now-available
primitive to the book's own back-catalogue.
-->

Placeholder — this chapter's story has not been shipped yet.
