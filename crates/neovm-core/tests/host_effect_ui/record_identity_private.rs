use neovm_core::host_effect::contracts::RootRecordId;
use std::num::NonZeroU64;

fn main() {
    let _forged = RootRecordId(NonZeroU64::MIN);
}
