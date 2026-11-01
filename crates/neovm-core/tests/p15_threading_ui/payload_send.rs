#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{buffer, tagged, threading};

#[path = "../thread_safety_ui/support/thread_bounds.rs"]
mod thread_bounds;
use thread_bounds::ThreadTransfer;

fn main() {
    ThreadTransfer::<threading::ThreadPayload>::check();
}
