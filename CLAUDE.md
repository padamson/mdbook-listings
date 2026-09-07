# mdbook-listings

Managed code listings for mdbook: inline callouts, freezing, and verification.

## Development

```bash
cargo build              # build
cargo nextest run        # run tests
cargo test --doc         # doc tests
cargo clippy             # lint
cargo fmt                # format
cargo audit              # security scan
cargo deny check         # license/dependency check
cargo vet                # supply chain review
```

## Plugin skill stays in sync

The repo ships a Claude Code plugin (`.claude-plugin/` + `skills/mdbook-listings/`).
When a change touches the CLI surface or directive syntax, updating the skill
is part of done, the same as tests and docs. The `plugin version bumped` prek
hook enforces the other half: any commit touching `skills/` or `.claude-plugin/`
must bump the `plugin.json` version, because that version is the update gate
for installed consumers — without a bump, `/plugin update` reports "already at
the latest version" and the edit never reaches anyone.

## playwright-rs skill is installed, not vendored

The e2e suite and `tools/capture-screenshots` use playwright-rs, and
upstream ships an authoring skill for it. Install it once per clone:

```bash
npx skills add padamson/playwright-rust -s playwright-rs-usage -a claude-code -y
npx skills update    # refresh later
```

`skills-lock.json` (tracked) records the source; the install itself lands
at `.claude/skills/playwright-rs-usage/` (gitignored). Do not commit a
copy: a tracked copy drifts against the crate and — because the skills CLI
scans `.claude/skills/` — gets republished stale to anyone running
`npx skills add padamson/mdbook-listings`. The install tracks upstream
main while Cargo.toml pins crates.io releases; that skew stays small
because releases land here within days of publish, and it beats a
tag-pinned install that `skills update` would freeze forever.

## Pre-commit hooks

```bash
cargo install prek
prek install
```

Hooks mirror CI checks: fmt, clippy, check, nextest, doctest, audit, deny, vet.

## Mutation testing

```bash
./scripts/mutants.sh                 # diff HEAD~1..HEAD (default)
./scripts/mutants.sh main            # diff main..HEAD
./scripts/mutants.sh -- --jobs 4     # pass extra cargo-mutants args
```

`scripts/mutants.sh` wraps `cargo mutants --in-diff`, scoping mutation
testing to just the lines a commit touched. A full-codebase run grows
linearly with codebase size and routinely takes hours; `--in-diff`
keeps the loop fast enough to use while the test is still warm.

CI runs the per-diff variant on every push and PR (`mutation-testing-diff`
in `security.yml`). The full-codebase job (`mutation-testing`) is
manual-only via `workflow_dispatch` — use it for occasional audits or
big refactors, never on a schedule.

Configuration lives in `.cargo/mutants.toml` — that exact path;
cargo-mutants reads no other location and won't complain about a file
elsewhere. It sets nextest, skips the browser/PDF integration binaries
(the same filter CI's test job uses), and lists the known-equivalent
mutants under `exclude_re` with the proof for each.

A surviving `MISSED` mutation is a missing test: write the test in the
same commit, or if it's provably equivalent under all reachable inputs,
add it to `exclude_re` with the reasoning. CI's `mutation-testing-diff`
job exits non-zero on any MISSED, so per-diff findings can't be
deferred. TIMEOUT outcomes pass (a mutant that hangs the tests is
detected, just not by an assertion); the script and workflow both
verify `missed.txt` is empty before treating exit 3 as success.

## Building the book locally

The book at `book/` uses three preprocessors (mdbook itself,
`mdbook-admonish`, and our own `mdbook-listings`) and one renderer
(`mdbook-typst-pdf`). All four must be on `PATH` before
`mdbook build` can run.

```bash
# One-time setup:
cargo install mdbook --locked
cargo install --git https://github.com/padamson/mdbook-typst-pdf \
  --branch fix/dynamic-fence-length --force    # until upstream catches up
cargo install --git https://github.com/padamson/mdbook-admonish \
  --branch feat/mdbook-0.5-compat --force      # until upstream catches up
cargo install --path . --locked --force         # our own crate

# Build:
cd book && mdbook build
# → book/build/html/         (HTML site)
# → book/build/typst-pdf/    (PDF, if mdbook-typst-pdf is installed)

# Live-reload while editing chapter prose (HTML only):
cd book && mdbook serve
# Opens http://localhost:3000

# After editing src/*.rs, the installed preprocessor is stale. Reinstall:
cargo install --path . --locked --force

# After editing assets/mdbook-listings.css, reinstall too. The preprocessor
# rewrites book/mdbook-listings.css from the copy embedded in the binary on
# every build, so copying the asset over by hand does nothing -- the next
# `mdbook build` overwrites it with the stale embedded bytes. book.toml runs
# `command = "mdbook-listings"` off PATH, so it is the *installed* binary that
# matters, not target/debug.
cargo install --path . --locked --force
```

## Watching CI

```bash
./scripts/ci-watch.sh              # the current HEAD
./scripts/ci-watch.sh <sha|ref>    # a specific commit
POLL=15 TIMEOUT=600 ./scripts/ci-watch.sh
```

Prints one line per job as it reaches a terminal state, one per workflow
when it finishes, then exits: 0 all green, 1 some job failed, 2 timed out.
Every terminal state is reported, not just successes -- a watcher that only
greps for "success" is silent through a crashloop, which looks exactly like
still-running.

It exists as a script for a sandbox reason worth knowing before writing any
other polling loop here. `excludedCommands` in `.claude/settings.json` match
the **top-level command line**. `gh run list` alone is excluded and works;
`gh` inside a `for` or `while` loop is not matched, so the whole invocation
stays sandboxed and every call fails on the read-denied `~/.config/gh` --
silently, if the loop swallows errors. Excluding the script instead
unsandboxes its whole process tree, and the `gh` it spawns inherits that,
the same route `git push` takes for SSH. Run it as its own command: chaining
an excluded command onto another line unsandboxes that line too.

## Release process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Commit: `git commit -m "Release vX.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push origin main --tags`

The tag triggers CI which builds, tests, creates a GitHub Release, and publishes to crates.io.
