#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

fn forge(id: threading::CondVarId, association: threading::MutexReference) {
    let _condition = threading::CondVarPayload {
        id,
        name: None,
        association,
    };
}

fn main() {}
