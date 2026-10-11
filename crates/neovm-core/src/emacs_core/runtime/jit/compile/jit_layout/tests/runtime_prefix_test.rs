//! The closure-entry capture bound addresses the actual shared atomic field,
//! independent of Arc headers and repr(Rust) field ordering.

use super::*;
use crate::emacs_core::jit::Runtime;
use std::sync::atomic::{AtomicU32, Ordering};

#[test]
fn runtime_patched_prefix_address_is_shared_aligned_and_live_after_widening() {
    let runtime = Runtime::new();
    let shared = runtime.clone();
    let other = Runtime::new();
    let address = runtime_patched_prefix_address(&runtime);
    assert_eq!(address % std::mem::align_of::<AtomicU32>(), 0);
    assert_eq!(address, runtime_patched_prefix_address(&shared));
    assert_ne!(address, runtime_patched_prefix_address(&other));
    // SAFETY: the helper addresses this live Runtime Arc's AtomicU32 field;
    // both owning handles remain alive throughout the reads.
    let counter = unsafe { &*(address as *const AtomicU32) };
    assert_eq!(counter.load(Ordering::Acquire), 0);
    assert_eq!(shared.note_patched_prefix(2), None);
    assert_eq!(counter.load(Ordering::Acquire), 2);
    assert_eq!(runtime.patched_prefix(), 2);
    assert_eq!(runtime_patched_prefix_address(&runtime), address);
}
