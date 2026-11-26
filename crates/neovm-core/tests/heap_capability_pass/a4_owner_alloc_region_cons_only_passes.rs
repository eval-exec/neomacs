//! gc A4 compiling control: the cons-only parallel owner region, its
//! stop-close refund, and its cons-only era reserve all compile cleanly.
use neovm_value::tagged::gc::{
    ConsTailRefund, MeasuredInterval, OwnerAllocRegion, OwnerEraReserve, OwnerRegionRefund,
};

fn cons_only_owner_api() {
    let mut region = OwnerAllocRegion::new();
    let _: Option<usize> = region.take_cons_cell();
    let refund: OwnerRegionRefund = region.close_for_stop();
    let _: ConsTailRefund = refund.cons;
    assert!(region.is_closed());

    let reserve = OwnerEraReserve::new();
    let _: usize = reserve.cons_blocks_held();
    let _interval = MeasuredInterval { conses: 0 };
}

fn main() {
    let _ = cons_only_owner_api;
}
