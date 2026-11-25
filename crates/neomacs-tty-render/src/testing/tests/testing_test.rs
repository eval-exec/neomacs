//! The fixtures are closed tables (DECISIONS D44): an input that no entry
//! lists panics instead of answering, so every answer a rasterizer test reads
//! is one that the display host's contract tests pin to the evaluator.

use super::*;

/// The neighbour of a listed scalar (U+4E2D): the width table answers by
/// entry, never by range.
const UNLISTED_SCALAR: char = '\u{4e2e}';

const UNLISTED_TEXT: &str = "abc";

#[test]
#[should_panic(expected = "U+4E2E has no evaluator width in the test table")]
fn gnu_char_width_panics_on_a_scalar_the_table_does_not_list() {
    assert!(
        GNU_CHAR_WIDTHS
            .iter()
            .all(|&(listed, _)| listed != UNLISTED_SCALAR),
        "the probe must stay outside GNU_CHAR_WIDTHS"
    );
    gnu_char_width(UNLISTED_SCALAR);
}

#[test]
#[should_panic(expected = "\"abc\" has no terminal composition in the test table")]
fn terminal_composition_panics_on_a_text_the_table_does_not_list() {
    assert!(
        !COMPOSED_TEXTS.contains(&UNLISTED_TEXT),
        "the probe must stay outside COMPOSED_TEXTS"
    );
    terminal_composition(UNLISTED_TEXT);
}
