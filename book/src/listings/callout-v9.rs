//! Parses inline `CALLOUT:` markers out of a frozen listing's source.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Position is a 1-based line number so error diagnostics and the eventual
/// rendered badge anchor can both refer to it directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Callout {
    pub line: usize,
    pub label: String,
    pub body: Option<String>,
    /// `--key=value` options written between the label and the body, e.g.
    /// `// CALLOUT: lbl --align=left Body text.` parses to
    /// `options = {"align" => "left"}`. Unknown keys round-trip but have
    /// no rendering effect today; that's how new per-callout options
    /// (alignment, width, theme) can land without a parser change.
    pub options: HashMap<String, String>,
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
    // Pull `--key=value` options off the front of `rest` while the
    // leading token matches the option shape; the rest becomes body.
    let (options, body_str) = parse_options(rest.map(|s| s.trim_start()));
    let body = body_str
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(Callout {
        line,
        label: label.to_string(),
        body,
        options,
    })
}

/// Parses a leading sequence of `--key=value` tokens, returning the
/// option map plus whatever's left (the body). Tokens that don't match
/// the `--key=value` shape end option parsing; everything from that
/// token onward is the body (verbatim, with the leading whitespace
/// preserved so callers can re-trim).
fn parse_options(rest: Option<&str>) -> (HashMap<String, String>, Option<&str>) {
    let mut options = HashMap::new();
    let mut cursor = match rest {
        Some(s) => s,
        None => return (options, None),
    };
    loop {
        let trimmed = cursor.trim_start();
        if !trimmed.starts_with("--") {
            return (options, Some(cursor));
        }
        // Token is the substring up to the next whitespace.
        let (token, after) = match trimmed.split_once(char::is_whitespace) {
            Some((t, a)) => (t, Some(a)),
            None => (trimmed, None),
        };
        // Must contain `=` to be a valid option; otherwise treat as body.
        let kv = token.strip_prefix("--").and_then(|s| s.split_once('='));
        let Some((key, value)) = kv else {
            return (options, Some(cursor));
        };
        if key.is_empty() {
            return (options, Some(cursor));
        }
        options.insert(key.to_string(), value.to_string());
        cursor = match after {
            Some(rest) => rest,
            None => return (options, None),
        };
    }
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
    /// The same label appears as BOTH an inline `// CALLOUT:` marker in
    /// the frozen listing AND as a `[[callout]]` entry in the sidecar
    /// TOML file. Cross-source collisions silently hide one of the two
    /// rendered badges, so the build fails loudly and names the
    /// duplicate label plus both source paths.
    LabelCollision {
        label: String,
        listing_tag: String,
        sidecar_path: PathBuf,
    },
    /// A sidecar `[[callout]]` entry's `line` value points at a source
    /// line that the strip pass removes (because the source line itself
    /// is an inline `// CALLOUT:` marker). The badge would have nowhere
    /// to land in the rendered listing; fail loudly so the author
    /// either re-points the sidecar entry or removes the inline marker.
    SidecarLineOnStrippedMarker {
        label: String,
        listing_tag: String,
        source_line: usize,
        sidecar_path: PathBuf,
    },
}

impl std::fmt::Display for SpliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpliceError::UnknownLabel { label } => write!(
                f,
                "{{{{#callout {label}}}}} references a label that no callout marker defines \
                 in this chapter",
            ),
            SpliceError::LabelCollision {
                label,
                listing_tag,
                sidecar_path,
            } => write!(
                f,
                "label `{label}` is defined by both an inline `// CALLOUT:` marker in \
                 listing `{listing_tag}` and a `[[callout]]` entry in sidecar {}",
                sidecar_path.display(),
            ),
            SpliceError::SidecarLineOnStrippedMarker {
                label,
                listing_tag,
                source_line,
                sidecar_path,
            } => write!(
                f,
                "sidecar entry `{label}` (in {}) points at line {source_line} of listing \
                 `{listing_tag}`, but that line is an inline `// CALLOUT:` marker that gets \
                 stripped from the rendered listing — re-point the sidecar entry at a \
                 non-marker line or remove the inline marker",
                sidecar_path.display(),
            ),
        }
    }
}

impl std::error::Error for SpliceError {}

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
}

/// In-memory map of `tag -> sidecar callouts`. Built once per chapter
/// pass from `<src>/listings/*.callouts.toml`; passed into
/// [`splice_chapter`] so the splicer can merge sidecar entries with
/// inline markers per matching `<div data-listing-tag>` block.
#[derive(Debug, Default)]
pub struct SidecarCallouts {
    /// Tag → (sidecar-file path, parsed callouts). The path is retained
    /// for diagnostic messages on label collisions.
    by_tag: HashMap<String, (PathBuf, Vec<Callout>)>,
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
            let callouts: Vec<Callout> = parsed
                .callouts
                .into_iter()
                .map(|e| Callout {
                    line: e.line,
                    label: e.label,
                    body: e
                        .body
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    options: HashMap::new(),
                })
                .collect();
            by_tag.insert(tag.to_string(), (path, callouts));
        }
        Ok(Self { by_tag })
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
    fn path_for_tag(&self, tag: &str) -> Option<&Path> {
        self.by_tag.get(tag).map(|(p, _)| p.as_path())
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
        }
    }
}

impl std::error::Error for SidecarLoadError {}

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
pub fn splice_chapter(
    content: &str,
    renderer: SupportedRenderer,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    let label_to_ordinal = collect_first_occurrence_ordinals(content, sidecars)?;
    let with_lists = splice_callout_lists(content, &label_to_ordinal, renderer, sidecars)?;
    replace_callout_refs(&with_lists, &label_to_ordinal, renderer)
}

/// Records each label's ordinal at its FIRST occurrence in the chapter.
/// Subsequent occurrences (e.g. when the same source file is shown via
/// `{{#diff}}` after being `{{#include}}`'d) are ignored: the first dt
/// gets `id="callout-<label>"` and acts as the canonical anchor target;
/// later dts render the badge but no `id` so the HTML stays valid.
fn collect_first_occurrence_ordinals(
    content: &str,
    sidecars: &SidecarCallouts,
) -> Result<HashMap<String, usize>, SpliceError> {
    let mut map = HashMap::new();
    let mut error: Option<SpliceError> = None;
    for_each_fenced_block_with_span(content, |info, block_text, _body_start, close_end| {
        if error.is_some() {
            return;
        }
        match split_callouts_for_block(info, block_text, content, close_end, sidecars) {
            Ok((inline, sidecar)) => {
                // Ordinal pass uses block-encounter order: inline by
                // source position (already sorted), then sidecar by
                // source line (sorted). Stable across render + ordinal
                // because the render path sorts by post-strip line,
                // which preserves source order when the source-line→
                // post-strip translation is monotone (which it is —
                // shift count only ever grows).
                let mut merged = inline;
                merged.extend(sidecar);
                merged.sort_by_key(|c| c.line);
                for (idx, c) in merged.iter().enumerate() {
                    map.entry(c.label.clone()).or_insert(idx + 1);
                }
            }
            Err(e) => error = Some(e),
        }
    });
    if let Some(e) = error {
        return Err(e);
    }
    Ok(map)
}

fn splice_callout_lists(
    content: &str,
    label_to_ordinal: &HashMap<String, usize>,
    renderer: SupportedRenderer,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    match renderer {
        SupportedRenderer::Html => splice_callout_lists_html(content, sidecars),
        SupportedRenderer::TypstPdf => {
            splice_callout_lists_pdf(content, label_to_ordinal, sidecars)
        }
    }
}

/// HTML splicer: for non-diff fenced blocks with markers, strip the marker
/// comment lines from the rendered listing and append a sibling
/// `<div class="callout-overlay">` carrying one interactive `<button>` +
/// hover-popover `<div>` per marker, each tagged with the post-strip
/// `data-callout-line` so CSS can position it on the line that previously
/// held the marker. Diff fences pass through unchanged — diffs show
/// history, the canonical anchor lives on the include's badge.
fn splice_callout_lists_html(
    content: &str,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut emitted_anchor: HashSet<String> = HashSet::new();
    let mut error: Option<SpliceError> = None;
    for_each_fenced_block_with_span(content, |info, block_text, body_start, close_end| {
        if error.is_some() {
            return;
        }
        let (inline, sidecar) =
            match split_callouts_for_block(info, block_text, content, close_end, sidecars) {
                Ok(c) => c,
                Err(e) => {
                    error = Some(e);
                    return;
                }
            };
        let is_diff = info == "diff";
        // Diff blocks always go through the strip pass even when no `+`/` `
        // callouts exist — `-`-side markers still need to be dropped from
        // the rendered body.
        if inline.is_empty() && sidecar.is_empty() && !is_diff {
            return;
        }
        let strip = if is_diff {
            strip_marker_lines_diff(block_text)
        } else {
            strip_marker_lines(block_text, info)
        };
        if is_diff && inline.is_empty() && sidecar.is_empty() && strip.body == block_text {
            // No-op diff: no markers of any kind to rewrite.
            return;
        }
        // Pair each inline callout with its already-computed post-strip
        // line, then add each sidecar callout. Sidecar lines are
        // SOURCE-file lines; translate via the anchor's range info
        // (if any) into block_text lines, then strip-aware translate
        // into post-strip lines. Sort by post-strip position so badges
        // emit in visual reading order.
        let mut positioned: Vec<(Callout, usize)> = inline
            .into_iter()
            .zip(strip.post_strip_lines.iter().copied())
            .collect();
        let anchor = listing_anchor_after_fence(content, close_end);
        let sidecar_path = anchor.as_ref().and_then(|a| sidecars.path_for_tag(a.tag));
        for entry in sidecar {
            let source_line = entry.line;
            let label = entry.label.clone();
            let block_line = match anchor.as_ref() {
                Some(a) => source_line_to_block_line(source_line, a),
                None => source_line,
            };
            match translate_sidecar_line_to_post_strip(
                block_line,
                &strip.stripped_source_lines,
                anchor.as_ref().map(|a| a.tag).unwrap_or(""),
                sidecar_path,
                &label,
                source_line,
            ) {
                Ok(p) => positioned.push((entry, p)),
                Err(e) => {
                    error = Some(e);
                    return;
                }
            }
        }
        positioned.sort_by_key(|(_, p)| *p);
        let (callouts, post_strip_lines): (Vec<_>, Vec<_>) = positioned.into_iter().unzip();
        let pre_fence = &content[cursor..body_start];
        let close_fence_line = closing_fence_text(content, close_end);
        out.push_str(pre_fence);
        out.push_str(&strip.body);
        if !strip.body.is_empty() && !strip.body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(close_fence_line);
        out.push('\n');
        out.push_str(&render_callout_overlay_html(
            &callouts,
            &post_strip_lines,
            strip.total_lines,
            &mut emitted_anchor,
        ));
        out.push('\n');
        cursor = close_end;
    });
    if let Some(e) = error {
        return Err(e);
    }
    out.push_str(&content[cursor..]);
    Ok(out)
}

/// Result of one `strip_marker_lines*` pass.
#[derive(Debug)]
struct StripResult {
    /// Block text with inline marker lines removed.
    body: String,
    /// Post-strip 1-based line where each inline marker's badge now lands
    /// (one entry per inline marker, in source-encounter order).
    post_strip_lines: Vec<usize>,
    /// 1-based source line of each stripped marker (one entry per inline
    /// marker, in source order). Used by the sidecar-merge step to
    /// translate sidecar source lines into post-strip positions.
    stripped_source_lines: Vec<usize>,
    /// Total visible lines in `body` (used for overlay sizing).
    total_lines: usize,
}

/// Compute the rewritten block body (marker lines removed) plus the
/// metadata the overlay renderer + sidecar-merge step both need.
fn strip_marker_lines(block_text: &str, info: &str) -> StripResult {
    let prefix = comment_prefix_for_language(info);
    let lines: Vec<&str> = block_text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(block_text.len());
    let mut post_strip_lines: Vec<usize> = Vec::new();
    let mut stripped_source_lines: Vec<usize> = Vec::new();
    let mut emitted_count: usize = 0;
    for (idx, raw_line) in lines.iter().enumerate() {
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
            stripped_source_lines.push(idx + 1);
        } else {
            out.push_str(raw_line);
            emitted_count += 1;
        }
    }
    StripResult {
        body: out,
        post_strip_lines,
        stripped_source_lines,
        total_lines: emitted_count,
    }
}

// CALLOUT: strip-diff Diff-aware strip: drop `-`-prefixed marker lines entirely (no badge — the callout is gone in the new state); strip `+`-prefixed and ` `-prefixed marker lines and record post-strip positions so badges land on the line that previously held them in the diff's right-hand side.
fn strip_marker_lines_diff(block_text: &str) -> StripResult {
    let lines: Vec<&str> = block_text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(block_text.len());
    let mut post_strip_lines: Vec<usize> = Vec::new();
    let mut stripped_source_lines: Vec<usize> = Vec::new();
    let mut emitted_count: usize = 0;
    for (idx, raw_line) in lines.iter().enumerate() {
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
                stripped_source_lines.push(idx + 1);
            }
        } else {
            out.push_str(raw_line);
            emitted_count += 1;
        }
    }
    StripResult {
        body: out,
        post_strip_lines,
        stripped_source_lines,
        total_lines: emitted_count,
    }
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
fn splice_callout_lists_pdf(
    content: &str,
    label_to_ordinal: &HashMap<String, usize>,
    sidecars: &SidecarCallouts,
) -> Result<String, SpliceError> {
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut emitted_anchor: HashSet<String> = HashSet::new();
    let mut error: Option<SpliceError> = None;
    for_each_fenced_block_with_span(content, |info, block_text, _body_start, close_end| {
        if error.is_some() {
            return;
        }
        let (inline, sidecar) =
            match split_callouts_for_block(info, block_text, content, close_end, sidecars) {
                Ok(c) => c,
                Err(e) => {
                    error = Some(e);
                    return;
                }
            };
        // PDF path doesn't strip markers (it keeps them visible in the
        // listing), so sidecar entries' source lines are also their
        // post-strip lines — no translation needed. Just merge and
        // sort by line for the blockquote ordering.
        let mut callouts = inline;
        callouts.extend(sidecar);
        callouts.sort_by_key(|c| c.line);
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
    if let Some(e) = error {
        return Err(e);
    }
    out.push_str(&content[cursor..]);
    Ok(out)
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

    let bytes = content.as_bytes();
    // Same shape as the diff/include parsers: count single backticks on
    // the line BEFORE the directive's opening offset; an odd count means
    // the directive sits between `…` markers (inline code span) and is a
    // documentation example, not a real cross-ref.
    let in_inline_backticks = |pos: usize| {
        let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        bytes[line_start..pos]
            .iter()
            .filter(|&&b| b == b'`')
            .count()
            % 2
            == 1
    };
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(rel) = content[cursor..].find(CALLOUT_DIRECTIVE_OPEN) {
        let open_at = cursor + rel;
        if in_fence(open_at) || in_inline_backticks(open_at) {
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

/// Anchor information extracted from a `<div data-listing-tag>` element
/// that the include splicer emits after each `{{#include listings/...}}`
/// expansion. `range_start_source_line` is `Some(N)` when the include
/// was a sliced range starting at source line N — the sidecar `source_line`
/// → block_text-line translation needs to know N and that the include
/// splicer prepends 2 header lines for ranged slices.
#[derive(Debug, PartialEq, Eq)]
struct ListingAnchor<'c> {
    tag: &'c str,
    range_start_source_line: Option<usize>,
}

/// Peek past the closing fence at `close_end` for the include
/// splicer's `<div data-listing-tag="<tag>"[ data-listing-tag-range="A:B"]...>`
/// anchor. Returns `None` when no anchor is present. Tolerates one
/// trailing newline between the fence and the anchor (the include
/// splicer emits exactly one).
fn listing_anchor_after_fence<'c>(content: &'c str, close_end: usize) -> Option<ListingAnchor<'c>> {
    let tail = &content[close_end..];
    let after_newline = tail.strip_prefix('\n').unwrap_or(tail);
    let anchor_open = after_newline.find("<div data-listing-tag=\"")?;
    if anchor_open > 64 {
        return None;
    }
    let value_start = anchor_open + "<div data-listing-tag=\"".len();
    let value_end = after_newline[value_start..].find('"')?;
    let tag = &after_newline[value_start..value_start + value_end];
    // Look for an optional `data-listing-tag-range="A:B"` attribute on the
    // same anchor element. The full element fits on one line, so cap the
    // search at the closing `>` of the `<div>`.
    let div_end = after_newline[anchor_open..]
        .find('>')
        .map(|i| anchor_open + i)
        .unwrap_or(after_newline.len());
    let div_text = &after_newline[anchor_open..div_end];
    let range_start_source_line = div_text
        .find("data-listing-tag-range=\"")
        .and_then(|r_open| {
            let r_value_start = r_open + "data-listing-tag-range=\"".len();
            let r_value_end = div_text[r_value_start..].find('"')?;
            let r_value = &div_text[r_value_start..r_value_start + r_value_end];
            // Range render shape is `<start>:<end>` or `<start>:` —
            // parse the start integer; ignore the rest.
            r_value.split(':').next()?.parse::<usize>().ok()
        });
    Some(ListingAnchor {
        tag,
        range_start_source_line,
    })
}

/// Back-compat shim for the ordinal pass + tests that only need the tag.
fn listing_tag_after_fence(content: &str, close_end: usize) -> Option<&str> {
    listing_anchor_after_fence(content, close_end).map(|a| a.tag)
}

/// Number of header lines the include splicer prepends to a ranged
/// `{{#include listings/...}}` expansion. The header is `<basename>\n@@
/// start,end @@\n` — exactly 2 lines, both commented when the source's
/// extension maps to a known single-line comment prefix.
const RANGED_INCLUDE_HEADER_LINES: usize = 2;

/// Translate a sidecar entry's source-file line into the corresponding
/// 1-based line within the rendered fenced block (`block_text`). For a
/// full-file include (`anchor.range_start_source_line` is `None`) source
/// line N is at block_text line N. For a ranged include starting at
/// source line S, source line N is at block_text line
/// (N - S + 1) + 2 (the 2 prepended header lines).
fn source_line_to_block_line(source_line: usize, anchor: &ListingAnchor<'_>) -> usize {
    match anchor.range_start_source_line {
        None => source_line,
        Some(start) => (source_line.saturating_sub(start).saturating_add(1))
            .saturating_add(RANGED_INCLUDE_HEADER_LINES),
    }
}

/// Split inline-marker callouts from sidecar callouts for a given block.
/// Returns `(inline, sidecar)` so the render path can keep their
/// post-strip line bookkeeping separate (inline have their post-strip
/// line in [`StripResult::post_strip_lines`]; sidecar lines need
/// translation from source-line to post-strip via
/// [`StripResult::stripped_source_lines`]).
///
/// Errors on cross-source label collisions (same label both inline AND
/// sidecar) — silently shadowing one would hide a rendered badge.
fn split_callouts_for_block(
    info: &str,
    block_text: &str,
    content: &str,
    close_end: usize,
    sidecars: &SidecarCallouts,
) -> Result<(Vec<Callout>, Vec<Callout>), SpliceError> {
    let inline = callouts_for_block(info, block_text);
    let Some(tag) = listing_tag_after_fence(content, close_end) else {
        return Ok((inline, Vec::new()));
    };
    let sidecar = sidecars.for_tag(tag);
    if sidecar.is_empty() {
        return Ok((inline, Vec::new()));
    }
    let inline_labels: HashSet<&str> = inline.iter().map(|c| c.label.as_str()).collect();
    for entry in sidecar {
        if inline_labels.contains(entry.label.as_str()) {
            return Err(SpliceError::LabelCollision {
                label: entry.label.clone(),
                listing_tag: tag.to_string(),
                sidecar_path: sidecars
                    .path_for_tag(tag)
                    .map(Path::to_path_buf)
                    .unwrap_or_default(),
            });
        }
    }
    Ok((inline, sidecar.to_vec()))
}

/// Translate a sidecar entry's block-text line (already mapped from
/// source file via [`source_line_to_block_line`]) into the post-strip
/// line where its badge should appear. The shift equals the number of
/// inline marker lines stripped at-or-before the block line; if the
/// block line itself is in [`StripResult::stripped_source_lines`]
/// (i.e. the author pointed the sidecar at a line that the strip pass
/// removed), returns `Err`. `source_line_reported` is the original
/// source-file line the author wrote, used in error messages so the
/// diagnostic points at what the author actually typed.
fn translate_sidecar_line_to_post_strip(
    block_line: usize,
    stripped_source_lines: &[usize],
    tag: &str,
    sidecar_path: Option<&Path>,
    label: &str,
    source_line_reported: usize,
) -> Result<usize, SpliceError> {
    if stripped_source_lines.contains(&block_line) {
        return Err(SpliceError::SidecarLineOnStrippedMarker {
            label: label.to_string(),
            listing_tag: tag.to_string(),
            source_line: source_line_reported,
            sidecar_path: sidecar_path.map(Path::to_path_buf).unwrap_or_default(),
        });
    }
    let shift = stripped_source_lines
        .iter()
        .filter(|&&s| s < block_line)
        .count();
    Ok(block_line.saturating_sub(shift).max(1))
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
        // CALLOUT: body-id-dedup The button id, body div id, and the button's `aria-describedby` are all dedup'd in lockstep on the first occurrence per label. Without lockstep dedup, the same label appearing in two blocks (e.g. an include and a diff `+` line, both processed for badges in slice 8) would emit duplicate body div ids — invalid HTML, rejected by playwright-rs's strict-mode locator.
        let is_first_occurrence = emitted_anchor.insert(c.label.clone());
        let id_attr = if is_first_occurrence {
            format!(" id=\"callout-{label_esc}\"")
        } else {
            String::new()
        };
        // The body div's `id` and the button's `aria-describedby` are
        // dedup'd identically: only the first occurrence per label gets
        // them. Subsequent occurrences still hover-reveal (CSS uses the
        // adjacent-sibling combinator inside .callout-entry, not the id),
        // but cannot be cross-referenced from prose — by design, since
        // `{{#callout LABEL}}` resolves to the canonical first-occurrence
        // anchor.
        let body_id_attr = if is_first_occurrence {
            format!(" id=\"callout-body-{label_esc}\"")
        } else {
            String::new()
        };
        let aria_describedby_attr = if is_first_occurrence {
            format!(" aria-describedby=\"callout-body-{label_esc}\"")
        } else {
            String::new()
        };
        // Per-callout author override: `--align=left` on the marker
        // surfaces as `data-callout-align="left"`, letting the runtime
        // JS skip its viewport-aware detection and pin the popover left
        // (over the listing) regardless of available right-side gutter.
        let align_attr = match c.options.get("align") {
            Some(value) if value == "left" || value == "right" => {
                format!(" data-callout-align=\"{value}\"")
            }
            _ => String::new(),
        };
        s.push_str(&format!(
            "  <div class=\"callout-entry\" data-callout-line=\"{line}\"{align_attr} \
             style=\"--callout-line: {line}; --callout-listing-lines: {total_lines};\">\n",
        ));
        s.push_str(&format!(
            "    <button type=\"button\" class=\"callout-badge\"{id_attr} \
             data-callout-badge=\"{label_esc}\" data-callout-ordinal=\"{ordinal}\"\
             {aria_describedby_attr}>{ordinal}</button>\n",
        ));
        if let Some(body) = &c.body {
            s.push_str(&format!(
                "    <div class=\"callout-body\"{body_id_attr} role=\"tooltip\">{}</div>\n",
                render_inline_markdown(body),
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

// CALLOUT: html-escape Standard HTML escapes plus `{` → `&#123;` so a callout body that documents a `{{#callout LABEL}}` or `{{#diff a b}}` directive (rendered into the overlay HTML, which sits OUTSIDE its fenced code block) doesn't get its example syntax mistaken for a real directive by the cross-ref scanner downstream. The browser still renders `&#123;&#123;` as `{{` visually.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('{', "&#123;")
}

// Render `body` as inline markdown (backticks → <code>, *em*, **strong**,
// [text](url)) for emission into the callout overlay popover.
fn render_inline_markdown(body: &str) -> String {
    use pulldown_cmark::{Event, Parser, html};
    // CALLOUT: raw-html-neutralisation Callout bodies come from code comments, not trusted markdown — `<script>` in a YAML comment must render as `&lt;script&gt;`, not execute. Remapping every `Event::Html`/`Event::InlineHtml` to `Event::Text` forces raw HTML through pulldown-cmark's text-escaping path.
    let parser = Parser::new(body).map(|event| match event {
        Event::Html(s) | Event::InlineHtml(s) => Event::Text(s),
        other => other,
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, parser);
    let trimmed = rendered.trim_end_matches('\n');
    // CALLOUT: inline-only-output pulldown-cmark wraps inline content in a single `<p>...</p>`. Callout bodies are inline annotations — the synthetic paragraph would break popover layout — so we strip it. Block-level markdown still parses but won't strip cleanly; that's a deliberate cue that the body shape isn't right for the construct.
    let stripped = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
        .unwrap_or(trimmed);
    // CALLOUT: curly-brace-escape pulldown-cmark escapes `&`, `<`, `>`, `"` for text but leaves `{` alone. The cross-ref scanner downstream looks for `{{...}}`; breaking the opening `{{` is enough to neutralise it, matching the pre-slice `html_escape` behaviour (only `{` was ever escaped — `}` always survived).
    stripped.replace('{', "&#123;")
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
                ..Default::default()
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
                ..Default::default()
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
            splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
                .expect("splice"),
            content
        );
    }

    #[test]
    fn splice_chapter_skips_block_with_unknown_language() {
        let content = "```\n# CALLOUT: anchor body text\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
    fn splice_chapter_html_dedups_body_id_when_label_appears_in_two_blocks() {
        // The button id and the body div id are dedup'd in lockstep: the
        // first occurrence per label gets `id="callout-LABEL"` AND
        // `id="callout-body-LABEL"`; subsequent occurrences emit neither.
        // Otherwise the rendered HTML would have duplicate ids and the
        // browser's strict-mode locator would refuse to resolve the body.
        let content = concat!(
            "```rust\n",
            "// CALLOUT: shared-label First body.\n",
            "fn one() {}\n",
            "```\n\n",
            "```rust\n",
            "// CALLOUT: shared-label Second body.\n",
            "fn two() {}\n",
            "```\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let id_count = out.matches("id=\"callout-shared-label\"").count();
        let body_id_count = out.matches("id=\"callout-body-shared-label\"").count();
        assert_eq!(
            id_count, 1,
            "expected exactly one id=\"callout-shared-label\"; got {id_count} in:\n{out}",
        );
        assert_eq!(
            body_id_count, 1,
            "expected exactly one id=\"callout-body-shared-label\"; got {body_id_count} in:\n{out}",
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
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
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
    fn replace_callout_refs_skips_directives_inside_inline_backticks_in_prose() {
        // A chapter that documents the cross-ref syntax in prose like
        // "use `{{#callout LABEL}}` to ..." must not have the example
        // text resolve as a real cross-ref — the inline backticks mark
        // it as a documentation example, mirroring how the diff parser
        // skips directives between `…` on the same line.
        let content =
            "```rust\n// CALLOUT: greeting Hello.\n```\n\nUse `{{#callout LABEL}}` to refer.\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("`{{#callout LABEL}}`"),
            "literal example syntax in inline backticks must survive verbatim; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_html_escapes_curly_braces_in_body_to_protect_cross_ref_scanner() {
        // A callout body that documents the `{{#callout LABEL}}` syntax
        // would, post-overlay-emit, land OUTSIDE its fenced code block
        // — the overlay div is a sibling of the pre. Without escaping,
        // the cross-ref scanner downstream sees the literal directive
        // text and tries to resolve `LABEL`, failing the build.
        let content =
            "```rust\n// CALLOUT: lbl Authors write `{{#callout LABEL}}` to cross-ref.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = out
            .split("<div class=\"callout-body\"")
            .nth(1)
            .unwrap_or("")
            .split("</div>")
            .next()
            .unwrap_or("");
        assert!(
            body.contains("&#123;&#123;#callout LABEL"),
            "expected `{{` escaped to `&#123;` so the cross-ref scanner can't see it; got body:\n{body}",
        );
        assert!(
            !body.contains("{{#callout LABEL"),
            "raw `{{#callout LABEL}}` must not survive into the overlay body; got body:\n{body}",
        );
    }

    #[test]
    fn splice_chapter_html_escapes_label_and_body() {
        let content = "```yaml\n# CALLOUT: lbl Body with <script> in it.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        // Scope the check to the rendered callout-body div, since the
        // overlay is now followed by a measurement <script> emitted by
        // the splicer itself (not user content).
        let body = out
            .split("<div class=\"callout-body\"")
            .nth(1)
            .unwrap_or("")
            .split("</div>")
            .next()
            .unwrap_or("");
        assert!(
            body.contains("&lt;script&gt;"),
            "callout body must escape user-supplied <script>; got:\n{body}",
        );
        assert!(
            !body.contains("<script>"),
            "callout body must not contain raw <script>; got:\n{body}",
        );
    }

    fn extract_callout_body(out: &str) -> &str {
        out.split("<div class=\"callout-body\"")
            .nth(1)
            .unwrap_or("")
            .split("</div>")
            .next()
            .unwrap_or("")
    }

    #[test]
    fn callout_body_renders_inline_backticks_as_code_spans() {
        let content =
            "```rust\n// CALLOUT: lbl Read the `PORT` env var, fall back to `3000`.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<code>PORT</code>") && body.contains("<code>3000</code>"),
            "expected backticks rendered as <code> spans; got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_renders_strong_and_emphasis() {
        let content = "```rust\n// CALLOUT: lbl A **bold** and *italic* note.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<strong>bold</strong>") && body.contains("<em>italic</em>"),
            "expected **/* rendered as <strong>/<em>; got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_renders_inline_link() {
        let content = "```rust\n// CALLOUT: lbl See [docs](https://example.com/).\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<a href=\"https://example.com/\">docs</a>"),
            "expected [text](url) rendered as anchor; got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_curly_brace_escape_survives_inside_code_span() {
        // Authors documenting the `{{#callout LABEL}}` directive will
        // wrap it in backticks for clarity. The inline-markdown render
        // must produce <code>...</code>, AND the `{` escape must still
        // apply inside that code span so the cross-ref scanner downstream
        // (which searches for `{{...}}`) doesn't see a real directive.
        // Only `{` needs escaping — breaking the opening `{{` is
        // sufficient; trailing `}}` survives, matching pre-markdown behaviour.
        let content =
            "```rust\n// CALLOUT: lbl Authors write `{{#callout LABEL}}` to cross-ref.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("<code>&#123;&#123;#callout LABEL}}</code>"),
            "expected `{{` escaped inside <code> (and `}}` left as-is, matching old behaviour); got body:\n{body}",
        );
    }

    #[test]
    fn callout_body_plain_text_passes_through_unchanged() {
        let content = "```rust\n// CALLOUT: lbl Just a plain sentence with no markup.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let body = extract_callout_body(&out);
        assert!(
            body.contains("role=\"tooltip\">Just a plain sentence with no markup."),
            "plain body must follow the opening tag directly (no <p> wrapper); got body:\n{body}",
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        let segment = out.split("data-callout-ref=\"two\"").nth(1).unwrap_or("");
        assert!(
            segment.contains("data-callout-ordinal=\"2\""),
            "ref to `two` should carry ordinal 2; got segment:\n{segment}",
        );
    }

    #[test]
    fn splice_chapter_unknown_callout_label_returns_error() {
        let content = "Unknown ref {{#callout missing}} here.\n";
        let err = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect_err("expected unknown-label error");
        match err {
            SpliceError::UnknownLabel { label } => assert_eq!(label, "missing"),
            other => panic!("expected UnknownLabel, got {other:?}"),
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
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
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
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
        let out = splice_chapter(
            content,
            SupportedRenderer::TypstPdf,
            &SidecarCallouts::empty(),
        )
        .expect("splice");
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
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("{{#callout greeting}}"),
            "literal directive inside code block should pass through; got:\n{out}",
        );
        assert!(
            !out.contains("href=\"#callout-greeting\""),
            "should not have rendered anchor for the in-code-block reference; got:\n{out}",
        );
    }

    // ---------------------------------------------------------------
    // ch.6 slice 4: per-callout `--align` (and other `--key=value`)
    // options after the label, before the body.
    // ---------------------------------------------------------------

    #[test]
    fn parses_align_option_only_no_body() {
        let s = "// CALLOUT: lbl --align=left\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: None,
                options,
            }]
        );
    }

    #[test]
    fn parses_align_option_followed_by_body() {
        let s = "// CALLOUT: lbl --align=left Body text here.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Body text here.".into()),
                options,
            }]
        );
    }

    #[test]
    fn parses_multiple_options_then_body() {
        let s = "// CALLOUT: lbl --align=left --width=20em Body text.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        options.insert("width".into(), "20em".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Body text.".into()),
                options,
            }]
        );
    }

    #[test]
    fn unknown_option_keys_are_preserved_in_options_map() {
        // Forward-compat: a marker that uses a key the renderer doesn't
        // recognise (here `--theme=dark`) is parsed normally; the unknown
        // key sits in `options` for future use and has no rendering effect
        // today. Bodies AFTER the unknown option still parse cleanly.
        let s = "// CALLOUT: lbl --theme=dark Body text.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("theme".into(), "dark".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Body text.".into()),
                options,
            }]
        );
    }

    #[test]
    fn malformed_option_without_equals_is_part_of_body() {
        // `--align` (no `=value`) doesn't match the `--key=value` shape,
        // so it's treated as the start of the body. The grammar stays
        // unambiguous: options are EXACTLY `--key=value` and body is
        // everything from the first non-matching token onward.
        let s = "// CALLOUT: lbl --align Body without an equals.\n";
        let got = parse_callouts(s, "//");
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("--align Body without an equals.".into()),
                options: HashMap::new(),
            }]
        );
    }

    #[test]
    fn double_dash_separator_inside_body_is_preserved() {
        // Once the body has started (first non-option token), any later
        // `--` is part of the body verbatim. Authors writing technical
        // prose like "--no-verify" stay safe.
        let s = "// CALLOUT: lbl --align=left Use --no-verify carefully.\n";
        let got = parse_callouts(s, "//");
        let mut options = HashMap::new();
        options.insert("align".into(), "left".into());
        assert_eq!(
            got,
            vec![Callout {
                line: 1,
                label: "lbl".into(),
                body: Some("Use --no-verify carefully.".into()),
                options,
            }]
        );
    }

    #[test]
    fn render_callout_overlay_html_emits_data_callout_align_when_align_option_set() {
        // The HTML emission side: an `--align=left` option on a callout
        // must surface as a `data-callout-align="left"` attribute on the
        // entry so the runtime JS knows to skip the viewport-aware
        // auto-detection and pin the popover left.
        let content =
            "```yaml\n# CALLOUT: pinned-left --align=left A body that should open left.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains(r#"data-callout-align="left""#),
            "entry must carry data-callout-align=\"left\" when the option is set; got:\n{out}",
        );
    }

    #[test]
    fn render_callout_overlay_html_omits_data_callout_align_when_no_option() {
        // The negative case: a callout WITHOUT --align=... gets no data
        // attribute. The runtime JS then uses viewport-aware detection.
        let content = "```yaml\n# CALLOUT: regular A body with default alignment.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            !out.contains("data-callout-align"),
            "entry must not carry data-callout-align when --align is not set; got:\n{out}",
        );
    }

    fn write_sidecar(dir: &std::path::Path, tag: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.join(format!("{tag}.callouts.toml"));
        std::fs::write(&path, contents).unwrap();
        path
    }

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
    fn splice_chapter_does_not_double_newline_when_rewritten_body_ends_with_newline() {
        let content = "```rust\n// CALLOUT: foo bar.\nlet x = 1;\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        // Find the position right before the closing fence. The
        // rewritten body must end with exactly one `\n`, then `` ``` ``.
        assert!(
            !out.contains("\n\n```\n"),
            "must not emit a blank line between body and closing fence; got:\n{out}",
        );
        assert!(
            out.contains("let x = 1;\n```\n"),
            "expected body line immediately followed by closing fence; got:\n{out}",
        );
    }

    /// When every line is a marker, the rewritten body is empty. The
    /// guard must not emit a stray `\n` between an empty body and the
    /// closing fence.
    #[test]
    fn splice_chapter_does_not_emit_newline_when_rewritten_body_is_empty() {
        let content = "```rust\n// CALLOUT: only-marker body.\n```\n";
        let out = splice_chapter(content, SupportedRenderer::Html, &SidecarCallouts::empty())
            .expect("splice");
        assert!(
            out.contains("```rust\n```\n"),
            "expected fence-open immediately followed by fence-close (empty body); got:\n{out}",
        );
    }

    /// `SpliceError`'s `Display` impl must actually format the variant's
    /// fields — without this assertion the whole body could be replaced
    /// with `Ok(Default::default())` (empty string) and no test would
    /// notice.
    #[test]
    fn splice_error_display_includes_label_and_paths_for_label_collision() {
        let err = SpliceError::LabelCollision {
            label: "duplicate".to_string(),
            listing_tag: "demo-v1".to_string(),
            sidecar_path: std::path::PathBuf::from("/tmp/demo.callouts.toml"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("duplicate"),
            "label missing from display; got: {msg}"
        );
        assert!(
            msg.contains("demo-v1"),
            "listing tag missing from display; got: {msg}"
        );
        assert!(
            msg.contains("/tmp/demo.callouts.toml"),
            "sidecar path missing from display; got: {msg}",
        );
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
    fn listing_tag_after_fence_finds_anchor_immediately_after_fence() {
        let content = concat!(
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"compose-v1\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.find("```\n").unwrap()
            + content[content.find("```\n").unwrap()..]
                .find("```\n")
                .map(|i| i + "```".len())
                .unwrap();
        let close_end = close_end + 1; // include the trailing newline of the close-fence line
        let tag = listing_tag_after_fence(content, close_end);
        assert_eq!(tag, Some("compose-v1"));
    }

    #[test]
    fn listing_tag_after_fence_returns_none_when_no_anchor() {
        let content = "```rust\nlet x = 1;\n```\n\nSome prose.\n";
        let close = content.find("```\n").unwrap() + 4;
        assert_eq!(listing_tag_after_fence(content, close), None);
    }

    #[test]
    fn splice_chapter_merges_sidecar_callouts_into_overlay_for_matching_tag() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 2
label = "sidecar-marker"
body = "Attached without modifying the listing bytes."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        // Listing has no inline markers; the language (css) has no recognised
        // single-line comment syntax in the table, so inline parsing yields
        // nothing — the sidecar is the only source.
        let content = concat!(
            "```css\n",
            ".callout-body { background: white; }\n",
            ".callout-body::after { content: ''; }\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap();
        assert!(
            out.contains(r#"data-callout-badge="sidecar-marker""#),
            "overlay should carry the sidecar badge; got:\n{out}",
        );
    }

    #[test]
    fn splice_chapter_errors_on_label_collision_between_inline_and_sidecar() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 2
label = "duplicate"
body = "Sidecar definition."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = concat!(
            "```rust\n",
            "// CALLOUT: duplicate Inline definition.\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let err = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap_err();
        match err {
            SpliceError::LabelCollision {
                label,
                listing_tag,
                sidecar_path,
            } => {
                assert_eq!(label, "duplicate");
                assert_eq!(listing_tag, "demo-v1");
                assert!(sidecar_path.ends_with("demo-v1.callouts.toml"));
            }
            other => panic!("expected LabelCollision, got {other:?}"),
        }
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
    fn translate_sidecar_line_with_no_strips_is_identity() {
        let result =
            translate_sidecar_line_to_post_strip(5, &[], "demo-v1", None, "label", 5).unwrap();
        assert_eq!(result, 5);
    }

    #[test]
    fn translate_sidecar_line_shifts_by_count_of_stripped_lines_before_it() {
        // Two inline markers stripped at block_text lines 2 and 4.
        // A sidecar callout at block_text line 7 should render at
        // post-strip line 7 - 2 = 5.
        let result =
            translate_sidecar_line_to_post_strip(7, &[2, 4], "demo-v1", None, "label", 7).unwrap();
        assert_eq!(result, 5);
    }

    #[test]
    fn translate_sidecar_line_errors_when_source_line_is_a_stripped_marker() {
        let err = translate_sidecar_line_to_post_strip(3, &[3, 7], "demo-v1", None, "collide", 3)
            .unwrap_err();
        match err {
            SpliceError::SidecarLineOnStrippedMarker {
                label, source_line, ..
            } => {
                assert_eq!(label, "collide");
                assert_eq!(source_line, 3);
            }
            other => panic!("expected SidecarLineOnStrippedMarker, got {other:?}"),
        }
    }

    #[test]
    fn source_line_to_block_line_is_identity_for_full_file_include() {
        let anchor = ListingAnchor {
            tag: "demo-v1",
            range_start_source_line: None,
        };
        assert_eq!(source_line_to_block_line(7, &anchor), 7);
    }

    #[test]
    fn source_line_to_block_line_offsets_for_ranged_include_with_header() {
        // Range starting at source line 28 — block_text layout is:
        //   block 1: `// foo.rs` (header line 1)
        //   block 2: `// @@ 28,50 @@` (header line 2)
        //   block 3: source line 28
        //   block 4: source line 29
        //   ...
        // So source line 32 → block_text line 3 + (32 - 28) = 7.
        let anchor = ListingAnchor {
            tag: "demo-v1",
            range_start_source_line: Some(28),
        };
        assert_eq!(source_line_to_block_line(32, &anchor), 7);
    }

    #[test]
    fn listing_anchor_after_fence_extracts_range_when_present() {
        let content = concat!(
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"foo-v1\" data-listing-tag-range=\"28:50\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.find("```\n").unwrap()
            + content[content.find("```\n").unwrap()..]
                .find("```\n")
                .map(|i| i + "```".len())
                .unwrap()
            + 1;
        let anchor = listing_anchor_after_fence(content, close_end).unwrap();
        assert_eq!(anchor.tag, "foo-v1");
        assert_eq!(anchor.range_start_source_line, Some(28));
    }

    /// Pinned even though the HTML diff path doesn't consume
    /// `stripped_source_lines` today — a future "sidecar on diffs"
    /// extension would, and a wrong recording shape would silently
    /// misplace badges.
    #[test]
    fn strip_marker_lines_diff_records_source_line_numbers_of_stripped_markers() {
        let block_text = concat!(
            "+// CALLOUT: first body.\n",
            "+let x = 1;\n",
            " // CALLOUT: second body.\n",
            " let y = 2;\n",
        );
        let result = strip_marker_lines_diff(block_text);
        assert_eq!(
            result.stripped_source_lines,
            vec![1, 3],
            "expected source lines [1, 3] for the two stripped markers",
        );
    }

    #[test]
    fn listing_anchor_after_fence_accepts_anchor_at_64_byte_offset() {
        let close_to_anchor: String = "x".repeat(64);
        let content = format!(
            "```rust\nlet x = 1;\n```\n{close_to_anchor}<div data-listing-tag=\"demo\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content
            .rfind("```\n")
            .map(|i| i + 4)
            .expect("fence close present");
        let anchor = listing_anchor_after_fence(&content, close_end).unwrap();
        assert_eq!(anchor.tag, "demo");
    }

    #[test]
    fn listing_anchor_after_fence_rejects_anchor_at_65_byte_offset() {
        let close_to_anchor: String = "x".repeat(65);
        let content = format!(
            "```rust\nlet x = 1;\n```\n{close_to_anchor}<div data-listing-tag=\"demo\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content
            .rfind("```\n")
            .map(|i| i + 4)
            .expect("fence close present");
        assert!(listing_anchor_after_fence(&content, close_end).is_none());
    }

    #[test]
    fn listing_anchor_after_fence_range_is_none_for_full_file_include() {
        let content = concat!(
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"foo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let close_end = content.find("```\n").unwrap()
            + content[content.find("```\n").unwrap()..]
                .find("```\n")
                .map(|i| i + "```".len())
                .unwrap()
            + 1;
        let anchor = listing_anchor_after_fence(content, close_end).unwrap();
        assert_eq!(anchor.tag, "foo-v1");
        assert_eq!(anchor.range_start_source_line, None);
    }

    #[test]
    fn splice_chapter_errors_when_sidecar_line_points_at_inline_marker_line() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Inline marker is on source line 1 of the block body.
        // Sidecar points at source line 1 too — would render onto
        // a line the strip pass removes.
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 1
label = "lands-on-stripped"
body = "Boom."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = concat!(
            "```rust\n",
            "// CALLOUT: inline Body.\n",
            "let x = 1;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let err = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap_err();
        match err {
            SpliceError::SidecarLineOnStrippedMarker {
                label,
                source_line,
                listing_tag,
                ..
            } => {
                assert_eq!(label, "lands-on-stripped");
                assert_eq!(source_line, 1);
                assert_eq!(listing_tag, "demo-v1");
            }
            other => panic!("expected SidecarLineOnStrippedMarker, got {other:?}"),
        }
    }

    #[test]
    fn splice_chapter_inline_and_sidecar_callouts_with_distinct_labels_compose_in_line_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_sidecar(
            tmp.path(),
            "demo-v1",
            r#"
[[callout]]
line = 3
label = "from-sidecar"
body = "Attached via sidecar."
"#,
        );
        let sidecars = SidecarCallouts::load(tmp.path()).unwrap();
        let content = concat!(
            "```rust\n",
            "// CALLOUT: from-inline Inline marker.\n",
            "let x = 1;\n",
            "let y = 2;\n",
            "```\n",
            "<div data-listing-tag=\"demo-v1\" aria-hidden=\"true\"></div>\n",
        );
        let out = splice_chapter(content, SupportedRenderer::Html, &sidecars).unwrap();
        // Both badges must render.
        assert!(
            out.contains(r#"data-callout-badge="from-inline""#),
            "got:\n{out}"
        );
        assert!(
            out.contains(r#"data-callout-badge="from-sidecar""#),
            "got:\n{out}"
        );
        // Ordinal 1 is the inline (line 1 before strip; line 1 after); ordinal
        // 2 is the sidecar (line 3). The HTML emits buttons in order;
        // ordinal "1" appears before ordinal "2" textually.
        let inline_pos = out.find(r#"data-callout-badge="from-inline""#).unwrap();
        let sidecar_pos = out.find(r#"data-callout-badge="from-sidecar""#).unwrap();
        assert!(
            inline_pos < sidecar_pos,
            "inline badge (line 1) should render before sidecar badge (line 3); got:\n{out}",
        );
    }
}
