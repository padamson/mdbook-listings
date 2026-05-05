use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mdbook_listings::callout::{SupportedRenderer, splice_chapter as splice_callouts};
use mdbook_listings::diff::splice_chapter as splice_diffs;
use mdbook_listings::freeze::{FreezeOptions, FreezeOutcome, freeze};
use mdbook_listings::include::splice_chapter as splice_includes;
use mdbook_listings::install::{InstallOutcome, install};
use mdbook_listings::manifest::Manifest;
use mdbook_preprocessor::book::BookItem;

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
        Some(Command::Install { book_root }) => {
            let book_root = book_root.unwrap_or_else(|| PathBuf::from("."));
            match install(&book_root)? {
                InstallOutcome::Installed => {
                    println!("installed mdbook-listings into {}", book_root.display());
                }
                InstallOutcome::Unchanged => {
                    println!(
                        "mdbook-listings already installed in {}; nothing changed",
                        book_root.display(),
                    );
                }
            }
            Ok(())
        }
        Some(Command::Freeze {
            tag,
            book_root,
            force,
            source,
        }) => {
            let book_root = book_root.unwrap_or_else(|| PathBuf::from("."));
            let outcome = freeze(FreezeOptions {
                book_root: &book_root,
                tag: &tag,
                source: &source,
                force,
            })?;
            let verb = match outcome {
                FreezeOutcome::Created => "created",
                FreezeOutcome::Unchanged => "unchanged",
                FreezeOutcome::Replaced => "replaced",
            };
            println!("{verb}: {tag}");
            Ok(())
        }
        Some(Command::Verify { book_root: _ }) => {
            anyhow::bail!("`mdbook-listings verify` is not yet implemented")
        }
    }
}

/// Default mode: read an mdbook preprocessor JSON payload from stdin, splice
/// rendered diffs into every `{{#diff …}}` directive, emit the transformed
/// payload on stdout.
fn preprocess() -> Result<()> {
    let (ctx, mut book) = mdbook_preprocessor::parse_input(std::io::stdin())?;
    let manifest = Manifest::load(&ctx.root)?;
    let src_dir = ctx.root.join(&ctx.config.book.src);
    let renderer = SupportedRenderer::from_renderer_name(&ctx.renderer)
        .with_context(|| format!("unsupported renderer: {}", ctx.renderer))?;

    let mut splice_err: Option<anyhow::Error> = None;
    book.for_each_mut(|item| {
        if splice_err.is_some() {
            return;
        }
        if let BookItem::Chapter(chapter) = item {
            let chapter_dir = chapter
                .source_path
                .as_ref()
                .and_then(|p| p.parent())
                .map(|d| src_dir.join(d))
                .unwrap_or_else(|| src_dir.clone());
            // CALLOUT: preprocessor-chain Three-stage chain per chapter: includes (expand listings/snippets + drop locator anchors) → diffs (render `{{#diff}}` blocks + emit dual-attribute anchors) → callouts (strip CALLOUT comments + emit overlay). The order matters: callouts need the included source bytes inline to find `CALLOUT:` markers.
            match splice_includes(&chapter.content, &src_dir, chapter.source_path.as_deref())
                .map_err(|e| {
                    anyhow::Error::new(e).context("expanding {{#include listings/...}} failed")
                })
                .and_then(|new_content| {
                    splice_diffs(
                        &new_content,
                        &manifest,
                        &ctx.root,
                        chapter.source_path.as_deref(),
                        &chapter_dir,
                    )
                    .map_err(|e| {
                        anyhow::Error::new(e).context("rendering {{#diff}} directive failed")
                    })
                })
                .and_then(|new_content| {
                    splice_callouts(&new_content, renderer)
                        .map_err(|e| anyhow::Error::new(e).context("rendering callouts failed"))
                }) {
                Ok(new_content) => chapter.content = new_content,
                Err(e) => splice_err = Some(e),
            }
        }
    });
    if let Some(e) = splice_err {
        return Err(e);
    }

    serde_json::to_writer(std::io::stdout(), &book).context("writing transformed book to stdout")
}

/// Answer mdbook's renderer-support probe by exiting 0 (supported) or 1
/// (unsupported). We do not return from this function.
fn supports(renderer: &str) -> ! {
    let supported = matches!(renderer, "html" | "typst-pdf");
    process::exit(if supported { 0 } else { 1 });
}
