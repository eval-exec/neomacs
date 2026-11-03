#[path = "../p15_threading_support/actual_payload.rs"]
mod actual_payload;
use actual_payload::{emacs_core, tagged, threading};

fn admitted_event(
    heap: &tagged::gc::TaggedHeap,
    roots: &[tagged::transport::SharedRoot; 3],
) -> Result<[tagged::value::TaggedValue; 3], tagged::transport::SharedRootError> {
    let event = threading::StoredThreadEvent::try_from_roots(heap, roots)?;
    Ok(event.values())
}

fn main() {
    // Only the real validating constructor can produce the stored words.
    // Runtime probes separately prove same-heap validation and error atomicity.
    let _ = admitted_event;
}
