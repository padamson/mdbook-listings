//! Stable listing cross-references: `label="..."` on `{{#include}}` /
//! `{{#diff}}` names a listing, and `{{#listing-ref <label>}}` in prose
//! resolves to the listing's *current* `Listing N.M`, hyperlinked — so prose
//! can say "see Listing 5.4" without going stale when numbers shift.
//!
//! This is the outermost (acceptance) test: it drives the feature end-to-end
//! through the preprocessor binary. Inner unit tests in `src/` cover the
//! pieces.

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
fn listing_ref_resolves_to_current_number_with_link() {
    let book = MinimalBook::new();
    let envelope = book.envelope(
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs label=\"reuse-manifest\" caption=\"The reuse manifest\"}}\n```\n",
        },
        Page {
            name: "Render callouts",
            path: "ch05.md",
            number: Some(&[5]),
            content: "See {{#listing-ref reuse-manifest}} for the manifest shape.\n\n\
                      ```rust\n{{#include listings/claim.rs label=\"claim-layer\"}}\n```\n\n\
                      Same-chapter ref: {{#listing-ref claim-layer}}.\n",
        },
    );

    let returned = run(envelope);
    let ch05 = chapter_content(&returned, "Render callouts");

    // Cross-chapter ref: current number, linked to the listing's anchor.
    assert!(
        ch05.contains("[Listing 3.1](ch03.md#listing-3-1)"),
        "cross-chapter ref should render the current number as a link; got:\n{ch05}",
    );
    // Same-chapter ref (captionless listing still gets a number + id).
    assert!(
        ch05.contains("[Listing 5.1](ch05.md#listing-5-1)"),
        "same-chapter ref should resolve; got:\n{ch05}",
    );
    // The raw directive never leaks.
    assert!(
        !ch05.contains("{{#listing-ref"),
        "directives should be consumed; got:\n{ch05}",
    );
}

#[test]
fn listing_ref_on_diff_listing_resolves() {
    let book = MinimalBook::new();
    let envelope = book.envelope(
        Page {
            name: "Show diffs",
            path: "ch04.md",
            number: Some(&[4]),
            content: "{{#diff sample claim label=\"the-diff\" caption=\"Sample to claim\"}}\n",
        },
        Page {
            name: "Render callouts",
            path: "ch05.md",
            number: Some(&[5]),
            content: "The change is in {{#listing-ref the-diff}}.\n",
        },
    );

    let returned = run(envelope);
    let ch05 = chapter_content(&returned, "Render callouts");
    assert!(
        ch05.contains("[Listing 4.1](ch04.md#listing-4-1)"),
        "diff listings take labels too; got:\n{ch05}",
    );
}

#[test]
fn listing_ref_inside_fence_is_left_verbatim() {
    let book = MinimalBook::new();
    let envelope = book.envelope(
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs label=\"reuse-manifest\"}}\n```\n",
        },
        Page {
            name: "Recipes",
            path: "ch08.md",
            number: Some(&[8]),
            content: "```text\n{{#listing-ref reuse-manifest}}\n```\n",
        },
    );

    let returned = run(envelope);
    let ch08 = chapter_content(&returned, "Recipes");
    assert!(
        ch08.contains("{{#listing-ref reuse-manifest}}"),
        "a fenced example must stay verbatim; got:\n{ch08}",
    );
}

#[test]
fn unknown_label_fails_the_build_naming_label_and_chapter() {
    let book = MinimalBook::new();
    let envelope = book.envelope(
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs label=\"reuse-manifest\"}}\n```\n",
        },
        Page {
            name: "Render callouts",
            path: "ch05.md",
            number: Some(&[5]),
            content: "See {{#listing-ref no-such-label}}.\n",
        },
    );

    let output = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);
    assert!(
        stderr.contains("no-such-label") && stderr.contains("Render callouts"),
        "failure must name the label and the chapter; got:\n{stderr}",
    );
}

#[test]
fn duplicate_label_fails_the_build() {
    let book = MinimalBook::new();
    let envelope = book.envelope(
        Page {
            name: "Freeze a listing",
            path: "ch03.md",
            number: Some(&[3]),
            content: "```rust\n{{#include listings/sample.rs label=\"dup\"}}\n```\n\
                      ```rust\n{{#include listings/claim.rs label=\"dup\"}}\n```\n",
        },
        Page {
            name: "Render callouts",
            path: "ch05.md",
            number: Some(&[5]),
            content: "See {{#listing-ref dup}}.\n",
        },
    );

    let output = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);
    assert!(
        stderr.contains("\"dup\"") && stderr.contains("twice"),
        "duplicate-label failure must name the label and say it is defined twice; got:\n{stderr}",
    );
}

// --- harness (mirrors tests/list_of_listings.rs) -------------------------

struct Page<'a> {
    name: &'a str,
    path: &'a str,
    number: Option<&'a [u32]>,
    content: &'a str,
}

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

    fn envelope(&self, a: Page, b: Page) -> String {
        let config =
            Config::from_str("[preprocessor.listings]\nnumber-listings = true\n").expect("config");
        let ctx = PreprocessorContext::new(self.root.clone(), config, "html".to_string());
        let mut book = Book::new_with_items(vec![chapter(a), chapter(b)]);
        serde_json::to_string(&(&ctx, &mut book)).expect("serialize envelope")
    }
}

fn chapter(p: Page) -> BookItem {
    let mut ch = Chapter::new(p.name, p.content.to_string(), p.path, vec![]);
    ch.number = p.number.map(SectionNumber::new);
    BookItem::Chapter(ch)
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
