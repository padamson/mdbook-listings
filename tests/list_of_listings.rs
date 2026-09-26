//! Phase 1 of the List-of-Listings feature: the `{{#list-of-listings}}`
//! directive renders an inline, book-wide index of every numbered listing,
//! grouped by the chapter it appears in and linking to each listing's anchor.
//!
//! This is the outermost (acceptance) test: it drives the whole feature
//! end-to-end through the preprocessor binary. Inner unit tests in
//! `src/number.rs` / `src/list_of_listings.rs` cover the pieces.

mod common;
use common::book::{MinimalBook, Page, chapter_content, run_preprocessor};
use common::book::{NUMBERED, NUMBERED_WITH_INDEX};

#[test]
fn list_of_listings_directive_renders_grouped_linked_index() {
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(NUMBERED_WITH_INDEX, &[
        // ch03: one numbered listing with a caption.
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs caption=\"The reuse manifest\"}}\n```\n",
        },
        // ch05: another numbered listing with a caption.
        Page {
            name: "Render callouts",
            path: "ch05.md",
            number: Some(&[5]),
            content: "```rust\n{{#include listings/claim.rs caption=\"The claim layer\"}}\n```\n",
        },
        // back-matter index page hosting the marker (unnumbered).
        Page {
            name: "List of Listings",
            path: "listings-index.md",
            number: None,
            content: "# List of Listings\n\n{{#list-of-listings}}\n",
        },
    ]);

    let returned = run_preprocessor(envelope);
    let index = chapter_content(&returned, "List of Listings");

    // Marker is consumed.
    assert!(
        !index.contains("{{#list-of-listings}}"),
        "directive should be replaced; got:\n{index}",
    );
    // Each listing appears as a link to its anchor, with number + caption.
    assert!(
        index.contains("[Listing 3.1 — The reuse manifest](ch03.md#listing-3-1)"),
        "expected linked entry for Listing 3.1; got:\n{index}",
    );
    assert!(
        index.contains("[Listing 5.1 — The claim layer](ch05.md#listing-5-1)"),
        "expected linked entry for Listing 5.1; got:\n{index}",
    );
    // Grouped by chapter, in document order (ch03 before ch05).
    let pos_ch03 = index.find("Freeze a listing").expect("ch03 group label");
    let pos_ch05 = index.find("Render callouts").expect("ch05 group label");
    assert!(
        pos_ch03 < pos_ch05,
        "groups should be in document order (ch03 before ch05); got:\n{index}",
    );

    // The link targets must exist: caption divs gain a stable id.
    let ch03 = chapter_content(&returned, "Freeze a listing");
    assert!(
        ch03.contains(r#"id="listing-3-1""#),
        "ch03 caption div should carry the link-target id; got:\n{ch03}",
    );
    let ch05 = chapter_content(&returned, "Render callouts");
    assert!(
        ch05.contains(r#"id="listing-5-1""#),
        "ch05 caption div should carry the link-target id; got:\n{ch05}",
    );
}

#[test]
fn list_of_listings_directive_is_stripped_when_feature_disabled() {
    let book = MinimalBook::with_sample_and_claim();
    let pages = [
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs caption=\"The reuse manifest\"}}\n```\n",
        },
        Page {
            name: "List of Listings",
            path: "listings-index.md",
            number: None,
            content: "# List of Listings\n\n{{#list-of-listings}}\n",
        },
    ];
    // number-listings on, list-of-listings OFF.
    let envelope = book.envelope(NUMBERED, &pages);

    let returned = run_preprocessor(envelope);
    let index = chapter_content(&returned, "List of Listings");

    assert!(
        !index.contains("{{#list-of-listings}}"),
        "disabled feature should still strip the directive, not leak it; got:\n{index}",
    );
    assert!(
        !index.contains("Listing 3.1"),
        "disabled feature should not emit an index; got:\n{index}",
    );
}

#[test]
fn sidebar_append_emits_manifest_on_every_page() {
    let book = MinimalBook::with_sample_and_claim();
    let pages = [
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs caption=\"The reuse manifest\"}}\n```\n",
        },
        Page {
            name: "List of Listings",
            path: "listings-index.md",
            number: None,
            content: "# List of Listings\n\n{{#list-of-listings}}\n",
        },
    ];
    // Sidebar on (append); the inline-page flag is independent and left off.
    let envelope = book.envelope(
        "[preprocessor.listings]\nnumber-listings = true\nlist-of-listings-sidebar = \"append\"\n",
        &pages,
    );

    let returned = run_preprocessor(envelope);

    // The manifest rides on every page (each carries its own sidebar), even the
    // chapter that hosts no marker.
    for page in ["Freeze a listing", "List of Listings"] {
        let content = chapter_content(&returned, page);
        assert!(
            content.contains(r#"<script id="mdbook-listings-manifest""#),
            "manifest script should be on page `{page}`; got:\n{content}",
        );
        assert!(
            content.contains(r#"data-sidebar="append""#),
            "manifest should carry the sidebar mode on `{page}`; got:\n{content}",
        );
        assert!(
            content.contains(r#""path":"ch03.html""#),
            "manifest links the .html page on `{page}`; got:\n{content}",
        );
    }

    // The page flag is off, so the inline `{{#list-of-listings}}` marker is
    // stripped, not rendered — the sidebar doesn't turn the page index on.
    let index_page = chapter_content(&returned, "List of Listings");
    assert!(
        !index_page.contains("{{#list-of-listings}}"),
        "marker still stripped when page index off; got:\n{index_page}",
    );
    assert!(
        !index_page.contains("## Freeze a listing"),
        "no inline index rendered when the page flag is off; got:\n{index_page}",
    );
}

#[test]
fn sidebar_off_emits_no_manifest() {
    let book = MinimalBook::with_sample_and_claim();
    let pages = [
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs caption=\"The reuse manifest\"}}\n```\n",
        },
        Page {
            name: "List of Listings",
            path: "listings-index.md",
            number: None,
            content: "# List of Listings\n\n{{#list-of-listings}}\n",
        },
    ];
    // Numbering on, but no sidebar option at all.
    let envelope = book.envelope(NUMBERED, &pages);

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Freeze a listing");
    assert!(
        !content.contains("mdbook-listings-manifest"),
        "no manifest when the sidebar is off; got:\n{content}",
    );
}

#[test]
fn appendix_listings_number_with_the_title_letter() {
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(NUMBERED_WITH_INDEX, &[
        // Numbered chapter: dotted section number as before.
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs caption=\"The reuse manifest\"}}\n```\n",
        },
        // Suffix chapter titled as an appendix: mdbook hands us number: None,
        // but the title names the letter — listings number A.1, A.2.
        Page {
            name: "Appendix A: The Worked Example",
            path: "appendix-a.md",
            number: None,
            content: "```rust\n{{#include listings/sample.rs caption=\"The catalog\"}}\n```\n\n\
                      ```rust\n{{#include listings/claim.rs}}\n```\n",
        },
        // Suffix chapter that is NOT an appendix: stays caption-only.
        Page {
            name: "List of Listings",
            path: "listings-index.md",
            number: None,
            content: "# List of Listings\n\n{{#list-of-listings}}\n",
        },
    ]);

    let returned = run_preprocessor(envelope);

    let appendix = chapter_content(&returned, "Appendix A: The Worked Example");
    assert!(
        appendix.contains("Listing A.1 — The catalog")
            && appendix.contains(r##"id="listing-A-1""##),
        "appendix listings should number from the title letter; got:\n{appendix}",
    );
    assert!(
        appendix.contains("Listing A.2"),
        "within-appendix ordinal should advance; got:\n{appendix}",
    );

    // The book-wide index picks the appendix up with its letter.
    let index = chapter_content(&returned, "List of Listings");
    assert!(
        index.contains("[Listing A.1 — The catalog](appendix-a.md#listing-A-1)"),
        "index should list appendix listings; got:\n{index}",
    );
    // ...and the non-appendix suffix page itself gained no number.
    assert!(
        !index.contains("List of Listings]("),
        "a non-appendix suffix chapter must not be numbered; got:\n{index}",
    );
}

// --- harness -------------------------------------------------------------
