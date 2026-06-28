use std::path::PathBuf;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mdbook_listings::callout::{
    SidecarCallouts, SupportedRenderer, splice_chapter as splice_callouts,
};
use mdbook_listings::diff::splice_chapter as splice_diffs;
use mdbook_listings::freeze::{
    FreezeOptions, FreezeOutcome, derive_default_tag, freeze, frozen_relative_path, path_to_string,
};
use mdbook_listings::include::splice_chapter as splice_includes;
use mdbook_listings::install::{InstallOutcome, ensure_assets_fresh, install};
use mdbook_listings::list_of_listings::{ChapterListings, render_index, replace_markers};
use mdbook_listings::manifest::Manifest;
use mdbook_listings::number::splice_chapter as splice_numbers;
use mdbook_listings::verify::{Severity, verify};
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
    let manifest = Manifest::load(&ctx.root)?;
    let src_dir = ctx.root.join(&ctx.config.book.src);
    let renderer = SupportedRenderer::from_renderer_name(&ctx.renderer)
        .with_context(|| format!("unsupported renderer: {}", ctx.renderer))?;
    let sidecars =
        SidecarCallouts::load(&src_dir.join("listings")).context("loading sidecar callouts")?;
    // Opt-in: numbering stays off unless `[preprocessor.listings] number-listings`
    // is set true. A malformed value defaults off rather than failing the build.
    let number_listings = ctx
        .config
        .get::<bool>("preprocessor.listings.number-listings")
        .ok()
        .flatten()
        .unwrap_or(false);
    // Opt-in: the {{#list-of-listings}} index renders only when this is set.
    // A malformed value defaults off rather than failing the build.
    let list_of_listings = ctx
        .config
        .get::<bool>("preprocessor.listings.list-of-listings")
        .ok()
        .flatten()
        .unwrap_or(false);

    // Accumulates each chapter's numbered listings, in document order, for the
    // book-wide List-of-Listings index emitted after the per-chapter passes.
    let mut collected: Vec<ChapterListings> = Vec::new();
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
            // CALLOUT: preprocessor-chain Four-stage chain per chapter: includes (expand listings/snippets + drop locator anchors) → diffs (render `{{#diff}}` blocks + emit dual-attribute anchors) → numbering (label each anchored listing `Listing N.M` + caption, stamp the number onto its anchor) → callouts (strip CALLOUT comments + emit overlay, scoping badges to the listing number). The order matters: numbering needs both anchor kinds in one stream to count M, and callouts need the included source bytes inline to find `CALLOUT:` markers.
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
                .map(|new_content| {
                    let (numbered, refs) = splice_numbers(
                        &new_content,
                        chapter.number.as_ref().map(|n| n.as_slice()),
                        number_listings,
                        renderer,
                    );
                    // Record this chapter's listings for the book-wide index.
                    // The link path is the chapter file relative to the book
                    // src root (Phase 1 assumes the index page is top-level).
                    // Chapters with no listings are still recorded; render_index
                    // skips them. The flag is the sole gate — when off, nothing
                    // is collected, so the index renders empty.
                    if list_of_listings {
                        collected.push(ChapterListings {
                            name: chapter.name.clone(),
                            path: chapter
                                .path
                                .as_ref()
                                .map(|p| p.to_string_lossy().replace('\\', "/"))
                                .unwrap_or_default(),
                            listings: refs,
                        });
                    }
                    numbered
                })
                .and_then(|new_content| {
                    splice_callouts(&new_content, renderer, &sidecars)
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

    // Final book-wide pass: replace every {{#list-of-listings}} marker with the
    // index built from the collected listings. When the feature is off nothing
    // was collected, so the index is empty and the marker is simply stripped —
    // the raw directive never leaks into the output.
    let index = render_index(&collected);
    book.for_each_mut(|item| {
        if let BookItem::Chapter(chapter) = item {
            chapter.content = replace_markers(&chapter.content, &index);
        }
    });

    serde_json::to_writer(std::io::stdout(), &book).context("writing transformed book to stdout")
}

/// Answer mdbook's renderer-support probe by exiting 0 (supported) or 1
/// (unsupported). We do not return from this function.
fn supports(renderer: &str) -> ! {
    let supported = matches!(renderer, "html" | "typst-pdf");
    process::exit(if supported { 0 } else { 1 });
}
