//! Decode Lisp's unread-command queue before its metadata can reach keymaps.
//!
//! The event record itself is input-path vocabulary and moved to
//! `neovm_value::system_vocab::input` for the stage-4 eval split (W1/E3);
//! this module keeps core's historical path.

pub(crate) use neovm_value::system_vocab::input::UnreadCommandEvent;
