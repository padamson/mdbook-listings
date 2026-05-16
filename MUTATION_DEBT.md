# Mutation testing debt log

Outstanding `MISSED` mutations from `cargo mutants` — each is a
test-coverage gap: the listed mutation survived the existing test
suite, meaning at least one assertion is missing that would have
caught it.

## Workflow

**Add entries** by running mutation testing and appending any `MISSED`
results to the relevant `Outstanding` subsection. Two ways:

```bash
# Diff-only (fast): the lines touched in HEAD~1..HEAD.
./scripts/mutants.sh

# Full sweep (slow): the entire codebase. Runs many hours locally;
# prefer triggering the `mutation-testing` (full) job on CI via the
# manual `workflow_dispatch` button in the Security & Quality
# workflow on GitHub.
cargo mutants
```

**Fix an entry** by writing the missing test, verifying locally that
the mutation is now `CAUGHT`, then crossing the entry out with `~~…~~`
and linking the fix commit:

```bash
# Targeted re-run: confirm the specific mutation is now caught.
cargo mutants --file src/install.rs --line 57
```

`MUTATION_DEBT.md` is committed alongside the fix in the same commit
that adds the new test, so the log stays in lockstep with the code.

## Outstanding

### src/install.rs

Surfaced by `scripts/mutants.sh 6e07b6a~1` (sweeping ch.6 slice 2's
new `ensure_assets_fresh` + `ensure_gitignore` helpers and the
refactored `install`). Commit
[`6e07b6a`](https://github.com/padamson/mdbook-listings/commit/6e07b6a).
Re-verified post-fix with `cargo mutants --file src/install.rs`.

- [x] ~~**L57:29** — `replace || with && in ensure_assets_fresh`.~~
  Closed by `ensure_assets_fresh_reports_write_when_only_one_asset_is_stale`
  in `tests/install.rs`: pre-writes one asset at the bundled bytes
  and the other stale, asserts the return is still `true`.
- [x] ~~**L77:8** — `delete ! in ensure_gitignore`.~~ Closed by
  `ensure_gitignore_inserts_separator_when_existing_file_lacks_trailing_newline`:
  pre-writes `target/` (no `\n`), asserts byte-exact output includes
  the separator.
- [x] ~~**L77:33** — `replace && with || in ensure_gitignore`.~~
  Closed by `ensure_gitignore_does_not_double_newline_when_existing_file_ends_with_newline`:
  pre-writes `target/\n` (trailing `\n`), asserts byte-exact output
  has NO blank line between existing content and new entries.
- [x] ~~**L77:36** — `delete ! in ensure_gitignore`.~~ Covered by the
  same byte-exact assertion as L77:8 — the second `!` flip would
  also produce a wrong separator decision.
- [x] ~~**L119:24** — `replace || with && in install`.~~ Closed by
  `install_reports_installed_when_only_book_toml_needs_change`:
  pre-seeds matching assets + complete `.gitignore`, asserts
  `install` returns `InstallOutcome::Installed`.
- [x] ~~**L119:42** — `replace || with && in install`.~~ Closed by
  `install_reports_installed_when_only_assets_need_change`: runs
  `install` to seed config, corrupts on-disk assets, asserts second
  `install` returns `InstallOutcome::Installed`.

New finding (re-run of `cargo mutants --file src/install.rs` after
the fixes above):

- [ ] **L94:19** — `replace match guard e.kind() ==
  std::io::ErrorKind::NotFound with true in install`. The error
  handler that distinguishes "book.toml missing" from any other IO
  error has no test for the non-NotFound branch — `permission
  denied`, etc., would currently take the `bail!("book.toml not
  found")` path silently. Fabricating a non-NotFound IO error in a
  unit test requires either a chmod fixture or a custom
  io::Error::other, both viable but neither yet written.

## Status

| | |
|---|---|
| Last full mutation run | Not yet performed against current `main`. Trigger via the `mutation-testing` (full) job in `.github/workflows/security.yml` using `workflow_dispatch` on GitHub. |
| Per-PR / per-push coverage | `mutation-testing-diff` job in the same workflow runs `scripts/mutants.sh` against the changed lines on every push and PR. New `MISSED` results appear in that job's `mutation-report-diff` artifact and should be added to this log. |

## When to delete this file

When every entry is crossed out **and** a full mutation run on `main`
returns zero `MISSED`. More realistically, this file evolves into "0
outstanding" indefinitely as new findings land alongside their fixes.
