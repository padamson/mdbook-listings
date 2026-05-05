//! Integration tests for slice 8: `{{#include listings/...}}` interception
//! and the `<div data-listing-tag>` locator anchor the include splicer
//! emits after each frozen-listing fenced block.

use std::fs;
use std::path::PathBuf;

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

#[test]
fn listing_include_directive_is_replaced_with_file_contents_inline() {
    let book = MinimalIncludesBook::new();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n```rust\n{{#include listings/sample.rs}}\n```\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Include Test");

    assert!(
        content.contains("fn sample_body() {}"),
        "expected file body inline; got:\n{content}",
    );
    assert!(
        !content.contains("{{#include"),
        "directive should be consumed; got:\n{content}",
    );
}

#[test]
fn listing_include_emits_anchor_after_closing_fence() {
    let book = MinimalIncludesBook::new();
    let envelope = book
        .envelope_with_chapter("```rust\n{{#include listings/sample.rs}}\n```\nAfter paragraph.\n");

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Include Test");

    assert!(
        content.contains("data-listing-tag=\"sample\""),
        "expected listing-tag anchor with file-stem tag; got:\n{content}",
    );
    let anchor_pos = content.find("data-listing-tag").expect("anchor present");
    let close_fence_pos = content
        .find("```\n")
        .map(|p| p + 4)
        .expect("close fence present");
    assert!(
        anchor_pos > close_fence_pos,
        "anchor must come AFTER the closing fence; anchor at {anchor_pos}, close-fence at {close_fence_pos}\ncontent:\n{content}",
    );
}

#[test]
fn snippet_include_is_expanded_inline_without_listing_tag_anchor() {
    let book = MinimalIncludesBook::new();
    book.write_snippet("excerpt.rs", "fn snippet_body() {}\n");
    let envelope = book.envelope_with_chapter("```rust\n{{#include snippets/excerpt.rs}}\n```\n");

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Include Test");

    assert!(
        content.contains("fn snippet_body() {}"),
        "snippet should be expanded inline; got:\n{content}",
    );
    assert!(
        !content.contains("data-listing-tag"),
        "snippets must not produce a listing-tag anchor; got:\n{content}",
    );
    assert!(
        !content.contains("{{#include"),
        "directive should be consumed; got:\n{content}",
    );
}

#[test]
fn listing_include_followed_by_diff_emits_both_anchor_kinds() {
    let book = MinimalIncludesBook::new();
    let envelope = book.envelope_with_chapter(concat!(
        "First show as include.\n\n",
        "```rust\n{{#include listings/sample.rs}}\n```\n\n",
        "Then diff against new-tag.\n\n",
        "{{#diff sample new-tag}}\n",
    ));

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Include Test");

    assert!(
        content.contains("data-listing-tag=\"sample\""),
        "expected include-side listing-tag anchor; got:\n{content}",
    );
    assert!(
        content.contains("data-listing-diff-left=\"sample\"")
            && content.contains("data-listing-diff-right=\"new-tag\""),
        "expected diff-side dual-attribute anchor for the (sample, new-tag) pair; got:\n{content}",
    );
}

#[test]
fn listing_include_with_missing_file_fails_with_chapter_path_in_diagnostic() {
    let book = MinimalIncludesBook::new();
    let envelope =
        book.envelope_with_chapter("intro\n\n```rust\n{{#include listings/missing-tag.rs}}\n```\n");

    let stderr = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&stderr);

    assert!(
        stderr.contains("missing-tag"),
        "diagnostic should name the missing tag; got:\n{stderr}",
    );
    assert!(
        stderr.contains("expanding") || stderr.contains("include") || stderr.contains("missing"),
        "diagnostic should mention the include-expansion failure; got:\n{stderr}",
    );
}

/// Pipes the envelope through the preprocessor binary and returns the
/// transformed `Book` parsed from stdout.
fn run_preprocessor(envelope: String) -> Book {
    let output = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("parse stdout as Book")
}

/// Tempdir laid out as a real mdbook book root: a frozen listing under
/// `src/listings/sample.rs` and a `listings.toml` manifest registering
/// it. `MinimalIncludesBook::write_snippet` lays a snippet down on demand
/// so individual tests opt into the snippet path explicitly.
struct MinimalIncludesBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalIncludesBook {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().to_path_buf();

        let listings_dir = root.join("src").join("listings");
        fs::create_dir_all(&listings_dir).unwrap();
        fs::write(listings_dir.join("sample.rs"), "fn sample_body() {}\n").unwrap();
        fs::write(listings_dir.join("new-tag.rs"), "fn sample_body_v2() {}\n").unwrap();

        fs::write(
            root.join("listings.toml"),
            "version = 1\n\n\
             [[listing]]\n\
             tag = \"sample\"\n\
             source = \"../sample.rs\"\n\
             frozen = \"src/listings/sample.rs\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
             [[listing]]\n\
             tag = \"new-tag\"\n\
             source = \"../new.rs\"\n\
             frozen = \"src/listings/new-tag.rs\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .unwrap();

        Self { _tmp: tmp, root }
    }

    /// Write a file at `src/snippets/<rel>` so a chapter can reference it
    /// via `{{#include snippets/<rel>}}`.
    fn write_snippet(&self, rel: &str, content: &str) {
        let snippets_dir = self.root.join("src").join("snippets");
        fs::create_dir_all(&snippets_dir).unwrap();
        fs::write(snippets_dir.join(rel), content).unwrap();
    }

    /// Build the `[PreprocessorContext, Book]` JSON tuple mdbook would send,
    /// with one chapter carrying `chapter_content`.
    fn envelope_with_chapter(&self, chapter_content: &str) -> String {
        let ctx =
            PreprocessorContext::new(self.root.clone(), Config::default(), "html".to_string());
        let chapter = Chapter::new(
            "Include Test",
            chapter_content.to_string(),
            "include-test.md",
            vec![],
        );
        let book = Book::new_with_items(vec![BookItem::Chapter(chapter)]);
        serde_json::to_string(&(&ctx, &book)).expect("serialize envelope")
    }
}

fn chapter_content(book: &Book, chapter_name: &str) -> String {
    book.iter()
        .find_map(|item| match item {
            BookItem::Chapter(ch) if ch.name == chapter_name => Some(ch.content.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("chapter `{chapter_name}` missing from returned book"))
}
