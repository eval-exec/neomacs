//! Pure, dependency-free half of GNU keyboard.c: modifier-bit constants,
//! key/event description helpers, and bootstrap DEFVAR registration.
//!
//! This is NOT the input event loop. The stateful keyboard.c machinery
//! (read_key_sequence, read_char, kboard, translation maps, kbd macros)
//! lives in crate::keyboard; the command loop proper lives in eval.rs.
//! This module exists so keymap.rs and builtins can share event-encoding
//! semantics without depending on the stateful command-loop module.

pub mod pure;

#[cfg(test)]
mod tests;
