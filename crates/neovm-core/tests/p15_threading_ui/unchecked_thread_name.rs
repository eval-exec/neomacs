#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

fn main() {
    let _name = threading::ThreadName(tagged::value::TaggedValue::fixnum(7));
}
