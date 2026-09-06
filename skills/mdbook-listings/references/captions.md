# Writing a listing caption

`{{#include}}` and `{{#diff}}` take `caption="..."`, rendered above the code
as `Listing N.M — caption` and, more consequentially, as the **only** text in
the book-wide List of Listings. [directives.md](directives.md) covers the
mechanics. This file covers what to write.

Because the caption sits above the block, it is read first: its job is to
orient, not to summarize. That is what makes the gerund head the default.

## The target

| property | target |
|---|---|
| length | 6–16 words, 11 typical; never past 20 |
| head word | gerund by default; `A`/`An`/`The` + noun for captured output |
| case | sentence case |
| terminal period | none, unless the caption is a real sentence |
| backticked identifiers | one or two |
| positional wording | none |
| filename in caption text | none |
| artifact name / version | only when the listing shows no tag or filename |
| number ⟺ caption | both or neither |

## Rules

1. **Write a phrase, not a sentence.** Default to a gerund naming what the
   listing does: `Adding a unit slot to QuantityValue`.

2. **The artifact type picks the head.** Source you author takes a gerund.
   Captured output takes a determiner plus a noun phrase naming what
   produced it: `The output from running panschema verify`, `Validation
   errors from the invalid fixture`, `Test results when one test fails`.
   This is the most transferable rule — a transcript is never `Running …`.

3. **Six to sixteen words. Hard-cap twenty.** A caption is a title, not a
   legend.

4. **Sentence case, and no terminal period on a fragment.** Add a period
   only when the caption is a genuine sentence, and be consistent across
   the book either way.

5. **Name one or two identifiers, in code font.** The caption is where a
   reader learns which class or slot the listing is about.

6. **Never positional.** Not "the schema below", not "the following
   fragment" — it breaks under reflow, print, and screen readers. Reference
   other listings by number via `{{#listing-ref}}`
   ([directives.md](directives.md)), which keeps the number current when
   listings shift.

7. **Cross-reference a numbered listing; don't introduce it with a colon.**
   A numbered listing is named in the prose ("Listing 5.4 adds the unit
   slot"). The colon is the *unnumbered* idiom — it introduces a bare code
   block that the sentence completes. Which one you reach for follows from
   whether you numbered the block, so this rule and rule 14 are one
   decision seen twice.

8. **Don't repeat what the listing's chrome already shows.** When the
   rendered listing displays its tag or filename, the caption must not
   restate the artifact's name or its version — a slot already carries
   both. When it displays neither, the caption is the only place identity
   can live, so lead with the artifact and follow with what this version
   shows. `The combustion benchmark: nine questions and the records their
   answers must reach` becomes `Pairing nine questions with the records
   their answers must reach` once a tag line carries the identity. A path
   is never the identity either way: `The combustion benchmark`, not
   `data/combustion-benchmark.yaml`.

9. **Say what it does, and to what.** Verb plus specific operands.
   `Changing parse_config into Config::new` beats "Refactoring the parser".

10. **Compress the prose; do not echo it.** Restating the introducing
    sentence's idea is right; reusing its wording is not.

11. **Announce a deliberate failure.** The house idiom is `Attempting to
    …`, plus an explicit tail: `; this doesn't compile yet`, `which fails
    --strict`, `that the validator rejects`. A listing that is meant to
    fail must say so in its caption.

12. **A diff names the target of the change, not the mechanism.** The
    verbs in use: `Adding`, `Changing X into Y`, `Updating the signature
    of X to …`, `Extracting X from Y`, `Replacing`, `A revision of Listing
    N that …`. Never "Adding lines 12–18".

13. **Escalate to a sentence only to carry the takeaway.** A full-sentence
    caption states the rule the listing proves while the prose states the
    mechanics: *"Reading from or writing to a mutable static variable is
    unsafe."* Reserve it for listings whose whole reason to exist is to
    establish a constraint.

14. **If it does not deserve a caption, do not number it.** An unnumbered
    block should be completed by the sentence that introduces it. Numbered
    and captioned, or neither — never one without the other.

15. **Test it.** The caption must grammatically complete
    *"Listing N.M shows ___"*.

## Anti-patterns

- **The Classifier** — names the category, not the content. "Schema
  excerpt", "YAML example", "The Sample class".
- **The Deictic** — "the code below".
- **The Filename** — the caption is a path. Identity is the artifact's
  name, not where its bytes live, and a path breaks when files move.
- **The Echo** — reproduces the introducing sentence's wording rather
  than compressing it.
- **The Heading Clone** — restates the nearest section heading. The fix is
  to keep the heading's verb and add the specific operands.
- **The Silent Failure** — a listing that does not compile or does not
  validate, captioned as though it does.
- **The Legend** — the caption swells into a paragraph. Overflow belongs
  in prose or a callout.
- **The Orphan Number** — numbering something you would not name.
- **The Colon Introduction** — "…as shown in the schema below:" ahead of a
  numbered listing. Numbered means cross-referenced; the colon belongs to
  unnumbered blocks.

## Where a book has to choose

The sources genuinely disagree on these. The tool settles none of them, so
match what the book already does rather than importing a rule:

- **Whether to caption code at all.** The Rust Book captions every numbered
  listing; Crafting Interpreters captions and numbers none of its snippets.
- **Leading articles.** Some houses forbid opening with `A`/`An`/`The`;
  others use them freely.
- **Terminal punctuation.** Rule 4 is the majority convention for code, but
  book-wide consistency matters more than which way you go.
- **Self-containment.** Journal figure guidance wants a caption readable
  without the text. A book listing sits in prose read linearly, and a
  caption this short cannot carry a standalone explanation — do not import
  that guidance wholesale.
