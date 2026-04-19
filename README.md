<!-- Replace "Project Name" with your project name. -->
# Project Name

<!-- Replace this line with a one-sentence description of your project. -->

<!--
Template users: after creating your repo from this template, see SETUP.md
for the full onboarding checklist. At minimum, replace every occurrence of
`my-project` and `Project Name` throughout this file, and update
`Cargo.toml` (name, description, repository, authors, keywords, categories).
-->

## Installation

<!-- Replace `my-project` with your crate name. -->
```bash
cargo install my-project
```

## Usage

<!-- Replace `my-project` with your binary name, or replace this section
     entirely with library usage examples if this is a library crate. -->
```bash
my-project --help
```

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
