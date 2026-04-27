//! Integration tests for the Show Diffs Between Slices story (ch. 3).

use std::fs;
use std::path::{Path, PathBuf};

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

#[test]
#[ignore = "directive parsing + diff rendering land in slices 2–5"]
fn diff_directive_renders_to_fenced_diff_block() {
    let book = MinimalDiffsBook::new();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n{{#diff old-tag new-tag}}\n\nAfter paragraph.\n",
    );

    let output = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let returned: Book = serde_json::from_slice(&output).expect("parse stdout as Book");
    let content = chapter_content(&returned, "Diff Test");

    assert!(
        content.contains("```diff"),
        "expected the directive to render as a ```diff fenced block; got:\n{content}",
    );
}

/// The smallest tempdir fixture that backs the diff preprocessor: a `book.toml`
/// that registers `[preprocessor.listings]`, a `book/listings.toml` with two
/// frozen entries the chapters can reference by tag, and the matching frozen
/// files. The tempdir is destroyed when the struct drops.
struct MinimalDiffsBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalDiffsBook {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().to_path_buf();

        fs::write(
            root.join("book.toml"),
            "[book]\ntitle = \"Test\"\n\n[preprocessor.listings]\ncommand = \"mdbook-listings\"\n",
        )
        .unwrap();

        let listings_dir = root.join("book").join("src").join("listings");
        fs::create_dir_all(&listings_dir).unwrap();
        fs::write(listings_dir.join("old-tag.txt"), "line one\nline two\n").unwrap();
        fs::write(listings_dir.join("new-tag.txt"), "line one\nline TWO\n").unwrap();

        fs::write(
            root.join("book").join("listings.toml"),
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

    #[allow(dead_code)]
    fn root(&self) -> &Path {
        &self.root
    }

    /// Build the JSON envelope mdbook would send a preprocessor: the tuple
    /// `(PreprocessorContext, Book)` serialised as a two-element JSON array,
    /// with one chapter carrying the supplied markdown.
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
