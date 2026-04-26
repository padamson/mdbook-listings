// mdbook-listings — typst template for PDF output.
//
// Replaces mdbook-typst-pdf's default template (which references Noto fonts
// that aren't installed in CI or on the user's local environment, producing
// font-substitution warnings and ugly fallbacks). Uses open-source fonts
// installed via apt on Linux/CI and via brew (or already system-installed)
// on macOS, with macOS system-font fallbacks.
//
//   - Body:     Charis SIL → Charter
//   - Headings: Inter      → Avenir Next
//   - Code:     JetBrains Mono → Menlo
//
// MDBOOK_TYPST_PDF_TITLE and MDBOOK_TYPST_PDF_PLACEHOLDER are substitution
// markers mdbook-typst-pdf rewrites at build time with the book's title and
// the rendered chapter content respectively.

#set text(
  lang: "en",
  font: ("Charis SIL", "Charter"),
  size: 11pt,
)

#set par(
  leading: 0.65em,
  first-line-indent: 0pt,
  justify: true,
)

#show heading: set text(font: ("Inter", "Avenir Next"))

#show link: underline

#show raw: set text(font: ("JetBrains Mono", "Menlo"), size: 9pt)

#show raw.where(block: true): block.with(
  width: 100%,
  fill: luma(245),
  inset: 10pt,
  radius: 4pt,
)

#show quote.where(block: true): block.with(
  width: 100%,
  fill: rgb("#f1f6f9"),
  inset: 10pt,
  radius: 4pt,
)

#set page(
  margin: (top: 2.5cm, bottom: 2.5cm, left: 2.5cm, right: 2.5cm),
  header: context {
    if counter(page).get().first() > 2 [
      #set text(size: 9pt, fill: luma(120))
      MDBOOK_TYPST_PDF_TITLE
    ]
  },
  footer: context {
    if counter(page).get().first() > 1 [
      #align(center)[
        #set text(size: 9pt)
        #counter(page).display()
      ]
    ]
  },
)

// Title page
#v(4cm)
#align(center)[
  #text(size: 28pt, weight: "bold", font: ("Inter", "Avenir Next"))[
    MDBOOK_TYPST_PDF_TITLE
  ]
  #v(1cm)
  #text(size: 14pt, fill: luma(100))[
    Managed code listings for mdbook
  ]
  #v(2cm)
  #text(size: 12pt)[Paul Adamson]
]

#pagebreak()

// Table of contents
#text(size: 18pt, weight: "bold", font: ("Inter", "Avenir Next"))[Table of Contents]
#v(0.5cm)
#outline(depth: 2, indent: 1em, title: none)

#pagebreak()

/**** MDBOOK_TYPST_PDF_PLACEHOLDER ****/
