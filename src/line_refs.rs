//! Line numbers that point into a frozen listing, and where each one lands
//! when `freeze --force` replaces the listing's bytes.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result};
use similar::{Algorithm, DiffTag, capture_diff_slices};

use crate::directive::line_number;
use crate::include::{IncludeDirective, parse_listing_includes};
use crate::remove::{chapter_file_links, normalize};
use crate::verify::{chapter_markdown, chapter_src_dir};

/// The lines a reference names, as mdBook reads the suffix after a path's
/// first `:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineSpec {
    /// `A:B`, `A:`, or `:B`; an empty end runs to the file's edge.
    Range {
        start: Option<usize>,
        end: Option<usize>,
    },
    /// `N`, the one line.
    Line(usize),
}

impl LineSpec {
    /// The suffix after a path's first `:`, read as mdBook's `links` reads
    /// it, with the length of the part that names lines. `None` for an
    /// anchor name or a suffix that names no line. mdBook ignores a third
    /// `:` part, reads an end it cannot parse as open, and reads `0` as
    /// line 1.
    pub(crate) fn parse(suffix: &str) -> Option<(Self, usize)> {
        let mut parts = suffix.splitn(3, ':');
        let first = parts.next()?;
        let start = match first {
            "" => None,
            n => Some(n.parse::<usize>().ok()?.max(1)),
        };
        let Some(second) = parts.next() else {
            return start.map(|n| (Self::Line(n), first.len()));
        };
        let end = second.parse::<usize>().ok();
        let names_a_line = end != Some(0) && (start.is_some() || end.is_some());
        names_a_line.then_some((Self::Range { start, end }, first.len() + 1 + second.len()))
    }
}

impl fmt::Display for LineSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = |v: Option<usize>| v.map(|n| n.to_string()).unwrap_or_default();
        match self {
            Self::Range { start, end } => write!(f, "{}:{}", n(*start), n(*end)),
            Self::Line(line) => write!(f, "{line}"),
        }
    }
}

/// Where a reference lands in the new bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carry {
    /// The same numbers name the same lines.
    Same,
    /// The lines survive under new numbers.
    Moved(LineSpec),
    /// An end line changed, or a line was added just outside an end, so the
    /// diff cannot say where the author meant the range to stop. The spec
    /// is the conservative guess, and the note says what changed.
    Boundary(LineSpec, String),
    /// None of the lines survive.
    Gone,
}

/// Where each line of the old bytes sits in the new ones.
pub(crate) struct LineMap {
    /// The 1-based new line of each old line, `None` when it was changed
    /// or deleted.
    new_of: Vec<Option<usize>>,
    /// `added_before[i]`: new lines were inserted just before old line `i`
    /// (0-based), `added_before[len]` at the end of the file.
    added_before: Vec<bool>,
}

impl LineMap {
    pub(crate) fn between(old: &str, new: &str) -> Self {
        // `str::lines` on both sides: it is how mdBook and the include
        // pass count lines, and it compares them without their
        // terminators, so a final newline or a CRLF is not an edit.
        let old: Vec<&str> = old.lines().collect();
        let new: Vec<&str> = new.lines().collect();
        let mut new_of = vec![None; old.len()];
        let mut added_before = vec![false; old.len() + 1];
        for op in capture_diff_slices(Algorithm::Myers, &old, &new) {
            let (tag, old_lines, new_lines) = op.as_tag_tuple();
            match tag {
                DiffTag::Equal => {
                    for (o, n) in old_lines.zip(new_lines) {
                        new_of[o] = Some(n + 1);
                    }
                }
                // A replaced line is changed, not added beside its
                // neighbours; only a pure insertion is.
                DiffTag::Insert => added_before[old_lines.start] = true,
                DiffTag::Delete | DiffTag::Replace => {}
            }
        }
        Self {
            new_of,
            added_before,
        }
    }

    pub(crate) fn carry(&self, spec: LineSpec) -> Carry {
        let len = self.new_of.len();
        let (start, end) = match spec {
            LineSpec::Line(n) if n > len => return Carry::Same,
            LineSpec::Line(n) => {
                return match self.new_of[n - 1] {
                    Some(m) if m == n => Carry::Same,
                    Some(m) => Carry::Moved(LineSpec::Line(m)),
                    None => Carry::Gone,
                };
            }
            LineSpec::Range { start, end } => (start, end),
        };
        let lo = start.unwrap_or(1);
        let hi = end.unwrap_or(len).min(len);
        if lo > hi {
            return Carry::Same;
        }
        let surviving = || (lo..=hi).filter_map(|i| self.new_of[i - 1]);
        let (Some(first), Some(last)) = (surviving().next(), surviving().next_back()) else {
            return Carry::Gone;
        };
        let mut notes = Vec::new();
        let new_start = start.map(|a| match self.new_of[a - 1] {
            Some(m) => {
                if self.added_before[a - 1] {
                    notes.push(format!("a line was added before line {a}"));
                }
                m
            }
            None => {
                notes.push(format!("line {a} changed"));
                first
            }
        });
        // An end past the old file's last line never named a line; the
        // build clamps it, so it is left as written.
        let new_end = end.map(|b| match self.new_of.get(b - 1) {
            None => b,
            Some(Some(m)) => {
                if self.added_before[b] {
                    notes.push(format!("a line was added after line {b}"));
                }
                *m
            }
            Some(None) => {
                notes.push(format!("line {b} changed"));
                last
            }
        });
        let carried = LineSpec::Range {
            start: new_start,
            end: new_end,
        };
        if !notes.is_empty() {
            Carry::Boundary(carried, notes.join("; "))
        } else if carried == spec {
            Carry::Same
        } else {
            Carry::Moved(carried)
        }
    }
}

/// One line reference into the replaced listing that does not stay put.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarriedRef {
    /// Book-relative chapter path and 1-based line, `src/ch01.md:5`.
    pub location: String,
    /// The path argument as the chapter wrote it, `listings/x.rs:2:4`.
    pub written: String,
    pub carry: Carry,
}

impl fmt::Display for CarriedRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            location,
            written,
            carry,
        } = self;
        match carry {
            Carry::Same => write!(f, "{location}  {written}"),
            Carry::Moved(spec) => write!(f, "{location}  {written} → {spec}"),
            Carry::Boundary(spec, note) => {
                write!(
                    f,
                    "{location}  {written} → {spec} ({note}; check the range)"
                )
            }
            Carry::Gone => write!(
                f,
                "{location}  {written}: no line of it is left; fix it by hand"
            ),
        }
    }
}

/// The references in one chapter, with the byte range of each one's line
/// suffix, ready to rewrite.
pub(crate) struct ChapterRefs {
    rel: String,
    content: String,
    refs: Vec<(Range<usize>, CarriedRef)>,
}

/// Every line reference into the frozen file `frozen_rel` (book-relative)
/// that does not stay put when its bytes change from `old` to `new`, by
/// chapter.
pub(crate) fn plan(book_root: &Path, frozen_rel: &str, old: &str, new: &str) -> Vec<ChapterRefs> {
    let target = normalize(&book_root.join(frozen_rel));
    let src_dir = chapter_src_dir(book_root);
    let map = LineMap::between(old, new);
    let mut chapters = Vec::new();
    for (rel, content) in chapter_markdown(book_root) {
        // (byte offset of the line suffix, the file argument as written,
        // location), from both readers of `{{#include}}`.
        let mut found: Vec<(usize, String, String)> = Vec::new();
        // The include pass takes a `listings/` include first and resolves
        // it from the book's `src/`, wherever the chapter sits.
        let mut taken = HashSet::new();
        for d in parse_listing_includes(&content) {
            taken.insert(d.span.start);
            if normalize(&src_dir.join(&d.rel_path)) != target {
                continue;
            }
            if let Some(suffix) = suffix_span(&content, &d) {
                found.push((
                    suffix.start,
                    format!("{}:{}", d.rel_path, &content[suffix.clone()]),
                    format!("{rel}:{}", line_number(&content, d.span.start)),
                ));
            }
        }
        // mdBook's `links` expands what is left, from the chapter's own
        // directory.
        for link in chapter_file_links(book_root, &rel, &content) {
            if taken.contains(&link.start) || !link.ranged || link.target != target {
                continue;
            }
            if let Some((path, _)) = link.spec.split_once(':') {
                found.push((link.spec_start + path.len() + 1, link.spec, link.location));
            }
        }
        found.sort();
        let mut refs = Vec::new();
        for (suffix_start, written, location) in found {
            let suffix = written.split_once(':').map_or("", |(_, suffix)| suffix);
            let Some((spec, len)) = LineSpec::parse(suffix) else {
                continue;
            };
            let carry = map.carry(spec);
            if carry == Carry::Same {
                continue;
            }
            refs.push((
                suffix_start..suffix_start + len,
                CarriedRef {
                    location,
                    written,
                    carry,
                },
            ));
        }
        if !refs.is_empty() {
            chapters.push(ChapterRefs { rel, content, refs });
        }
    }
    chapters.sort_by(|a, b| a.rel.cmp(&b.rel));
    chapters
}

/// Where a `listings/` include wrote the line suffix after its path. The
/// path may follow a `key="…"` attribute, and a quoted value may hold the
/// same text, so quoted values are skipped.
fn suffix_span(content: &str, d: &IncludeDirective<'_>) -> Option<Range<usize>> {
    let text = &content[d.span.clone()];
    let needle = format!("{}:", d.rel_path);
    let mut quoted = 0..0;
    let start = text.match_indices(&needle).map(|(i, _)| i).find(|&i| {
        while quoted.end <= i {
            let Some(open) = text[quoted.end..].find("=\"").map(|o| quoted.end + o + 2) else {
                quoted = text.len()..text.len();
                break;
            };
            let close = text[open..].find('"').map_or(text.len(), |c| open + c);
            quoted = open..close;
        }
        !quoted.contains(&i) && text[..i].ends_with(char::is_whitespace)
    })? + needle.len();
    let len = text[start..].find(|c: char| c.is_whitespace() || c == '}')?;
    Some(d.span.start + start..d.span.start + start + len)
}

/// The references a plan reports, in chapter order.
pub(crate) fn carried(plan: &[ChapterRefs]) -> Vec<CarriedRef> {
    plan.iter()
        .flat_map(|c| c.refs.iter().map(|(_, r)| r.clone()))
        .collect()
}

/// Rewrite each chapter's line suffixes to where they land. A reference
/// with no line left is not touched. If a chapter cannot be written, the
/// ones already rewritten are put back.
pub(crate) fn apply(book_root: &Path, plan: &[ChapterRefs]) -> Result<()> {
    for (done, chapter) in plan.iter().enumerate() {
        let mut content = chapter.content.clone();
        for (span, r) in chapter.refs.iter().rev() {
            if let Carry::Moved(spec) | Carry::Boundary(spec, _) = &r.carry {
                content.replace_range(span.clone(), &spec.to_string());
            }
        }
        let path = book_root.join(&chapter.rel);
        if let Err(e) = fs::write(&path, content) {
            restore(book_root, &plan[..done]);
            return Err(e).with_context(|| format!("writing {}", path.display()));
        }
    }
    Ok(())
}

/// Put the planned chapters back as they were read, after a later step of
/// the freeze failed: ranges carried for bytes that were never frozen
/// would be carried a second time by the retry. Best effort; the freeze
/// is already failing with its own error.
pub(crate) fn restore(book_root: &Path, plan: &[ChapterRefs]) {
    for chapter in plan {
        let _ = fs::write(book_root.join(&chapter.rel), &chapter.content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "a\nb\nc\nd\ne\n";

    fn range(start: Option<usize>, end: Option<usize>) -> LineSpec {
        LineSpec::Range { start, end }
    }

    fn carry(new: &str, spec: LineSpec) -> Carry {
        LineMap::between(OLD, new).carry(spec)
    }

    #[test]
    fn a_suffix_parses_as_mdbook_reads_it() {
        let parsed = |suffix: &str| LineSpec::parse(suffix);
        assert_eq!(parsed("2:4"), Some((range(Some(2), Some(4)), 3)));
        assert_eq!(parsed("12:"), Some((range(Some(12), None), 3)));
        assert_eq!(parsed(":4"), Some((range(None, Some(4)), 2)));
        assert_eq!(parsed("13"), Some((LineSpec::Line(13), 2)));
        // A third part is ignored, an end that is no number is open, and
        // line 0 is line 1.
        assert_eq!(parsed("2:4:9"), Some((range(Some(2), Some(4)), 3)));
        assert_eq!(parsed("2:x"), Some((range(Some(2), None), 3)));
        assert_eq!(parsed("0:3"), Some((range(Some(1), Some(3)), 3)));
        assert_eq!(parsed("0"), Some((LineSpec::Line(1), 1)));
    }

    #[test]
    fn a_suffix_that_names_no_line_does_not_parse() {
        for suffix in ["", ":", "name", "name:2", "x:2", ":x", "2:0", ":0"] {
            assert_eq!(LineSpec::parse(suffix), None, "{suffix:?}");
        }
    }

    #[test]
    fn a_spec_prints_in_the_form_it_was_written() {
        for suffix in ["2:4", "2:", ":4", "3"] {
            assert_eq!(LineSpec::parse(suffix).unwrap().0.to_string(), suffix);
        }
    }

    #[test]
    fn a_line_replaced_just_outside_a_range_is_not_a_line_added_beside_it() {
        let new = "a\nb\nc\nD\ne\n";
        assert_eq!(carry(new, range(Some(2), Some(3))), Carry::Same);
        assert_eq!(carry(new, range(Some(5), None)), Carry::Same);
    }

    #[test]
    fn a_lone_carriage_return_is_part_of_its_line_as_mdbook_reads_it() {
        let map = LineMap::between("a\rb\nc\n", "x\na\rb\nc\n");
        assert_eq!(
            map.carry(LineSpec::Line(2)),
            Carry::Moved(LineSpec::Line(3))
        );
    }

    #[test]
    fn a_line_whose_only_change_is_its_terminator_is_unchanged() {
        let map = LineMap::between("a\nb\nc", "a\nb\nc\nd\n");
        assert_eq!(
            map.carry(range(Some(2), Some(3))),
            Carry::Boundary(
                range(Some(2), Some(3)),
                "a line was added after line 3".into()
            )
        );
        let map = LineMap::between("a\r\nb\r\n", "a\nb\n");
        assert_eq!(map.carry(range(Some(1), Some(2))), Carry::Same);
    }

    #[test]
    fn a_range_with_no_change_near_it_stays() {
        let new = "a\nb\nc\nd\nE\n";
        assert_eq!(carry(new, range(Some(2), Some(3))), Carry::Same);
    }

    #[test]
    fn a_range_moves_down_past_lines_added_above_it() {
        let new = "x\ny\na\nb\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(4))),
            Carry::Moved(range(Some(4), Some(6)))
        );
    }

    #[test]
    fn a_range_moves_up_past_lines_deleted_above_it() {
        let new = "b\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(3), Some(4))),
            Carry::Moved(range(Some(2), Some(3)))
        );
    }

    #[test]
    fn a_range_grows_over_lines_added_inside_it() {
        let new = "a\nb\nx\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(4))),
            Carry::Moved(range(Some(2), Some(5)))
        );
    }

    #[test]
    fn a_line_added_just_after_the_end_stays_outside_and_is_named() {
        let new = "a\nb\nc\nx\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(3))),
            Carry::Boundary(
                range(Some(2), Some(3)),
                "a line was added after line 3".into()
            )
        );
    }

    #[test]
    fn a_line_added_just_before_the_start_stays_outside_and_is_named() {
        let new = "a\nx\nb\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(3))),
            Carry::Boundary(
                range(Some(3), Some(4)),
                "a line was added before line 2".into()
            )
        );
    }

    #[test]
    fn a_line_appended_after_a_range_ending_on_the_last_line_is_named() {
        let new = "a\nb\nc\nd\ne\nx\n";
        assert_eq!(
            carry(new, range(Some(4), Some(5))),
            Carry::Boundary(
                range(Some(4), Some(5)),
                "a line was added after line 5".into()
            )
        );
    }

    #[test]
    fn a_changed_end_line_snaps_inward() {
        let new = "a\nb\nc\nD\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(4))),
            Carry::Boundary(range(Some(2), Some(3)), "line 4 changed".into())
        );
    }

    #[test]
    fn a_deleted_start_line_snaps_inward() {
        let new = "a\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(4))),
            Carry::Boundary(range(Some(2), Some(3)), "line 2 changed".into())
        );
    }

    #[test]
    fn both_ends_are_named_when_both_changed() {
        let new = "a\nB\nc\nD\ne\n";
        assert_eq!(
            carry(new, range(Some(2), Some(4))),
            Carry::Boundary(
                range(Some(3), Some(3)),
                "line 2 changed; line 4 changed".into()
            )
        );
    }

    #[test]
    fn a_range_whose_every_line_changed_is_gone() {
        let new = "a\nX\nY\nd\ne\n";
        assert_eq!(carry(new, range(Some(2), Some(3))), Carry::Gone);
    }

    #[test]
    fn an_open_start_takes_in_lines_added_at_the_top() {
        let new = "x\na\nb\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(None, Some(2))),
            Carry::Moved(range(None, Some(3)))
        );
    }

    #[test]
    fn an_open_end_takes_in_lines_added_at_the_bottom() {
        let new = "a\nb\nc\nd\ne\nx\n";
        assert_eq!(carry(new, range(Some(4), None)), Carry::Same);
    }

    #[test]
    fn an_open_ended_range_whose_lines_all_changed_is_gone() {
        let new = "a\nb\nc\n";
        assert_eq!(carry(new, range(Some(4), None)), Carry::Gone);
    }

    #[test]
    fn an_end_past_the_old_file_is_left_as_written() {
        let new = "x\na\nb\nc\nd\ne\n";
        assert_eq!(
            carry(new, range(Some(4), Some(9))),
            Carry::Moved(range(Some(5), Some(9)))
        );
    }

    #[test]
    fn a_range_starting_past_the_old_file_is_left_as_written() {
        let new = "x\na\nb\nc\nd\ne\n";
        assert_eq!(carry(new, range(Some(7), Some(9))), Carry::Same);
    }

    #[test]
    fn a_single_line_moves_with_its_text() {
        let new = "x\na\nb\nc\nd\ne\n";
        assert_eq!(
            carry(new, LineSpec::Line(3)),
            Carry::Moved(LineSpec::Line(4))
        );
    }

    #[test]
    fn a_single_line_ignores_lines_added_beside_it() {
        let new = "a\nb\nx\nc\ny\nd\ne\n";
        assert_eq!(
            carry(new, LineSpec::Line(3)),
            Carry::Moved(LineSpec::Line(4))
        );
        assert_eq!(carry(new, LineSpec::Line(1)), Carry::Same);
    }

    #[test]
    fn a_single_line_that_changed_is_gone() {
        let new = "a\nb\nC\nd\ne\n";
        assert_eq!(carry(new, LineSpec::Line(3)), Carry::Gone);
    }

    #[test]
    fn a_single_line_past_the_old_file_is_left_as_written() {
        let new = "x\na\nb\nc\nd\ne\n";
        assert_eq!(carry(new, LineSpec::Line(9)), Carry::Same);
    }
}
