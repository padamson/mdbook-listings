//! Phase 1 of the List-of-Listings feature: a book-wide index of every
//! numbered listing, grouped by the chapter it appears in, rendered inline
//! wherever an author drops a `{{#list-of-listings}}` marker.
//!
//! The numbering pass ([`crate::number`]) is the data source: it returns a
//! [`crate::number::ListingRef`] per numbered listing and stamps a matching
//! `id` on each caption div. This module groups those refs by chapter and
//! replaces the marker with a linked Markdown list. It runs as a final,
//! book-wide pass — after every chapter has been numbered — because the index
//! spans the whole book.

use crate::directive::{FencePolicy, scan_directives};
use crate::number::{ListingRef, label_text};

/// The marker's literal prefix. It takes no arguments, so it has no trailing
/// space (unlike `"{{#include "`); the scanner finds the closing `}}` itself.
const MARKER_PREFIX: &str = "{{#list-of-listings";

/// One chapter's numbered listings, paired with the chapter title (the group
/// heading) and the link path to the chapter (the anchor target's page).
pub struct ChapterListings {
    pub name: String,
    /// Link path to the chapter, relative to the page hosting the marker.
    /// Phase 1 assumes both sit at the book's top level.
    pub path: String,
    pub listings: Vec<ListingRef>,
}

/// Render the grouped, linked index as Markdown: an `## <chapter>` subheading
/// per chapter that has listings, then a bullet linking each listing's
/// `Listing N.M — caption` label to its anchor. Chapters with no numbered
/// listings are skipped, so the order is document order minus the gaps.
pub fn render_index(chapters: &[ChapterListings]) -> String {
    let mut out = String::new();
    for ch in chapters {
        if ch.listings.is_empty() {
            continue;
        }
        out.push_str("## ");
        out.push_str(&ch.name);
        out.push_str("\n\n");
        for l in &ch.listings {
            let label = label_text(Some(&l.number), l.caption.as_deref());
            out.push_str(&format!("- [{label}]({}#{})\n", ch.path, l.id));
        }
        out.push('\n');
    }
    out
}

/// Replace every `{{#list-of-listings}}` marker in `content` with
/// `replacement` (the rendered index, or `""` when the feature is off so the
/// raw directive never leaks). Markers inside fenced code blocks are left
/// alone so a chapter can show the directive verbatim.
pub fn replace_markers(content: &str, replacement: &str) -> String {
    let occs: Vec<_> = scan_directives(content, MARKER_PREFIX, FencePolicy::SkipInside)
        .into_iter()
        // The prefix would also match e.g. `{{#list-of-listings-foo}}`; the
        // real marker takes no arguments, so require empty args.
        .filter(|o| o.args.trim().is_empty())
        .collect();
    if occs.is_empty() {
        return content.to_string();
    }
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for occ in &occs {
        out.push_str(&content[cursor..occ.span.start]);
        out.push_str(replacement);
        cursor = occ.span.end;
    }
    out.push_str(&content[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(number: &str, caption: Option<&str>, id: &str) -> ListingRef {
        ListingRef {
            number: number.to_string(),
            caption: caption.map(str::to_string),
            id: id.to_string(),
        }
    }

    #[test]
    fn render_groups_by_chapter_with_linked_entries() {
        let chapters = vec![
            ChapterListings {
                name: "Freeze a listing".into(),
                path: "ch03.md".into(),
                listings: vec![listing("3.1", Some("The reuse manifest"), "listing-3-1")],
            },
            ChapterListings {
                name: "Render callouts".into(),
                path: "ch05.md".into(),
                listings: vec![listing("5.1", Some("The claim layer"), "listing-5-1")],
            },
        ];
        let out = render_index(&chapters);
        assert!(
            out.contains("## Freeze a listing"),
            "chapter group heading; got:\n{out}"
        );
        assert!(
            out.contains("- [Listing 3.1 — The reuse manifest](ch03.md#listing-3-1)"),
            "linked entry; got:\n{out}"
        );
        assert!(
            out.contains("- [Listing 5.1 — The claim layer](ch05.md#listing-5-1)"),
            "linked entry; got:\n{out}"
        );
        let p3 = out.find("Freeze a listing").unwrap();
        let p5 = out.find("Render callouts").unwrap();
        assert!(p3 < p5, "groups in document order; got:\n{out}");
    }

    #[test]
    fn render_omits_caption_when_absent() {
        let chapters = vec![ChapterListings {
            name: "Ch".into(),
            path: "ch.md".into(),
            listings: vec![listing("1.1", None, "listing-1-1")],
        }];
        let out = render_index(&chapters);
        assert!(
            out.contains("- [Listing 1.1](ch.md#listing-1-1)"),
            "number-only entry; got:\n{out}"
        );
    }

    #[test]
    fn render_skips_chapters_without_listings() {
        let chapters = vec![ChapterListings {
            name: "Empty".into(),
            path: "empty.md".into(),
            listings: vec![],
        }];
        assert_eq!(render_index(&chapters), "");
    }

    #[test]
    fn replace_swaps_marker_for_replacement() {
        let content = "# List of Listings\n\n{{#list-of-listings}}\n";
        let out = replace_markers(content, "INDEX");
        assert_eq!(out, "# List of Listings\n\nINDEX\n");
    }

    #[test]
    fn replace_strips_marker_with_empty_replacement() {
        let content = "before\n\n{{#list-of-listings}}\n\nafter\n";
        let out = replace_markers(content, "");
        assert_eq!(out, "before\n\n\n\nafter\n");
    }

    #[test]
    fn replace_leaves_content_without_marker_untouched() {
        let content = "no marker here\n";
        assert_eq!(replace_markers(content, "INDEX"), content);
    }

    #[test]
    fn replace_skips_marker_inside_code_fence() {
        // A chapter documenting the directive must be able to show it verbatim.
        let content = "```text\n{{#list-of-listings}}\n```\n";
        assert_eq!(replace_markers(content, "INDEX"), content);
    }
}
