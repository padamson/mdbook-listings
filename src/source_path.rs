//! What the provenance line prints for a listing's source.
//!
//! The manifest records `source` relative to the book root, because that is
//! what `freeze` was handed and what tag derivation keys on. A reader of the
//! published page knows the repository, not the book root, so `../src/foo.rs`
//! reads as a path that leaves the project. The rewrite happens here, at
//! display time, and the manifest keeps the form its other readers depend on.

use std::path::{Path, PathBuf};

/// A listing's source path and the URL recorded for it. Going into
/// [`SourcePaths::display_pair`] the path is book-root-relative, as the
/// manifest records it; coming out it is what the provenance line shows,
/// linked to `url` when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShownSource {
    pub(crate) path: String,
    pub(crate) url: Option<String>,
}

pub(crate) enum SourcePaths {
    /// No repository was found above the book, so there is nothing better to
    /// say than what the manifest recorded.
    Verbatim,
    InRepo {
        book_root: PathBuf,
        repo_root: PathBuf,
    },
}

impl SourcePaths {
    pub(crate) fn for_book(book_root: &Path) -> Self {
        Self::find(book_root, |dir| dir.join(".git").exists())
    }

    /// `is_repo_root` is a seam: the real check looks for `.git` (a directory
    /// in a clone, a file in a worktree or submodule), and a test cannot
    /// promise that no directory above its tempdir has one.
    fn find(book_root: &Path, is_repo_root: impl Fn(&Path) -> bool) -> Self {
        // Canonical, so the book root and every source joined onto it agree
        // with the repo root found by walking its ancestors.
        let book_root = book_root
            .canonicalize()
            .unwrap_or_else(|_| book_root.to_path_buf());
        match book_root.ancestors().find(|dir| is_repo_root(dir)) {
            Some(repo_root) => Self::InRepo {
                repo_root: repo_root.to_path_buf(),
                book_root,
            },
            None => Self::Verbatim,
        }
    }

    /// The sources to show for a diff's two operands, each as recorded (the
    /// book-root-relative path and its URL). When either operand's source is
    /// hidden,
    /// both are: the provenance line collapses or joins the pair, and a lone
    /// surviving path would read as the file both operands were frozen from.
    pub(crate) fn display_pair(
        &self,
        left: Option<ShownSource>,
        right: Option<ShownSource>,
    ) -> (Option<ShownSource>, Option<ShownSource>) {
        // `Some(None)` is a source the display withholds, as distinct from a
        // tag with no source at all.
        let show = |s: ShownSource| self.display(&s.path, s.url.as_deref());
        match (left.map(show), right.map(show)) {
            (Some(None), _) | (_, Some(None)) => (None, None),
            (l, r) => (l.flatten(), r.flatten()),
        }
    }

    /// What to show for a book-root-relative `source` recorded with `url`.
    /// Outside the repository the path names a directory on the author's
    /// machine, which a reader can neither open nor follow: with a URL the
    /// line shows the file name linked to it, and without one it shows the
    /// tag alone (`None`).
    pub(crate) fn display(&self, source: &str, url: Option<&str>) -> Option<ShownSource> {
        let path = match self.repo_path(source) {
            Some(path) => path,
            None => {
                url?;
                Path::new(source)
                    .file_name()?
                    .to_string_lossy()
                    .into_owned()
            }
        };
        Some(ShownSource {
            path,
            url: url.map(str::to_string),
        })
    }

    /// `source` relative to the repository root, or as recorded when there is
    /// no repository. `None` when it lies outside the repository.
    fn repo_path(&self, source: &str) -> Option<String> {
        match self {
            Self::Verbatim => Some(source.to_string()),
            Self::InRepo {
                book_root,
                repo_root,
            } => {
                let resolved = lexical_normalize(&book_root.join(source));
                let rel = resolved.strip_prefix(repo_root).ok()?;
                let parts: Vec<_> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect();
                Some(parts.join("/"))
            }
        }
    }
}

/// `path` expressed relative to `base`, with `/` separators for display.
/// Both are normalised lexically rather than through the filesystem: the
/// result is what an author reading the path would compute, it does not
/// depend on what exists on disk, and it stays stable across machines.
pub(crate) fn relative_to(path: &Path, base: &Path) -> String {
    let path = lexical_normalize(path);
    let base = lexical_normalize(base);
    let shared = path
        .components()
        .zip(base.components())
        .take_while(|(p, b)| p == b)
        .count();
    let mut parts = vec!["..".to_string(); base.components().count() - shared];
    parts.extend(
        path.components()
            .skip(shared)
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

/// Collapse `.` and `..` textually. `..` past the start is dropped rather
/// than escaping above the root, so a manifest source with more `..` than
/// the book root has ancestors resolves somewhere under `/`, outside any
/// repository, and shows no path.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_to_walks_up_out_of_the_base() {
        assert_eq!(
            relative_to(Path::new("/repo/src/diff.rs"), Path::new("/repo/book")),
            "../src/diff.rs"
        );
        assert_eq!(
            relative_to(Path::new("/repo/book/src/x.rs"), Path::new("/repo/book")),
            "src/x.rs"
        );
        assert_eq!(
            relative_to(Path::new("/repo/book"), Path::new("/repo/book")),
            "."
        );
    }

    fn in_repo(book_root: &str, repo_root: &str) -> SourcePaths {
        SourcePaths::InRepo {
            book_root: PathBuf::from(book_root),
            repo_root: PathBuf::from(repo_root),
        }
    }

    fn cqa() -> SourcePaths {
        in_repo("/work/cqa/book", "/work/cqa")
    }

    /// The path shown for a source recorded without a URL.
    fn path(paths: &SourcePaths, source: &str) -> Option<String> {
        paths.display(source, None).map(|s| s.path)
    }

    fn shown(path: &str, url: Option<&str>) -> ShownSource {
        ShownSource {
            path: path.to_string(),
            url: url.map(str::to_string),
        }
    }

    const URL: &str = "https://example.com/blob/abc/data/wine.yaml";

    #[test]
    fn a_source_beside_the_book_shows_its_repo_path() {
        assert_eq!(
            path(&cqa(), "../schema/cqa.yaml").as_deref(),
            Some("schema/cqa.yaml")
        );
    }

    #[test]
    fn a_source_inside_the_book_keeps_the_book_directory() {
        assert_eq!(
            path(&cqa(), "src/listings/a.rs").as_deref(),
            Some("book/src/listings/a.rs")
        );
    }

    #[test]
    fn a_source_in_a_sibling_checkout_shows_no_path() {
        assert_eq!(
            path(&cqa(), "../../ontology-authoring-template/data/wine.yaml"),
            None
        );
    }

    #[test]
    fn a_sibling_whose_name_extends_the_repos_is_still_outside_it() {
        // A string prefix test would accept `/work/cqa-schema/...` as being
        // inside `/work/cqa`; the path has to match by component.
        assert_eq!(path(&cqa(), "../../cqa-schema/x.yaml"), None);
    }

    #[test]
    fn an_absolute_source_inside_the_repo_shows_its_repo_path() {
        // freeze falls back to recording an absolute path when it cannot
        // relativize one.
        assert_eq!(
            path(&cqa(), "/work/cqa/schema/cqa.yaml").as_deref(),
            Some("schema/cqa.yaml")
        );
    }

    #[test]
    fn a_book_at_the_repo_root_shows_the_source_unchanged() {
        let paths = in_repo("/work/cqa", "/work/cqa");
        assert_eq!(
            path(&paths, "schema/cqa.yaml").as_deref(),
            Some("schema/cqa.yaml")
        );
    }

    #[test]
    fn without_a_repo_the_source_shows_as_recorded() {
        assert_eq!(
            path(&SourcePaths::Verbatim, "../../elsewhere/x.yaml").as_deref(),
            Some("../../elsewhere/x.yaml")
        );
    }

    #[test]
    fn a_url_links_the_repo_path() {
        assert_eq!(
            cqa().display("../schema/cqa.yaml", Some(URL)),
            Some(shown("schema/cqa.yaml", Some(URL)))
        );
    }

    #[test]
    fn a_sibling_source_with_a_url_shows_its_file_name_linked() {
        assert_eq!(
            cqa().display(
                "../../ontology-authoring-template/data/wine.yaml",
                Some(URL)
            ),
            Some(shown("wine.yaml", Some(URL)))
        );
    }

    #[test]
    fn find_takes_the_nearest_ancestor_that_is_a_repo_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().canonicalize().unwrap();
        let book = root.join("outer/inner/book");
        std::fs::create_dir_all(&book).unwrap();
        let inner = root.join("outer/inner");
        let outer = root.join("outer");
        let paths = SourcePaths::find(&book, |d| d == inner || d == outer);
        match paths {
            SourcePaths::InRepo {
                book_root,
                repo_root,
            } => {
                assert_eq!(repo_root, inner);
                assert_eq!(book_root, book);
            }
            SourcePaths::Verbatim => panic!("expected InRepo, got Verbatim"),
        }
    }

    #[test]
    fn find_falls_back_to_verbatim_when_no_ancestor_is_a_repo_root() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(matches!(
            SourcePaths::find(tmp.path(), |_| false),
            SourcePaths::Verbatim
        ));
    }

    #[test]
    fn a_git_file_marks_a_repo_root_as_a_git_directory_does() {
        // A worktree or submodule has `.git` as a file pointing at the real
        // git dir.
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().canonicalize().unwrap();
        std::fs::write(root.join(".git"), "gitdir: /elsewhere\n").unwrap();
        std::fs::create_dir(root.join("book")).unwrap();
        let paths = SourcePaths::for_book(&root.join("book"));
        assert_eq!(
            path(&paths, "../schema/cqa.yaml").as_deref(),
            Some("schema/cqa.yaml")
        );
    }

    #[test]
    fn a_diff_with_one_operand_outside_the_repo_shows_neither_path() {
        let paths = cqa();
        let outside = || Some(shown("../../other/x.yaml", None));
        let inside = || Some(shown("../schema/x.yaml", None));
        assert_eq!(paths.display_pair(outside(), inside()), (None, None));
        assert_eq!(paths.display_pair(inside(), outside()), (None, None));
    }

    #[test]
    fn a_diff_operand_outside_the_repo_with_a_url_does_not_hide_the_other() {
        assert_eq!(
            cqa().display_pair(
                Some(shown("../../other/x.yaml", Some(URL))),
                Some(shown("../schema/x.yaml", None))
            ),
            (
                Some(shown("x.yaml", Some(URL))),
                Some(shown("schema/x.yaml", None))
            )
        );
    }

    #[test]
    fn a_diff_inside_the_repo_shows_both_paths() {
        assert_eq!(
            cqa().display_pair(
                Some(shown("../schema/a.yaml", None)),
                Some(shown("../schema/b.yaml", None))
            ),
            (
                Some(shown("schema/a.yaml", None)),
                Some(shown("schema/b.yaml", None))
            )
        );
    }

    #[test]
    fn a_diff_operand_with_no_source_does_not_hide_the_other() {
        // A tag the manifest lacks has nothing to show, which is not the same
        // as a path withheld; the other side keeps its path, as before.
        assert_eq!(
            cqa().display_pair(None, Some(shown("../schema/b.yaml", None))),
            (None, Some(shown("schema/b.yaml", None)))
        );
    }
}
