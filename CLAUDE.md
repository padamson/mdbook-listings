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

Scope the baseline with `.mutants.toml` (test_tool = nextest, --lib
only, examine_globs = src/). Surviving `MISSED` mutations are logged
in [`MUTATION_DEBT.md`](MUTATION_DEBT.md); add new findings there
when they surface and cross them out as tests close the gaps.

## Building the book locally

The book at `book/` uses three preprocessors (mdbook itself,
`mdbook-admonish`, and our own `mdbook-listings`) and one renderer
(`mdbook-typst-pdf`). All four must be on `PATH` before
`mdbook build` can run.

```bash
# One-time setup:
cargo install mdbook --locked
cargo install mdbook-typst-pdf --locked
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

# After editing assets/mdbook-listings.css, the CSS embedded in the binary is stale.
# To see changes without a full recompile, bypass the binary and copy directly:
cp assets/mdbook-listings.css book/mdbook-listings.css
```

## Release process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Commit: `git commit -m "Release vX.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push origin main --tags`

The tag triggers CI which builds, tests, creates a GitHub Release, and publishes to crates.io.
