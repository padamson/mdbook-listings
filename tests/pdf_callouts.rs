//! Asserts the typst-pdf renderer emits callout bodies into the PDF.
//! Runs against the just-built `book/build/typst-pdf/*.pdf` (the same
//! artifact CI publishes with the HTML site).

use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "slow (~20s): extracts full book PDF — run with `cargo test --test pdf_callouts -- --ignored`"]
fn ch04_pdf_contains_callout_bodies_emitted_by_pdf_splicer() {
    let pdf_path = pdf_path();
    let bytes =
        fs::read(&pdf_path).unwrap_or_else(|e| panic!("read PDF at {}: {}", pdf_path.display(), e));
    let text =
        pdf_extract::extract_text_from_mem(&bytes).expect("extract text from typst-pdf output");

    // The callout-v3 splice-entry marker has a body that's stable across
    // splicer revisions; the PDF emitter should emit it as a blockquote
    // line. We match on the body text fragment so we're robust to ordinal
    // and label-formatting changes.
    assert!(
        text.contains("HTML splicer entry point") || text.contains("splicer entry point"),
        "expected callout body text in extracted PDF; got first 4KB:\n{}",
        &text[..text.len().min(4096)],
    );
    // The cross-ref-emit body (added in slice 5) should also appear since
    // slice 5's diff exposes its callout marker.
    assert!(
        text.contains("Renders the prose-side anchor"),
        "expected cross-ref-emit body text in extracted PDF; got first 4KB:\n{}",
        &text[..text.len().min(4096)],
    );
}

fn pdf_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("book")
        .join("build")
        .join("typst-pdf")
        .join("mdbook-listings.pdf")
}
