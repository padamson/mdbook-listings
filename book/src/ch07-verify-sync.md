# Verify Frozen Listings

```admonish note title="How this chapter was built"
This is the first chapter written with every earlier primitive already
shipped. Its narrative freezes each slice's code, shows evolution with
`\{{#diff}}`, and annotates new modules with sidecar callouts. That's the
methodology the book has been building since ch.2, now self-supporting.
```

## Story

> As a book author, I want a single command that fails my CI when any
> frozen listing in my book is no longer the intact snapshot it claims
> to be, so that readers can trust that the code in the book is real.

## Acceptance criteria

Errors (any one fails the build, non-zero exit):

1. **Every frozen file exists.** A manifest record whose `frozen` path
   no longer resolves to a file is an error naming the tag and path.
2. **Every snapshot is intact.** A frozen file whose bytes no longer
   hash to the `sha256` recorded at freeze time is an error. The snapshot
   was edited or corrupted after freezing.
3. **Every reference resolves.** A `\{{#include listings/…}}` path or a
   `\{{#diff}}` tag operand in chapter prose that names no manifest
   record is an error naming the chapter and line. A `<tag>.callouts.toml`
   sidecar whose `<tag>` names no frozen listing is the same kind of
   broken reference: its annotations would silently attach to nothing and
   the build never complains, so it too is an error.

Warnings (reported, but the build stays green):

4. **Orphans.** A frozen file under `src/listings/` that no manifest
   record claims is flagged for cleanup. Sidecar `*.callouts.toml` files
   are not orphan candidates; their consistency is covered by AC 3.
5. **Stability audit.** Every `live:` operand is listed with its chapter
   and line. These are the places the book is deliberately coupled to
   moving source, and an author should be able to see the list at a
   glance.

Exit contract:

6. Errors produce a non-zero exit (the CI gate). Warnings alone exit 0.
   Either way the command prints a one-line summary of what it checked.

## What verify deliberately does not check

Verify never compares a frozen listing against the *current* content of
its source file. The first sketch of this story did exactly that: "fail
when the latest frozen listing no longer matches the source." It sounds
right until you hold it against what freezing is for. A frozen tag exists
to keep prose stable while the code moves. In a book like this one, where
each chapter freezes versioned snapshots of an evolving codebase, the
latest freeze differs from live source almost all the time, by design. A
check that flagged that difference would fail nearly every build.

The need behind that sketch already has a mechanism. A `live:` operand
shows current source and accepts the instability in trade. So verify
enforces what freezing promises, that each snapshot is still byte-for-byte
what was frozen, and audits what `live:` trades away, without judging
either choice.

## The slice — outside-in narrative outline

| Slice | What it adds |
|---|---|
| 1 | Snapshot integrity (ACs 1, 2, 6). A failing `tests/verify.rs` integration test drives the first real `verify` behavior: a fixture book whose frozen file was edited after freezing must fail with a diagnostic naming the tag and path. `src/verify.rs` re-hashes every frozen file against the manifest's recorded sha256; the CLI handler replaces the `not yet implemented` bail. |
| 2 | Reference resolution and orphans (ACs 3, 4). Chapter markdown is scanned with the shared directive scanner from ch.6 slice 11; `\{{#include listings/…}}` paths and `\{{#diff}}` tag operands must resolve to manifest records. Files in `src/listings/` that no record claims warn. |
| 3 | `live:` stability audit (AC 5) and dogfooding. Every `live:` operand is reported with chapter and line. `verify` is wired into this repo's CI, making this book its first production user; the audit flags ch.4's `live:` diff as the demonstration. |

## Outside-in narrative

Sections appear here as slices ship. All three slices have shipped.

### Slice 1 — snapshot integrity

The failure this slice catches: a frozen listing gets "fixed" in place.
Someone corrects a typo directly in `src/listings/foo-v1.rs` instead of
fixing the source and refreezing, and from that point the book renders
code that was never frozen from anywhere. Nothing in the build notices.
The manifest has carried a `sha256` per listing since ch.3 for this case;
until now, nothing read it back.

Tests first. The fixture freezes a source file through the real `freeze`
subcommand, so the manifest entry and recorded hash are what production
wrote:

```rust
{{#include listings/verify-tests-v1.rs:14:45}}
```

The headline test edits the frozen file after freezing and demands a
failing exit plus a diagnostic naming the tag, the path, and the hash
mismatch:

```rust
{{#include listings/verify-tests-v1.rs:61:81}}
```

Three more tests pin the rest of the contract: an intact book succeeds
with a `1 frozen listing checked` summary, a deleted frozen file is an
error rather than a crash, and a book with two broken listings reports
both. Verify is a report, not a first-failure bail.

The module is small. `verify` loads the manifest and runs the integrity
pass; findings carry a severity that the CLI maps to the exit code
(callout {{#callout severity-split}}). The pass re-hashes each frozen file
with the same helper `freeze` used to record it (callout
{{#callout integrity-check}}):

```rust
{{#include listings/verify-v1.rs:1:83}}
```

The only change to `freeze.rs` is visibility: `hex_sha256` becomes
`pub(crate)` so verify hashes bytes the same way freeze recorded them. One
function, no drift between writer and checker:

{{#diff freeze-v5 freeze-v6}}

The CLI handler replaces the `not yet implemented` bail the subcommand has
carried since it was first added. Findings print to stderr with
`error:`/`warning:` prefixes, the summary to stdout, and any error makes
the exit non-zero for CI to gate on:

{{#diff main-v15 main-v16}}

What a failure looks like:

```text
$ mdbook-listings verify --book-root book
error: frozen listing `compose-v1` no longer matches its recorded sha256: src/listings/compose-v1.yaml (edited after freezing? refreeze or restore the snapshot)
1 frozen listing checked
error: verify found 1 error(s)
```

And a clean run:

```text
$ mdbook-listings verify --book-root book
1 frozen listing checked
```

### Slice 2 — references and orphans

Slice 1 proved each frozen snapshot is intact. Slice 2 asks the next
question: does every *reference* to a listing point at something real,
and is every file in `src/listings/` accounted for? `verify` gains three
purely additive passes (the integrity check is untouched):

```rust
{{#include listings/verify-v2.rs:109:203}}
```

`check_references` (callout {{#callout check-references}}) reuses the
directive scanner from ch.6 slice 11 to walk chapter prose. The include
side resolves the `listings/<tag>` path against the manifest; `snippets/`
paths and `live:` operands are not manifest records, so they're left to
later checks or skipped. Wrong-arity `{{#diff}}` forms are skipped too,
matching what the diff splicer leaves literal, so verify reports what
would actually break and no more.

`check_sidecars` (callout {{#callout check-sidecars}}) answers "what about
the `.callouts.toml` files?" A sidecar attaches annotations to the listing
whose stem it shares; if no such listing exists, those annotations
silently attach to nothing. `check_orphans` (callout
{{#callout check-orphans}}) is the gentler mirror: a frozen file no
manifest record claims is a warning, stray rather than broken.

The tests grew the same way, one case per pass:

{{#diff verify-tests-v1 verify-tests-v2}}

A book with a dangling reference now fails fast:

```text
$ mdbook-listings verify --book-root book
error: src/ch04.md:88: {{#diff}} operand `compose-v9` names no frozen listing
1 frozen listing checked
error: verify found 1 error(s)
```

### Slice 3 — the `live:` audit, and verify finds drift in its own book

The last pass closes AC 5. A `live:` diff operand (ch.4) renders current
source instead of a frozen snapshot — a deliberate trade of stability for
currency. `check_live_operands` (callout {{#callout live-audit}}) reports
each one with chapter and line, a warning rather than an error:

```rust
{{#include listings/verify-v3.rs:211:235}}
```

With every pass in place, this book becomes verify's first production
user: `mdbook-listings verify --book-root book` now runs in CI and as a
pre-commit hook, the same gate any downstream book would wire up.

The first real run is the point of the whole chapter. It failed — and it
was *right* to:

```text
$ mdbook-listings verify --book-root book
error: frozen listing `e2e-callouts-v1` no longer matches its recorded sha256: ...
... (10 more) ...
116 frozen listings checked
error: verify found 11 error(s)
```

Eleven frozen snapshots had drifted from their recorded hashes. Not
corruption: each had been edited in place by a legitimate sweep — the
chapter renumber that turned `ch04-…` into `ch05-…` across the e2e
listings, and the `locator!` migration from ch.5's refactor — and none
had been re-frozen, so the integrity records went stale. Confirming that
(the freeze-commit diff for each showed only the deliberate edit), the fix
was to re-seal: recompute each hash from the current bytes. The root cause
got a fix too — the repo's `typos` pre-commit hook was rewriting files
under `src/listings/`, so it's now scoped to leave frozen snapshots alone.

That is the chapter's thesis demonstrated on itself. The book had quietly
stopped being able to prove eleven of its listings were the snapshots it
claimed — and the tool built to catch exactly that caught exactly that.

The slice-3 code delta — the `live:` audit, plus a fix for a false
positive the dogfood surfaced (a `\{{#include listings/<tag>.callouts.toml}}`
that displays a sidecar file is not a listing reference):

{{#diff verify-v2 verify-v3}}

{{#diff verify-tests-v2 verify-tests-v3}}

With the book re-sealed, verify is green — one warning remains, the `live:`
operand ch.4 uses on purpose:

```text
$ mdbook-listings verify --book-root book
warning: src/ch04-show-diffs-between-slices.md:413: {{#diff}} uses a live operand `live:../../src/diff.rs` — shows current source, not a frozen snapshot, so freeze stability is traded away here
116 frozen listings checked
```

## What this story does not solve

verify is shallow: it confirms a snapshot is byte-for-byte what was
frozen, not that the code still compiles or passes its tests. A *deep*
verify — building or running the frozen listings — is a much larger
story for a later release. So is auto-remediation: verify reports, and
the author decides whether to re-seal, re-freeze, or leave drift in
place; it never edits the manifest for you. And it has no notion of a
listing that's *meant* to track current source — that's what `live:` is
for, and verify audits rather than enforces it. A future opt-in
"mirror" mode for reference-style books is sketched in
[ch. 9](ch09-future-work.md).
