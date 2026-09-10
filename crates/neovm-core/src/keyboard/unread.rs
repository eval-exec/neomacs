//! Decode Lisp's unread-command queue before its metadata can reach keymaps.

use crate::emacs_core::value::Value;

/// GNU `read_char` distinguishes forced fresh input, ordinary rereads, and
/// events already recorded by Quail (keyboard.c, unread-command-events).
#[derive(Clone, Copy, Debug)]
pub(crate) enum UnreadCommandEvent {
    Fresh(Value),
    Reread(Value),
    AlreadyRecorded(Value),
}

impl UnreadCommandEvent {
    pub(crate) fn decode(value: Value) -> Self {
        if value.is_cons() {
            let tag = value.cons_car();
            if tag.is_t() {
                return Self::Fresh(value.cons_cdr());
            }
            if tag == Value::symbol("no-record") {
                return Self::AlreadyRecorded(value.cons_cdr());
            }
        }
        Self::Reread(value)
    }

    pub(crate) fn event(self) -> Value {
        match self {
            Self::Fresh(event) | Self::Reread(event) | Self::AlreadyRecorded(event) => event,
        }
    }
}
