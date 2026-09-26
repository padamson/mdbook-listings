//! Integration tests for slice 8: `{{#include listings/...}}` interception
//! and the `<div data-listing-tag>` locator anchor the include splicer
//! emits after each frozen-listing fenced block.

mod common;
use common::book::{CHAPTER, MinimalBook, chapter_content, run_preprocessor};
use common::mdbook_listings;

#[test]
fn listing_include_directive_is_replaced_with_file_contents_inline() {
    let book = includes_book();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n```rust\n{{#include listings/sample.rs}}\n```\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

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
    let book = includes_book();
    let envelope = book
        .envelope_with_chapter("```rust\n{{#include listings/sample.rs}}\n```\nAfter paragraph.\n");

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

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
fn listing_include_without_a_fence_renders_its_own_highlighted_block() {
    // The directive on a line by itself, the way `{{#diff}}` has always
    // been written. The preprocessor supplies the fence and picks the
    // language off the file extension.
    let book = includes_book();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n{{#include listings/sample.rs}}\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

    assert!(
        content.contains("```rust\nfn sample_body() {}\n```\n"),
        "expected a self-contained rust block; got:\n{content}",
    );
    assert!(
        content.contains("data-listing-tag=\"sample\""),
        "the locator anchor still lands; got:\n{content}",
    );
    assert!(
        content.contains("Before paragraph.") && content.contains("After paragraph."),
        "surrounding prose preserved; got:\n{content}",
    );
}

#[test]
fn fenced_and_unfenced_listing_includes_render_identically() {
    // Dropping the fence from an existing book must be a no-op, so an
    // author can migrate a chapter without re-reading its output.
    let book = includes_book();
    let fenced = chapter_content(
        &run_preprocessor(
            book.envelope_with_chapter("```rust\n{{#include listings/sample.rs}}\n```\n"),
        ),
        CHAPTER,
    );
    let bare = chapter_content(
        &run_preprocessor(book.envelope_with_chapter("{{#include listings/sample.rs}}\n")),
        CHAPTER,
    );
    assert_eq!(fenced, bare, "fenced:\n{fenced}\nbare:\n{bare}");
}

#[test]
fn snippet_include_is_expanded_inline_without_listing_tag_anchor() {
    let book = includes_book();
    book.write_snippet("excerpt.rs", "fn snippet_body() {}\n");
    let envelope = book.envelope_with_chapter("```rust\n{{#include snippets/excerpt.rs}}\n```\n");

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

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
    let book = includes_book();
    let envelope = book.envelope_with_chapter(concat!(
        "First show as include.\n\n",
        "```rust\n{{#include listings/sample.rs}}\n```\n\n",
        "Then diff against new-tag.\n\n",
        "{{#diff sample new-tag}}\n",
    ));

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

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
    let book = includes_book();
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

    // The chapter path and line lead so an author can jump to the directive;
    // the tag names the file that is missing. The prose between them can
    // change freely.
    assert!(
        stderr.contains("chapter.md:4:"),
        "diagnostic should lead with the chapter path and directive line; got:\n{stderr}",
    );
    assert!(
        stderr.contains("references missing file"),
        "diagnostic should say the file is missing, not that expansion failed generically; got:\n{stderr}",
    );
    assert!(
        stderr.contains("listings/missing-tag.rs"),
        "diagnostic should name the missing file; got:\n{stderr}",
    );
}

/// A frozen listing `sample` plus a second tag `new-tag` to diff it against.
fn includes_book() -> MinimalBook {
    MinimalBook::new()
        .with_listing("sample", "sample.rs", b"fn sample_body() {}\n")
        .with_listing("new-tag", "new-tag.rs", b"fn sample_body_v2() {}\n")
}
