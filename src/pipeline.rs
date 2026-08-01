//! The preprocessor pipeline, as a library unit: the per-chapter splice
//! chain and the book-wide finale, extracted from the CLI so the ordering
//! invariant is unit-testable instead of living only behind stdin/stdout.
//!
//! Per chapter, four stages in a fixed order — includes (expand
//! listings/snippets + drop locator anchors) → diffs (render `{{#diff}}`
//! blocks + emit dual-attribute anchors) → numbering (label each anchored
//! listing `Listing N.M`, stamp the number onto its anchor) → callouts
//! (strip `CALLOUT:` comments + emit the overlay, scoping badges to the
//! listing number). The order matters: numbering needs both anchor kinds in
//! one stream to count `M`, and callouts need the included source bytes
//! inline to find their markers.
//!
//! Book-wide, after every chapter is spliced: build the label index and
//! resolve `{{#listing-ref}}` (refs cross chapters), replace
//! `{{#list-of-listings}}` markers with the rendered index, and append the
//! sidebar manifest.

use std::path::Path;

use anyhow::{Context, Result};
use mdbook_preprocessor::book::{Book, BookItem};

use crate::callout::{SidecarCallouts, SupportedRenderer, splice_chapter as splice_callouts};
use crate::diff::splice_chapter as splice_diffs;
use crate::include::splice_chapter as splice_includes;
use crate::list_of_listings::{
    ChapterListings, SidebarMode, render_index, render_manifest, replace_markers,
};
use crate::listing_ref::{LabelIndex, replace_refs};
use crate::manifest::Manifest;
use crate::number::{listing_prefix, splice_chapter as splice_numbers};

/// The `[preprocessor.listings]` opt-ins the pipeline consumes. All default
/// off; a malformed value defaults off rather than failing the build.
pub struct PipelineOptions {
    pub number_listings: bool,
    pub list_of_listings: bool,
    pub sidebar_mode: SidebarMode,
}

impl PipelineOptions {
    pub fn from_config(config: &mdbook_preprocessor::config::Config) -> Self {
        let flag = |key: &str| config.get::<bool>(key).ok().flatten().unwrap_or(false);
        Self {
            number_listings: flag("preprocessor.listings.number-listings"),
            list_of_listings: flag("preprocessor.listings.list-of-listings"),
            sidebar_mode: config
                .get::<String>("preprocessor.listings.list-of-listings-sidebar")
                .ok()
                .flatten()
                .as_deref()
                .map(SidebarMode::parse)
                .unwrap_or(SidebarMode::Off),
        }
    }
}

/// Run the whole pipeline over `book` in place.
pub fn process_book(
    book: &mut Book,
    manifest: &Manifest,
    sidecars: &SidecarCallouts,
    book_root: &Path,
    src_dir: &Path,
    renderer: SupportedRenderer,
    opts: &PipelineOptions,
) -> Result<()> {
    // Accumulates each chapter's numbered listings, in document order, for
    // the book-wide passes (List-of-Listings index, sidebar manifest, and
    // {{#listing-ref}} resolution). Always collected: listing refs are
    // opted into by writing the directive, not by a config flag, so the
    // label index must exist regardless of the index/sidebar settings.
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
                .unwrap_or_else(|| src_dir.to_path_buf());
            match splice_includes(&chapter.content, src_dir, chapter.source_path.as_deref())
                .map_err(|e| {
                    anyhow::Error::new(e).context("expanding {{#include listings/...}} failed")
                })
                .and_then(|new_content| {
                    splice_diffs(
                        &new_content,
                        manifest,
                        book_root,
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
                        listing_prefix(
                            chapter.number.as_ref().map(|n| n.as_slice()),
                            &chapter.name,
                        )
                        .as_deref(),
                        opts.number_listings,
                        renderer,
                    );
                    // Record this chapter's listings for the book-wide
                    // passes. The link path is the chapter file relative to
                    // the book src root (the index page is assumed
                    // top-level). Chapters with no listings are still
                    // recorded; render_index and the label index skip them.
                    collected.push(ChapterListings {
                        name: chapter.name.clone(),
                        path: chapter
                            .path
                            .as_ref()
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default(),
                        listings: refs,
                    });
                    numbered
                })
                .and_then(|new_content| {
                    splice_callouts(&new_content, renderer, sidecars)
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

    // Final book-wide pass: replace every {{#list-of-listings}} marker with
    // the index built from the collected listings, and append the sidebar
    // manifest to every page when the sidebar is on. The inline index stays
    // gated on its own flag (so the sidebar doesn't make a marker render
    // when the page index is off); when off the index is empty and the
    // marker is simply stripped, so the raw directive never leaks. The
    // manifest is emitted per page because each rendered page carries its
    // own sidebar.
    let index = if opts.list_of_listings {
        render_index(&collected)
    } else {
        String::new()
    };
    let sidebar_manifest = render_manifest(&collected, opts.sidebar_mode);
    // {{#listing-ref}} resolution needs the whole book's labels, so it
    // rides the same final pass. Unknown or duplicate labels fail the
    // build — a broken pointer in prose is a defect, not a warning.
    let label_index = LabelIndex::build(&collected).map_err(anyhow::Error::msg)?;
    let mut ref_err: Option<anyhow::Error> = None;
    book.for_each_mut(|item| {
        if ref_err.is_some() {
            return;
        }
        if let BookItem::Chapter(chapter) = item {
            match replace_refs(&chapter.content, &chapter.name, &label_index, renderer) {
                Ok(resolved) => {
                    let mut content = replace_markers(&resolved, &index);
                    content.push_str(&sidebar_manifest);
                    chapter.content = content;
                }
                Err(e) => ref_err = Some(anyhow::Error::msg(e)),
            }
        }
    });
    if let Some(e) = ref_err {
        return Err(e);
    }
    Ok(())
}

/// Convenience for the CLI: load everything `process_book` needs from a
/// book root and preprocessor context, then run it.
pub fn process(ctx: &mdbook_preprocessor::PreprocessorContext, book: &mut Book) -> Result<()> {
    let manifest = Manifest::load(&ctx.root)?;
    let src_dir = ctx.root.join(&ctx.config.book.src);
    let renderer = SupportedRenderer::from_renderer_name(&ctx.renderer)
        .with_context(|| format!("unsupported renderer: {}", ctx.renderer))?;
    let sidecars =
        SidecarCallouts::load(&src_dir.join("listings")).context("loading sidecar callouts")?;
    let opts = PipelineOptions::from_config(&ctx.config);
    process_book(
        book, &manifest, &sidecars, &ctx.root, &src_dir, renderer, &opts,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdbook_preprocessor::book::{Chapter, SectionNumber};
    use std::fs;

    /// A minimal on-disk book root: one frozen listing whose source carries
    /// a `CALLOUT:` marker, registered in listings.toml.
    fn book_root() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let listings = tmp.path().join("src").join("listings");
        fs::create_dir_all(&listings).unwrap();
        fs::write(
            listings.join("sample.rs"),
            "fn sample() {\n    // CALLOUT: entry The entry point.\n}\n",
        )
        .unwrap();
        fs::write(listings.join("other.rs"), "fn other() {}\n").unwrap();
        fs::write(
            tmp.path().join("listings.toml"),
            "version = 1\n\n\
             [[listing]]\ntag = \"sample\"\nsource = \"../sample.rs\"\n\
             frozen = \"src/listings/sample.rs\"\nsha256 = \"0\"\n\n\
             [[listing]]\ntag = \"other\"\nsource = \"../other.rs\"\n\
             frozen = \"src/listings/other.rs\"\nsha256 = \"0\"\n",
        )
        .unwrap();
        tmp
    }

    fn run(content: &str, opts: &PipelineOptions) -> String {
        let root = book_root();
        let src_dir = root.path().join("src");
        let manifest = Manifest::load(root.path()).expect("manifest");
        let sidecars = SidecarCallouts::load(&src_dir.join("listings")).expect("sidecars");
        let mut ch = Chapter::new("Ch", content.to_string(), "ch05.md", vec![]);
        ch.number = Some(SectionNumber::new([5]));
        let mut book = Book::new_with_items(vec![BookItem::Chapter(ch)]);
        process_book(
            &mut book,
            &manifest,
            &sidecars,
            root.path(),
            &src_dir,
            SupportedRenderer::Html,
            opts,
        )
        .expect("pipeline");
        match book.iter().next() {
            Some(BookItem::Chapter(c)) => c.content.clone(),
            _ => panic!("chapter missing"),
        }
    }

    #[test]
    fn stages_run_in_order_include_then_number_then_callout() {
        // One chapter proves three orderings at once: the include ran
        // (source bytes inline), numbering saw the include's anchor
        // (Listing 5.1 caption), and the callout pass ran AFTER both — the
        // marker from the INCLUDED file is stripped and its badge is scoped
        // to the number the numbering pass assigned.
        let out = run(
            "```rust\n{{#include listings/sample.rs caption=\"S\"}}\n```\n",
            &PipelineOptions {
                number_listings: true,
                list_of_listings: false,
                sidebar_mode: SidebarMode::Off,
            },
        );
        assert!(out.contains("fn sample()"), "include ran; got:\n{out}");
        assert!(
            out.contains("Listing 5.1"),
            "numbering saw the include's anchor; got:\n{out}"
        );
        assert!(
            !out.contains("CALLOUT: entry"),
            "callouts ran after the include (marker stripped); got:\n{out}"
        );
        assert!(
            out.contains(">5.1.1<"),
            "badge scoped to the number the numbering pass assigned — \
             callouts ran after numbering; got:\n{out}"
        );
    }

    #[test]
    fn numbering_counts_diff_and_include_anchors_in_one_stream() {
        // A diff above an include: M counts across both anchor kinds in
        // document order, which only works if numbering runs after BOTH
        // splices on the same content stream.
        let out = run(
            "{{#diff sample other caption=\"D\"}}\n\n\
             ```rust\n{{#include listings/other.rs caption=\"I\"}}\n```\n",
            &PipelineOptions {
                number_listings: true,
                list_of_listings: false,
                sidebar_mode: SidebarMode::Off,
            },
        );
        assert!(
            out.contains("Listing 5.1 — D") && out.contains("Listing 5.2 — I"),
            "diff and include share one numbering stream; got:\n{out}"
        );
    }
}
