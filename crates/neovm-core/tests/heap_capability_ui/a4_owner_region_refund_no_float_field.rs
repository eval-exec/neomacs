//! gc A4 regression guard: `OwnerRegionRefund::float` must stay retired;
//! parallel float allocation belongs solely to N2 (`NumericStorage`).
use neovm_value::tagged::gc::OwnerAllocRegion;

fn main() {
    let mut region = OwnerAllocRegion::new();
    let refund = region.close_for_stop();
    let _ = refund.float;
}
