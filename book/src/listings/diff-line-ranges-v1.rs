//! Integration tests for slice 9: the `{{#diff a b START:END START:END}}`
//! line-range form. Each test exercises one facet of the feature; the
//! file is frozen as `diff-line-ranges-v1.rs` and shown in ch.4 slice 9
//! via `{{#include listings/diff-line-ranges-v1.rs:LR}}` to dogfood the
//! companion `{{#include}}` line-range syntax in the same slice that
//! introduces it.

use std::fs;
use std::path::PathBuf;

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

// CALLOUT: diff-range-slices Both source files are sliced to their respective ranges before the diff algorithm runs, so a `+`/`-` line in the rendered diff reflects only differences inside the selected windows.
#[test]
fn diff_with_line_ranges_renders_only_the_sliced_portion() {
    let book = MinimalLineRangeBook::new();
    book.write_listing("old.txt", b"line1\nold-2\nline3\nold-4\nline5\n");
    book.write_listing("new.txt", b"line1\nnew-2\nline3\nnew-4\nline5\n");
    let envelope = book.envelope_with_chapter("{{#diff old new 1:2 1:2}}\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(content.contains("-old-2") && content.contains("+new-2"));
    assert!(!content.contains("old-4") && !content.contains("new-4"));
}

// CALLOUT: diff-range-absolute-line-numbers The hunk-header `@@ -A,B +C,D @@` line numbers in a sliced diff reference absolute positions in the parent files, not slice-relative offsets — readers can map a `+` line in the diff straight back to its line number in the unsliced source.
#[test]
fn diff_with_line_ranges_emits_absolute_line_numbers_in_hunk_headers() {
    let book = MinimalLineRangeBook::new();
    let mut left = String::new();
    let mut right = String::new();
    for i in 1..=70 {
        left.push_str(&format!("line{i}\n"));
        if i == 60 {
            right.push_str("line60-CHANGED\n");
        } else {
            right.push_str(&format!("line{i}\n"));
        }
    }
    book.write_listing("old.txt", &left.into_bytes());
    book.write_listing("new.txt", &right.into_bytes());
    let envelope = book.envelope_with_chapter("{{#diff old new 55:65 55:65}}\n");
    let content = chapter_content(&run_preprocessor(envelope));
    let hunk = content
        .lines()
        .find(|l| l.starts_with("@@ "))
        .unwrap_or_else(|| panic!("no hunk header in:\n{content}"));
    let parts: Vec<&str> = hunk.split_whitespace().collect();
    let l: usize = parts[1]
        .trim_start_matches('-')
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let r: usize = parts[2]
        .trim_start_matches('+')
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (55..=65).contains(&l) && (55..=65).contains(&r),
        "expected absolute line numbers in [55,65]; got `{hunk}`",
    );
}

// CALLOUT: diff-range-anchor-attrs The locator anchor that follows a sliced diff carries `data-listing-diff-{left,right}-range` attributes, so the screenshot tool can address the same `(LEFT, RIGHT)` pair sliced two different ways without selector collisions.
#[test]
fn diff_with_line_ranges_emits_range_data_attributes_on_locator_anchor() {
    let book = MinimalLineRangeBook::new();
    book.write_listing("old.txt", b"a\nb\nc\nd\n");
    book.write_listing("new.txt", b"a\nB\nc\nD\n");
    let envelope = book.envelope_with_chapter("{{#diff old new 1:2 1:3}}\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(content.contains(r#"data-listing-diff-left-range="1:2""#));
    assert!(content.contains(r#"data-listing-diff-right-range="1:3""#));
}

// CALLOUT: diff-range-callout-composes A `// CALLOUT:` marker that lives inside the slice window flows through the full pipeline: the diff splicer hands the sliced bytes to the callout splicer, which strips the marker comment from the rendered listing and emits a `<button id="callout-LABEL">` badge keyed on the label — the line-range form composes with callouts the same way whole-file diffs do.
#[test]
fn diff_with_line_ranges_renders_a_badge_for_a_callout_inside_the_window() {
    let book = MinimalLineRangeBook::new();
    let mut right = String::new();
    for i in 1..=20 {
        if i == 10 {
            right.push_str(
                "// CALLOUT: sliced-callout Demonstrates that callouts compose with line ranges.\n",
            );
        } else {
            right.push_str(&format!("// row {i}\n"));
        }
    }
    let mut left = String::new();
    for i in 1..=20 {
        left.push_str(&format!("// row {i}\n"));
    }
    book.write_listing("old.rs", &left.into_bytes());
    book.write_listing("new.rs", &right.into_bytes());
    let envelope = book.envelope_with_chapter("{{#diff old.rs new.rs 5:15 5:15}}\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains(r#"id="callout-sliced-callout""#),
        "expected a badge with id=callout-sliced-callout for the marker inside the slice; got:\n{content}",
    );
    assert!(
        !content.contains("CALLOUT: sliced-callout"),
        "the marker comment line should be stripped from the rendered listing once the badge is emitted; got:\n{content}",
    );
}

/// Tempdir + helpers shared by every line-range test in this file.
struct MinimalLineRangeBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalLineRangeBook {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        // Manifest that registers `old` and `new` tags pointing at the
        // listings the per-test `write_listing` helper writes below.
        fs::write(
            root.join("listings.toml"),
            "version = 1\n\n\
             [[listing]]\n\
             tag = \"old\"\nsource = \"../old\"\nfrozen = \"src/listings/old.txt\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
             [[listing]]\n\
             tag = \"new\"\nsource = \"../new\"\nfrozen = \"src/listings/new.txt\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
             [[listing]]\n\
             tag = \"old.rs\"\nsource = \"../old.rs\"\nfrozen = \"src/listings/old.rs\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n\n\
             [[listing]]\n\
             tag = \"new.rs\"\nsource = \"../new.rs\"\nfrozen = \"src/listings/new.rs\"\n\
             sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .unwrap();
        Self { _tmp: tmp, root }
    }

    fn write_listing(&self, rel: &str, bytes: &[u8]) {
        fs::write(self.root.join("src/listings").join(rel), bytes).unwrap();
    }

    fn envelope_with_chapter(&self, content: &str) -> String {
        let ctx =
            PreprocessorContext::new(self.root.clone(), Config::default(), "html".to_string());
        let chapter = Chapter::new("Line Ranges", content.to_string(), "lr.md", vec![]);
        let book = Book::new_with_items(vec![BookItem::Chapter(chapter)]);
        serde_json::to_string(&(&ctx, &book)).expect("serialize envelope")
    }
}

fn run_preprocessor(envelope: String) -> Book {
    let out = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).expect("Book")
}

fn chapter_content(book: &Book) -> String {
    for item in &book.items {
        if let BookItem::Chapter(c) = item {
            return c.content.clone();
        }
    }
    panic!("no chapter in book");
}
