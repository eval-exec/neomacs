//! Terminal input bytes whose meaning only the evaluator defines.

/// The evaluator's internal multibyte encoding of the raw byte `0xa4`
/// (`EmacsChar::from_byte8(0xa4)`; GNU `BYTE8_STRING`, `src/character.h:258-263`).
/// The terminal reader must deliver it unchanged even when a host read splits
/// it; the display host pins the bytes to the evaluator.
pub const RAW_BYTE_A4_INTERNAL: &[u8] = &[0xc0, 0xa4];
