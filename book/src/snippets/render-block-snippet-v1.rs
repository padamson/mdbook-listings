/// The fence to wrap `body` in so the body cannot close it: one longer
/// than the longest fence-shaped run already in the body, and never
/// shorter than CommonMark's minimum of 3.
pub(crate) fn fence_for_body(body: &str, char: u8) -> Fence {
    // CALLOUT: fence-line-initial Only line-initial runs count, at most 3 leading spaces in — backticks inside a line can't close a block, so they must not widen the wrapper either.
    let longest_run = body
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if line.len() - trimmed.len() > 3 {
                return 0;
            }
            trimmed.bytes().take_while(|&b| b == char).count()
        })
        .filter(|&run| run >= 3)
        .max()
        .unwrap_or(0);
    Fence {
        char,
        count: longest_run.saturating_add(1).max(3),
    }
}

/// Render `body` as a self-contained fenced block carrying `info` as its
/// opener's info string.
// CALLOUT: render-block-shared Both splicers emit through here, so a listing and a diff of that same listing are wrapped by identical rules.
pub(crate) fn render_block(info: &str, body: &str) -> String {
    let fence = fence_for_body(body, b'`').render();
    // CALLOUT: render-block-normalise Two normalisations the callers used to each do for themselves: escaping `{{` so mdbook's `links` preprocessor leaves directive-shaped bytes alone, and reducing the body to one trailing newline so the closing fence gets its own line.
    let body = body.trim_end_matches('\n').replace("{{", "\\{{");
    format!("{fence}{info}\n{body}\n{fence}\n")
}
