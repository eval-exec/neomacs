//! gc A4 regression guard: `FloatTailRefund` must stay retired; parallel
//! float allocation belongs solely to N2 (`NumericStorage`).
use neovm_value::tagged::gc::FloatTailRefund;

fn main() {
    let _ = std::mem::size_of::<FloatTailRefund>();
}
