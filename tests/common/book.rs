//! A tempdir laid out as an mdbook book root, the `[ctx, book]` envelope
//! mdbook hands a preprocessor, and the helpers that run it and read the
//! result. The files that drive the preprocessor through its stdin
//! envelope build their book here; the CLI-driven files (`freeze`,
//! `verify`, `install`, `list`) lay out their own roots for the command
//! under test.

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use mdbook_preprocessor::PreprocessorContext;
use mdbook_preprocessor::book::{Book, BookItem, Chapter, SectionNumber};
use mdbook_preprocessor::config::Config;
use tempfile::TempDir;

use super::mdbook_listings;

/// The name of the one chapter `envelope_with_chapter` builds, for reading
/// it back with `chapter_content`.
pub const CHAPTER: &str = "Chapter";

/// The manifest sha for a fixture listing. Only `verify` reads it, and no
/// test here runs `verify` against a fixture book.
const UNCHECKED_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The `[preprocessor.listings]` table with numbering on.
pub const NUMBERED: &str = "[preprocessor.listings]\nnumber-listings = true\n";

/// Numbering on and the book-wide index on.
pub const NUMBERED_WITH_INDEX: &str =
    "[preprocessor.listings]\nnumber-listings = true\nlist-of-listings = true\n";

/// A book root with `src/listings/` and, once an envelope is built, a
/// `listings.toml` naming what was registered. Alive as long as the struct.
pub struct MinimalBook {
    _tmp: TempDir,
    root: PathBuf,
    registered: Vec<(String, String)>,
}

impl MinimalBook {
    pub fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(root.join("src/listings")).unwrap();
        Self {
            _tmp: tmp,
            root,
            registered: Vec::new(),
        }
    }

    /// Two frozen listings, `sample` and `claim`, for pages to number,
    /// index and refer to.
    pub fn with_sample_and_claim() -> Self {
        Self::new()
            .with_listing("sample", "sample.rs", b"fn sample_body() {}\n")
            .with_listing("claim", "claim.rs", b"fn claim_body() {}\n")
    }

    /// Register `tag` for `src/listings/<file>` in the manifest. The bytes
    /// come later, from `write_listing`, so a test can vary them.
    pub fn registered(mut self, tag: &str, file: &str) -> Self {
        self.registered.push((tag.to_string(), file.to_string()));
        self
    }

    /// Register `tag` and write its frozen bytes in one step.
    pub fn with_listing(self, tag: &str, file: &str, bytes: &[u8]) -> Self {
        let book = self.registered(tag, file);
        book.write_listing(file, bytes);
        book
    }

    /// A file under `src/listings/`, registered or not.
    pub fn write_listing(&self, file: &str, bytes: &[u8]) {
        fs::write(self.root.join("src/listings").join(file), bytes).unwrap();
    }

    /// A file under `src/snippets/`, for `{{#include snippets/<file>}}`.
    pub fn write_snippet(&self, file: &str, content: &str) {
        let dir = self.root.join("src/snippets");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file), content).unwrap();
    }

    /// A file at `rel` under the book root. A `live:` diff operand resolves
    /// from the chapter's directory, so a file written at `src/x.yaml` is
    /// `live:x.yaml` in the chapter.
    pub fn write_file(&self, rel: &str, bytes: &[u8]) {
        let abs = self.root.join(rel);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&abs, bytes).unwrap();
    }

    /// The envelope for one chapter named [`CHAPTER`] at `chapter.md`, with
    /// the default preprocessor config.
    pub fn envelope_with_chapter(&self, content: &str) -> String {
        self.envelope(
            "",
            &[Page {
                name: CHAPTER,
                path: "chapter.md",
                number: None,
                content,
            }],
        )
    }

    /// The envelope for `pages` under `config_toml`, the book's `book.toml`
    /// body (empty for the defaults). Writes the manifest, the one thing
    /// the preprocessor reads from disk besides the listings.
    pub fn envelope(&self, config_toml: &str, pages: &[Page]) -> String {
        self.write_manifest();
        let config = Config::from_str(config_toml).expect("parse config");
        let ctx = PreprocessorContext::new(self.root.clone(), config, "html".to_string());
        let book = Book::new_with_items(pages.iter().map(chapter).collect());
        serde_json::to_string(&(&ctx, &book)).expect("serialize envelope")
    }

    fn write_manifest(&self) {
        let mut manifest = String::from("version = 1\n");
        for (tag, file) in &self.registered {
            manifest.push_str(&format!(
                "\n[[listing]]\ntag = \"{tag}\"\nsource = \"../{file}\"\n\
                 frozen = \"src/listings/{file}\"\nsha256 = \"{UNCHECKED_SHA}\"\n"
            ));
        }
        fs::write(self.root.join("listings.toml"), manifest).unwrap();
    }
}

/// One chapter of a multi-page envelope. `number` is the section number
/// mdbook assigns from SUMMARY.md; `None` is an unnumbered page.
pub struct Page<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub number: Option<&'a [u32]>,
    pub content: &'a str,
}

fn chapter(p: &Page) -> BookItem {
    let mut ch = Chapter::new(p.name, p.content.to_string(), p.path, vec![]);
    ch.number = p.number.map(SectionNumber::new);
    BookItem::Chapter(ch)
}

/// Pipes the envelope through the preprocessor binary and returns the
/// transformed `Book` parsed from stdout.
pub fn run_preprocessor(envelope: String) -> Book {
    let output = mdbook_listings()
        .write_stdin(envelope)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("parse stdout as Book")
}

/// The content of the chapter named `name` in a returned book.
pub fn chapter_content(book: &Book, name: &str) -> String {
    book.iter()
        .find_map(|item| match item {
            BookItem::Chapter(ch) if ch.name == name => Some(ch.content.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("chapter `{name}` missing from returned book"))
}
