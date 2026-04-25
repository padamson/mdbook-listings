# Recipes

<!--
Grab-bag chapter — unlike the other chapters, this one isn't a
single user story. It's a collection of small reference recipes
that crystallise as workflows emerge.

Candidate recipes (some with concrete implementation work
attached; others are docs-only):

  ## Migrating a chapter that uses a numbered list after a code block

  The motivating use case from t2t issue #6 — walk through the
  before/after and the steps in between:
    1. Freeze the source file with a memorable tag.
    2. Replace the `\{{#include <live-source>}}` with
       `\{{#include listings/<tag>.<ext>}}`.
    3. Add `# CALLOUT` markers (inline or sidecar) at the lines the
       numbered list was keyed to.
    4. Delete the numbered list; rewrite the prose to reference
       callouts by label.
    5. `mdbook-listings verify` — should exit 0.

  ## Managing multiple revisions of the same source file

  When to freeze a new tag (`manifest-v2`) versus `--force`-
  overwriting an existing one:
    * **New chapter needs old code to illustrate a different
      historical moment** → new tag. Both listings coexist in the
      manifest.
    * **Fixing a typo in the frozen copy before the chapter has
      shipped** → `--force`.
    * **Fixing a typo after the chapter has shipped and readers
      have seen it** → don't `--force`; make a new tag; update the
      chapter to reference the new tag. `--force` silently rewrites
      history for readers.

  ## Running `mdbook-listings verify` in CI

  Drop-in GitHub Actions step (to be filled in once the crate is
  on crates.io):

      - name: Verify listings sync
        run: mdbook-listings verify book/

  Exit-nonzero-on-drift behaviour means CI fails when the latest
  frozen listings don't match current source.

  ## `mdbook-listings verify` as a pre-commit hook

  A copy-pasteable `.pre-commit-config.yaml` snippet that runs
  verify on every commit touching `src/` or `book/`. Docs-only in
  the initial release; a later release may add `mdbook-listings
  install --hook` to write the hook automatically.

  ## Interop with other mdbook preprocessors

  Why `mdbook-listings` has to run *before* `mdbook-admonish` in
  the preprocessor chain (the callouts → admonish-note pipeline
  for PDF output requires this ordering), and how to express that
  in `book.toml`.
-->

Placeholder — recipes will crystallise as shipped stories reveal
workflows worth writing down.
