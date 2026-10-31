use neovm_core::host_effect::contracts::HostRequestGeneration;
use std::num::NonZeroU64;

fn main() {
    let _forged = HostRequestGeneration(NonZeroU64::MIN);
}
