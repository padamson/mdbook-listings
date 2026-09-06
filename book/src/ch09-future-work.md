# Future Work

This chapter is the project's canonical "what's planned" reference. The
[CHANGELOG](https://github.com/padamson/mdbook-listings/blob/main/CHANGELOG.md)
records what has shipped; this chapter records what is intended, with
enough design depth that a future implementer doesn't have to rediscover
it.

Releases are grouped by theme rather than by a fixed feature list, and
the versions are emergent: a release happens when its theme has shipped
enough to feel useful, even if not every item below has landed. The
groupings are judgement, not commitment — a v0.3.0 may ship with two of
the items in its section if those land cleanly and the others need more
design.

When one of these ships, it moves to the CHANGELOG and its entry leaves
this chapter, reappearing as a slice in its parent story chapter or as
its own new chapter. New ideas land here by editing this file in a pull
request; substantive shifts (adopting something deferred to v1.0.0 into
the next release, say) get discussed in an issue first.

## v0.3.0 — power-user ergonomics

- `mdbook-listings unfreeze <tag>` for orphan cleanup.
- `verify --prune` for interactive orphan removal.
- Per-chapter tag namespacing under `book/src/listings/<chapter>/`.

### Internals

Carried over from the 2026-06 architecture review; no user-visible
change.

- Pass structured per-chapter listing metadata between pipeline stages
  instead of round-tripping through the `<div data-listing-…>` anchor
  protocol. `src/anchor.rs` centralises the current string protocol;
  this replaces it.
- `thiserror` for the structured error enums, dropping the hand-written
  `Display`/`Error` impls.
- Centralise the escaping policy scattered across `html_escape` and
  `render_inline_markdown` — moot if the structured-metadata item lands
  first. The `{{` escape the include and diff splicers each applied is
  already done: both emit through `fence::render_block`, which owns it.

## v0.4.0 — richer rendering

- Syntax-highlighted diffs (currently plain unified-diff text).
- Multi-paragraph callout bodies, inline code in callouts.
- Callouts overlaid on diff output.

### PDF inline-badge rendering

HTML callouts render as interactive inline badges on the line that
previously held the marker comment (ch.5 slice 7). PDF renders the same
callouts in a complementary shape: marker comment visible in the listing
plus a styled blockquote below (ch.5 slice 6). A future iteration could
match the HTML form in PDF — strip the marker comment from the PDF
listing too, and render a typst inline-superscript marker on the source
line instead. Bodies stay in the blockquote (no hover popover in print),
each entry keyed by the same ordinal that appears on the listing-side
badge.

The `pdf_callouts` integration test grows assertions for the inline
marker; the existing assertions for blockquote bodies stay.

## v0.5.0 — language reach + workflow

- Block-comment-only languages for inline callouts (CSS, plain
  Markdown).
- `mdbook-listings install --hook` writes a pre-commit hook that runs
  `verify` on every commit.
- Watch mode (re-freeze on source change, opt-in).

## v1.0.0 — stability + deep verify

- Manifest schema and preprocessor JSON protocol committed, with a
  compatibility promise across future minors.
- Upgrade flow when the bundled CSS asset bumps versions.
- Detection of conflicting preprocessor configs at install time.
- Uninstall command.

### Deeper verification

[ch.7 (Verify Frozen Listings)](ch07-verify-sync.md) ships a *shallow*
verify: it proves each snapshot is byte-for-byte what was frozen, and
that references resolve. Three extensions are sketched:

- **Deep verify.** Build or run the frozen listings, so verify catches
  a snapshot that is intact but no longer compiles (e.g. against a bumped
  dependency). Much larger — it needs a per-listing toolchain/run
  harness — and belongs in its own story.
- **Re-seal / auto-remediation.** Today verify reports drift and the
  author decides what to do; the one-time cleanup of pre-existing drift
  was a manual sha recompute. A `verify --reseal` (or a `reseal`
  subcommand) would recompute hashes from current bytes after the author
  confirms the edits were deliberate — convenience, never automatic.
- **Opt-in mirror mode.** Verify deliberately does *not* compare a frozen
  snapshot against current source, because freezing exists to decouple
  from a moving codebase. A reference-style book (API docs whose example
  should always match HEAD) wants the opposite. An explicit per-listing
  "tracks current source" flag would let such a book ask verify to fail
  on drift-from-source, without imposing that on the versioning workflow
  this book uses.

## Unscheduled

### Retrospective application of callouts to earlier chapters

A chore-level pass walks back through the listings frozen by ch.2
(Install), ch.3 (Freeze), and ch.4 (Show Diffs) and adds callouts to them
via the sidecar form. The point is to demonstrate, in place, how callouts
replace the conventional inline-comment style of code documentation: the
prose lives in the chapter, the labels make the prose addressable from
the source position, and the source stays comment-light.

The sidecar form it depends on has shipped; modifying the already-frozen
source listings would defeat the back-catalogue concept, which is why it
waited for sidecars.
