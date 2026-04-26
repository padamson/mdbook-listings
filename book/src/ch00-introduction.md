# Introduction

`mdbook-listings` is a small `mdbook` preprocessor and companion command
line interface (CLI) for authors writing technical prose about code.
The tool and the book you are reading now were co-developed. This book
is `mdbook-listings`'s documentation, the record of the methodology
used to build it, and the tool's primary dog food. Several chapters in this
book describe building an `mdbook-listings` primitive, and the tool is
used to manage the listings in the book,
so a regression in the tool breaks the book build.

A PDF version of this book is available at
[mdbook-listings.pdf](mdbook-listings.pdf).

## How books drift from code

A book quotes code from the project it documents. Months later the
code has been refactored — a function is renamed, a config file gains
keys, an example stops compiling. The book quotes don't change with
the code. A chapter cites a function that no longer exists; a
`{{#include}}` directive picks up different bytes than the surrounding
prose describes; a sample fails to compile against the current
project.

`mdbook-listings` provides four primitives to keep the book coherent
with the code:

1. **Stability** — embed a code listing in the book with a guarantee
   that the rendered output does not change when the source file
   changes. (*Freeze a Listing*, ch. 2.)
2. **Evolution** — show how code evolves across a chapter's slices
   without repeating the full file contents each time. (*Show Diffs
   Between Slices*, ch. 3.)
3. **Annotation** — attach prose to specific lines of an embedded
   listing, and reference those attachment points stably from
   surrounding prose. (*Render Inline Callouts*, ch. 4.)
4. **Synchronization** — warn the author when a listing that is
   *supposed* to track the current source has fallen out of sync.
   (*Verify Sync with Source*, ch. 5.)

Ch. 1 (*Install the Preprocessor*) ships the one-shot onboarding
command an author runs before they can use any of the four
primitives — it's the entry point of the user's journey through the
tool. Everything else that isn't a primitive — small ergonomics,
recipes, troubleshooting — lives in ch. 6.

What's planned beyond what's shipped (v0.2.0 themes onward) lives
in [`ROADMAP.md`](https://github.com/padamson/mdbook-listings/blob/main/ROADMAP.md)
at the repo root, not in this book. The book documents shipped
stories; the roadmap documents intended ones.

## Scope boundaries — things the tool deliberately does not do

Worth stating up front so readers aren't surprised:

- **Replacement of a shipped tag is not a primitive.** Once a frozen
  tag has been referenced by a shipped chapter, its bytes are
  immutable. `--force` exists as an escape hatch for pre-ship
  corrections (typos, missed formatting, accidental debug prints),
  but using it on a tag readers have already seen silently changes
  what they see. If you need different bytes, make a new tag. If
  you need to retire an old tag nobody references anymore,
  hand-edit `listings.toml` and delete the file.
- **The tool does not parse your source code.** Freezing is a
  byte-level copy; diffing is a byte-level diff; CALLOUT markers are
  matched by substring within comment lines, not by a language-aware
  parser. One consequence: a CALLOUT marker inside a multi-line
  string literal in Rust *looks* like a callout and will be
  rendered as one. Write your callouts in comments.
- **The tool does not run your code.** No compile check, no test
  execution, no "does the listing still typecheck." Deep verify
  (eventually — not in the initial release) might add a compile
  check as an opt-in. For now, if your listing's code rots, verify
  catches the byte-level drift but not the semantic drift.

## How the book is organized

Each content chapter is a **user story**: a single slice of value
stated in the first person, from the point of view of a book author
using the tool. A chapter takes the reader through:

1. **The story.** One sentence in user-story form — *as a book
   author I want X so that Y*.
2. **Acceptance criteria (AC).** Statements of the behavior the
   implementation must exhibit for the story to be "done." Each AC is
   verified by one or more tests in the crate's `tests/` directory.
   Each user story has its own integration-test file
   (`tests/<story>.rs`), with shared helpers in `tests/common/mod.rs`,
   so a chapter's frozen test listing focuses on the story it
   documents rather than re-freezing a growing monolith. The tests are
   the executable form; the AC are the specification.
3. **Outside-in narrative.** A sequence of slice-sized sub-sections
   walking through the implementation (see below). Each slice that
   modifies a source file — *including test files* — freezes the
   new state under a fresh `<file>-vN` tag and embeds the listing
   in the sub-section. Even tiny changes (e.g. removing a
   `#[ignore]` attribute) get a new version: predictability beats
   negotiating "is this change substantive enough?" every time, and
   the diff primitive (ch. 3) lets later chapters render compact
   diffs between consecutive tags rather than full files.

   **Until the diff primitive ships**, each slice's narrative must
   explicitly describe in prose *what changed* in the new version
   relative to the previous one. The reader sees two full file
   listings across consecutive sub-sections; the prose tells them
   where to look. After diffs ship, the prose can cede that work
   to a `\{{#diff <prev> <new>}}` block.
4. **Design decisions** *(optional).* The rationale for choices the
   tests and implementation can't show on their own — *why* this
   approach and not the alternatives. Include when the story made
   non-obvious choices that a future maintainer would need to
   reconstruct from scratch; omit when nothing about the story
   needed defending.
5. **Final state** *(optional).* `{{#include}}`s of the frozen
   listings of every file in the slice at their end-of-story state
   — the latest tag per file. Include when the narrative
   sub-sections didn't already show the latest tag for every file
   (which can happen in retrospective chapters or chapters where
   only some slices re-froze). Omit when every file in the slice
   has its latest tag embedded somewhere in the narrative above —
   adding Final state in that case just duplicates the bytes.
6. **What this slice does not solve** *(optional).* The deliberate
   edges of the slice — features the author *knew* they wanted but
   deferred — with forward references to the stories or chores
   that will pick them up. Include when there is a backlog worth
   surfacing; omit when the slice closes the loop on its own.

## Outside-in TDD and slices

Stories are implemented outside-in:

1. **Slice 1 — the failing integration test.** Write an acceptance
   test at the outermost layer (typically a CLI-level test that
   invokes the binary). It fails because the code it needs doesn't
   exist. Commit.
2. **Slices 2..N — inner unit tests, one per piece.** Drop down a
   level. Identify the first piece of code the integration test
   needs. Write a unit test for it (red). Write the minimum code
   that greens it. Commit. Repeat: next piece, another red-then-
   green pair, another commit. Continue until the integration test
   goes green.
3. **Slice N+1 — refactor (optional).** Tidy up while everything is
   green. Commit.

Each slice is its own commit. Commit messages name the slice
(`Show Diffs slice 3/6: unit test + impl for diff_between`). The
chapter's outside-in-narrative section walks through the slice
sequence in reading order, quoting snippets from each.

## Commits and stories are not always the same thing

Most commits ship a slice of a story. Some commits are chores
(scaffolding, dependency bumps, supply-chain exemptions) or narrow
bug fixes that don't belong to any story. Those commits don't get
their own chapter — they live in `git log` where anyone who needs
them can find them. What the chapter count tracks is *stories
shipped*, not commits.

## Early chapters don't fully illustrate the methodology

There's a chicken-and-egg problem at the start of this book. Several
of the tool's features (diffs between slices, inline callouts) are
exactly the features that would make an outside-in narrative compact
— and those features don't exist yet when the stories that build
them are shipped.

Specifically:

- **Ch. 1 (Install the Preprocessor)** was built fully outside-in
  across 8 slices plus a refactor — install doesn't depend on
  either diffs or callouts, so the chapter has no chicken-and-egg
  constraint and serves as the cleanest example of the
  methodology in the book.

- **Ch. 2 (Freeze a Listing)** is reconstructed retrospectively.
  The freeze work landed in a single commit before the book adopted
  this methodology; there is no slice-by-slice sequence to walk
  through.
- **Ch. 3 (Show Diffs Between Slices)** is built outside-in, but it
  can't use the diff primitive in its own narrative because the
  diff primitive is what the story builds.
- **Ch. 4 (Render Inline Callouts)** can use diffs from ch. 3 but
  can't annotate its own code with callouts for the same reason.

From ch. 5 onward every story has both diffs and callouts
available, and the outside-in narrative settles into its compact
shape. Early chapters will be noticeably longer — we quote whole
intermediate file states where later chapters quote diffs.

## Reading in order vs skipping around

Each story depends only on the ones before it — the slices stack.
If you want to rebuild the tool from scratch by following the
chapters, you can; each one leaves the crate in a compiling,
shipping state. If you just want to learn how to use
`mdbook-listings`, skim the **Story** and **Acceptance criteria**
blocks and the **Final state** listings (when the chapter has
one) and ignore the narratives. If you're here for the
methodology, read the narratives and skim the reference content.
