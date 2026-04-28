//! Integration tests for the Show Diffs Between Slices story (ch. 3).

use std::fs;
use std::path::PathBuf;

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

#[test]
fn diff_directive_renders_to_fenced_diff_block() {
    let book = MinimalDiffsBook::new();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n{{#diff old-tag new-tag}}\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Diff Test");

    assert!(
        content.contains("```diff"),
        "expected the directive to render as a ```diff fenced block; got:\n{content}",
    );
    assert!(
        content.contains("--- old-tag") && content.contains("+++ new-tag"),
        "expected unified-diff headers naming the operands; got:\n{content}",
    );
    assert!(
        content.contains("-line two") && content.contains("+line TWO"),
        "expected the +/- lines from the frozen pair; got:\n{content}",
    );
}

#[test]
fn diff_directive_does_not_disturb_surrounding_chapter_content() {
    let book = MinimalDiffsBook::new();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n{{#diff old-tag new-tag}}\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Diff Test");

    assert!(
        content.starts_with("Before paragraph.\n"),
        "leading text should survive verbatim; got:\n{content}",
    );
    assert!(
        content.ends_with("After paragraph.\n"),
        "trailing text should survive verbatim; got:\n{content}",
    );
    assert!(
        !content.contains("{{#diff"),
        "directive should be consumed; got:\n{content}",
    );
}

#[test]
fn live_path_operand_diffs_against_disk_relative_to_book_root() {
    let book = MinimalDiffsBook::new();
    book.write_live_file("compose-live.yaml", b"line one\nline LIVE\n");

    let envelope = book.envelope_with_chapter(
        "Diffing live source.\n\n{{#diff old-tag live:compose-live.yaml}}\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Diff Test");

    assert!(
        content.contains("--- old-tag") && content.contains("+++ live:compose-live.yaml"),
        "expected headers naming the frozen tag and the live operand; got:\n{content}",
    );
    assert!(
        content.contains("-line two") && content.contains("+line LIVE"),
        "expected +/- lines reflecting the live source; got:\n{content}",
    );
}

#[test]
fn escaped_diff_directive_is_left_literal_minus_the_backslash() {
    let book = MinimalDiffsBook::new();
    let envelope =
        book.envelope_with_chapter("Use \\{{#diff old-tag new-tag}} verbatim in prose.\n");

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, "Diff Test");

    assert_eq!(
        content,
        "Use {{#diff old-tag new-tag}} verbatim in prose.\n"
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

/// Tempdir laid out as a real mdbook book root: `listings.toml` at the top
/// plus the two frozen files under `src/listings/` that the integration
/// chapters reference by tag.
struct MinimalDiffsBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalDiffsBook {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().to_path_buf();

        let listings_dir = root.join("src").join("listings");
        fs::create_dir_all(&listings_dir).unwrap();
        fs::write(listings_dir.join("old-tag.txt"), "line one\nline two\n").unwrap();
        fs::write(listings_dir.join("new-tag.txt"), "line one\nline TWO\n").unwrap();

        fs::write(
            root.join("listings.toml"),
            "version = 1\n\n\
             [[listing]]\n\
             tag = \"old-tag\"\n\
             source = \"../old.txt\"\n\
             frozen = \"src/listings/old-tag.txt\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
             [[listing]]\n\
             tag = \"new-tag\"\n\
             source = \"../new.txt\"\n\
             frozen = \"src/listings/new-tag.txt\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .unwrap();

        Self { _tmp: tmp, root }
    }

    /// Write a file at `rel` (relative to the book root) so a chapter can
    /// reference it via a `live:<rel>` operand.
    fn write_live_file(&self, rel: &str, bytes: &[u8]) {
        let abs = self.root.join(rel);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&abs, bytes).unwrap();
    }

    /// Build the `[PreprocessorContext, Book]` JSON tuple mdbook would send,
    /// with one chapter carrying `chapter_content`.
    fn envelope_with_chapter(&self, chapter_content: &str) -> String {
        let ctx =
            PreprocessorContext::new(self.root.clone(), Config::default(), "html".to_string());
        let chapter = Chapter::new(
            "Diff Test",
            chapter_content.to_string(),
            "diff-test.md",
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
