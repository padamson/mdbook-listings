# Reading this book

This book is a record of how `mdbook-listings` reached v0.1.0. The crate
kept moving after the prose closed, so these pages are rendered by a newer
preprocessor than the one the chapters describe, and the chapters are left
as the v0.1.0 record rather than rewritten to match.

That gap is visible, and this page explains what you will notice. For the
itemized record of what changed and when, see the project
[CHANGELOG](https://github.com/padamson/mdbook-listings/blob/main/CHANGELOG.md);
for what is planned, [ch.9 (Future Work)](ch09-future-work.md).

## Features switched on that no chapter teaches

The book turns on every listing feature that postdates it, so you are
reading v0.1.0 prose through a v0.2.0-and-later rendering. The captured
screenshots predate all of it.

- **Numbers and scoped badges.** Every listing carries a `Listing N.M`
  label, and callout badges scope to it — a badge reads `5.3.1` rather than
  a bare `1`. The screenshots still show bare ordinals, and no chapter
  mentions numbering at all.
- **Captions and stable cross-references.** A listing can carry a caption,
  and prose can point at one by a label that resolves to its *current*
  number. That is live here: see {{#listing-ref freeze-acceptance-tests}},
  which resolves into chapter 3 and stays correct when numbers shift.
- **A book-wide index.** The [List of Listings](listings-index.md) collects
  every numbered listing, and the sidebar carries the same index for
  whichever page you are on.
- **Provenance.** Each listing names the file it was frozen from and the tag
  it was frozen under — `../src/main.rs (main-v1)`. This book demonstrates
  the cost of not having it better than any argument could: it freezes
  `../src/main.rs` sixteen times and `../tests/e2e_callouts.rs` twelve, so
  the chapters still spend prose identifying listings that now identify
  themselves.

## Pointers the chapters make that have moved

Chapters 4, 5 and 6 describe wrap-up steps taken against a `ROADMAP.md` at
the repository root. That file existed while the book was being written and
those steps were really carried out; its planned work has since been folded
into [ch.9 (Future Work)](ch09-future-work.md), and what had shipped moved
to the CHANGELOG. The chapters keep the original wording, because they are a
record of what was done rather than instructions to follow.

## The one page that uses the newer include form

Every chapter wraps its includes in a ` ```rust ` fence, because until
v0.2.0 the build failed without one. `{{#diff}}` never had that
requirement, and nothing in the syntax said which directive wanted the
wrapper — a downstream author hit the asymmetry and reported it.

An include on a line of its own now renders the whole block. This page is
the only place in the book that uses the new form: what follows is a bare
`\{{#include snippets/render-block-snippet-v1.rs}}` with no fence around
it, showing the code that does the work.

{{#include snippets/render-block-snippet-v1.rs}}

Both forms produce byte-identical output, so the fenced includes throughout
these chapters render exactly as they always did, and there is nothing to
migrate in an existing book. The fence a directive emits is now sized to
what it wraps — before that, a diff of two Markdown listings broke out of
its own block at the first ` ``` ` inside it — which is what
{{#callout fence-line-initial}} computes.
