use std::path::PathBuf;
use std::process;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Managed code listings for mdbook: inline callouts, freezing, and verification.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Check whether a renderer is supported by this preprocessor.
    ///
    /// Invoked by mdbook during the build to decide whether to pipe the book
    /// through this preprocessor for a given renderer. Exits 0 if supported,
    /// 1 otherwise.
    Supports {
        /// Name of the renderer mdbook is asking about (e.g. `html`, `typst-pdf`).
        renderer: String,
    },

    /// Install preprocessor assets and register mdbook-listings in `book.toml`.
    Install {
        /// Root directory of the book (contains `book.toml`). Defaults to the
        /// current directory.
        #[arg(long)]
        book_root: Option<PathBuf>,
    },

    /// Freeze a source file into the book's listings directory and update
    /// the manifest.
    Freeze {
        /// Human-readable tag used as the frozen filename and as the manifest
        /// entry key. Should be unique within the book.
        #[arg(long)]
        tag: String,

        /// Root directory of the book. Defaults to the current directory.
        #[arg(long)]
        book_root: Option<PathBuf>,

        /// Overwrite an existing frozen copy with the same tag.
        #[arg(long)]
        force: bool,

        /// Path to the source file to freeze.
        source: PathBuf,
    },

    /// Verify consistency between the manifest, frozen listings, and `{{#include}}`
    /// references in the book's markdown.
    Verify {
        /// Root directory of the book. Defaults to the current directory.
        #[arg(long)]
        book_root: Option<PathBuf>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:?}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => preprocess(),
        Some(Command::Supports { renderer }) => supports(&renderer),
        Some(Command::Install { book_root: _ }) => {
            anyhow::bail!("`mdbook-listings install` is not yet implemented")
        }
        Some(Command::Freeze {
            tag: _,
            book_root: _,
            force: _,
            source: _,
        }) => anyhow::bail!("`mdbook-listings freeze` is not yet implemented"),
        Some(Command::Verify { book_root: _ }) => {
            anyhow::bail!("`mdbook-listings verify` is not yet implemented")
        }
    }
}

/// Default mode: read an mdbook preprocessor JSON payload from stdin, emit the
/// transformed payload on stdout.
fn preprocess() -> Result<()> {
    anyhow::bail!("mdbook-listings preprocessor mode is not yet implemented")
}

/// Answer mdbook's renderer-support probe by exiting 0 (supported) or 1
/// (unsupported). We do not return from this function.
fn supports(renderer: &str) -> ! {
    let supported = matches!(renderer, "html" | "typst-pdf");
    process::exit(if supported { 0 } else { 1 });
}
