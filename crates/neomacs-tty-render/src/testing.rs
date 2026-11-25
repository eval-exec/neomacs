//! Evaluator answers for the inputs this crate's tests use.
//!
//! The rasterizer's tests need GNU character widths and terminal
//! compositions, which only the evaluator computes. Each part here is a
//! closed table: a scalar or a string the table does not list panics, so a
//! new test input has to be added here, and the display host's contract
//! tests (`neomacs-display-host`, `tty_render_contract_test.rs`) pin every
//! entry to the evaluator (DECISIONS D44).

use crate::raster_policy::{TextFillEdge, TtyRasterPolicy};
use neomacs_display_protocol::glyph_matrix::{
    TerminalCharWidths, TerminalComposition, TerminalCompositionCell,
};

/// The evaluator's `char_width` for each non-ASCII scalar the tests use,
/// sorted by scalar.
pub const GNU_CHAR_WIDTHS: &[(char, usize)] = &[
    ('\u{e9}', 1),
    ('\u{301}', 0),
    ('\u{302}', 0),
    ('\u{308}', 0),
    ('\u{627}', 1),
    ('\u{628}', 1),
    ('\u{633}', 1),
    ('\u{644}', 1),
    ('\u{651}', 0),
    ('\u{180e}', 0),
    ('\u{1828}', 1),
    ('\u{200c}', 0),
    ('\u{200d}', 0),
    ('\u{200f}', 0),
    ('\u{2014}', 1),
    ('\u{2591}', 1),
    ('\u{25a1}', 1),
    ('\u{4e16}', 2),
    ('\u{4e2d}', 2),
    ('\u{4f60}', 2),
    ('\u{597d}', 2),
    ('\u{65e5}', 2),
    ('\u{672c}', 2),
    ('\u{a938}', 1),
    ('\u{a949}', 0),
    ('\u{a9ae}', 0),
    ('\u{a9b6}', 0),
    ('\u{10d11}', 1),
    ('\u{10d13}', 1),
    ('\u{10d1d}', 1),
    ('\u{10d20}', 1),
    ('\u{10d24}', 1),
    ('\u{1f468}', 2),
    ('\u{1f469}', 2),
    ('\u{1f980}', 2),
];

/// GNU `CHARACTER_WIDTH` (`src/buffer.h:1707-1715`) as the evaluator answers
/// it, with `ctl-arrow` non-nil and a tab width of 8: printable ASCII is one
/// cell, a newline none, a tab eight and any other control character two
/// (`^X`). Any other scalar must be listed in [`GNU_CHAR_WIDTHS`].
///
/// # Panics
///
/// On a non-ASCII scalar the table does not list.
pub fn gnu_char_width(ch: char) -> usize {
    match ch {
        '\t' => 8,
        '\n' => 0,
        ' '..='~' => 1,
        '\0'..='\x7f' => 2,
        _ => match GNU_CHAR_WIDTHS.binary_search_by_key(&ch, |&(listed, _)| listed) {
            Ok(index) => GNU_CHAR_WIDTHS[index].1,
            Err(_) => panic!(
                "U+{:04X} has no evaluator width in the test table; add it to \
                 GNU_CHAR_WIDTHS (the display host's contract test pins it)",
                u32::from(ch)
            ),
        },
    }
}

/// The policy the tests rasterize with: [`gnu_char_width`] and the
/// evaluator's default text fill (`NEOMACS_POSN_OBJECT_EXTENT` unset).
pub fn gnu_raster_policy() -> TtyRasterPolicy {
    TtyRasterPolicy {
        char_widths: TerminalCharWidths::new(gnu_char_width),
        text_fill: TextFillEdge::Structural,
    }
}

/// The strings the tests compose for a terminal.
pub const COMPOSED_TEXTS: &[&str] = &["ꤸꥉ", "ꦮꦶ", "ᠨ\u{180E}", "𐴓𐴠𐴑𐴤𐴝"];

/// GNU's `compose-gstring-for-terminal` lowering of `text`, as the
/// evaluator's `automatic_composition_for_terminal` produces it.
///
/// # Panics
///
/// On a string [`COMPOSED_TEXTS`] does not list.
pub fn terminal_composition(text: &str) -> TerminalComposition {
    let cells: &[(char, &str, u8, u16)] = match text {
        // A base with a following zero-width vowel sign attached.
        "ꤸꥉ" => &[('ꤸ', "ꥉ", 1, 2)],
        // Two zero-width non-format characters: a space cell each.
        "ꦮꦶ" => &[(' ', "ꦮ", 1, 1), (' ', "ꦶ", 1, 1)],
        // A format character (Cf) attached to the preceding base.
        "ᠨ\u{180E}" => &[('ᠨ', "\u{180E}", 1, 2)],
        "𐴓𐴠𐴑𐴤𐴝" => &[
            ('𐴓', "", 1, 1),
            ('𐴠', "", 1, 1),
            ('𐴑', "", 1, 1),
            ('𐴤', "", 1, 1),
            ('𐴝', "", 1, 1),
        ],
        _ => panic!(
            "{text:?} has no terminal composition in the test table; add it to COMPOSED_TEXTS"
        ),
    };
    TerminalComposition {
        cells: cells
            .iter()
            .map(
                |&(base, extenders, width_cols, source_char_len)| TerminalCompositionCell {
                    base,
                    extenders: extenders.into(),
                    width_cols,
                    source_char_len,
                },
            )
            .collect(),
        width_cols: cells
            .iter()
            .map(|&(_, _, width_cols, _)| u16::from(width_cols))
            .sum(),
    }
}

#[cfg(test)]
#[path = "testing/tests/testing_test.rs"]
mod tests;
