use neovm_core::host_effect::contracts::OwnerContinuationId;
use std::num::NonZeroU64;

fn main() {
    let _forged = OwnerContinuationId(NonZeroU64::MIN);
}
