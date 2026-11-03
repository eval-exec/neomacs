#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading};
use tagged::value::TaggedValue;

fn main() {
    let event = threading::StoredThreadEvent {
        sender: TaggedValue::NIL,
        error_symbol: TaggedValue::NIL,
        data: TaggedValue::NIL,
    };
    let _ = event.values();
}
