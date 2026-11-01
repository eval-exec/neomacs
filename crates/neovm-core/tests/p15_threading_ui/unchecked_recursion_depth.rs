#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

fn forge(depth: std::num::NonZeroU32) {
    let _depth = threading::RecursionDepth(depth);
}

fn main() {}
