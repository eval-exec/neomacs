//! WebAssembly host inventory.
//!
//! WebAssembly engines do not expose the embedding machine's identity or OS
//! inventory. These results represent absence; browser-specific hints can be
//! introduced later through the explicit host ABI without pretending they are
//! native system facts.

use std::num::NonZeroU64;

use super::{BootTime, LoadAverage};

pub(crate) fn system_name() -> Option<String> {
    None
}

pub(crate) fn operating_system_release() -> Option<String> {
    None
}

pub(crate) fn load_average() -> Option<LoadAverage> {
    None
}

pub(crate) fn configured_processor_count() -> Option<NonZeroU64> {
    None
}

pub(crate) fn boot_time() -> Option<BootTime> {
    None
}
