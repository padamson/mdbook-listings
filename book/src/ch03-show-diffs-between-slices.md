# Show Diffs Between Slices

<!--
Story scaffold — first story to be built outside-in. Populated by the
work that stands up the preprocessor pipeline and adds the `{{#diff}}`
directive.

## Story (placeholder — tighten before committing slice 1)

> As a book author, I want to render a unified diff between two
> frozen listings of the same file in a chapter so that I can walk
> the reader through slice-by-slice evolution without repeating the
> full file contents on every slice.

## Acceptance criteria (placeholder — tighten before implementation)

  1. An author can request a diff between two frozen listings to be
     rendered inline in a chapter at the point of request.
  2. The diff is computed against the frozen bytes of the listings,
     not against any current source files. Once a chapter is built,
     subsequent source changes do not change the rendered diff.
  3. A diff request that names a listing not present in the freeze
     records fails the build with a diagnostic identifying the
     missing listing and where it was referenced.
  4. A diff between listings of different file types fails the
     build. Cross-type diffs are not a meaningful operation in this
     book's context.
  5. A diff between byte-identical listings produces a clear "no
     changes" indication, not an empty diff block.
  6. Adding diff rendering to a chapter does not change any other
     content in that chapter.
  7. An author can opt in to a diff that compares a frozen listing
     against the current content of a source file. This defeats
     the stability guarantee that freeze provides and is flagged
     by the **Verify Sync with Source** story.

## The slice — outside-in narrative outline

Anticipated commits (subject to change as slices are actually
written):

  slice 1/N: Failing integration test that asserts ACs 1, 3, 5 on a
             minimal fixture book. Nothing to make it pass yet.
  slice 2/N: Preprocessor entry point in main.rs (no-subcommand arm
             now reads stdin JSON, writes stdout JSON, passes book
             through unchanged). Register [preprocessor.listings]
             in book/book.toml. Integration test now gets further
             — fails differently.
  slice 3/N: Detect {{#diff a b}} in chapter text. Unit-test the
             directive parser in isolation first, then wire it into
             the preprocessor.
  slice 4/N: Resolve tags against the manifest (re-use
             manifest::Manifest::load). Error paths for ACs 3, 4.
  slice 5/N: Compute the unified diff. Unit-test the diff rendering
             with synthetic inputs first.
  slice 6/N: Splice the rendered diff back into the chapter text.
             Integration test now passes.
  slice 7/N: `live:<path>` handling (AC 8). Unit test + integration
             test + plumb through the preprocessor.
  optional refactor slice.

## Notes for implementers

  * The mdbook crate gives us `mdbook::preprocess::CmdPreprocessor`
    helpers for the JSON round-trip.
  * For unified diff rendering, `similar` is the widely-used crate
    in the Rust ecosystem; `imara-diff` is a lighter alternative.
    Decide in slice 5, preferring fewer transitive deps.
  * Directive-parsing note: watch out for `{{#diff}}` references
    that appear inside *prose* (as examples for the reader, shown
    literally) — mdbook already handles literal `{{#include}}` via
    backslash escape (`\{{#include ...}}`); we should match that
    convention.
  * Per-chapter tag namespacing (e.g.,
    `book/src/listings/<chapter>/...`) is on the backlog as a
    separate tiny story. Don't entangle with this one.
  * **Composition note (narrative arc).** This is the first
    chapter that gets to demonstrate two primitives composing:
    freeze (from ch. 2) plus diff. The outside-in narrative
    section of this very chapter, once enough slices exist, can
    use diffs between its own slice-tagged listings — eating its
    own dog food in the same chapter that ships the primitive.
    Earlier chapters could only show freeze in isolation; later
    chapters get to add callouts on top.

## What this slice will not solve (anticipated)

  * No syntax highlighting on diff output (renders as plain
    unified-diff text). Syntax-highlighted diffs are a separate
    small story.
  * No per-line callouts / anchors on diff output (callouts are a
    later story; they'll layer cleanly).
  * No three-way diffs or diffs across file renames.
-->

Placeholder — this chapter's story has not been shipped yet.
