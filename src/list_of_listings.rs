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

use serde::Serialize;

use crate::directive::{FencePolicy, scan_directives};
use crate::number::{ListingRef, label_text};

/// The marker's literal prefix. It takes no arguments, so it has no trailing
/// space (unlike `"{{#include "`); the scanner finds the closing `}}` itself.
const MARKER_PREFIX: &str = "{{#list-of-listings";

/// Where the sidebar variant of the index renders, set by
/// `[preprocessor.listings] list-of-listings-sidebar`. Off is the default;
/// `append` and `nested` are the two client-side rungs (Phases 2 and 3),
/// distinguished only in the JS — the Rust side emits the same manifest for
/// both, tagged with the mode so the script knows which layout to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    Off,
    Append,
    Nested,
}

impl SidebarMode {
    /// Parse the config value; anything unrecognised (including a malformed or
    /// empty string) falls back to `Off` rather than failing the build, matching
    /// how the boolean flags default off.
    pub fn parse(value: &str) -> Self {
        match value {
            "append" => Self::Append,
            "nested" => Self::Nested,
            _ => Self::Off,
        }
    }

    /// The `data-sidebar` value the JS reads off the manifest script.
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Append => "append",
            Self::Nested => "nested",
        }
    }
}

/// One chapter as the client-side manifest sees it: the same shape as
/// [`ChapterListings`] but with the link path pointing at the rendered `.html`
/// page (the JS consumes this at runtime, where mdbook's markdown-link
/// rewriting never reaches).
#[derive(Serialize)]
struct ManifestChapter<'a> {
    name: &'a str,
    path: String,
    listings: &'a [ListingRef],
}

/// Rewrite a chapter's source path to its rendered page: `ch03.md` ->
/// `ch03.html`. A path without a `.md` suffix is left untouched.
fn html_path(src_path: &str) -> String {
    match src_path.strip_suffix(".md") {
        Some(stem) => format!("{stem}.html"),
        None => src_path.to_string(),
    }
}

/// Emit the sidebar's mode marker (and, for `append`, its data) as an inline
/// script — `<script id="mdbook-listings-manifest" type="application/json"
/// data-sidebar="…">…</script>`. Returns `""` when the sidebar is off or no
/// chapter has a numbered listing.
///
/// `append` serialises the whole book-wide index into the body, since that
/// rung renders every listing regardless of page. `nested` reads the current
/// page's listings from the rendered content itself (the `listing-N-M` caption
/// anchors), so its script is just the mode marker with an empty body — no
/// point shipping a book-wide index every page won't use. Captions in the
/// `append` body are already HTML-escaped (as stored on the anchor), which
/// also keeps one from smuggling a `</script>` that would close the tag early.
pub fn render_manifest(chapters: &[ChapterListings], mode: SidebarMode) -> String {
    if mode == SidebarMode::Off {
        return String::new();
    }
    let view: Vec<ManifestChapter> = chapters
        .iter()
        .filter(|ch| !ch.listings.is_empty())
        .map(|ch| ManifestChapter {
            name: &ch.name,
            path: html_path(&ch.path),
            listings: &ch.listings,
        })
        .collect();
    if view.is_empty() {
        return String::new();
    }
    let body = match mode {
        SidebarMode::Append => serde_json::to_string(&view).unwrap_or_default(),
        // Nested reads listings from the page; the marker alone is enough.
        SidebarMode::Nested | SidebarMode::Off => String::new(),
    };
    format!(
        "\n<script id=\"mdbook-listings-manifest\" type=\"application/json\" data-sidebar=\"{}\">{body}</script>\n",
        mode.as_str()
    )
}

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
            label: None,
        }
    }

    fn one_chapter() -> Vec<ChapterListings> {
        vec![ChapterListings {
            name: "Freeze a listing".into(),
            path: "ch03.md".into(),
            listings: vec![listing("3.1", Some("The reuse manifest"), "listing-3-1")],
        }]
    }

    #[test]
    fn sidebar_mode_parses_known_values_and_defaults_off() {
        assert_eq!(SidebarMode::parse("append"), SidebarMode::Append);
        assert_eq!(SidebarMode::parse("nested"), SidebarMode::Nested);
        assert_eq!(SidebarMode::parse("off"), SidebarMode::Off);
        // Unknown / malformed values fall back to off rather than failing.
        assert_eq!(SidebarMode::parse("wobble"), SidebarMode::Off);
        assert_eq!(SidebarMode::parse(""), SidebarMode::Off);
    }

    #[test]
    fn manifest_is_empty_when_mode_off() {
        assert_eq!(render_manifest(&one_chapter(), SidebarMode::Off), "");
    }

    #[test]
    fn manifest_is_empty_when_no_listings() {
        let chapters = vec![ChapterListings {
            name: "Empty".into(),
            path: "empty.md".into(),
            listings: vec![],
        }];
        assert_eq!(render_manifest(&chapters, SidebarMode::Append), "");
    }

    #[test]
    fn manifest_emits_json_script_with_mode_and_html_paths() {
        let out = render_manifest(&one_chapter(), SidebarMode::Append);
        assert!(
            out.contains(r#"<script id="mdbook-listings-manifest" type="application/json" data-sidebar="append">"#),
            "manifest script tag with mode; got:\n{out}"
        );
        assert!(
            out.contains("</script>"),
            "closes the script tag; got:\n{out}"
        );
        // The chapter link target is the rendered .html page, not the .md source
        // (mdbook only rewrites markdown links, not JSON inside a <script>).
        assert!(
            out.contains(r#""path":"ch03.html""#),
            "path rewritten .md -> .html; got:\n{out}"
        );
        assert!(
            out.contains(r#""number":"3.1""#) && out.contains(r#""id":"listing-3-1""#),
            "listing number and id; got:\n{out}"
        );
        assert!(
            out.contains(r#""caption":"The reuse manifest""#),
            "caption carried through; got:\n{out}"
        );
    }

    #[test]
    fn manifest_carries_the_nested_mode_with_an_empty_body() {
        let out = render_manifest(&one_chapter(), SidebarMode::Nested);
        // Nested reads the listings from the page, so the marker's body is
        // empty — no book-wide index is shipped.
        assert!(
            out.contains(r#"data-sidebar="nested"></script>"#),
            "nested marker with empty body; got:\n{out}"
        );
        assert!(
            !out.contains("listing-3-1"),
            "nested body should carry no listing data; got:\n{out}"
        );
    }

    #[test]
    fn manifest_skips_chapters_without_listings() {
        let chapters = vec![
            ChapterListings {
                name: "Empty".into(),
                path: "empty.md".into(),
                listings: vec![],
            },
            ChapterListings {
                name: "Freeze a listing".into(),
                path: "ch03.md".into(),
                listings: vec![listing("3.1", Some("The reuse manifest"), "listing-3-1")],
            },
        ];
        let out = render_manifest(&chapters, SidebarMode::Append);
        assert!(
            !out.contains("empty.html"),
            "chapter with no listings is omitted; got:\n{out}"
        );
        assert!(
            out.contains("ch03.html"),
            "populated chapter kept; got:\n{out}"
        );
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
