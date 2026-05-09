# Dogfooding-Driven Polish

```admonish note title="Why this chapter exists"
Chapters 2–5 shipped the v0.1.0 primitives (install, freeze, diff,
callouts). The first real downstream project to take a dependency
on those primitives — the
[t2t](https://github.com/padamson/t2t) book — surfaced a handful of
rendering and ergonomic gaps that the in-house book never exercised
hard enough to notice. This chapter collects the resulting polish
work, one slice per gap. The verify story (ch.7) is still
placeholder; it'll close the v0.1.0 loop separately.

If we identify dogfood, we eat it. New gaps that surface on later
downstream passes get appended as new acceptance criteria and new
slices — there is no "out of scope" exit door.
```

## Story

> As a downstream book author, I want the v0.1.0 primitives to feel
> finished when I write real annotated prose against them — not just
> "the happy path runs to completion," but "the rendered output is
> the output I wrote, and the CLI tells me what I need to know to
> keep going."

## Acceptance criteria

1. **Inline markdown in callout body text.** A callout body that
   contains inline markdown (backticks for code spans, `*emphasis*`,
   `**strong**`, `[text](url)`) renders as the corresponding inline
   HTML in the body popover — not as literal punctuation. Block-level
   markdown (lists, blockquotes, headings) is out of scope: callouts
   are inline annotations. Raw HTML in a callout body renders as
   escaped text, not as pass-through HTML.
2. **Callout popover never covers the line it annotates.** The
   default opens the popover to the right of the badge (the
   un-annotated gutter), an author override switches a specific
   callout to the left for narrow viewports, and a transparent /
   `backdrop-filter: blur` fallback keeps the underlying code legible
   when overlap is unavoidable.
3. **`freeze` output closes the authoring loop.** Every successful
   `freeze` prints the frozen path AND the ready-to-paste
   `\{{#include listings/<tag>.<ext>}}` directive — the author
   shouldn't have to grep `listings.toml` to find the include path.
4. **A `list` (or `status`) subcommand prints `tag → frozen path →
   source` rows** so authors can browse the manifest as a book
   accumulates listings.
5. **`install` is idempotent.** Re-running `install` on an
   already-configured book is a no-op with a friendly "already
   installed" message; never duplicates registrations.
6. **`freeze` derives a default tag when `--tag` is omitted.** The
   default `<basename>-v<next>` removes the "invent your own scheme"
   tax on every first-time author. Already on the v0.2.0 ROADMAP;
   downstream surfaced it as a real pain point, so it lives here.

## The slice — outside-in narrative outline

| Slice | What it adds |
|---|---|
| 1 | Inline markdown in callout body text (AC 1). Downstream dogfooding noticed that backticks around an identifier in a callout body rendered as literal backtick characters rather than a `<code>` span. The fix routes the body through pulldown-cmark's inline parser before wrapping it in the `<div class="callout-body">`, strips the synthetic `<p>` wrapper, and re-applies the `{` → `&#123;` escape for cross-ref-scanner safety. Raw HTML events are remapped to text events so a body containing `<script>` still renders as `&lt;script&gt;`, not as pass-through HTML. |
| 2 | Open the popover to the right by default (AC 2, fix 1 of 3). CSS-only positioning change on the `<div class="callout-body">` so the natural reading direction (left-to-right) drops the popover into the un-annotated gutter rather than over the line it annotates. |
| 3 | Per-callout `--align` override (AC 2, fix 2 of 3). Tiny extension to the `// CALLOUT: <label>` grammar — `// CALLOUT: <label> --align=left <body>` flips a single callout when the right-side gutter isn't usable (sidebar, narrow viewport, badge near the page edge). The extension is shaped to scale to other per-callout options later (width, theme). |
| 4 | Transparent / `backdrop-filter: blur` fallback (AC 2, fix 3 of 3). Pure CSS. When the popover must cover the listing (narrow viewport, author override, very long body), a translucent background + backdrop blur keeps the underlying code legible behind it. |
| 5 | `freeze` output closes the loop (AC 3). Augments the `created: <tag>` line with the frozen path and the exact `\{{#include listings/<tag>.<ext>}}` directive to copy-paste into the chapter. |
| 6 | `mdbook-listings list` subcommand (AC 4). Prints one row per `[[listing]]` in `listings.toml`: tag, frozen path, source path. No filtering options yet — just the basic catalogue view. |
| 7 | `install` idempotency (AC 5). The first run continues to register the preprocessor, write the CSS, and write the JS (idempotent line-by-line per-section already, but the surface message says "installed"). A second run detects all three sections already present and prints "already installed" with no writes. |
| 8 | Default tag derivation (AC 6). When `--tag` is omitted, derive `<basename>-v<next>` by reading existing `[[listing]]` entries for the same source path and bumping the highest `vN` suffix. Surfaces a clean error if any existing tag for the same source doesn't match the `<basename>-vN` shape (the heuristic is opinionated; an author who's invented their own scheme keeps using `--tag` explicitly). |

## Outside-in narrative

Sections appear here as slices ship. Slice 1 is the only one shipped
so far; slices 2–8 are sketched in the table above.

### Slice 1 — inline markdown in callout body text

The symptom: a callout body whose author reached for inline
backticks — say, to call out a name like `PORT` — rendered to the
popover with the literal backtick characters intact instead of a
`<code>` span around the name. Annotated technical prose leans on
inline-code formatting to distinguish identifiers from prose; a
callout body that can't render inline code reads worse than the
surrounding chapter, which defeats the whole point of attaching
context to a specific line.

The diff is between the two frozen snapshots of `src/callout.rs`
that bracket this slice — `callout-v6` (the last freeze, made when
ch.5 wrapped) and `callout-v7` (frozen as part of this slice). It's
the full file diff: there's no freeze between them. Two earlier
commits modified `callout.rs` without refreezing, so their changes
show up here too: the e2e-harness refactor rescoped the
`splice_chapter_html_escapes_label_and_body` test assertion, and
ch.5's slice 9 added the `in_inline_backticks` check near the top
of `replace_callout_refs` plus the `// CALLOUT: html-escape`
comment and `.replace('{', "&#123;")` line on `html_escape`. This
slice's contribution is the call-site swap (line 640 of v7), the
new `render_inline_markdown` function just below `html_escape`,
and the unit tests at the bottom.

{{#diff callout-v6 callout-v7}}

Three details inside `render_inline_markdown` earn their own
callout: {{#callout raw-html-neutralisation}} guards against
untrusted HTML in source comments; {{#callout inline-only-output}}
explains the `<p>` strip and what happens if an author reaches for
block markdown anyway; {{#callout curly-brace-escape}} preserves
the cross-ref-scanner safety property the original `html_escape`
provided.

The PDF path needs no change. `render_callout_list_pdf` interpolates
the body into a markdown blockquote that typst-pdf re-parses, so
markdown in the body has always rendered correctly in print — the
gap was HTML-only.

Tests added in this slice:

- `callout_body_renders_inline_backticks_as_code_spans` — backticks
  → `<code>`.
- `callout_body_renders_strong_and_emphasis` — `**bold**` and
  `*italic*` → `<strong>` and `<em>`.
- `callout_body_renders_inline_link` — `[docs](https://example.com/)`
  → `<a href>`.
- `callout_body_curly_brace_escape_survives_inside_code_span` —
  the cross-ref-scanner safety property holds inside a `<code>` span.
- `callout_body_plain_text_passes_through_unchanged` — the synthetic
  `<p>` wrapper is stripped on plain bodies.
- The pre-existing `splice_chapter_html_escapes_label_and_body`
  guards the raw-HTML neutralisation (it asserts `<script>` →
  `&lt;script&gt;`).

A new e2e assertion in `tests/e2e_callouts.rs` —
`callout_body_renders_inline_backticks_as_code_spans` — closes the
loop end-to-end: it hovers the `snippets-intercept` badge in the
rendered ch.5 HTML and asserts that the popover contains a `<code>`
element with the expected text.

The diff between `e2e-callouts-v8` (last freeze, made when ch.5
wrapped) and `e2e-callouts-v9` (frozen as part of this slice) shows
the new test plus a couple of mechanical changes that came with
this commit's chapter renumbering — `CH04` was renamed to `CH05`
and its value bumped to `"ch05-render-inline-callouts"`. Ch.5
slice 9 also modified this file without refreezing (the
`callout_inside_a_sliced_include_renders_with_resolvable_cross_ref`
and `cross_ref_badges_in_prose_render_with_full_opacity_not_subdued`
tests), so those appear in the diff too.

{{#diff e2e-callouts-v8 e2e-callouts-v9}}
