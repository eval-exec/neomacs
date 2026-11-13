//! Value-test-only constructor for `StoppedMutatorsProof`.
//!
//! Kept out of `scan_contract.rs` so that file (which the compile-fail
//! fixtures `#[path]`-include) carries no `test-support` cfg token beyond
//! its pre-existing visibility attr (CG-5 mechanism (b)): the fixtures'
//! scratch crate does not declare the feature, so every such token would
//! add an `unexpected_cfgs` note to their stderr.

use super::scan_contract::StoppedMutatorsProof;
use std::marker::PhantomData;

impl StoppedMutatorsProof {
    /// Transport-only tests own every buffer locally and have no heap aliases.
    pub(super) fn for_test() -> Self {
        Self {
            _owner: PhantomData,
        }
    }
}
