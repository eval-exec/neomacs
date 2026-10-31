use neovm_core::host_effect::contracts::HostSessionId;
use std::num::NonZeroU64;

fn main() {
    let _forged = HostSessionId(NonZeroU64::MIN);
}
