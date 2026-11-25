//! `remove`: retire a frozen listing the book no longer uses, dropping its
//! manifest record, its frozen file, and any sidecar that names it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::directive::line_number;
use crate::manifest::Manifest;
use crate::verify::{LISTINGS_REL, chapter_markdown, listing_references, sidecar_tags};

/// What `remove` deleted, as book-relative paths with forward slashes.
#[derive(Debug, Default)]
pub struct RemoveReport {
    pub deleted: Vec<String>,
}

/// Remove the listing tagged `tag` from the book at `book_root`. Refuses,
/// changing nothing, while any chapter still uses the listing or a file it
/// would delete: the next build would fail on the dangling reference
/// instead.
pub fn remove(book_root: &Path, tag: &str) -> Result<RemoveReport> {
    let mut manifest = Manifest::load(book_root)?;
    let Some(listing) = manifest.find(tag) else {
        bail!("no frozen listing is tagged `{tag}`");
    };
    let frozen = listing.frozen.clone();
    if !inside_listings_dir(&frozen) {
        bail!(
            "`{tag}` records its frozen file as `{frozen}`, outside {LISTINGS_REL}/, \
             so remove will not delete it; fix the record in listings.toml"
        );
    }
    let sidecars = sidecars_naming(book_root, tag)?;

    let refs: Vec<_> = listing_references(book_root)
        .into_iter()
        .filter(|r| r.tag == tag)
        .collect();
    let seen: BTreeSet<&str> = refs.iter().map(|r| r.location.as_str()).collect();
    let doomed: Vec<PathBuf> = std::iter::once(&frozen)
        .chain(&sidecars)
        .map(|rel| normalize(&book_root.join(rel)))
        .collect();
    // The listing scan already names a canonical include; the file scan
    // adds only the uses it alone can see.
    let mut uses: BTreeSet<String> = refs.iter().map(|r| r.to_string()).collect();
    uses.extend(
        file_links(book_root)
            .into_iter()
            .filter(|link| doomed.contains(&link.target))
            .filter(|link| !seen.contains(link.location.as_str()))
            .map(|link| format!("{}: {}", link.location, link.directive)),
    );
    if !uses.is_empty() {
        bail!(
            "`{tag}` is still used, so nothing was removed. Take these out first:\n{}",
            uses.into_iter()
                .map(|u| format!("  {u}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // Sidecars first: a failure there leaves the record in place, so
    // `remove` can be run again. Then the record, then the frozen file: a
    // failure on that last one leaves an orphan, which verify only warns
    // about.
    let mut report = RemoveReport::default();
    for rel in sidecars {
        delete(book_root, rel, &mut report)?;
    }
    manifest.remove(tag);
    manifest.save(book_root)?;
    delete(book_root, frozen, &mut report)?;
    Ok(report)
}

fn delete(book_root: &Path, rel: String, report: &mut RemoveReport) -> Result<()> {
    let path = book_root.join(&rel);
    match fs::remove_file(&path) {
        Ok(()) => report.deleted.push(rel),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("deleting {}", path.display())),
    }
    Ok(())
}

/// `rel` names a file directly under `src/listings/`, the only place
/// `freeze` writes, so a hand-edited record cannot point the delete
/// elsewhere.
fn inside_listings_dir(rel: &str) -> bool {
    rel.strip_prefix(LISTINGS_REL)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| {
            !name.is_empty() && !name.contains(['/', '\\']) && name != ".." && name != "."
        })
}

/// Book-relative paths of the sidecars under `src/listings/` that name
/// `tag`: its own `<tag>.callouts.toml`, and any diff sidecar
/// `<old>..<new>.callouts.toml` with `tag` as either side.
pub(crate) fn sidecars_naming(book_root: &Path, tag: &str) -> Result<Vec<String>> {
    let dir = book_root.join(LISTINGS_REL);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut found = Vec::new();
    for entry in entries {
        let name = entry
            .with_context(|| format!("reading {}", dir.display()))?
            .file_name();
        let Some(name) = name.to_str() else { continue };
        if sidecar_tags(name).is_some_and(|tags| tags.contains(&tag)) {
            found.push(format!("{LISTINGS_REL}/{name}"));
        }
    }
    found.sort();
    Ok(found)
}

/// A file mdBook reads into a chapter, and where.
pub(crate) struct FileLink {
    /// Book-relative chapter path and 1-based line, `src/ch01.md:5`.
    pub(crate) location: String,
    pub(crate) directive: String,
    pub(crate) target: PathBuf,
    /// Byte offset of the directive in the chapter.
    pub(crate) start: usize,
    /// The file argument as written, and its byte offset in the chapter.
    pub(crate) spec: String,
    pub(crate) spec_start: usize,
    /// Whether mdBook reads a line range or anchor after the argument's
    /// first `:`. `playground` takes the whole argument as the path.
    pub(crate) ranged: bool,
}

/// mdBook's own file directives, wherever it expands them: inline code and
/// HTML comments included, a backslash-escaped one not.
static FILE_DIRECTIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\\)?\{\{\s*#(include|rustdoc_include|playground)\s+([^}]+?)\s*\}\}")
        .expect("valid regex")
});

/// Every file a chapter pulls in through mdBook's file directives,
/// resolved from the chapter's own directory the way mdBook resolves it.
/// This catches what the listing scan does not: a path that reaches
/// `src/listings/` another way (`../listings/…`), and `rustdoc_include` and
/// `playground`.
fn file_links(book_root: &Path) -> Vec<FileLink> {
    chapter_markdown(book_root)
        .into_iter()
        .flat_map(|(rel, content)| chapter_file_links(book_root, &rel, &content))
        .collect()
}

/// [`file_links`] for one chapter, `rel` being its book-relative path.
pub(crate) fn chapter_file_links(book_root: &Path, rel: &str, content: &str) -> Vec<FileLink> {
    let chapter_dir = book_root
        .join(rel)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| book_root.to_path_buf());
    let mut links = Vec::new();
    for caps in FILE_DIRECTIVE.captures_iter(content) {
        if caps.get(1).is_some() {
            continue;
        }
        let whole = caps.get(0).expect("whole match");
        let args = caps.get(3).expect("directive arguments");
        let spec = args.as_str().split_whitespace().next().unwrap_or("");
        let ranged = &caps[2] != "playground";
        let path = match spec.split_once(':') {
            Some((path, _)) if ranged => path,
            _ => spec,
        };
        links.push(FileLink {
            location: format!("{rel}:{}", line_number(content, whole.start())),
            directive: whole.as_str().to_string(),
            target: normalize(&chapter_dir.join(path)),
            start: whole.start(),
            spec: spec.to_string(),
            spec_start: args.start(),
            ranged,
        });
    }
    links
}

/// `path` with `.` and `..` folded away, without touching the filesystem,
/// so a link to a file and the file itself compare equal.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playground_argument_is_all_path_and_an_include_argument_ends_at_its_colon() {
        let root = Path::new("book");
        let links = chapter_file_links(
            root,
            "src/ch.md",
            "{{#playground listings/x.rs:2}}\n{{#include listings/x.rs:2}}\n",
        );
        let seen: Vec<(PathBuf, bool)> = links.into_iter().map(|l| (l.target, l.ranged)).collect();
        assert_eq!(
            seen,
            [
                (root.join("src/listings/x.rs:2"), false),
                (root.join("src/listings/x.rs"), true),
            ]
        );
    }

    #[test]
    fn a_frozen_path_under_the_listings_dir_is_inside() {
        assert!(inside_listings_dir("src/listings/menu-v1.yaml"));
    }

    #[test]
    fn a_frozen_path_that_climbs_out_is_not_inside() {
        for rel in [
            "src/listings/../../Cargo.toml",
            "src/listings/..",
            "src/listings/sub/menu-v1.yaml",
            "src/listings/",
            "src/listingsX/menu-v1.yaml",
            "/etc/passwd",
            "../Cargo.toml",
        ] {
            assert!(!inside_listings_dir(rel), "{rel} was accepted");
        }
    }

    #[test]
    fn normalize_folds_parent_and_current_components() {
        assert_eq!(
            normalize(Path::new("/b/src/part1/./../listings/x.rs")),
            PathBuf::from("/b/src/listings/x.rs")
        );
    }
}
