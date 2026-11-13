#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

fn forge(heap: tagged::gc::HeapIdentity) {
    let _thread = threading::ThreadId { heap, ordinal: 0 };
    let _mutex = threading::MutexId { heap, ordinal: 1 };
    let _condition = threading::CondVarId { heap, ordinal: 1 };
}

fn main() {}
