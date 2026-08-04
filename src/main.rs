use std::process;

fn main() {
    if let Err(err) = mdbook_listings::cli::run() {
        eprintln!("error: {err:?}");
        process::exit(1);
    }
}
