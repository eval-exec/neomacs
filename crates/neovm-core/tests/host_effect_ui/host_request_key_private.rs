//! The public vocabulary cannot expose the registry's composite request key.
use neovm_core::host_effect::contracts::HostRequestKey;

fn main() {
    let _ = std::mem::size_of::<HostRequestKey>();
}
