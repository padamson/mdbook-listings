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
