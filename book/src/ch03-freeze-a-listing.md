# Freeze a Listing

```admonish note title="This chapter is reconstructed retrospectively"
The Freeze a Listing story landed in a single commit before this book
adopted outside-in TDD as its development discipline. As a result,
this chapter shows the story's **end state** — story, acceptance
criteria, final listings, design decisions — without an outside-in
narrative walking through slices. Chapters from ch. 2 onward have
one narrative section per slice.
```

## Story

> As a book author, I want to freeze a source file into my book under
> a memorable tag so that a later edit to the source file does not
> silently change what my chapter renders.

## Acceptance criteria

1. When an author freezes a source file under a tag, the file's
   bytes are preserved verbatim and become consumable from any
   chapter by that tag, using mdbook's existing include machinery
   (no additional wiring required).
2. Each freeze is recorded persistently. The record captures the
   chosen tag, the original source path, the frozen location, and
   an integrity hash of the frozen bytes.
3. Re-freezing the same source under the same tag with no change
   to its bytes does not modify disk state and confirms to the
   author that nothing changed.
4. Re-freezing under a tag that already exists, but with different
   bytes — whether the original source was edited or a different
   source was supplied — is rejected. No disk state changes.
5. The author can opt in to overwriting an existing tag's frozen
   copy with new bytes. Doing so updates the record and the frozen
   content, and confirms the replacement to the author.
6. Tags that could escape the listings area (e.g. containing path
   separators or relative-path segments) are rejected before any
   disk state changes.

Acceptance criteria 3, 4, and 5 together are the "idempotency
discipline" that keeps this tool honest: the tag is the identity,
the bytes are the content, and the author has to be explicit when
they want to break the association.

## The slice

The slice cuts top-to-bottom through the crate:

| File | Role | Frozen tag |
|---|---|---|
| `tests/freeze.rs` | Acceptance criteria as CLI-level tests | `freeze-tests-v1` |
| `src/main.rs` | `clap` subcommand handler — the CLI adapter | `main-v1` |
| `src/freeze.rs` | Core logic: hash, decide, write, upsert | `freeze-v1` |
| `src/manifest.rs` | TOML load / save / upsert | `manifest-v1` |
| `src/lib.rs` | Public module registrations | `lib-v1` |

Every file contributing to the slice is frozen as of this commit and
embedded below. When later stories edit these files, they freeze a
new tag (`freeze-v2`, etc.) and this chapter keeps pointing at `-v1`.

## Design decisions

Four decisions shape the behaviour of `freeze`. They are called out
explicitly here so that later slices that extend `freeze` know which
invariants they are allowed to break and which they are not.

### Manifest format: TOML

The manifest is TOML, matching mdbook's own `book.toml` and Cargo's
`Cargo.toml`/`Cargo.lock`. The alternatives would have been YAML
(more concise for nested structures but breaks ecosystem fit) or
JSON (nicer for machines, worse for humans to hand-edit). TOML wins
on ecosystem coherence.

### Listing identifier: author-chosen tag plus content hash

Listings are named by an author-supplied tag (`manifest-v1`,
`freeze-v1`, etc.) — the *human* identifier. The manifest also
stores a SHA-256 of the frozen bytes, which is the *integrity*
identifier. This is the pattern Git uses for refs and commits:
humans remember the name, machines verify the hash.

Pure content-addressed identifiers (name the file by its SHA) were
rejected because `\{{#include listings/a1b2c3d4.rs}}` is unreadable.
Auto-derived identifiers (chapter + section + index) were rejected
because inserting or reordering a section silently renumbers
everything.

### Frozen directory layout: `<book-root>/src/listings/<tag>.<ext>`

Frozen files live under `src/listings/` inside the book so the
built-in `\{{#include}}` resolver finds them without any path
gymnastics. The extension is inherited from the source so syntax
highlighting works automatically.

### Freeze trigger: manual CLI only (for now)

`mdbook-listings freeze` is the only way a listing gets frozen.
Pre-commit hooks and tag-triggered freezes are on the backlog but
deferred — they introduce policy questions (whose tag? which
commit?) that are not worth answering until later stories reveal
which of those policies authors actually want.

## Final state

The four source files and the integration test, as of this
commit, all frozen.

### `tests/freeze.rs` — the acceptance criteria as tests

```rust
{{#include listings/freeze-tests-v1.rs}}
```

The `freeze_rejects_conflicting_content_without_force` and
`freeze_rejects_duplicate_tag_from_different_source` tests together
pin the two halves of **AC 4**, which is the one criterion the book
itself can't exercise (because the book only drives the happy
paths).

### `src/main.rs` — the CLI adapter

```rust
{{#include listings/main-v1.rs}}
```

The no-subcommand arm (`preprocess()`) is a stub that errors — the
preprocessor pipeline belongs to the **Show Diffs Between Slices**
story and isn't implemented yet. Likewise `install` and `verify` are
stubs that will fill in later. The `supports` arm is real and was
shipped by the CLI-scaffolding chore, not this story; it's here
because the dispatch table has to mention every subcommand.

### `src/freeze.rs` — the freeze logic

```rust
{{#include listings/freeze-v1.rs}}
```

`FreezeOutcome` carries the Create / Unchanged / Replaced decision
up to `main.rs` purely so the CLI can print a different verb for
each case. Everything else — the four-way match on
`manifest.find(tag)`, the sha256 comparison, the early return on
`Unchanged` to avoid spurious disk writes — falls directly out of
the acceptance criteria.

`frozen_relative_path` is the guard that rejects tags containing
`/`, `\`, or `.` (**AC 6**). Rejecting these early, before any
disk writes, matters: a tag like `../escape` would otherwise write
outside the listings directory.

### `src/manifest.rs` — the persistence layer

```rust
{{#include listings/manifest-v1.rs}}
```

`upsert` preserves insertion order when replacing an existing
entry. That's invisible in the CLI today but matters for reading
the `listings.toml` diff in code review: a re-freeze of an
existing tag should show up as a sha change on one entry, not as
a reorder of the whole file.

### `src/lib.rs` — public module registrations

```rust
{{#include listings/lib-v1.rs}}
```

Nothing to see here; `lib.rs` exists only so `src/main.rs` and the
integration tests can reach into the crate's modules as
`mdbook_listings::freeze::…`. Every future story will add one
more `pub mod` line.

## What this slice does not solve

* **No drift detection between source and frozen copy.** If any of
  the files above is edited but `mdbook-listings freeze` is not
  re-run, the book keeps showing the old bytes. That's the *feature*
  of freezing, but it also means nothing in the tool warns you that
  your latest refactor didn't make it into the book. This gap is
  closed by the **Verify Sync with Source** story.
* **No way to show evolution within a chapter.** This chapter's
  final-state section prints every file in full. When later chapters
  are built outside-in with many slices, doing the same would make
  them unreadable. The **Show Diffs Between Slices** story closes
  that gap.
* **No inline annotations or cross-references.** The five frozen
  listings above are bare code blocks with no way to attach prose
  to a specific line. The **Render Inline Callouts** story
  addresses this, with YAML first.
* **`--tag` is required.** Running `mdbook-listings freeze
  src/manifest.rs` (no tag) fails today. Auto-derivation of the
  tag from the source filename plus the next available `-v<N>`
  suffix is a planned ergonomic enhancement and will land as a
  small follow-up commit — not as a new story, because the
  behaviour it adds is narrow enough to describe in a single AC
  amendment to this chapter when it ships.
* **No automatic installation here.** Registering the preprocessor
  in `book.toml`, shipping the CSS asset, and adding the
  additional-css entry are the responsibility of ch. 1 (*Install
  the Preprocessor*). Freeze itself doesn't need any of that — it's
  a standalone subcommand that operates on a book directory; the
  preprocessor only matters once you want diffs or callouts.
* **The `-v1` listings here are superseded by ch. 1's freezes.**
  Ch. 1 (*Install the Preprocessor*) shipped *after* this chapter
  in implementation order and modified `src/lib.rs`, `src/main.rs`,
  and added `src/install.rs`. The current state of those files is
  captured by ch. 1's outside-in narrative — `lib-v2` in slice 2,
  `main-v2` in slice 6, and `install-v8` in the Refactor sub-section.
  The original consolidated
  `tests/integration.rs` was split into per-story files in ch. 1's
  first slice; what was here as `integration-tests-v1` is now
  `freeze-tests-v1`. This chapter keeps pointing at the
  freeze-story end-of-story snapshots so it shows the code as it
  actually was when freeze shipped, not the post-install state.
  Until the diff primitive ships, readers reading both chapters
  in sequence see overlapping full-file listings.
