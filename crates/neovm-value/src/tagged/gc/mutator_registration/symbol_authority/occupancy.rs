//! Fence-only symbol-authority word occupancy for the re-homed core tests.
//!
//! The authority word itself never leaves value: callers receive a copied u64
//! reading or an RAII occupancy guard whose Drop restores quiescence. The
//! guard is the only mutation path, it refuses a non-quiescent word, and the
//! reader/writer values stay private to `authority_word`.
use super::*;
use crate::tagged::gc::mutator_registration::authority_word::WRITER;
use crate::tagged::gc::mutator_registration::{EraKeeper, GenericProofEra, RunAccess};
use std::sync::atomic::{AtomicU64, Ordering};

/// Which side of the symbol authority protocol a test occupies.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolWordOccupancy {
    Reader,
    Writer,
}

/// Occupies the symbol authority word for as long as it lives, restoring the
/// quiescent zero word on Drop. The word reference never escapes.
#[doc(hidden)]
#[must_use = "the symbol authority word stays occupied until the guard drops"]
pub struct OccupiedSymbolWord<'a> {
    word: &'a AtomicU64,
}

impl Drop for OccupiedSymbolWord<'_> {
    fn drop(&mut self) {
        self.word.store(0, Ordering::Release);
    }
}

fn occupy<'a>(
    word: &'a AtomicU64,
    occupancy: SymbolWordOccupancy,
) -> Result<OccupiedSymbolWord<'a>, RegistrationError> {
    let occupied = match occupancy {
        SymbolWordOccupancy::Reader => 1,
        SymbolWordOccupancy::Writer => WRITER,
    };
    word.compare_exchange(0, occupied, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| OccupiedSymbolWord { word })
        .map_err(|_| RegistrationError::Busy)
}

#[cfg(any(test, feature = "test-support"))]
impl<K: EraKeeper> GenericProofEra<'_, K> {
    /// Copied reading of the symbol authority word; no reference escapes.
    #[doc(hidden)]
    pub fn symbol_word_for_test(&self) -> u64 {
        self.core.registry().symbols.word.load(Ordering::Acquire)
    }

    /// Occupy the symbol authority word through the signed RAII guard. Fails
    /// with `RegistrationError::Busy` unless the word is quiescent.
    #[doc(hidden)]
    pub fn occupy_symbol_word_for_test(
        &self,
        occupancy: SymbolWordOccupancy,
    ) -> Result<OccupiedSymbolWord<'_>, RegistrationError> {
        occupy(&self.core.registry().symbols.word, occupancy)
    }
}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    /// Owner-side twin for the re-homed busy-refusal test: the guard is tied
    /// to the era (not to this borrow), so the caller keeps using the run.
    #[doc(hidden)]
    pub fn occupy_symbol_word_for_test(
        &self,
        occupancy: SymbolWordOccupancy,
    ) -> Result<OccupiedSymbolWord<'era>, RegistrationError> {
        let registry: &'era Registry = self.owner.registry;
        occupy(&registry.symbols.word, occupancy)
    }
}
