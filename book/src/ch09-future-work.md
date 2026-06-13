# Future Work

The project's canonical "what's planned" reference is the top-level
[`ROADMAP.md`](https://github.com/padamson/mdbook-listings/blob/main/ROADMAP.md)
file. This chapter holds detailed design sketches for features
the project plans to add — depth that doesn't fit in a roadmap's
one-line bullets, written down while the design is fresh so a
future implementer doesn't have to rediscover it.

When one of these features ships, its sketch leaves this chapter
and reappears as a slice in its parent story chapter or as its
own new chapter.

## PDF inline-badge rendering

HTML callouts render as interactive inline badges on the line
that previously held the marker comment (ch.5 slice 7). PDF
renders the same callouts in a complementary shape: marker
comment visible in the listing + a styled blockquote below
(ch.5 slice 6). A future iteration could match the HTML form in
PDF — strip the marker comment from the PDF listing too, and
render a typst inline-superscript marker on the source line
instead. Bodies stay in the blockquote (no hover popover in
print), each entry keyed by the same ordinal that appears on the
listing-side badge.

The `pdf_callouts` integration test grows assertions for the
inline marker; the existing assertions for blockquote bodies
stay.

## Retrospective application of callouts to earlier chapters

Once sidecar callouts are available, a chore-level pass walks
back through the listings frozen by ch.2 (Install), ch.3
(Freeze), and ch.4 (Show Diffs) and adds callouts to them via
the sidecar form. The point is to demonstrate, in place, how
callouts replace the conventional inline-comment style of code
documentation: the prose lives in the chapter, the labels make
the prose addressable from the source position, and the source
stays comment-light.

This depends on the sidecar form above, since modifying the
already-frozen source listings would defeat the back-catalogue
concept.

## Deeper verification

[ch.7 (Verify Frozen Listings)](ch07-verify-sync.md) ships a *shallow*
verify: it proves each snapshot is byte-for-byte what was frozen, and
that references resolve. Three extensions are sketched but out of scope
for v0.1.0:

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
