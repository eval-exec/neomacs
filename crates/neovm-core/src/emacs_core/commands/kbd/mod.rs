//! Facade for the `kbd` parser/encoder (stage-4 K2 batch K4).
//!
//! The pure parser moved to `emacs_core::eval::kbd` because the evaluator's
//! command readers call it downward once eval is the lower crate. Core command
//! bodies (`commands/interactive`, the keymap builtins) keep the historical
//! `crate::emacs_core::kbd` path through this re-export.

pub use crate::emacs_core::eval::kbd::{
    KeyDesignatorError, key_events_from_designator, parse_kbd_string,
};
