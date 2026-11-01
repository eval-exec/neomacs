#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

fn forge(id: threading::MutexId) {
    let _edge = threading::MutexReference {
        id,
        value: tagged::value::TaggedValue::NIL,
    };
}

fn main() {}
