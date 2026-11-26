//! gc A4 regression guard: `OwnerAllocRegion` must stay cons-only; parallel
//! float allocation belongs solely to N2 (`NumericStorage`).
use neovm_value::tagged::gc::OwnerAllocRegion;

fn main() {
    let region = OwnerAllocRegion::new();
    let _ = region.take_float_slot();
}
