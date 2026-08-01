use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mdbook_listings::callout::SupportedRenderer;
use mdbook_listings::freeze::{
    FreezeOptions, FreezeOutcome, derive_default_tag, freeze, frozen_relative_path, path_to_string,
};
use mdbook_listings::install::{InstallOutcome, ensure_assets_fresh, install};
use mdbook_listings::manifest::Manifest;
use mdbook_listings::pipeline;
use mdbook_listings::verify::{Severity, verify};

/// Managed code listings for mdbook: inline callouts, freezing, and verification.
#[derive(Parser)]
// The version carries the short git sha for non-release builds (see
// build.rs), so a binary built from main is distinguishable from the
// release it would otherwise impersonate.
#[command(version = env!("CRATE_VERSION_WITH_BUILD"), about, long_about = None)]
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
        /// entry key. Should be unique within the book. When omitted,
        /// derived from the source basename and existing manifest entries
        /// (`<basename>-v1` for the first freeze; `<basename>-(v|ver|rev|
        /// version)<N+1>` to bump an existing series).
        #[arg(long)]
        tag: Option<String>,

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

    /// List frozen listings recorded in `listings.toml`. Prints one
    /// tab-separated row per entry: `<tag>\t<frozen-path>\t<source-path>`.
    /// Order matches manifest insertion order.
    List {
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
            let tag = match tag {
                Some(t) => t,
                None => {
                    let manifest = Manifest::load(&book_root)?;
                    derive_default_tag(&manifest, &source, &book_root)
                        .map_err(|e| anyhow::anyhow!("{e}"))?
                }
            };
            let report = freeze(FreezeOptions {
                book_root: &book_root,
                tag: &tag,
                source: &source,
                force,
            })?;
            let verb = match report.outcome {
                FreezeOutcome::Created => "created",
                FreezeOutcome::Unchanged => "unchanged",
                FreezeOutcome::Replaced => "replaced",
            };
            println!("{verb}: {tag}");
            let frozen_rel = frozen_relative_path(&tag, &source)?;
            // Include directive is resolved relative to the chapter file,
            // which already sits under `src/`; the on-disk path carries
            // the `src/` prefix that the directive must drop.
            let include_rel = frozen_rel
                .strip_prefix("src")
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|_| frozen_rel.clone());
            // path_to_string normalises Windows backslashes to forward
            // slashes so book directives render identically regardless of
            // build host.
            println!("  frozen:  {}", path_to_string(&frozen_rel)?);
            println!(
                "  include: {{{{#include {}}}}}",
                path_to_string(&include_rel)?
            );
            if let Some(prev) = report.previous_tag {
                println!("  diff:    {{{{#diff {prev} {tag}}}}}");
            }
            Ok(())
        }
        Some(Command::Verify { book_root }) => {
            let book_root = book_root.unwrap_or_else(|| PathBuf::from("."));
            let report = verify(&book_root)?;
            for finding in &report.findings {
                match finding.severity {
                    Severity::Error => eprintln!("error: {}", finding.message),
                    Severity::Warning => eprintln!("warning: {}", finding.message),
                }
            }
            let n = report.listings_checked;
            let plural = if n == 1 { "" } else { "s" };
            println!("{n} frozen listing{plural} checked");
            let errors = report.error_count();
            if errors > 0 {
                anyhow::bail!("verify found {errors} error(s)");
            }
            Ok(())
        }
        Some(Command::List { book_root }) => {
            let book_root = book_root.unwrap_or_else(|| PathBuf::from("."));
            let manifest = Manifest::load(&book_root)?;
            for listing in &manifest.listings {
                println!("{}\t{}\t{}", listing.tag, listing.frozen, listing.source);
            }
            Ok(())
        }
    }
}

/// Default mode: read an mdbook preprocessor JSON payload from stdin, splice
/// rendered diffs into every `{{#diff …}}` directive, emit the transformed
/// payload on stdout.
fn preprocess() -> Result<()> {
    let (ctx, mut book) = mdbook_preprocessor::parse_input(std::io::stdin())?;
    // CALLOUT: assets-on-build Refresh the bundled CSS/JS on every build so the rendered HTML always uses assets matching the binary version. No-op when bytes already match. Prevents asset-version skew when an author bumps the binary forward without re-running `install` — the stale on-disk copies would otherwise be mixed with new HTML emission.
    ensure_assets_fresh(&ctx.root).context("refreshing bundled CSS/JS assets")?;
    // The chain itself lives in the library (`pipeline::process_book`) so
    // its ordering invariant is unit-tested; this stays a thin adapter.
    pipeline::process(&ctx, &mut book)?;
    serde_json::to_writer(std::io::stdout(), &book).context("writing transformed book to stdout")
}

/// Answer mdbook's renderer-support probe by exiting 0 (supported) or 1
/// (unsupported). We do not return from this function.
fn supports(renderer: &str) -> ! {
    // Delegate to the renderer enum so the supported set has one home —
    // a literal match here silently drifts when a renderer is added.
    let supported = SupportedRenderer::from_renderer_name(renderer).is_some();
    process::exit(if supported { 0 } else { 1 });
}
