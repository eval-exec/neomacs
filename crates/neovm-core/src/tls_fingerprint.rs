//! Debug-only fingerprints for the carrier TLS snapshot.
//!
//! The carrier pool keeps an OS thread's TLS across Lisp jobs. A debug test
//! captures this fingerprint on a fresh carrier and compares it after each
//! park/reset; a record missing from a reset makes the two diverge. Only
//! `debug_assertions` builds compile this module or its per-module probes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// FNV offset basis.
pub(crate) const SEED: u64 = 0xcbf2_9ce4_8422_2325;

/// Mix one already-computed scalar into the running fingerprint.
pub(crate) fn mix(hash: &mut u64, value: u64) {
    *hash = (*hash ^ value).wrapping_mul(0x0000_0100_0000_01b3);
}

/// Mix a debug representation (used where the value has no cheap scalar
/// projection). Debug builds only; allocation cost is irrelevant here.
pub(crate) fn mix_debug(hash: &mut u64, value: impl std::fmt::Debug) {
    let mut hasher = DefaultHasher::new();
    format!("{value:?}").hash(&mut hasher);
    mix(hash, hasher.finish());
}
