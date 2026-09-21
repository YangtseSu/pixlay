//! S5: canvas-level text layers.
//!
//! Two forms, one mechanism: a free layer and a tiled watermark are the same
//! `TextLayer` with a different mode, and both substitute `{date}` / `{filename}` /
//! `{index}` from what the slot's photo reports.
//!
//! Everything measurable about text depends on the font (line breaks, glyph
//! advances, how half-width punctuation looks), so the measurements live in
//! [`measure`] and are run from a child process whose fontconfig knows only the
//! committed subset font — see [`fonts`].

// `tests/text.rs` is a crate root, so its submodules resolve next to it rather
// than in `tests/text/`; the measurement files live there to keep them out of
// Cargo's test-target auto-discovery.
#[path = "text/fonts.rs"]
mod fonts;
#[path = "text/measure.rs"]
mod measure;

/// Runs the text measurements with the committed test font.
///
/// This test is the only thing a plain `cargo test` executes here: the
/// measurements themselves are `#[ignore]`d, because running them with whatever
/// font the machine happens to have is how a test suite starts lying.
#[test]
fn the_text_measurements_run_with_the_committed_test_font() {
    fonts::run_pinned("measure", measure::MEASUREMENTS);
}
