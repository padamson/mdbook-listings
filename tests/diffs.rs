//! Integration tests for the Show Diffs Between Slices story (ch. 3).

mod common;
use common::book::{CHAPTER, MinimalBook, chapter_content, run_preprocessor};

#[test]
fn diff_directive_renders_to_fenced_diff_block() {
    let book = diffs_book();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n{{#diff old-tag new-tag}}\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

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
    let book = diffs_book();
    let envelope = book.envelope_with_chapter(
        "Before paragraph.\n\n{{#diff old-tag new-tag}}\n\nAfter paragraph.\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

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
fn live_path_operand_resolves_relative_to_chapter_directory() {
    let book = diffs_book();
    book.write_file("src/compose-live.yaml", b"line one\nline LIVE\n");

    let envelope = book.envelope_with_chapter(
        "Diffing live source.\n\n{{#diff old-tag live:compose-live.yaml}}\n",
    );

    let returned = run_preprocessor(envelope);
    let content = chapter_content(&returned, CHAPTER);

    assert!(
        content.contains("--- old-tag") && content.contains("+++ live:compose-live.yaml"),
        "expected headers naming the frozen tag and the live operand; got:\n{content}",
    );
    assert!(
        content.contains("-line two") && content.contains("+line LIVE"),
        "expected +/- lines reflecting the live source; got:\n{content}",
    );
}

/// Two frozen versions of one file, `old-tag` and `new-tag`, one line apart.
fn diffs_book() -> MinimalBook {
    MinimalBook::new()
        .with_listing("old-tag", "old-tag.txt", b"line one\nline two\n")
        .with_listing("new-tag", "new-tag.txt", b"line one\nline TWO\n")
}
