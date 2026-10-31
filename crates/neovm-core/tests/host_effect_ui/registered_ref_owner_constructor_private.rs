//! Even naming the owner-only constructor would permit issuing false metadata.
use neovm_core::host_effect::contracts::{MessageArguments, RegisteredRootRef};

fn main() {
    let _constructor = RegisteredRootRef::<MessageArguments>::from_registered;
}
