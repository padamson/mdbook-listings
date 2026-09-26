//! Stable listing cross-references: `label="..."` on `{{#include}}` /
//! `{{#diff}}` names a listing, and `{{#listing-ref <label>}}` in prose
//! resolves to the listing's *current* `Listing N.M`, hyperlinked — so prose
//! can say "see Listing 5.4" without going stale when numbers shift.
//!
//! This is the outermost (acceptance) test: it drives the feature end-to-end
//! through the preprocessor binary. Inner unit tests in `src/` cover the
//! pieces.

mod common;
use common::book::NUMBERED;
use common::book::{MinimalBook, Page, chapter_content, run_preprocessor};
use common::mdbook_listings;

#[test]
fn listing_ref_resolves_to_current_number_with_link() {
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(NUMBERED, &[
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
    ]);

    let returned = run_preprocessor(envelope);
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
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(
        NUMBERED,
        &[
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
        ],
    );

    let returned = run_preprocessor(envelope);
    let ch05 = chapter_content(&returned, "Render callouts");
    assert!(
        ch05.contains("[Listing 4.1](ch04.md#listing-4-1)"),
        "diff listings take labels too; got:\n{ch05}",
    );
}

#[test]
fn listing_ref_inside_fence_is_left_verbatim() {
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(
        NUMBERED,
        &[
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
        ],
    );

    let returned = run_preprocessor(envelope);
    let ch08 = chapter_content(&returned, "Recipes");
    assert!(
        ch08.contains("{{#listing-ref reuse-manifest}}"),
        "a fenced example must stay verbatim; got:\n{ch08}",
    );
}

#[test]
fn unknown_label_fails_the_build_naming_label_and_chapter() {
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(
        NUMBERED,
        &[
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
        ],
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
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(
        NUMBERED,
        &[
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
        ],
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

#[test]
fn listing_ref_resolves_to_an_appendix_letter_listing() {
    let book = MinimalBook::with_sample_and_claim();
    let envelope = book.envelope(NUMBERED, &[
        Page {
            name: "Render callouts",
            path: "ch05.md",
            number: Some(&[5]),
            content: "The full catalog is in {{#listing-ref worked-example}}.\n",
        },
        Page {
            name: "Appendix A: The Worked Example",
            path: "appendix-a.md",
            number: None,
            content: "```rust\n{{#include listings/sample.rs label=\"worked-example\" caption=\"The catalog\"}}\n```\n",
        },
    ]);

    let returned = run_preprocessor(envelope);
    let ch05 = chapter_content(&returned, "Render callouts");
    assert!(
        ch05.contains("[Listing A.1](appendix-a.md#listing-A-1)"),
        "refs should resolve to appendix-lettered listings; got:\n{ch05}",
    );
}
