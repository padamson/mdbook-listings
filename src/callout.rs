//! Parses inline `CALLOUT:` markers out of a frozen listing's source.

use std::collections::{HashMap, HashSet};

/// Position is a 1-based line number so error diagnostics and the eventual
/// rendered badge anchor can both refer to it directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callout {
    pub line: usize,
    pub label: String,
    pub body: Option<String>,
}

/// Walks `content` line by line and returns every well-formed callout
/// marker. A marker is a line whose first non-whitespace content matches
/// `<comment_prefix> CALLOUT: <label>[ <body>]`. Malformed lines are
/// silently skipped — the splicer leaves them in the rendered listing
/// unchanged.
// CALLOUT: parse-entry The single entry point: walks lines, calls parse_line, collects every match.
pub fn parse_callouts(content: &str, comment_prefix: &str) -> Vec<Callout> {
    let mut out = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        if let Some(callout) = parse_line(raw_line, comment_prefix, idx + 1) {
            out.push(callout);
        }
    }
    out
}

fn parse_line(raw_line: &str, comment_prefix: &str, line: usize) -> Option<Callout> {
    let after_prefix = raw_line.trim_start().strip_prefix(comment_prefix)?;
    let after_keyword = after_prefix.strip_prefix(' ')?.strip_prefix("CALLOUT:")?;
    let payload = after_keyword.strip_prefix(' ')?;
    let (label, rest) = match payload.split_once(char::is_whitespace) {
        Some((l, r)) => (l, Some(r)),
        None => (payload, None),
    };
    if label.is_empty() || !is_valid_label(label) {
        return None;
    }
    let body = rest.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Some(Callout {
        line,
        label: label.to_string(),
        body,
    })
}

// CALLOUT: label-grammar Labels are deliberately narrow: alphanumerics, hyphens, underscores. Anything else is rejected so labels stay safe to use as HTML id attributes and URL fragments.
fn is_valid_label(label: &str) -> bool {
    label
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Maps a listing's file extension to the language's single-line comment
/// prefix. Returns `None` for languages without a recognised inline-marker
/// syntax (block-comment-only languages take callouts via the sidecar form
/// instead).
pub fn comment_prefix_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "yaml" | "yml" | "toml" | "py" | "sh" | "bash" | "tf" | "hcl" => Some("#"),
        "rs" | "c" | "h" | "cpp" | "hpp" | "js" | "ts" | "jsx" | "tsx" => Some("//"),
        "sql" => Some("--"),
        _ => None,
    }
}

/// Maps a fenced-code-block info string to the language's single-line
/// comment prefix. Accepts the language names authors typically write
/// after the opening fence (`rust`, `yaml`, `python`, etc.) and falls back
/// to [`comment_prefix_for_extension`] for any input that's already an
/// extension (`rs`, `yml`).
pub fn comment_prefix_for_language(language: &str) -> Option<&'static str> {
    let normalised = match language {
        "rust" => "rs",
        "python" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "shell" | "zsh" => "sh",
        "c++" => "cpp",
        other => other,
    };
    comment_prefix_for_extension(normalised)
}

/// Errors raised by the callout splicer.
#[derive(Debug)]
pub enum SpliceError {
    /// A `{{#callout <label>}}` directive named a label that no callout
    /// marker in the chapter defines.
    UnknownLabel { label: String },
}

impl std::fmt::Display for SpliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpliceError::UnknownLabel { label } => write!(
                f,
                "{{{{#callout {label}}}}} references a label that no callout marker defines \
                 in this chapter",
            ),
        }
    }
}

impl std::error::Error for SpliceError {}

/// Which renderer the splicer is producing output for. The HTML emitter
/// uses raw `<dl>` tags so the rendered DOM carries stable
/// `data-callout-badge` and `dt[id]` attributes for cross-refs and e2e
/// assertions; the PDF emitter falls back to a markdown blockquote so
/// the typst-pdf backend renders the callouts as a styled note block
/// without relying on raw HTML passthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedRenderer {
    Html,
    TypstPdf,
}

impl SupportedRenderer {
    pub fn from_renderer_name(name: &str) -> Option<Self> {
        match name {
            "html" => Some(Self::Html),
            "typst-pdf" => Some(Self::TypstPdf),
            _ => None,
        }
    }
}

/// Replace each fenced code block in `content` with the original block plus
/// (when the block contains callout markers) a trailing per-renderer
/// callout list (`<dl class="callouts">` for HTML, markdown blockquote for
/// PDF), and replace each `{{#callout <label>}}` directive in chapter
/// prose with an inline anchor (HTML) or marker badge (PDF) that links
/// back to the listing badge.
// CALLOUT: splice-entry The HTML splicer entry point. The diff splicer in src/diff.rs has a sister function with the same shape — fence-aware walk + per-block emit.
pub fn splice_chapter(content: &str, renderer: SupportedRenderer) -> Result<String, SpliceError> {
    let label_to_ordinal = collect_first_occurrence_ordinals(content);
    let with_lists = splice_callout_lists(content, &label_to_ordinal, renderer);
    replace_callout_refs(&with_lists, &label_to_ordinal, renderer)
}

/// Records each label's ordinal at its FIRST occurrence in the chapter.
/// Subsequent occurrences (e.g. when the same source file is shown via
/// `{{#diff}}` after being `{{#include}}`'d) are ignored: the first dt
/// gets `id="callout-<label>"` and acts as the canonical anchor target;
/// later dts render the badge but no `id` so the HTML stays valid.
fn collect_first_occurrence_ordinals(content: &str) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for_each_fenced_block_with_span(content, |info, block_text, _body_start, _close_end| {
        for (idx, c) in callouts_for_block(info, block_text).iter().enumerate() {
            map.entry(c.label.clone()).or_insert(idx + 1);
        }
    });
    map
}

fn splice_callout_lists(
    content: &str,
    label_to_ordinal: &HashMap<String, usize>,
    renderer: SupportedRenderer,
) -> String {
    match renderer {
        SupportedRenderer::Html => splice_callout_lists_html(content),
        SupportedRenderer::TypstPdf => splice_callout_lists_pdf(content, label_to_ordinal),
    }
}

/// HTML splicer: for non-diff fenced blocks with markers, strip the marker
/// comment lines from the rendered listing and append a sibling
/// `<div class="callout-overlay">` carrying one interactive `<button>` +
/// hover-popover `<div>` per marker, each tagged with the post-strip
/// `data-callout-line` so CSS can position it on the line that previously
/// held the marker. Diff fences pass through unchanged — diffs show
/// history, the canonical anchor lives on the include's badge.
fn splice_callout_lists_html(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut emitted_anchor: HashSet<String> = HashSet::new();
    for_each_fenced_block_with_span(content, |info, block_text, body_start, close_end| {
        let callouts = callouts_for_block(info, block_text);
        let is_diff = info == "diff";
        // Diff blocks always go through the strip pass even when no `+`/` `
        // callouts exist — `-`-side markers still need to be dropped from
        // the rendered body.
        if callouts.is_empty() && !is_diff {
            return;
        }
        let (rewritten_body, post_strip_lines, total_lines) = if is_diff {
            strip_marker_lines_diff(block_text)
        } else {
            strip_marker_lines(block_text, info)
        };
        if is_diff && callouts.is_empty() && rewritten_body == block_text {
            // No-op diff: no markers of any kind to rewrite.
            return;
        }
        let pre_fence = &content[cursor..body_start];
        let close_fence_line = closing_fence_text(content, close_end);
        out.push_str(pre_fence);
        out.push_str(&rewritten_body);
        if !rewritten_body.is_empty() && !rewritten_body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(close_fence_line);
        out.push('\n');
        out.push_str(&render_callout_overlay_html(
            &callouts,
            &post_strip_lines,
            total_lines,
            &mut emitted_anchor,
        ));
        out.push('\n');
        cursor = close_end;
    });
    out.push_str(&content[cursor..]);
    out
}

/// Compute the rewritten block body (marker lines removed) and the
/// post-strip 1-based line numbers each marker now lands on (i.e. the
/// line that took its place after the strip — typically the next non-
/// marker code line).
fn strip_marker_lines(block_text: &str, info: &str) -> (String, Vec<usize>, usize) {
    let prefix = comment_prefix_for_language(info);
    let lines: Vec<&str> = block_text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(block_text.len());
    let mut post_strip_lines: Vec<usize> = Vec::new();
    let mut emitted_count: usize = 0;
    for raw_line in lines {
        let line_no_newline = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let is_marker = prefix
            .and_then(|p| parse_line(line_no_newline, p, 0))
            .is_some();
        if is_marker {
            // Marker stripped; the next non-marker line we emit becomes the
            // line the badge points at. If we're at the end of the block we
            // still record a line — the badge clamps to the last visible
            // line of the listing.
            let target = (emitted_count + 1).max(1);
            post_strip_lines.push(target);
        } else {
            out.push_str(raw_line);
            emitted_count += 1;
        }
    }
    (out, post_strip_lines, emitted_count)
}

// CALLOUT: strip-diff Diff-aware strip: drop `-`-prefixed marker lines entirely (no badge — the callout is gone in the new state); strip `+`-prefixed and ` `-prefixed marker lines and record post-strip positions so badges land on the line that previously held them in the diff's right-hand side.
fn strip_marker_lines_diff(block_text: &str) -> (String, Vec<usize>, usize) {
    let lines: Vec<&str> = block_text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(block_text.len());
    let mut post_strip_lines: Vec<usize> = Vec::new();
    let mut emitted_count: usize = 0;
    for raw_line in lines {
        let line_no_newline = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        // Diff metadata lines pass through unchanged.
        if line_no_newline.starts_with("---")
            || line_no_newline.starts_with("+++")
            || line_no_newline.starts_with("@@")
            || line_no_newline.starts_with('\\')
        {
            out.push_str(raw_line);
            emitted_count += 1;
            continue;
        }
        // Identify the diff-line prefix and try to parse the trailing
        // payload as a marker against any known comment prefix.
        let (prefix_char, payload) = if let Some(rest) = line_no_newline.strip_prefix('+') {
            (Some('+'), rest)
        } else if let Some(rest) = line_no_newline.strip_prefix('-') {
            (Some('-'), rest)
        } else if let Some(rest) = line_no_newline.strip_prefix(' ') {
            (Some(' '), rest)
        } else {
            (None, line_no_newline)
        };
        let is_marker = ALL_COMMENT_PREFIXES
            .iter()
            .any(|p| parse_line(payload, p, 0).is_some());
        if is_marker {
            // `+` and ` ` markers: strip the line, record post-strip position
            // for badge placement. `-` markers: drop silently.
            if matches!(prefix_char, Some('+') | Some(' ')) {
                let target = (emitted_count + 1).max(1);
                post_strip_lines.push(target);
            }
        } else {
            out.push_str(raw_line);
            emitted_count += 1;
        }
    }
    (out, post_strip_lines, emitted_count)
}

fn closing_fence_text(content: &str, close_end: usize) -> &str {
    // close_end is one past the trailing newline of the closing fence
    // (or equal to bytes.len() if the file ends without a trailing newline).
    let end = close_end.saturating_sub(1);
    let slice = &content[..end];
    let line_start = slice.rfind('\n').map(|i| i + 1).unwrap_or(0);
    &content[line_start..end]
}

/// PDF splicer (slice 6 shape): keep the marker comment visible in the
/// rendered listing, append a markdown blockquote summarising each
/// callout below the block. Slice 8 will pivot this to strip + inline
/// badge marker.
fn splice_callout_lists_pdf(content: &str, label_to_ordinal: &HashMap<String, usize>) -> String {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut emitted_anchor: HashSet<String> = HashSet::new();
    for_each_fenced_block_with_span(content, |info, block_text, _body_start, close_end| {
        let callouts = callouts_for_block(info, block_text);
        if !callouts.is_empty() {
            out.push_str(&content[cursor..close_end]);
            out.push('\n');
            out.push_str(&render_callout_list(
                &callouts,
                label_to_ordinal,
                &mut emitted_anchor,
                SupportedRenderer::TypstPdf,
            ));
            out.push('\n');
            cursor = close_end;
        }
    });
    out.push_str(&content[cursor..]);
    out
}

pub(crate) fn for_each_fenced_block_with_span<F>(content: &str, mut visit: F)
where
    F: FnMut(&str, &str, usize, usize),
{
    let bytes = content.as_bytes();
    let mut line_start = 0;
    let mut open: Option<OpenFence> = None;
    while line_start < bytes.len() {
        let line_end = match content[line_start..].find('\n') {
            Some(off) => line_start + off,
            None => bytes.len(),
        };
        let line = &content[line_start..line_end];
        match &open {
            None => {
                if let Some((info, opener)) = fence_open_info(line) {
                    open = Some(OpenFence {
                        info,
                        opener,
                        body_start: line_end + 1,
                    });
                }
            }
            Some(o) => {
                if line_closes_fence(line, o.opener) {
                    let block_text = &content[o.body_start..line_start];
                    let close_end = if line_end < bytes.len() {
                        line_end + 1
                    } else {
                        line_end
                    };
                    visit(&o.info, block_text, o.body_start, close_end);
                    open = None;
                }
            }
        }
        if line_end == bytes.len() {
            break;
        }
        line_start = line_end + 1;
    }
}

const CALLOUT_DIRECTIVE_OPEN: &str = "{{#callout ";
const CALLOUT_DIRECTIVE_CLOSE: &str = "}}";

/// Replace `{{#callout <label>}}` directives that sit outside fenced code
/// blocks. Directives inside fenced blocks (e.g. literal documentation
/// examples) pass through untouched so authors can show the syntax.
// CALLOUT: cross-ref-replace Two-pass entry: skips directives inside fenced blocks, errors on labels not in the chapter's collected map.
fn replace_callout_refs(
    content: &str,
    label_to_ordinal: &HashMap<String, usize>,
    renderer: SupportedRenderer,
) -> Result<String, SpliceError> {
    let mut fence_spans: Vec<(usize, usize)> = Vec::new();
    for_each_fenced_block_with_span(content, |_info, _text, body_start, close_end| {
        fence_spans.push((body_start, close_end));
    });

    let in_fence = |pos: usize| {
        fence_spans
            .iter()
            .any(|&(start, end)| pos >= start && pos < end)
    };

    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(rel) = content[cursor..].find(CALLOUT_DIRECTIVE_OPEN) {
        let open_at = cursor + rel;
        if in_fence(open_at) {
            // Step past the opener so we don't loop on it forever.
            out.push_str(&content[cursor..open_at + CALLOUT_DIRECTIVE_OPEN.len()]);
            cursor = open_at + CALLOUT_DIRECTIVE_OPEN.len();
            continue;
        }
        let label_start = open_at + CALLOUT_DIRECTIVE_OPEN.len();
        let close_rel = match content[label_start..].find(CALLOUT_DIRECTIVE_CLOSE) {
            Some(off) => off,
            None => {
                out.push_str(&content[cursor..label_start]);
                cursor = label_start;
                continue;
            }
        };
        let label = content[label_start..label_start + close_rel].trim();
        if !is_valid_label(label) {
            out.push_str(&content[cursor..label_start]);
            cursor = label_start;
            continue;
        }
        let ordinal =
            label_to_ordinal
                .get(label)
                .copied()
                .ok_or_else(|| SpliceError::UnknownLabel {
                    label: label.to_string(),
                })?;
        out.push_str(&content[cursor..open_at]);
        out.push_str(&render_callout_ref(label, ordinal, renderer));
        cursor = label_start + close_rel + CALLOUT_DIRECTIVE_CLOSE.len();
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

// CALLOUT: cross-ref-emit Renders the prose-side anchor for HTML; falls back to a bracketed badge for typst-pdf where raw HTML doesn't carry through.
fn render_callout_ref(label: &str, ordinal: usize, renderer: SupportedRenderer) -> String {
    match renderer {
        SupportedRenderer::Html => {
            let label_esc = html_escape(label);
            format!(
                "<a class=\"callout-badge callout-ref\" href=\"#callout-{label_esc}\" \
                 data-callout-ref=\"{label_esc}\" data-callout-ordinal=\"{ordinal}\">{ordinal}</a>",
            )
        }
        SupportedRenderer::TypstPdf => format!("**[{ordinal}]**"),
    }
}

struct OpenFence {
    info: String,
    opener: Fence,
    body_start: usize,
}

#[derive(Clone, Copy)]
struct Fence {
    char: u8,
    count: usize,
}

fn fence_open_info(line: &str) -> Option<(String, Fence)> {
    let trimmed = line.trim_start();
    let leading_spaces = line.len() - trimmed.len();
    if leading_spaces > 3 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let fence_char = match bytes.first()? {
        b'`' => b'`',
        b'~' => b'~',
        _ => return None,
    };
    let count = bytes.iter().take_while(|&&b| b == fence_char).count();
    if count < 3 {
        return None;
    }
    Some((
        trimmed[count..].trim().to_string(),
        Fence {
            char: fence_char,
            count,
        },
    ))
}

/// CommonMark closes a fenced block only with a fence of the same character
/// at least as long as the opener and a blank info string. Same-character
/// fences shorter than the opener stay inside the block as literal text —
/// which is what lets included source files contain `\`\`\`yaml` inside
/// string literals without prematurely terminating the outer fence.
fn line_closes_fence(line: &str, opener: Fence) -> bool {
    let trimmed = line.trim_start();
    let leading_spaces = line.len() - trimmed.len();
    if leading_spaces > 3 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let count = bytes.iter().take_while(|&&b| b == opener.char).count();
    if count < opener.count {
        return false;
    }
    trimmed[count..].trim().is_empty()
}

/// Produce the callout list for a fenced block. `info` is the fence's info
/// string (`rust`, `yaml`, `diff`, …). Diff blocks are handled specially:
/// added (`+`) and context (` `) lines are stripped of their diff indicator
/// before being parsed against every known comment prefix; removed (`-`)
/// lines and diff metadata (`---`, `+++`, `@@`, `\`) are skipped, since a
/// callout that's been deleted shouldn't carry a badge in the post-diff
/// state.
fn callouts_for_block(info: &str, block_text: &str) -> Vec<Callout> {
    if info == "diff" {
        return callouts_from_diff_block(block_text);
    }
    if let Some(prefix) = comment_prefix_for_language(info) {
        return parse_callouts(block_text, prefix);
    }
    Vec::new()
}

const ALL_COMMENT_PREFIXES: &[&str] = &["//", "#", "--"];

fn callouts_from_diff_block(block_text: &str) -> Vec<Callout> {
    let mut out = Vec::new();
    for (idx, raw_line) in block_text.lines().enumerate() {
        if raw_line.starts_with("---")
            || raw_line.starts_with("+++")
            || raw_line.starts_with("@@")
            || raw_line.starts_with('\\')
        {
            continue;
        }
        let stripped = if let Some(rest) = raw_line.strip_prefix('+') {
            rest
        } else if let Some(rest) = raw_line.strip_prefix(' ') {
            rest
        } else {
            continue;
        };
        for prefix in ALL_COMMENT_PREFIXES {
            if let Some(callout) = parse_line(stripped, prefix, idx + 1) {
                out.push(callout);
                break;
            }
        }
    }
    out
}

/// PDF-only dispatch from the slice 6 splicer path. The HTML splicer
/// uses [`render_callout_overlay_html`] directly because it also rewrites
/// the listing body, not just the trailing block.
fn render_callout_list(
    callouts: &[Callout],
    _label_to_ordinal: &HashMap<String, usize>,
    _emitted_anchor: &mut HashSet<String>,
    renderer: SupportedRenderer,
) -> String {
    match renderer {
        SupportedRenderer::Html => unreachable!("HTML uses render_callout_overlay_html directly"),
        SupportedRenderer::TypstPdf => render_callout_list_pdf(callouts),
    }
}

/// HTML overlay (slice 7 shape): one interactive `<button>` per marker
/// laid out in an absolutely-positioned overlay sibling of the rendered
/// listing. `data-callout-line` (1-based, post-strip) lets CSS or JS
/// position each badge on the line that previously held the marker
/// comment. The body lives in a sibling `<div>` shown on hover/focus
/// via the bundled mdbook-listings.css.
// CALLOUT: html-overlay One button per marker, each tagged with post-strip line; body renders in a hover-popover sibling div for label-bearing markers and is omitted entirely when body is None.
fn render_callout_overlay_html(
    callouts: &[Callout],
    post_strip_lines: &[usize],
    total_lines: usize,
    emitted_anchor: &mut HashSet<String>,
) -> String {
    let mut s = String::new();
    s.push_str("<div class=\"callout-overlay\" data-callout-overlay>\n");
    for (idx, c) in callouts.iter().enumerate() {
        let ordinal = idx + 1;
        let label_esc = html_escape(&c.label);
        let line = post_strip_lines.get(idx).copied().unwrap_or(1);
        let id_attr = if emitted_anchor.insert(c.label.clone()) {
            format!(" id=\"callout-{label_esc}\"")
        } else {
            String::new()
        };
        s.push_str(&format!(
            "  <div class=\"callout-entry\" data-callout-line=\"{line}\" \
             style=\"--callout-line: {line}; --callout-listing-lines: {total_lines};\">\n",
        ));
        s.push_str(&format!(
            "    <button type=\"button\" class=\"callout-badge\"{id_attr} \
             data-callout-badge=\"{label_esc}\" data-callout-ordinal=\"{ordinal}\" \
             aria-describedby=\"callout-body-{label_esc}\">{ordinal}</button>\n",
        ));
        if let Some(body) = &c.body {
            s.push_str(&format!(
                "    <div class=\"callout-body\" id=\"callout-body-{label_esc}\" role=\"tooltip\">{}</div>\n",
                html_escape(body),
            ));
        }
        s.push_str("  </div>\n");
    }
    s.push_str("</div>");
    s
}

// CALLOUT: pdf-emit Markdown blockquote with bold ordinal + label, one paragraph per callout. typst-pdf renders this as a quoted note block; bodyless markers render as just the label.
fn render_callout_list_pdf(callouts: &[Callout]) -> String {
    let mut s = String::new();
    for (idx, c) in callouts.iter().enumerate() {
        let ordinal = idx + 1;
        if idx > 0 {
            s.push_str("> \n");
        }
        match &c.body {
            Some(body) => {
                s.push_str(&format!("> **[{ordinal}] {}** — {body}\n", c.label));
            }
            None => {
                s.push_str(&format!("> **[{ordinal}] {}**\n", c.label));
            }
        }
    }
    s
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_label_with_body_for_hash_prefix() {
        let s = "key: value\n# CALLOUT: greeting Says hello to the user.\nfoo: bar\n";
        let got = parse_callouts(s, "#");
        assert_eq!(
            got,
            vec![Callout {
                line: 2,
                label: "greeting".into(),
                body: Some("Says hello to the user.".into()),
            }]
        );
    }

    #[test]
    fn parses_label_only_form_for_hash_prefix() {
        let s = "# CALLOUT: anchor-only\n";
        let got = parse_callouts(s, "#");
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "anchor-only".into(),
                body: None,
            }]
        );
    }

    #[test]
    fn parses_double_slash_prefix() {
        let s = "fn main() {\n    // CALLOUT: entry The program starts here.\n}\n";
        let got = parse_callouts(s, "//");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].line, 2);
        assert_eq!(got[0].label, "entry");
        assert_eq!(got[0].body.as_deref(), Some("The program starts here."));
    }

    #[test]
    fn parses_double_dash_prefix_for_sql() {
        let s = "SELECT *\n-- CALLOUT: filter Limits to active rows.\nFROM users;\n";
        let got = parse_callouts(s, "--");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "filter");
    }

    #[test]
    fn skips_marker_with_wrong_prefix() {
        let s = "# CALLOUT: hash-marker\n";
        assert!(parse_callouts(s, "//").is_empty());
    }

    #[test]
    fn skips_missing_space_between_prefix_and_keyword() {
        let s = "#CALLOUT: nope\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn skips_missing_space_after_keyword() {
        let s = "# CALLOUT:nope\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn skips_empty_label() {
        let s = "# CALLOUT:  body-without-label\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn skips_label_with_invalid_characters() {
        let s = "# CALLOUT: bad/label has body\n";
        assert!(parse_callouts(s, "#").is_empty());
    }

    #[test]
    fn returns_none_body_when_label_alone_with_trailing_whitespace() {
        let s = "# CALLOUT: alone   \n";
        let got = parse_callouts(s, "#");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, None);
    }

    #[test]
    fn collects_multiple_callouts_in_one_listing() {
        let s = "\
            # first comment\n\
            # CALLOUT: one Body of one.\n\
            key: value\n\
            # CALLOUT: two\n\
            other: thing\n\
            # CALLOUT: three Body of three.\n\
        ";
        let got = parse_callouts(s, "#");
        assert_eq!(got.len(), 3);
        assert_eq!((got[0].line, &got[0].label[..]), (2, "one"));
        assert_eq!((got[1].line, &got[1].label[..]), (4, "two"));
        assert_eq!((got[2].line, &got[2].label[..]), (6, "three"));
        assert_eq!(got[1].body, None);
    }

    #[test]
    fn tolerates_indented_marker() {
        let s = "    # CALLOUT: indented Body text.\n";
        let got = parse_callouts(s, "#");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].label, "indented");
    }

    #[test]
    fn comment_prefix_for_extension_covers_initial_table() {
        for ext in ["yaml", "yml", "toml", "py", "sh", "bash", "tf", "hcl"] {
            assert_eq!(comment_prefix_for_extension(ext), Some("#"), "ext: {ext}");
        }
        for ext in ["rs", "c", "h", "cpp", "hpp", "js", "ts", "jsx", "tsx"] {
            assert_eq!(comment_prefix_for_extension(ext), Some("//"), "ext: {ext}");
        }
        assert_eq!(comment_prefix_for_extension("sql"), Some("--"));
    }

    #[test]
    fn comment_prefix_for_extension_returns_none_for_unknown_languages() {
        assert_eq!(comment_prefix_for_extension("css"), None);
        assert_eq!(comment_prefix_for_extension(""), None);
        assert_eq!(comment_prefix_for_extension("md"), None);
    }

    #[test]
    fn comment_prefix_for_language_normalises_common_fence_labels() {
        assert_eq!(comment_prefix_for_language("rust"), Some("//"));
        assert_eq!(comment_prefix_for_language("python"), Some("#"));
        assert_eq!(comment_prefix_for_language("javascript"), Some("//"));
        assert_eq!(comment_prefix_for_language("shell"), Some("#"));
        assert_eq!(comment_prefix_for_language("c++"), Some("//"));
        assert_eq!(comment_prefix_for_language("yaml"), Some("#"));
        assert_eq!(comment_prefix_for_language("rs"), Some("//"));
    }

    #[test]
    fn splice_chapter_html_strips_markers_and_emits_overlay_with_badges() {
        let content = concat!(
            "Before paragraph.\n\n",
            "```yaml\n",
            "service: greeting\n",
            "# CALLOUT: greeting-name The service identifier.\n",
            "endpoint: /hello\n",
            "# CALLOUT: endpoint-path\n",
            "```\n\n",
            "After paragraph.\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(out.contains("Before paragraph.\n"));
        assert!(out.contains("After paragraph.\n"));
        assert!(
            !out.contains("# CALLOUT:"),
            "marker comment line must be stripped from rendered listing; got:\n{out}",
        );
        assert!(
            out.contains("<div class=\"callout-overlay\""),
            "expected overlay sibling div; got:\n{out}",
        );
        assert!(out.contains("data-callout-badge=\"greeting-name\""));
        assert!(out.contains("data-callout-ordinal=\"1\""));
        assert!(out.contains("data-callout-badge=\"endpoint-path\""));
        assert!(out.contains("data-callout-ordinal=\"2\""));
        assert!(
            out.contains("<div class=\"callout-body\"") && out.contains("The service identifier."),
            "expected body popover for marker with body; got:\n{out}",
        );
        assert!(
            !out.contains("id=\"callout-body-endpoint-path\""),
            "label-only callout should have no body popover; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_leaves_block_alone_when_no_markers_present() {
        let content = "```yaml\nservice: greeting\nendpoint: /hello\n```\n";
        assert_eq!(
            splice_chapter(content, SupportedRenderer::Html).expect("splice"),
            content
        );
    }

    #[test]
    fn splice_chapter_skips_block_with_unknown_language() {
        let content = "```\n# CALLOUT: anchor body text\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(!out.contains("data-callout-badge"));
    }

    #[test]
    fn splice_chapter_handles_two_blocks_independently_for_per_listing_numbering() {
        let content = "\
            ```yaml\n\
            # CALLOUT: a-one\n\
            ```\n\n\
            ```rust\n\
            // CALLOUT: b-one\n\
            // CALLOUT: b-two\n\
            ```\n";
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(out.contains("data-callout-badge=\"a-one\""));
        assert!(out.contains("data-callout-badge=\"b-one\""));
        assert!(out.contains("data-callout-badge=\"b-two\""));
        let a_one_ordinal = out
            .split("data-callout-badge=\"a-one\"")
            .nth(1)
            .and_then(|s| s.split("data-callout-ordinal=\"").nth(1))
            .unwrap_or("");
        assert!(
            a_one_ordinal.starts_with("1\""),
            "first listing's first marker should be ordinal 1; got prefix {}",
            &a_one_ordinal[..a_one_ordinal.len().min(10)],
        );
        let b_two_ordinal = out
            .split("data-callout-badge=\"b-two\"")
            .nth(1)
            .and_then(|s| s.split("data-callout-ordinal=\"").nth(1))
            .unwrap_or("");
        assert!(
            b_two_ordinal.starts_with("2\""),
            "second listing's second marker should be ordinal 2; got prefix {}",
            &b_two_ordinal[..b_two_ordinal.len().min(10)],
        );
    }

    #[test]
    fn splice_chapter_html_strips_added_marker_lines_from_diff_and_emits_badge() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,1 +1,2 @@\n",
            " fn unchanged() {}\n",
            "+// CALLOUT: added-marker Body for an added marker.\n",
            "+fn added() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(
            !out.contains("// CALLOUT: added-marker"),
            "added marker comment line should be stripped from rendered diff; got:\n{out}",
        );
        assert!(
            out.contains("data-callout-badge=\"added-marker\""),
            "expected badge for the added marker; got:\n{out}",
        );
        assert!(
            out.contains("+fn added() {}"),
            "non-marker `+` line should survive; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_strips_context_marker_lines_from_diff_and_emits_badge() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,2 @@\n",
            " // CALLOUT: kept-marker A marker carried over unchanged.\n",
            " fn carried() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(
            !out.contains("// CALLOUT: kept-marker"),
            "context marker comment line should be stripped; got:\n{out}",
        );
        assert!(
            out.contains("data-callout-badge=\"kept-marker\""),
            "expected badge for the carried-over marker; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_drops_removed_marker_lines_from_diff_with_no_badge() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,2 +1,1 @@\n",
            "-// CALLOUT: gone-marker Removed in this slice.\n",
            " fn unchanged() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(
            !out.contains("// CALLOUT: gone-marker"),
            "removed marker comment line should be dropped, not visible; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-badge=\"gone-marker\""),
            "removed-side marker must not produce a badge; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_dedups_id_when_label_appears_in_diff_then_include() {
        // First non-empty fenced block to contain a label gets the
        // `id="callout-LABEL"` anchor. Subsequent occurrences (same label
        // in another block) emit the badge but skip the id so the HTML
        // stays valid (no duplicate IDs).
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1 +1,2 @@\n",
            " fn unchanged() {}\n",
            "+// CALLOUT: same-label First occurrence is in a diff.\n",
            "```\n\n",
            "```rust\n",
            "// CALLOUT: same-label Second occurrence is in an include.\n",
            "fn body() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        let id_count = out.matches("id=\"callout-same-label\"").count();
        assert_eq!(
            id_count, 1,
            "expected exactly one id=\"callout-same-label\" across the chapter; got {id_count} in:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_pdf_picks_up_callouts_from_added_and_context_diff_lines() {
        // The PDF emitter still emits per-block callouts for diff fences as
        // a markdown blockquote (slice 6 shape). The HTML emitter (slice 7+)
        // skips diff blocks since the canonical badge anchor lives on the
        // include, not on the diff history.
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1,3 +1,4 @@\n",
            " fn unchanged() {}\n",
            "-fn removed() {}\n",
            "+// CALLOUT: added-marker Body for a freshly added marker.\n",
            " // CALLOUT: context-marker Body for a marker that survived the diff.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::TypstPdf).expect("splice");
        assert!(
            out.contains("[1] added-marker"),
            "added line marker should render in pdf blockquote; got:\n{out}",
        );
        assert!(
            out.contains("[2] context-marker"),
            "context line marker should render in pdf blockquote; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_pdf_skips_callouts_on_removed_diff_lines() {
        let content = concat!(
            "```diff\n",
            "--- a-tag\n",
            "+++ b-tag\n",
            "@@ -1 +1 @@\n",
            "-// CALLOUT: gone-marker This callout was removed.\n",
            "+// CALLOUT: kept-marker This one stays.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::TypstPdf).expect("splice");
        // `gone-marker` will still appear inside the diff fence itself
        // (PDF emitter doesn't strip diff content); we only need the
        // appended blockquote to omit it.
        let blockquote = out.split("```\n\n").nth(1).unwrap_or("");
        assert!(blockquote.contains("[1] kept-marker"));
        assert!(
            !blockquote.contains("gone-marker"),
            "removed-line markers should not render in the appended blockquote; got:\n{blockquote}",
        );
    }

    #[test]
    fn splice_chapter_does_not_close_outer_fence_on_shorter_inner_fence() {
        let content = concat!(
            "````rust\n",
            "let s = \"```yaml\\n# CALLOUT: not-real-marker\\n```\";\n",
            "// CALLOUT: real-marker This one should be picked up.\n",
            "````\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(
            out.contains("data-callout-badge=\"real-marker\""),
            "expected the marker outside the embedded ```yaml string to render; got:\n{out}",
        );
        assert!(
            !out.contains("data-callout-badge=\"not-real-marker\""),
            "the marker inside the embedded string is YAML, not Rust — and the outer fence is rust; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_escapes_label_and_body() {
        let content = "```yaml\n# CALLOUT: lbl Body with <script> in it.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        let overlay = out
            .split("<div class=\"callout-overlay\"")
            .nth(1)
            .unwrap_or("");
        assert!(
            overlay.contains("&lt;script&gt;"),
            "overlay body should escape <script>; got:\n{overlay}",
        );
        assert!(
            !overlay.contains("<script>"),
            "overlay body must not contain raw <script>; got:\n{overlay}",
        );
    }

    #[test]
    fn splice_chapter_replaces_callout_directive_with_anchor_to_listing_badge() {
        let content = concat!(
            "Prose mentions {{#callout greeting}} the marker.\n\n",
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(
            out.contains("href=\"#callout-greeting\""),
            "expected anchor href pointing at listing badge id; got:\n{out}",
        );
        assert!(
            out.contains("data-callout-ref=\"greeting\""),
            "expected ref-side data attribute; got:\n{out}",
        );
        assert!(
            !out.contains("{{#callout greeting}}"),
            "directive should be replaced; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_resolves_forward_reference_to_callout_defined_below() {
        let content = concat!(
            "See {{#callout later}} below.\n\n",
            "```rust\n",
            "// CALLOUT: later Defined after the reference.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(out.contains("href=\"#callout-later\""));
    }

    #[test]
    fn splice_chapter_callout_ref_carries_per_listing_ordinal() {
        let content = concat!(
            "Reference {{#callout two}} here.\n\n",
            "```rust\n",
            "// CALLOUT: one First.\n",
            "// CALLOUT: two Second.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        let segment = out.split("data-callout-ref=\"two\"").nth(1).unwrap_or("");
        assert!(
            segment.contains("data-callout-ordinal=\"2\""),
            "ref to `two` should carry ordinal 2; got segment:\n{segment}",
        );
    }

    #[test]
    fn splice_chapter_unknown_callout_label_returns_error() {
        let content = "Unknown ref {{#callout missing}} here.\n";
        let err = splice_chapter(content, SupportedRenderer::Html)
            .expect_err("expected unknown-label error");
        match err {
            SpliceError::UnknownLabel { label } => assert_eq!(label, "missing"),
        }
    }

    #[test]
    fn splice_chapter_emits_id_only_on_first_occurrence_of_repeated_label() {
        // Same source file shown via {{#include}} and {{#diff}} produces two
        // dl entries for the same label; only the first carries id="callout-X"
        // so the rendered HTML stays valid (no duplicate IDs).
        let content = concat!(
            "```rust\n",
            "// CALLOUT: same Body.\n",
            "```\n\n",
            "```diff\n",
            "+// CALLOUT: same Body.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        let id_count = out.matches("id=\"callout-same\"").count();
        assert_eq!(
            id_count, 1,
            "expected exactly one id=\"callout-same\"; got {id_count} in:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_pdf_renderer_emits_blockquote_per_callout_list() {
        let content = concat!(
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "# CALLOUT: anchor-only\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::TypstPdf).expect("splice");
        assert!(
            !out.contains("<dl"),
            "PDF renderer must not emit raw HTML; got:\n{out}",
        );
        assert!(
            out.contains("> **[1] greeting** — Says hello."),
            "expected blockquote with bold ordinal+label and body; got:\n{out}",
        );
        assert!(
            out.contains("> **[2] anchor-only**"),
            "expected label-only callout to render with just label; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_pdf_renderer_replaces_cross_ref_with_bracketed_badge() {
        let content = concat!(
            "Reference {{#callout greeting}} here.\n\n",
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::TypstPdf).expect("splice");
        assert!(
            out.contains("**[1]**"),
            "expected bracketed bold ordinal in prose; got:\n{out}",
        );
        assert!(
            !out.contains("<a "),
            "PDF renderer must not emit raw HTML anchor; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_does_not_replace_callout_directive_inside_code_block() {
        // Authors show literal directive syntax inside fenced examples; the
        // splicer must not rewrite them into anchors.
        let content = concat!(
            "```text\n",
            "See {{#callout greeting}} for details.\n",
            "```\n\n",
            "```yaml\n",
            "# CALLOUT: greeting Says hello.\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html).expect("splice");
        assert!(
            out.contains("{{#callout greeting}}"),
            "literal directive inside code block should pass through; got:\n{out}",
        );
        assert!(
            !out.contains("href=\"#callout-greeting\""),
            "should not have rendered anchor for the in-code-block reference; got:\n{out}",
        );
    }
}
