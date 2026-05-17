// Demonstrates the ch.6 slice 4 `--align=left` author override:
// even on a wide viewport (where the JS would normally open the
// popover into the right-side gutter), this callout pins it LEFT.
fn pinned_left_example() {
    // CALLOUT: align-left-demo --align=left Pinned LEFT by the `--align=left` option on the marker — author override beats viewport-aware auto-detection.
    let _ = "the body popover here opens over the listing on the left";
}
