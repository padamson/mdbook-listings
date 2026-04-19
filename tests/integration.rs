// Template placeholder — replace with integration tests for your crate.
//
// Integration tests live in `tests/` and exercise your crate as an external
// consumer would: only `pub` items are accessible. Each file here is compiled
// as a separate binary.

use my_project::add;

#[test]
fn add_integrates() {
    assert_eq!(add(1, 2), 3);
}
