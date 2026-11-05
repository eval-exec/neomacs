//! Pure, dependency-free half of GNU keyboard.c: modifier-bit constants,
//! key/event description helpers, and bootstrap DEFVAR registration.
//!
//! This is NOT the input event loop. The stateful keyboard.c machinery
//! (read_key_sequence, read_char, kboard, translation maps, kbd macros)
//! lives in crate::keyboard; the command loop proper lives in eval.rs.
//! This module exists so keymap.rs and builtins can share event-encoding
//! semantics without depending on the stateful command-loop module.
//!
//! `input_entries` adds the read-char / read-event / read-key-sequence Lisp
//! entries and the input-mode setters: they bind the terminal layer, so they
//! are R0 command-layer bodies (STAGE4 design §5).  They drive the evaluator
//! through the `KeyboardInputRuntime` abstraction, which stays beside the
//! reader because the minibuffer read bodies still use it.

pub use neovm_value::keyboard_pure as pure;

mod command_loop;
pub(crate) mod input_entries;

pub(crate) use command_loop::CommandLoopHost;
pub use command_loop::{initialize_termination_signals, recursive_edit};
pub(crate) use input_entries::*;

#[cfg(test)]
#[path = "tests/keyboard_test.rs"]
mod tests;
