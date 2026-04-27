//! Parses `{{#diff <left> <right>}}` directives out of chapter markdown.
//! The resolver and renderer that consume the parsed [`DiffDirective`]s land
//! in later slices of the *Show Diffs Between Slices* story.

use std::ops::Range;

/// One parsed `{{#diff <left> <right>}}` directive. `span` indexes into the
/// chapter content the parser was handed and covers the directive in full
/// (`{{#diff …}}` inclusive) so the splicer can replace the whole substring
/// in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDirective {
    pub left: String,
    pub right: String,
    pub span: Range<usize>,
}

/// Walks `content` and returns every well-formed `{{#diff a b}}` directive.
/// Directives prefixed with a backslash (`\{{#diff …}}`, matching mdbook's
/// `{{#include}}` escape convention) are skipped here; the splicer that
/// lands later strips the leading backslash so the literal directive renders
/// to the reader. Directives with the wrong arity (`{{#diff a}}`,
/// `{{#diff a b c}}`) are silently skipped — the resolver in the next slice
/// surfaces the useful diagnostic, and being over-eager here would fight it.
pub fn parse_directives(content: &str) -> Vec<DiffDirective> {
    const PREFIX: &[u8] = b"{{#diff";
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + PREFIX.len() <= bytes.len() {
        if &bytes[i..i + PREFIX.len()] != PREFIX {
            i += 1;
            continue;
        }
        if i > 0 && bytes[i - 1] == b'\\' {
            i += PREFIX.len();
            continue;
        }
        let inner_start = i + PREFIX.len();
        let Some(end_rel) = content[inner_start..].find("}}") else {
            break;
        };
        let directive_end = inner_start + end_rel + 2;
        let tokens: Vec<&str> = content[inner_start..inner_start + end_rel]
            .split_whitespace()
            .collect();
        if tokens.len() == 2 {
            out.push(DiffDirective {
                left: tokens[0].to_string(),
                right: tokens[1].to_string(),
                span: i..directive_end,
            });
        }
        i = directive_end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_directives_extracts_well_formed_directive() {
        let s = "before {{#diff old-tag new-tag}} after";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1, "expected one directive; got {got:?}");
        assert_eq!(got[0].left, "old-tag");
        assert_eq!(got[0].right, "new-tag");
        assert_eq!(&s[got[0].span.clone()], "{{#diff old-tag new-tag}}");
    }

    #[test]
    fn parse_directives_handles_multiple_occurrences() {
        let s = "{{#diff a b}} mid {{#diff c d}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].left, "a");
        assert_eq!(got[1].right, "d");
        assert_eq!(&s[got[0].span.clone()], "{{#diff a b}}");
        assert_eq!(&s[got[1].span.clone()], "{{#diff c d}}");
    }

    #[test]
    fn parse_directives_skips_escaped_form() {
        let s = "use \\{{#diff a b}} verbatim";
        let got = parse_directives(s);
        assert!(
            got.is_empty(),
            "escaped directive should not parse; got {got:?}",
        );
    }

    #[test]
    fn parse_directives_tolerates_extra_whitespace_around_operands() {
        let s = "{{#diff   a    b   }}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].left, "a");
        assert_eq!(got[0].right, "b");
    }

    #[test]
    fn parse_directives_skips_malformed_arity() {
        for s in ["{{#diff only-one}}", "{{#diff a b c}}", "{{#diff}}"] {
            let got = parse_directives(s);
            assert!(
                got.is_empty(),
                "malformed directive `{s}` should not parse; got {got:?}",
            );
        }
    }

    #[test]
    fn parse_directives_accepts_arbitrary_operand_strings() {
        let s = "{{#diff live:src/foo.rs new-tag}}";
        let got = parse_directives(s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].left, "live:src/foo.rs");
        assert_eq!(got[0].right, "new-tag");
    }
}
