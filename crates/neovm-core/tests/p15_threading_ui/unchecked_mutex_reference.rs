#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

fn forge(id: threading::MutexId) {
    let _edge = threading::MutexReference {
        id,
        value: tagged::value::TaggedValue::NIL,
    };
}

fn main() {}
