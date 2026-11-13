//! GNU-compatible interpretation of `invisible` property values.
//!
//! The pure classifier moved to `neovm_value::display_vocab` (TYPE-DOWN);
//! this module keeps the historical display path as a re-export so the
//! display engine and the layout engine keep their imports.

pub use neovm_value::display_vocab::{Invisibility, text_prop_means_invisible};
