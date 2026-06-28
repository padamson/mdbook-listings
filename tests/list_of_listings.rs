//! Phase 1 of the List-of-Listings feature: the `{{#list-of-listings}}`
//! directive renders an inline, book-wide index of every numbered listing,
//! grouped by the chapter it appears in and linking to each listing's anchor.
//!
//! This is the outermost (acceptance) test: it drives the whole feature
//! end-to-end through the preprocessor binary. Inner unit tests in
//! `src/number.rs` / `src/list_of_listings.rs` cover the pieces.

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter, SectionNumber};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

#[test]
fn list_of_listings_directive_renders_grouped_linked_index() {
    let book = MinimalBook::new();
    let envelope = book.envelope(
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
    );

    let returned = run(envelope);
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
    let book = MinimalBook::new();
    let mut envelope_book = book.book(
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
    );
    // number-listings on, list-of-listings OFF.
    let ctx = book.context("[preprocessor.listings]\nnumber-listings = true\n");
    let envelope = serialize(&ctx, &mut envelope_book);

    let returned = run(envelope);
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

// --- harness -------------------------------------------------------------

struct Page<'a> {
    name: &'a str,
    path: &'a str,
    number: Option<&'a [u32]>,
    content: &'a str,
}

/// Tempdir laid out as a real book root: two frozen listings under
/// `src/listings/` and a `listings.toml` registering them.
struct MinimalBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalBook {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let listings_dir = root.join("src").join("listings");
        fs::create_dir_all(&listings_dir).unwrap();
        fs::write(listings_dir.join("sample.rs"), "fn sample_body() {}\n").unwrap();
        fs::write(listings_dir.join("claim.rs"), "fn claim_body() {}\n").unwrap();
        fs::write(
            root.join("listings.toml"),
            "version = 1\n\n\
             [[listing]]\n\
             tag = \"sample\"\n\
             source = \"../sample.rs\"\n\
             frozen = \"src/listings/sample.rs\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
             [[listing]]\n\
             tag = \"claim\"\n\
             source = \"../claim.rs\"\n\
             frozen = \"src/listings/claim.rs\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .unwrap();
        Self { _tmp: tmp, root }
    }

    fn context(&self, config_toml: &str) -> PreprocessorContext {
        let config = Config::from_str(config_toml).expect("parse config");
        PreprocessorContext::new(self.root.clone(), config, "html".to_string())
    }

    fn book(&self, a: Page, b: Page) -> Book {
        Book::new_with_items(vec![chapter(a), chapter(b)])
    }

    /// The common case: both feature flags on, three pages.
    fn envelope(&self, a: Page, b: Page, c: Page) -> String {
        let ctx = self
            .context("[preprocessor.listings]\nnumber-listings = true\nlist-of-listings = true\n");
        let mut book = Book::new_with_items(vec![chapter(a), chapter(b), chapter(c)]);
        serialize(&ctx, &mut book)
    }
}

fn chapter(p: Page) -> BookItem {
    let mut ch = Chapter::new(p.name, p.content.to_string(), p.path, vec![]);
    ch.number = p.number.map(SectionNumber::new);
    BookItem::Chapter(ch)
}

fn serialize(ctx: &PreprocessorContext, book: &mut Book) -> String {
    serde_json::to_string(&(ctx, &*book)).expect("serialize envelope")
}

fn run(envelope: String) -> Book {
    let output = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("parse stdout as Book")
}

fn chapter_content(book: &Book, name: &str) -> String {
    book.iter()
        .find_map(|item| match item {
            BookItem::Chapter(ch) if ch.name == name => Some(ch.content.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("chapter `{name}` missing from returned book"))
}
