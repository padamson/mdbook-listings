# Install the Preprocessor

<!--
Story scaffold — first story in shipping order under CD ordering, but not
yet shipped. Built outside-in (no chicken-and-egg here: install doesn't
depend on diffs or callouts, so the chapter can show a full slice
narrative when implemented).

## Story (placeholder — tighten before committing slice 1)

> As a book author adopting mdbook-listings, I want a single command that
> configures my existing book to use mdbook-listings, so that I can start
> freezing listings and rendering callouts without hand-editing the book's
> configuration or manually copying CSS assets.

## Acceptance criteria (placeholder — tighten before implementation)

  1. Running the install command in a directory that already contains a
     valid book configuration registers mdbook-listings as a preprocessor
     of that book and places the CSS asset where the book's HTML build
     picks it up.
  2. After install, building the book invokes mdbook-listings as a
     preprocessor without further author intervention.
  3. The install operation is idempotent: a second run on an already-
     installed book makes no further changes and confirms to the author
     that nothing changed.
  4. The install operation preserves any existing book configuration —
     comments, formatting, and the order of any already-registered
     preprocessors and outputs are not disturbed; only the entries
     relevant to mdbook-listings are added.
  5. Running the install command in a directory without a valid book
     configuration is rejected with a diagnostic identifying what was
     expected and not found.

## The slice — outside-in narrative outline

Anticipated commits:

  slice 1/N: Failing integration test that copies a fixture book to a
             tempdir, runs install, and asserts post-conditions on the
             book configuration and the CSS asset on disk.
  slice 2/N: Read/modify/write of the book configuration that preserves
             comments and ordering. Unit-tested on synthetic inputs.
  slice 3/N: Preprocessor registration entry. Unit test for AC 3
             (idempotency) on top of slice 2.
  slice 4/N: CSS asset bundling (compile-time) plus copy at install
             time.
  slice 5/N: Diagnostic for missing book configuration (AC 5).
  optional refactor slice.

## Notes for implementers

  * `toml_edit` is the standard crate for read-modify-write of a TOML
    file while preserving comments and ordering. mdbook-admonish's own
    install is a good reference implementation; we already studied it
    while writing the Freeze chapter.
  * The CSS file content depends on the rendering chosen for inline
    callouts — coordinate with the **Render Inline Callouts** story so
    the CSS install ships matches the badges and details that story
    actually renders.
  * Adding the preprocessor entry must place it *before* mdbook-admonish
    in the preprocessor chain when admonish is also registered, so
    callout-emitted admonish-note blocks get styled correctly in PDF.
    Decide whether install enforces this ordering or merely warns.
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

Placeholder — this chapter's story has not been shipped yet.
