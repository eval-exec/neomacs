//! value-gc C2 regression guard (G1): `OwnerGcState::publish` and
//! `PublishedGcImage::from_legacy` must require `unsafe` so safe code cannot
//! mint `Send + Sync` `PublishedGcImage` payloads wrapping `MarkWord`s.
use neovm_value::tagged::gc::TaggedHeap;
use neovm_value::tagged::gc::mutator_registration::image::{OwnerGcState, PublishedGcImage};

fn check(owner: &mut OwnerGcState, heap: &mut TaggedHeap) {
    let _ = owner.publish();
    let _ = PublishedGcImage::from_legacy(heap);
}

fn main() {
    let _ = check;
}
