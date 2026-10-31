use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Callout;
use super::parse::is_valid_label;

/// Sidecar TOML file shape. Deserialised from `<tag>.callouts.toml` for
/// listings that can't carry inline markers (generated code, no-comment
/// languages). `[[callout]]` entries become [`Callout`]s with the
/// supplied line number and label.
#[derive(Debug, Deserialize)]
struct SidecarFile {
    #[serde(default, rename = "callout")]
    callouts: Vec<SidecarEntry>,
}

#[derive(Debug, Deserialize)]
struct SidecarEntry {
    line: usize,
    label: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    side: Option<String>,
}

/// Which file of a diff a diff-sidecar entry's `line` counts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffSide {
    Old,
    New,
}

impl DiffSide {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DiffSide::Old => "old",
            DiffSide::New => "new",
        }
    }
}

/// One entry of a diff sidecar: its callout, with `line` a line of the
/// `side` file.
#[derive(Debug, Clone)]
pub(crate) struct DiffSidecarEntry {
    pub(crate) side: DiffSide,
    pub(crate) callout: Callout,
}

/// In-memory map of `tag -> sidecar callouts`. Built once per chapter
/// pass from `<src>/listings/*.callouts.toml`; passed into
/// [`splice_chapter`] so the splicer can merge sidecar entries with
/// inline markers per matching `<div data-listing-tag>` block.
///
/// A file named `<old>..<new>.callouts.toml` is a diff sidecar instead: it
/// annotates the `{{#diff <old> <new>}}` blocks, and nothing else.
#[derive(Debug, Default)]
pub struct SidecarCallouts {
    /// Tag → (sidecar-file path, parsed callouts). The path is retained
    /// for diagnostic messages on label collisions.
    by_tag: HashMap<String, (PathBuf, Vec<Callout>)>,
    /// (old tag, new tag) → (sidecar-file path, parsed entries).
    by_diff: HashMap<(String, String), (PathBuf, Vec<DiffSidecarEntry>)>,
}

impl SidecarCallouts {
    /// Empty sidecar set. The default state when a book has no
    /// `<tag>.callouts.toml` files; lets all callers use the same
    /// splicer signature regardless of whether sidecars exist.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Scan `listings_dir` for `*.callouts.toml` files. Missing directory
    /// returns an empty set (not an error) so a book that uses no
    /// sidecars Just Works. Each file's tag is the basename minus
    /// `.callouts.toml` — e.g. `compose-v1.callouts.toml` maps to tag
    /// `compose-v1`.
    pub fn load(listings_dir: &Path) -> Result<Self, SidecarLoadError> {
        let mut by_tag = HashMap::new();
        let mut by_diff = HashMap::new();
        let entries = match std::fs::read_dir(listings_dir) {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::empty()),
            Err(err) => {
                return Err(SidecarLoadError::ReadDir {
                    dir: listings_dir.to_path_buf(),
                    source: err,
                });
            }
        };
        for entry in entries {
            let entry = entry.map_err(|source| SidecarLoadError::ReadDir {
                dir: listings_dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(tag) = name.strip_suffix(".callouts.toml") else {
                continue;
            };
            let text =
                std::fs::read_to_string(&path).map_err(|source| SidecarLoadError::ReadFile {
                    path: path.clone(),
                    source,
                })?;
            let parsed: SidecarFile =
                toml::from_str(&text).map_err(|source| SidecarLoadError::Parse {
                    path: path.clone(),
                    source,
                })?;
            // Validate labels at load time so a malformed sidecar
            // fails the build during scan, not during a chapter pass.
            // Also detect same-source duplicate labels here: a single
            // sidecar TOML with two `[[callout]]` entries sharing a
            // label would silently overwrite one in any map-keyed
            // downstream, masking the bug.
            let mut seen: HashSet<&str> = HashSet::new();
            for entry in &parsed.callouts {
                if !is_valid_label(&entry.label) {
                    return Err(SidecarLoadError::InvalidLabel {
                        path: path.clone(),
                        label: entry.label.clone(),
                    });
                }
                if !seen.insert(entry.label.as_str()) {
                    return Err(SidecarLoadError::DuplicateLabel {
                        path: path.clone(),
                        label: entry.label.clone(),
                    });
                }
            }
            let pair = tag.split_once("..");
            let mut callouts = Vec::new();
            let mut diff_entries = Vec::new();
            for e in parsed.callouts {
                let callout = Callout {
                    line: e.line,
                    label: e.label,
                    body: e
                        .body
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    options: HashMap::new(),
                };
                let side = match (pair, e.side.as_deref()) {
                    (None, None) => {
                        callouts.push(callout);
                        continue;
                    }
                    (None, Some(_)) => {
                        return Err(SidecarLoadError::SideOnListingSidecar {
                            path: path.clone(),
                            label: callout.label,
                        });
                    }
                    (Some(_), None | Some("new")) => DiffSide::New,
                    (Some(_), Some("old")) => DiffSide::Old,
                    (Some(_), Some(other)) => {
                        return Err(SidecarLoadError::InvalidSide {
                            path: path.clone(),
                            label: callout.label,
                            side: other.to_string(),
                        });
                    }
                };
                diff_entries.push(DiffSidecarEntry { side, callout });
            }
            match pair {
                Some((old, new)) => {
                    by_diff.insert((old.to_string(), new.to_string()), (path, diff_entries));
                }
                None => {
                    by_tag.insert(tag.to_string(), (path, callouts));
                }
            }
        }
        Ok(Self { by_tag, by_diff })
    }

    /// Callouts attached to the listing with this tag, or `&[]` when
    /// no sidecar exists for the tag.
    pub fn for_tag(&self, tag: &str) -> &[Callout] {
        self.by_tag
            .get(tag)
            .map(|(_, c)| c.as_slice())
            .unwrap_or(&[])
    }

    /// Sidecar file path for the tag, used in collision diagnostics.
    pub(super) fn path_for_tag(&self, tag: &str) -> Option<&Path> {
        self.by_tag.get(tag).map(|(p, _)| p.as_path())
    }

    /// Entries of the sidecar for the diff of `old` against `new`, or `&[]`
    /// when there is none.
    pub(crate) fn for_diff(&self, old: &str, new: &str) -> &[DiffSidecarEntry] {
        self.by_diff
            .get(&(old.to_string(), new.to_string()))
            .map(|(_, e)| e.as_slice())
            .unwrap_or(&[])
    }

    /// Sidecar file path for the diff of `old` against `new`.
    pub(super) fn path_for_diff(&self, old: &str, new: &str) -> Option<&Path> {
        self.by_diff
            .get(&(old.to_string(), new.to_string()))
            .map(|(p, _)| p.as_path())
    }
}

/// Errors raised by [`SidecarCallouts::load`]. Surface at load time so
/// the build fails on a malformed sidecar before any chapter is
/// processed, rather than partway through a render.
#[derive(Debug)]
pub enum SidecarLoadError {
    ReadDir {
        dir: PathBuf,
        source: std::io::Error,
    },
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    InvalidLabel {
        path: PathBuf,
        label: String,
    },
    DuplicateLabel {
        path: PathBuf,
        label: String,
    },
    /// A diff sidecar entry whose `side` is neither `old` nor `new`.
    InvalidSide {
        path: PathBuf,
        label: String,
        side: String,
    },
    /// A `side` on an entry of a listing's sidecar, which has one file.
    SideOnListingSidecar {
        path: PathBuf,
        label: String,
    },
}

impl std::fmt::Display for SidecarLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SidecarLoadError::ReadDir { dir, source } => {
                write!(f, "reading listings directory {}: {source}", dir.display())
            }
            SidecarLoadError::ReadFile { path, source } => {
                write!(f, "reading sidecar {}: {source}", path.display())
            }
            SidecarLoadError::Parse { path, source } => {
                write!(f, "parsing sidecar {}: {source}", path.display())
            }
            SidecarLoadError::InvalidLabel { path, label } => write!(
                f,
                "sidecar {} has invalid label `{label}` (must be alphanumeric, hyphen, or underscore)",
                path.display(),
            ),
            SidecarLoadError::DuplicateLabel { path, label } => write!(
                f,
                "sidecar {} has duplicate label `{label}` — each `[[callout]]` entry must have a unique label",
                path.display(),
            ),
            SidecarLoadError::InvalidSide { path, label, side } => write!(
                f,
                "sidecar {} entry `{label}` has side `{side}` (must be `old` or `new`)",
                path.display(),
            ),
            SidecarLoadError::SideOnListingSidecar { path, label } => write!(
                f,
                "sidecar {} entry `{label}` has a `side`, which only a diff sidecar \
                 (`<old>..<new>.callouts.toml`) takes",
                path.display(),
            ),
        }
    }
}

impl std::error::Error for SidecarLoadError {}

#[cfg(test)]
pub(super) fn write_sidecar(
    dir: &std::path::Path,
    tag: &str,
    contents: &str,
) -> std::path::PathBuf {
    let path = dir.join(format!("{tag}.callouts.toml"));
    std::fs::write(&path, contents).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_load_returns_empty_when_dir_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let s = SidecarCallouts::load(&missing).unwrap();
        assert!(s.for_tag("anything").is_empty());
    }

    /// `load` distinguishes "no listings dir" (legitimately empty) from
    /// "io error reading what should be a dir" — only NotFound becomes
    /// the empty set; anything else surfaces as `ReadDir`.
    #[test]
    fn sidecar_load_surfaces_non_notfound_io_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let not_a_dir = tmp.path().join("regular-file.txt");
        std::fs::write(&not_a_dir, "I am a file, not a directory").unwrap();
        let err = SidecarCallouts::load(&not_a_dir).unwrap_err();
        match err {
            SidecarLoadError::ReadDir { dir, .. } => {
                assert_eq!(dir, not_a_dir);
            }
            other => panic!("expected ReadDir error, got {other:?}"),
        }
    }

    #[test]
    fn sidecar_load_parses_well_formed_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "compose-v1",
            r#"
[[callout]]
line = 5
label = "service-list"
body = "Each top-level key is one service."

[[callout]]
line = 8
label = "version-pin"
"#,
        );
        let s = SidecarCallouts::load(tmp.path()).unwrap();
        let cs = s.for_tag("compose-v1");
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].line, 5);
        assert_eq!(cs[0].label, "service-list");
        assert_eq!(
            cs[0].body.as_deref(),
            Some("Each top-level key is one service.")
        );
        assert_eq!(cs[1].line, 8);
        assert_eq!(cs[1].label, "version-pin");
        assert!(cs[1].body.is_none());
    }

    #[test]
    fn sidecar_load_rejects_invalid_label() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "bad",
            r#"
[[callout]]
line = 1
label = "has spaces"
"#,
        );
        let err = SidecarCallouts::load(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("has spaces"), "got: {msg}");
        assert!(msg.contains("invalid label"), "got: {msg}");
    }

    #[test]
    fn sidecar_load_ignores_files_not_matching_extension() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "not a sidecar").unwrap();
        std::fs::write(tmp.path().join("compose-v1.rs"), "// some code").unwrap();
        let s = SidecarCallouts::load(tmp.path()).unwrap();
        assert!(s.for_tag("compose-v1").is_empty());
        assert!(s.for_tag("README").is_empty());
    }

    #[test]
    fn sidecar_load_rejects_same_source_duplicate_label() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "dup",
            r#"
[[callout]]
line = 1
label = "twice"

[[callout]]
line = 2
label = "twice"
"#,
        );
        let err = SidecarCallouts::load(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate label"), "got: {msg}");
        assert!(msg.contains("twice"), "got: {msg}");
    }

    #[test]
    fn a_diff_sidecar_loads_under_its_operand_pair_with_new_as_the_default_side() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "parse-0.13.4..parse-0.13.5",
            r#"
[[callout]]
line = 12
label = "added"
body = "New check."

[[callout]]
side = "new"
line = 14
label = "explicit"

[[callout]]
side = "old"
line = 9
label = "removed"
"#,
        );
        let s = SidecarCallouts::load(tmp.path()).unwrap();
        let got: Vec<(DiffSide, usize, &str)> = s
            .for_diff("parse-0.13.4", "parse-0.13.5")
            .iter()
            .map(|e| (e.side, e.callout.line, e.callout.label.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (DiffSide::New, 12, "added"),
                (DiffSide::New, 14, "explicit"),
                (DiffSide::Old, 9, "removed")
            ]
        );
        assert!(
            s.for_tag("parse-0.13.4..parse-0.13.5").is_empty(),
            "a diff sidecar must not also attach to a listing"
        );
    }

    #[test]
    fn a_sidecar_for_a_dotted_tag_loads_under_the_whole_tag() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "parse-0.13.4",
            "[[callout]]\nline = 1\nlabel = \"x\"\n",
        );
        let s = SidecarCallouts::load(tmp.path()).unwrap();
        assert_eq!(s.for_tag("parse-0.13.4").len(), 1);
    }

    #[test]
    fn a_diff_sidecar_rejects_a_side_other_than_old_or_new() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "a-v1..a-v2",
            "[[callout]]\nside = \"left\"\nline = 1\nlabel = \"x\"\n",
        );
        let err = SidecarCallouts::load(tmp.path()).unwrap_err();
        let msg = err.to_string();
        match err {
            SidecarLoadError::InvalidSide { label, side, .. } => {
                assert_eq!((label.as_str(), side.as_str()), ("x", "left"));
            }
            other => panic!("expected InvalidSide, got {other:?}"),
        }
        for token in ["a-v1..a-v2.callouts.toml", "`x`", "`left`"] {
            assert!(msg.contains(token), "missing {token}; got: {msg}");
        }
    }

    #[test]
    fn a_listing_sidecar_rejects_a_side() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "a-v1",
            "[[callout]]\nside = \"old\"\nline = 1\nlabel = \"x\"\n",
        );
        let err = SidecarCallouts::load(tmp.path()).unwrap_err();
        let msg = err.to_string();
        match err {
            SidecarLoadError::SideOnListingSidecar { label, .. } => assert_eq!(label, "x"),
            other => panic!("expected SideOnListingSidecar, got {other:?}"),
        }
        for token in ["a-v1.callouts.toml", "`x`"] {
            assert!(msg.contains(token), "missing {token}; got: {msg}");
        }
    }
}
