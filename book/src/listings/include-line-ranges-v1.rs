//! Integration tests for slice 9: the `{{#include path:START:END}}` line-
//! range form. Each test exercises one facet of the feature; the file is
//! frozen as `include-line-ranges-v1.rs` and shown in ch.4 slice 9 via
//! `{{#include listings/include-line-ranges-v1.rs:LR}}` — the slice
//! demonstrates the include-line-range syntax by using it on this very
//! file.

use std::fs;
use std::path::PathBuf;

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

mod common;
use common::mdbook_listings;

// CALLOUT: include-range-slices The line-range form `path:start:end` slices the file body before inlining; lines outside the range never appear in the rendered chapter.
#[test]
fn include_with_line_range_inlines_only_the_sliced_lines() {
    let book = MinimalIncludeLineRangeBook::new();
    book.write_listing(
        "ranged.rs",
        b"line1\nline2\nline3\nline4\nline5\nline6\nline7\n",
    );
    let envelope =
        book.envelope_with_chapter("```rust\n{{#include listings/ranged.rs:3:5}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(content.contains("line3\nline4\nline5"));
    assert!(!content.contains("line1") && !content.contains("line7"));
}

// CALLOUT: include-range-header The rendered listing prepends a two-line header mirroring unified-diff's `--- TAG\n@@ -A,B +C,D @@` shape: file basename on line 1, `@@ start,end @@` on line 2. Both are comment-prefixed when the file extension has a known single-line syntax, so highlighters render them as metadata rather than invalid code.
#[test]
fn include_with_line_range_prepends_a_two_line_diff_style_header() {
    let book = MinimalIncludeLineRangeBook::new();
    book.write_listing(
        "ranged.rs",
        b"line1\nline2\nline3\nline4\nline5\nline6\nline7\n",
    );
    let envelope =
        book.envelope_with_chapter("```rust\n{{#include listings/ranged.rs:3:5}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains("// ranged.rs\n// @@ 3,5 @@"),
        "expected two-line `// basename\\n// @@ start,end @@` header; got:\n{content}",
    );
}

// CALLOUT: include-range-header-language-aware The header's comment prefix tracks the file extension via the same `comment_prefix_for_extension` table the callout parser uses. Python/YAML/TOML/shell get `#`; SQL gets `--`; files with no recognised extension get a raw header with no prefix at all (which the highlighter may render as plain text).
#[test]
fn include_range_header_uses_hash_comment_prefix_for_python_extension() {
    let book = MinimalIncludeLineRangeBook::new();
    book.write_listing("script.py", b"a\nb\nc\nd\n");
    let envelope =
        book.envelope_with_chapter("```python\n{{#include listings/script.py:1:2}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains("# script.py\n# @@ 1,2 @@"),
        "expected `#`-prefixed header for `.py` extension; got:\n{content}",
    );
}

#[test]
fn include_range_header_uses_double_dash_comment_prefix_for_sql_extension() {
    let book = MinimalIncludeLineRangeBook::new();
    book.write_listing("schema.sql", b"a\nb\nc\nd\n");
    let envelope =
        book.envelope_with_chapter("```sql\n{{#include listings/schema.sql:1:2}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains("-- schema.sql\n-- @@ 1,2 @@"),
        "expected `--`-prefixed header for `.sql` extension; got:\n{content}",
    );
}

#[test]
fn include_range_header_omits_comment_prefix_for_unknown_extension() {
    let book = MinimalIncludeLineRangeBook::new();
    book.write_listing("readme.txt", b"a\nb\nc\nd\n");
    let envelope =
        book.envelope_with_chapter("```text\n{{#include listings/readme.txt:1:2}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains("readme.txt\n@@ 1,2 @@"),
        "expected raw header (no prefix) for unknown `.txt` extension; got:\n{content}",
    );
    assert!(
        !content.contains("// readme.txt") && !content.contains("# readme.txt"),
        "no comment prefix expected; got:\n{content}",
    );
}

// CALLOUT: include-range-anchor The locator anchor for a sliced include carries a `data-listing-tag-range` attribute, so the screenshot tool can address the same listing sliced two different ways without selector collisions.
#[test]
fn include_with_line_range_emits_range_data_attribute_on_locator_anchor() {
    let book = MinimalIncludeLineRangeBook::new();
    book.write_listing("ranged.rs", b"a\nb\nc\nd\ne\n");
    let envelope =
        book.envelope_with_chapter("```rust\n{{#include listings/ranged.rs:2:4}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(content.contains(r#"data-listing-tag="ranged""#));
    assert!(content.contains(r#"data-listing-tag-range="2:4""#));
}

// CALLOUT: include-range-callout-composes A `// CALLOUT:` marker that lives inside the slice window flows through the full pipeline: the include splicer slices the file bytes, the callout splicer strips the marker line and emits a `<button id="callout-LABEL">` badge — the line-range form composes with callouts the same way whole-file includes do.
#[test]
fn include_with_line_range_renders_a_badge_for_a_callout_inside_the_window() {
    let book = MinimalIncludeLineRangeBook::new();
    let mut body = String::new();
    for i in 1..=20 {
        if i == 10 {
            body.push_str("// CALLOUT: in-slice-callout Demo body for sliced-include callouts.\n");
        } else {
            body.push_str(&format!("// row {i}\n"));
        }
    }
    book.write_listing("with-callouts.rs", body.as_bytes());
    let envelope =
        book.envelope_with_chapter("```rust\n{{#include listings/with-callouts.rs:5:15}}\n```\n");
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains(r#"id="callout-in-slice-callout""#),
        "expected a badge with id=callout-in-slice-callout from the slice; got:\n{content}",
    );
    assert!(
        !content.contains("CALLOUT: in-slice-callout"),
        "the marker line itself should be stripped from the rendered listing; got:\n{content}",
    );
}

// CALLOUT: include-range-cross-ref-resolves Chapter prose can `{{#callout LABEL}}` a callout whose marker lives inside a sliced include — the cross-reference resolves to the same `id="callout-LABEL"` anchor the badge gets, regardless of which range the include used.
#[test]
fn cross_ref_to_callout_inside_sliced_include_resolves_to_badge_anchor() {
    let book = MinimalIncludeLineRangeBook::new();
    let mut body = String::new();
    for i in 1..=20 {
        if i == 10 {
            body.push_str("// CALLOUT: refed-from-prose Cross-ref target.\n");
        } else {
            body.push_str(&format!("// row {i}\n"));
        }
    }
    book.write_listing("with-callouts.rs", body.as_bytes());
    let envelope = book.envelope_with_chapter(concat!(
        "Cross-ref demo: see callout {{#callout refed-from-prose}}.\n\n",
        "```rust\n{{#include listings/with-callouts.rs:5:15}}\n```\n",
    ));
    let content = chapter_content(&run_preprocessor(envelope));
    assert!(
        content.contains(r##"href="#callout-refed-from-prose""##),
        "cross-ref must resolve to the badge anchor; got:\n{content}",
    );
    assert!(
        content.contains(r#"id="callout-refed-from-prose""#),
        "badge id must exist as the anchor target; got:\n{content}",
    );
}

/// Tempdir helper. Writes test fixtures under `src/listings/` and builds
/// the `[ctx, book]` envelope mdbook hands a preprocessor.
struct MinimalIncludeLineRangeBook {
    _tmp: TempDir,
    root: PathBuf,
}

impl MinimalIncludeLineRangeBook {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        // Empty manifest — the include splicer doesn't consult it; only
        // the diff splicer does.
        fs::write(root.join("listings.toml"), "version = 1\n").unwrap();
        Self { _tmp: tmp, root }
    }

    fn write_listing(&self, rel: &str, bytes: &[u8]) {
        fs::write(self.root.join("src/listings").join(rel), bytes).unwrap();
    }

    fn envelope_with_chapter(&self, content: &str) -> String {
        let ctx =
            PreprocessorContext::new(self.root.clone(), Config::default(), "html".to_string());
        let chapter = Chapter::new("Include Line Ranges", content.to_string(), "ilr.md", vec![]);
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
