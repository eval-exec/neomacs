//! The evaluator's half of the terminal rasterizer's split tests: every
//! answer `neomacs_tty_render::testing` gives in the rasterizer's tests is the
//! evaluator's answer, and the production policy maps the evaluator's mode
//! exhaustively (DECISIONS D44, D47).

use crate::tty_raster_policy::{evaluator_tty_raster_policy, text_fill_edge};
use neomacs_tty_render::TextFillEdge;
use neomacs_tty_render::testing::{
    COMPOSED_TEXTS, GNU_CHAR_WIDTHS, gnu_char_width, gnu_raster_policy, terminal_composition,
};
use neovm_core::window::{PosnObjectExtentMode, posn_object_extent_mode};

/// Backs every rasterizer test that reads a width through
/// `gnu_raster_policy` (`rif::tests`, `rif::packed_raster_tests`,
/// `rif::window_row_anchor_tests`, `painter::tests`) and `damage::tests`'
/// wide-glyph rows: each listed scalar has the evaluator's width.
#[test]
fn tty_render_width_table_is_the_evaluator_char_width() {
    for &(character, width) in GNU_CHAR_WIDTHS {
        assert_eq!(
            neovm_core::encoding::char_width(character),
            width,
            "U+{:04X}",
            u32::from(character)
        );
    }
    assert!(
        GNU_CHAR_WIDTHS.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "the width table must stay sorted for its binary search"
    );
}

/// The ASCII arms of the same fixture: every ASCII scalar, control
/// characters included.
#[test]
fn tty_render_ascii_widths_are_the_evaluator_char_width() {
    for character in (0u8..0x80).map(char::from) {
        assert_eq!(
            gnu_char_width(character),
            neovm_core::encoding::char_width(character),
            "U+{:04X}",
            u32::from(character)
        );
    }
}

/// Backs `rif::tests::automatic_composite_uses_gnu_terminal_gstring_cells`
/// and the other `automatic_composite` cases, and the packed rasterizer's
/// composed row: each literal composition is the evaluator's lowering.
#[test]
fn tty_render_compositions_are_the_evaluator_terminal_lowering() {
    for &text in COMPOSED_TEXTS {
        assert_eq!(
            terminal_composition(text),
            neovm_core::emacs_core::composite::automatic_composition_for_terminal(text),
            "{text:?}"
        );
    }
}

/// The posn-object-extent mode maps to the rasterizer's own enum, and the
/// production policy reads it: with `NEOMACS_POSN_OBJECT_EXTENT` unset (the
/// test environment), the evaluator's text fill is the structural edge the
/// rasterizer's tests use.
#[test]
fn tty_render_text_fill_follows_the_evaluator_posn_mode() {
    assert_eq!(
        text_fill_edge(PosnObjectExtentMode::Off),
        TextFillEdge::Structural
    );
    assert_eq!(
        text_fill_edge(PosnObjectExtentMode::On),
        TextFillEdge::PosnObjectExtent
    );
    let policy = evaluator_tty_raster_policy();
    assert_eq!(policy.text_fill, text_fill_edge(posn_object_extent_mode()));
    if std::env::var_os("NEOMACS_POSN_OBJECT_EXTENT").is_none() {
        assert_eq!(policy.text_fill, gnu_raster_policy().text_fill);
    }
    for character in ['a', '\t', '\u{301}', '中'] {
        assert_eq!(
            policy.char_widths.cells(character),
            neovm_core::encoding::char_width(character)
        );
    }
}
