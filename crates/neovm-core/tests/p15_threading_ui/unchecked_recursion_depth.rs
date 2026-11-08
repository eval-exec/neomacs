#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading, value};

fn forge(depth: std::num::NonZeroU32) {
    let _depth = threading::RecursionDepth(depth);
}

fn main() {}
