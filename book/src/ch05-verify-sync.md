# Verify Sync with Source

<!--
Story scaffold — populated by the work that implements the `verify`
subcommand.

## Story (placeholder — tighten before committing slice 1)

> As a book author, I want a single command that warns me when the
> book's latest frozen listings have fallen out of sync with the
> current source files they were snapshotted from, so that I can
> run it in CI and have my build fail when I've refactored code
> without refreshing the listings that should have tracked the
> refactor.

## Acceptance criteria (placeholder — tighten before implementation)

  Primary (the drift check — this is the headline behavior):

  1. For each source file that has been frozen at some point, the
     latest frozen version is compared against the current content
     of the source file. If they differ, the operation fails with a
     diagnostic naming the source, the latest tag, and a summary of
     the difference.
  2. If every latest frozen listing matches its current source, the
     operation succeeds.
  3. A source file that has been removed since it was frozen is
     reported as drift, not as an unhandled error.

  Secondary (sanity checks):

  4. The set of recorded freezes is internally consistent: every
     record corresponds to an actual frozen file.
  5. The integrity hashes recorded with each freeze match the
     bytes of the frozen files, catching post-freeze tampering
     with frozen content.
  6. Every chapter reference to a frozen listing resolves to an
     actual record.
  7. Frozen files that are not recorded (orphans) produce a
     warning but do not cause the operation to fail.

  Interaction with stability-defeating chapter references:

  8. Chapter references that compare a frozen listing against the
     current source (rather than against another frozen listing)
     defeat the stability guarantee that freeze provides, and are
     flagged with a warning.

## The slice — outside-in narrative outline

Anticipated commits:

  slice 1/N: Failing integration test that sets up a fixture book
             with a frozen listing whose source has drifted (one
             line added), and asserts `verify` exits non-zero with
             a useful diagnostic.
  slice 2/N: Walk the manifest to compute "latest tag per source
             path". Unit test on synthetic manifests with multiple
             tags per source.
  slice 3/N: Byte comparison of latest-tag frozen file vs current
             source file. Unit test for match, mismatch, source-
             missing.
  slice 4/N: Diff summarization — "lines changed: N, added: M,
             removed: K". Re-use the diff primitive from ch. 2.
             Unit test.
  slice 5/N: Wire into the `verify` CLI handler. Integration test
             for AC 1 passes.
  slice 6/N: Sanity checks (ACs 4, 5, 6, 7). Added as additional
             passes; each emits a diagnostic but only AC 1 and AC
             3 cause a non-zero exit.
  slice 7/N: `live:` scan (AC 8). Re-uses the chapter walker from
             slice 6.
  optional refactor slice.

## Notes for implementers

  * "Latest tag per source" definition matters. Current lean: last
    `[[listing]]` entry for that source path, in manifest order.
    Alternatives considered: tag suffix comparison (`-v2 > -v1`)
    — rejected because it couples semantics to a naming
    convention. Entry order is explicit; tag naming is a
    convention the author chooses.
  * The sanity checks exist largely for human reassurance. The
    drift check (AC 1) is what gives CI a useful signal. Keep the
    diagnostic text clear about which is which.
  * `{{#include}}` directives may include anchors and line-range
    suffixes (`\{{#include foo.rs:bar}}`,
    `\{{#include foo.rs:1:10}}`). For AC 6 we only care about the
    path component; ignore the rest.
  * **Composition note (narrative arc).** This is the first
    chapter built with all three primitives — freeze (ch. 2),
    diffs (ch. 3), and callouts (ch. 4) — already available. The
    outside-in narrative should use diffs between slice-tagged
    listings to show evolution and callouts to annotate the
    interesting lines as they appear, end-to-end. After this
    chapter the methodology is fully self-supporting; subsequent
    stories follow the same pattern with no chicken-and-egg
    notes to add.

## What this slice will not solve (anticipated)

  * No deep verify (compile check / test run of the frozen
    listings). That's a separate, much bigger story for a later
    release.
  * No auto-remediation. Verify reports; author decides whether to
    refreeze, make a new tag, or leave drift in place.
  * No per-chapter verify (scoped to one chapter). Whole-book or
    nothing.
-->

Placeholder — this chapter's story has not been shipped yet.
