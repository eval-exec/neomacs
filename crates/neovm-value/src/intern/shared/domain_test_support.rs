//! Value-test-only constructor for `InternReadDomain`.
//!
//! Kept out of `domain.rs` so that file (which the compile-fail fixtures
//! `#[path]`-include) carries no `test-support` cfg token: the fixtures'
//! scratch crate does not declare the feature (CG-5 mechanism (b)).

use super::InternReadDomain;
use std::sync::atomic::AtomicU64;

impl InternReadDomain {
    pub(crate) fn seeded_for_test(word: u64) -> Self {
        Self {
            word: AtomicU64::new(word),
        }
    }
}
