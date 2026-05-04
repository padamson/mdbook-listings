/// Which renderer the splicer is producing output for. The HTML emitter
/// uses raw <dl> tags so the rendered DOM carries stable
/// data-callout-badge and dt[id] attributes for cross-refs and e2e
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

fn render_callout_list(
    callouts: &[Callout],
    _label_to_ordinal: &HashMap<String, usize>,
    emitted_anchor: &mut HashSet<String>,
    renderer: SupportedRenderer,
) -> String {
    match renderer {
        SupportedRenderer::Html => render_callout_list_html(callouts, emitted_anchor),
        SupportedRenderer::TypstPdf => render_callout_list_pdf(callouts),
    }
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

// CALLOUT: cross-ref-replace Two-pass entry: skips directives inside fenced blocks, errors on labels not in the chapter's collected map.
fn replace_callout_refs(
    content: &str,
    label_to_ordinal: &HashMap<String, usize>,
) -> Result<String, SpliceError> {
    /* ... directive scan + label resolve, omitted for brevity ... */
    Ok(content.to_string())
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
