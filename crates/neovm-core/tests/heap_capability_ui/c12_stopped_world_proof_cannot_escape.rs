//! value-gc C12 regression guard: the higher-ranked lifetime on
//! `StoppedWorldProof::with_stopped_world` prevents callers from escaping
//! `&StoppedWorldProof<'_>` outside the stopped scope.

use neovm_value::tagged::gc::{StoppedWorldProof, TaggedHeap};

fn try_escape(heap: &mut TaggedHeap) {
    let mut escaped: Option<&StoppedWorldProof<'_>> = None;
    unsafe {
        StoppedWorldProof::with_stopped_world(heap, |_heap, stopped| {
            escaped = Some(stopped);
        });
    }
    let _ = escaped;
}

fn main() {}
