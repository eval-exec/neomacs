#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

fn duplicate(ids: threading::ThreadIds) {
    let _copy = ids.clone();
}

fn main() {}
