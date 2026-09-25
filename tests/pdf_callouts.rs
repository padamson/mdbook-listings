//! Asserts the typst-pdf renderer emits each callout into the PDF as a
//! numbered label-and-body line. Runs against the just-built
//! `book/build/typst-pdf/*.pdf` (the same artifact CI publishes with the
//! HTML site).

use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "needs the built book PDF (~20s to extract); run with `cargo nextest run -E 'binary(pdf_callouts)' --run-ignored only`"]
fn ch05_pdf_carries_each_callout_as_a_numbered_label_and_body_line() {
    let text = collapse_whitespace(&extracted_pdf_text());

    // The PDF emitter writes `**[N.N.N] label** — body` per callout. The
    // bodies are read from the frozen listing ch05 renders (callout-v3 is
    // the version whose diff exposes both markers), so a copy edit in
    // chapter prose cannot fail this test and a byte change in the frozen
    // file already fails `verify`.
    for label in ["splice-entry", "cross-ref-emit"] {
        let body = marker_body("callout-v3.rs", label);
        let needle = format!("] {label} — {body}");
        let at = text.find(&needle).unwrap_or_else(|| {
            let head: String = text.chars().take(4096).collect();
            panic!("expected `{needle}` in the extracted PDF text; it starts:\n{head}")
        });
        // The badge sits right before the `]`; look back a few characters,
        // not to the previous `[` wherever that is.
        let before: String = text[..at]
            .chars()
            .rev()
            .take(16)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let badge = before.rsplit('[').next().unwrap_or("");
        assert!(
            !badge.is_empty() && badge.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "the label should follow a `[listing.ordinal]` badge; got `…{before}] {label}`",
        );
    }
}

fn extracted_pdf_text() -> String {
    let pdf_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("book")
        .join("build")
        .join("typst-pdf")
        .join("mdbook-listings.pdf");
    let bytes =
        fs::read(&pdf_path).unwrap_or_else(|e| panic!("read PDF at {}: {}", pdf_path.display(), e));
    pdf_extract::extract_text_from_mem(&bytes).expect("extract text from typst-pdf output")
}

/// The body of the `// CALLOUT: <label> <body>` marker in a frozen listing.
fn marker_body(listing: &str, label: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("book/src/listings")
        .join(listing);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read frozen listing {}: {}", path.display(), e));
    let prefix = format!("// CALLOUT: {label} ");
    source
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(collapse_whitespace)
        .unwrap_or_else(|| panic!("no `{prefix}` marker in {}", path.display()))
}

/// Text extraction breaks the PDF's wrapped lines wherever typst did and
/// pads around bold runs; the claim is about words, not layout.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
