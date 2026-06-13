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

Sections appear here as slices ship. Slice 1 has shipped.

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
