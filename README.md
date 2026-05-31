# mdbook-listings

Managed code listings for mdbook: inline callouts, freezing, and verification.

## Installation

```bash
cargo install mdbook-listings
```

## Usage

```bash
mdbook-listings --help
```

Full documentation at <https://padamson.github.io/mdbook-listings/>.

What's planned beyond what's shipped: see [ROADMAP.md](ROADMAP.md).

## Claude Code plugin

If you use [Claude Code](https://claude.com/claude-code), you can install an
authoring assistant for this preprocessor. It gives the agent a concise,
always-current reference for the CLI and directive syntax while it edits a
book, so it doesn't have to re-derive the commands each session.

The plugin lives in this repo. To install it, add the repo as a plugin source
and then install:

```text
/plugin marketplace add padamson/mdbook-listings
/plugin install mdbook-listings@mdbook-listings
```

Install at user scope (every project) or, in a book repo, at project scope
(`--scope project`) so collaborators pick it up too.

## Development

See [CLAUDE.md](CLAUDE.md) for development commands.

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (MSRV: 1.88)
- [prek](https://github.com/j178/prek) for pre-commit hooks: `cargo install prek && prek install`

### Build and test

```bash
cargo build
cargo nextest run
```

## License

MIT License. See [LICENSE](LICENSE).
