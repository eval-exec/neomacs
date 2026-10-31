use neovm_core::host_effect::contracts::HostRequestId;
use std::num::NonZeroU64;

fn main() {
    let _forged = HostRequestId(NonZeroU64::MIN);
}
