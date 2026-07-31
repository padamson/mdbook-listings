//! Stable listing cross-references: `{{#listing-ref <label>}}` in prose
//! resolves to the *current* `Listing N.M` of the listing that declared
//! `label="<label>"`, hyperlinked in HTML output. Numbers are assigned by
//! order of appearance, so a hand-written "see Listing 5.4" goes stale the
//! moment a listing is inserted above it — the ref directive is what makes
//! pointing at a listing safe, mirroring what `{{#callout <label>}}` already
//! does for badges one level down.
//!
//! Runs as a final, book-wide pass (a ref can point across chapters), on the
//! label index collected by the numbering pass. Unknown and duplicate labels
//! fail the build: a broken pointer in prose is a defect, not a warning.

use std::collections::HashMap;

use crate::callout::SupportedRenderer;
use crate::directive::{FencePolicy, line_number, scan_directives};
use crate::list_of_listings::ChapterListings;

/// The directive's literal prefix; the trailing space separates it from
/// `{{#listing-ref}}`-shaped prose typos, which are left untouched.
const REF_PREFIX: &str = "{{#listing-ref ";

/// Where a label points: the listing's current number, its caption-div
/// anchor id, and the chapter file that hosts it.
#[derive(Debug)]
struct Target {
    number: String,
    path: String,
    id: String,
}

/// Book-wide label → listing lookup, built from the collected numbering
/// output once per build.
#[derive(Debug)]
pub struct LabelIndex {
    targets: HashMap<String, Target>,
}

impl LabelIndex {
    /// Index every labelled listing. A label defined twice is ambiguous —
    /// there is no right listing for a ref to resolve to — so it fails the
    /// build naming both occurrences.
    pub fn build(chapters: &[ChapterListings]) -> Result<Self, String> {
        let mut targets = HashMap::new();
        for ch in chapters {
            for l in &ch.listings {
                let Some(label) = &l.label else { continue };
                let target = Target {
                    number: l.number.clone(),
                    path: ch.path.clone(),
                    id: l.id.clone(),
                };
                if let Some(prev) = targets.insert(label.clone(), target) {
                    return Err(format!(
                        "listing label \"{label}\" is defined twice \
                         (Listing {} and Listing {}) — labels must be unique \
                         across the book so a {{{{#listing-ref}}}} has one target",
                        prev.number,
                        chapters
                            .iter()
                            .flat_map(|c| &c.listings)
                            .find(|r| r.label.as_deref() == Some(label))
                            .map(|r| r.number.as_str())
                            .unwrap_or("?"),
                    ));
                }
            }
        }
        Ok(Self { targets })
    }
}

/// Replace every `{{#listing-ref <label>}}` outside fenced code with the
/// target listing's current number — a markdown link to its anchor in HTML
/// (mdbook rewrites the `.md` path downstream), plain `Listing N.M` text for
/// the typst-pdf renderer. An unknown label fails the build with the chapter
/// and line, like an unknown `{{#callout}}`.
pub fn replace_refs(
    content: &str,
    chapter_name: &str,
    index: &LabelIndex,
    renderer: SupportedRenderer,
) -> Result<String, String> {
    let occs = scan_directives(content, REF_PREFIX, FencePolicy::SkipInside);
    if occs.is_empty() {
        return Ok(content.to_string());
    }
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for occ in &occs {
        let label = occ.args.trim();
        let Some(target) = index.targets.get(label) else {
            return Err(format!(
                "{{{{#listing-ref {label}}}}} in chapter \"{chapter_name}\" \
                 (line {}) references a label that no numbered listing \
                 defines — check the label, and note that refs resolve to \
                 listing numbers, so the target listing must be numbered \
                 (number-listings on, numbered chapter)",
                line_number(content, occ.span.start),
            ));
        };
        let replacement = match renderer {
            SupportedRenderer::Html => {
                format!("[Listing {}]({}#{})", target.number, target.path, target.id)
            }
            SupportedRenderer::TypstPdf => format!("Listing {}", target.number),
        };
        out.push_str(&content[cursor..occ.span.start]);
        out.push_str(&replacement);
        cursor = occ.span.end;
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::ListingRef;

    fn chapters() -> Vec<ChapterListings> {
        vec![ChapterListings {
            name: "Freeze a listing".into(),
            path: "ch03.md".into(),
            listings: vec![ListingRef {
                number: "3.1".into(),
                caption: Some("The reuse manifest".into()),
                id: "listing-3-1".into(),
                label: Some("reuse-manifest".into()),
            }],
        }]
    }

    #[test]
    fn ref_resolves_to_linked_number_in_html() {
        let index = LabelIndex::build(&chapters()).expect("index");
        let out = replace_refs(
            "See {{#listing-ref reuse-manifest}} here.\n",
            "Ch",
            &index,
            SupportedRenderer::Html,
        )
        .expect("resolve");
        assert_eq!(out, "See [Listing 3.1](ch03.md#listing-3-1) here.\n");
    }

    #[test]
    fn ref_renders_plain_text_for_typst() {
        let index = LabelIndex::build(&chapters()).expect("index");
        let out = replace_refs(
            "See {{#listing-ref reuse-manifest}}.\n",
            "Ch",
            &index,
            SupportedRenderer::TypstPdf,
        )
        .expect("resolve");
        assert_eq!(out, "See Listing 3.1.\n");
    }

    #[test]
    fn unknown_label_errors_with_chapter_and_line() {
        let index = LabelIndex::build(&chapters()).expect("index");
        let err = replace_refs(
            "line one\nSee {{#listing-ref nope}}.\n",
            "Render callouts",
            &index,
            SupportedRenderer::Html,
        )
        .expect_err("unknown label must fail");
        assert!(err.contains("nope"), "names the label; got: {err}");
        assert!(
            err.contains("Render callouts") && err.contains("line 2"),
            "names chapter and line; got: {err}"
        );
    }

    #[test]
    fn duplicate_label_errors_naming_label() {
        let mut chs = chapters();
        chs.push(ChapterListings {
            name: "Another".into(),
            path: "ch05.md".into(),
            listings: vec![ListingRef {
                number: "5.1".into(),
                caption: None,
                id: "listing-5-1".into(),
                label: Some("reuse-manifest".into()),
            }],
        });
        let err = LabelIndex::build(&chs).expect_err("duplicate must fail");
        assert!(
            err.contains("reuse-manifest") && err.contains("twice"),
            "names the label; got: {err}"
        );
    }

    #[test]
    fn ref_inside_fence_left_verbatim() {
        let index = LabelIndex::build(&chapters()).expect("index");
        let content = "```text\n{{#listing-ref reuse-manifest}}\n```\n";
        let out = replace_refs(content, "Ch", &index, SupportedRenderer::Html).expect("ok");
        assert_eq!(out, content);
    }
}
