//! Coordinate conversions at the VM/layout boundary.

use neovm_core::buffer::{CharPos0, EmacsBytePos, LispCharPos1};

pub(crate) fn layout_char_pos_from_i64(charpos: i64) -> Option<CharPos0> {
    usize::try_from(charpos).ok().map(CharPos0::new)
}

pub(crate) fn lisp_charpos_to_layout_char_pos(charpos: i64) -> Option<CharPos0> {
    usize::try_from(charpos.checked_sub(1)?)
        .ok()
        .map(CharPos0::new)
}

pub(crate) fn lisp_char_pos_to_layout_i64(pos: LispCharPos1) -> i64 {
    lisp_charpos_to_layout_char_pos(pos.as_i64())
        .map(|pos| pos.get() as i64)
        .unwrap_or(0)
}

pub(crate) fn layout_i64_char_pos_to_lisp_char_pos(charpos: i64) -> LispCharPos1 {
    let one_based = usize::try_from(charpos.saturating_add(1).max(1))
        .expect("layout character position fits usize");
    LispCharPos1::from_one_based_usize(one_based)
}

#[inline]
pub(crate) fn clamped_lisp_charpos_to_layout_i64(charpos: i64) -> i64 {
    lisp_charpos_to_layout_char_pos(charpos)
        .map(|pos| pos.get() as i64)
        .unwrap_or(0)
}

pub(crate) fn layout_emacs_byte_pos_from_i64(bytepos: i64) -> Option<EmacsBytePos> {
    usize::try_from(bytepos).ok().map(EmacsBytePos::new)
}

#[cfg(test)]
#[path = "coords/tests/coords_test.rs"]
mod tests;
