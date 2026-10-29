//! The book's line-width budget: `max-line-width` under
//! `[preprocessor.listings]`, measured on the lines a listing renders.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use mdbook_preprocessor::config::Config;
use unicode_width::UnicodeWidthStr;

use crate::callout::marker_lines;

/// mdBook's theme leaves `tab-size` at the browser default.
const TAB_STOP: usize = 8;

/// A line a listing renders wider than the budget.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LongLine {
    /// 1-based line of the frozen file.
    pub line: usize,
    pub width: usize,
}

/// The budget from `<book_root>/book.toml`, or `None` when the book sets
/// none (or has no `book.toml`). Read through mdBook's own config, as the
/// preprocessor reads its other keys.
pub(crate) fn max_line_width(book_root: &Path) -> Result<Option<usize>> {
    let path = book_root.join("book.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };
    let config: Config = text
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let invalid = || {
        format!(
            "`max-line-width` under [preprocessor.listings] in {} must be a positive integer",
            path.display()
        )
    };
    let Some(budget) = config
        .get::<i64>("preprocessor.listings.max-line-width")
        .with_context(invalid)?
    else {
        return Ok(None);
    };
    match usize::try_from(budget) {
        Ok(budget) if budget > 0 => Ok(Some(budget)),
        _ => Err(anyhow!("{}, got `{budget}`", invalid())),
    }
}

/// Display columns `line` occupies in a code block.
pub(crate) fn display_width(line: &str) -> usize {
    // Each run between tabs is measured whole, so an emoji sequence counts
    // as the one glyph it renders.
    let mut runs = line.split('\t');
    let first = runs.next().map_or(0, UnicodeWidthStr::width);
    runs.fold(first, |col, run| {
        col + TAB_STOP - col % TAB_STOP + run.width()
    })
}

/// The lines of the frozen file at `frozen` (holding `bytes`) wider than
/// `budget`, measured as an include renders it: the callout markers the
/// renderer strips are left out.
pub(crate) fn long_lines_in(frozen: &Path, bytes: &[u8], budget: usize) -> Vec<LongLine> {
    let ext = frozen.extension().and_then(|e| e.to_str()).unwrap_or("");
    long_lines(&String::from_utf8_lossy(bytes), ext, budget)
}

fn long_lines(content: &str, ext: &str, budget: usize) -> Vec<LongLine> {
    let markers: HashSet<usize> = marker_lines(content, ext).into_iter().collect();
    content
        .lines()
        .enumerate()
        .map(|(idx, text)| (idx + 1, text))
        .filter(|(line, _)| !markers.contains(line))
        .filter_map(|(line, text)| {
            let width = display_width(text);
            (width > budget).then_some(LongLine { line, width })
        })
        .collect()
}

/// `line 12 (75 columns), line 30 (74 columns)`.
pub(crate) fn describe(long: &[LongLine]) -> String {
    long.iter()
        .map(|l| format!("line {} ({} columns)", l.line, l.width))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book_toml(body: &str) -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::write(tmp.path().join("book.toml"), body).unwrap();
        tmp
    }

    #[test]
    fn the_budget_is_read_from_the_listings_preprocessor_table() {
        let tmp =
            book_toml("[book]\ntitle = \"t\"\n\n[preprocessor.listings]\nmax-line-width = 72\n");
        assert_eq!(max_line_width(tmp.path()).unwrap(), Some(72));
    }

    #[test]
    fn a_book_without_a_budget_has_none() {
        let tmp = book_toml("[preprocessor.listings]\nnumber-listings = true\n");
        assert_eq!(max_line_width(tmp.path()).unwrap(), None);
        let tmp = book_toml("[book]\ntitle = \"t\"\n");
        assert_eq!(max_line_width(tmp.path()).unwrap(), None);
        let empty = tempfile::TempDir::new().unwrap();
        assert_eq!(max_line_width(empty.path()).unwrap(), None);
    }

    #[test]
    fn a_budget_that_is_not_a_positive_integer_is_an_error() {
        for value in ["0", "-1", "\"72\"", "72.5"] {
            let tmp = book_toml(&format!(
                "[preprocessor.listings]\nmax-line-width = {value}\n"
            ));
            let err = max_line_width(tmp.path()).unwrap_err();
            assert!(
                format!("{err:#}").contains("max-line-width"),
                "{value}: the error names the key; got: {err:#}"
            );
        }
    }

    #[test]
    fn a_budget_of_one_column_is_accepted() {
        let tmp = book_toml("[preprocessor.listings]\nmax-line-width = 1\n");
        assert_eq!(max_line_width(tmp.path()).unwrap(), Some(1));
    }

    #[test]
    fn a_book_toml_that_cannot_be_read_is_an_error_not_a_missing_budget() {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("book.toml")).unwrap();
        assert!(max_line_width(tmp.path()).is_err());
    }

    #[test]
    fn an_unparsable_book_toml_is_an_error() {
        let tmp = book_toml("[preprocessor.listings\n");
        assert!(max_line_width(tmp.path()).is_err());
    }

    #[test]
    fn width_counts_display_columns_not_bytes_or_chars() {
        assert_eq!(display_width("abc"), 3);
        // An em dash is three bytes and one column.
        assert_eq!(display_width("a — b"), 5);
        // A CJK character is one char and two columns.
        assert_eq!(display_width("漢字"), 4);
        // A combining accent takes no column of its own.
        assert_eq!(display_width("e\u{301}"), 1);
    }

    #[test]
    fn an_emoji_sequence_counts_as_it_renders() {
        // A check mark with the emoji variation selector renders two wide.
        assert_eq!(display_width("\u{2714}\u{fe0f}"), 2);
        // A thumbs-up with a skin-tone modifier is one two-column glyph.
        assert_eq!(display_width("\u{1f44d}\u{1f3fd}"), 2);
        assert_eq!(display_width("\t\u{2714}\u{fe0f}\tx"), 17);
    }

    #[test]
    fn a_tab_advances_to_the_next_multiple_of_eight() {
        assert_eq!(display_width("\tx"), 9);
        assert_eq!(display_width("abc\tx"), 9);
        assert_eq!(display_width("abcdefgh\tx"), 17);
        assert_eq!(display_width("\t\t"), 16);
    }

    #[test]
    fn long_lines_reports_each_line_over_the_budget_with_its_width() {
        let content = "short\n0123456789\n012345678\n01234567890\n";
        assert_eq!(
            long_lines(content, "yaml", 9),
            vec![
                LongLine { line: 2, width: 10 },
                LongLine { line: 4, width: 11 },
            ]
        );
    }

    #[test]
    fn a_line_exactly_at_the_budget_fits() {
        assert_eq!(long_lines("012345678\n", "yaml", 9), vec![]);
    }

    #[test]
    fn a_callout_marker_line_is_not_measured() {
        let content =
            "a: 1\n# CALLOUT: why This body runs well past the nine-column budget.\nb: 2\n";
        assert_eq!(long_lines(content, "yaml", 9), vec![]);
    }

    #[test]
    fn a_marker_in_another_languages_comment_syntax_is_measured() {
        // `//` is not a YAML comment, so the renderer leaves this line in.
        let content = "// CALLOUT: why This body runs past the budget.\n";
        assert_eq!(
            long_lines(content, "yaml", 9),
            vec![LongLine { line: 1, width: 47 }]
        );
    }

    #[test]
    fn a_language_with_no_line_comment_measures_every_line() {
        let content = "// CALLOUT: why This body runs past the budget.\n";
        assert_eq!(long_lines(content, "css", 9).len(), 1);
    }

    #[test]
    fn crlf_line_endings_do_not_count_as_a_column() {
        assert_eq!(long_lines("012345678\r\n", "yaml", 9), vec![]);
    }

    #[test]
    fn describe_lists_each_line_and_its_width() {
        let long = [
            LongLine {
                line: 12,
                width: 75,
            },
            LongLine {
                line: 30,
                width: 74,
            },
        ];
        assert_eq!(
            describe(&long),
            "line 12 (75 columns), line 30 (74 columns)"
        );
    }
}
