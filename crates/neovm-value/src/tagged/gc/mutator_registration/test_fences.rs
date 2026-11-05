//! Fence delegates that must be written inside the `crate::tagged::gc` module
//! tree because their signatures name crate-visible registry/resume types.
//!
//! The module is `pub(crate)`; `neovm_value::test_support::authority` is the
//! only public re-export path, so a compile-fail fixture that names an
//! original item keeps failing. Each item has one AUTHORITY-ITEMS row.

use super::Mutator;
use super::rendezvous::{RendezvousError, ResumeGcImage};
use crate::tagged::gc::ProofEra;

/// Deliberate registry poisoning for the re-homed root-slot panic-path test:
/// the test proves that an abandoned era is conservatively retained, so it
/// injects the same poisoned state the production Drop path publishes. The
/// original `Registry::poison` stays `pub(super)`.
#[allow(private_interfaces)]
pub fn poison_registry_for_test<'heap, K: crate::tagged::gc::EraKeeper>(era: &ProofEra<'heap, K>) {
    era.registry().poison();
}

/// REGWAIT-GAP fence: begin the retrying restore image for the re-homed
/// root-slot restore loop. The original stays
/// `pub(in crate::tagged::gc::mutator_registration)`; the caller cannot name
/// the returned image type, only drive it.
#[allow(private_interfaces)]
pub fn begin_restore_retry_for_test<'owner, 'era>(
    owner: &'owner mut Mutator<'era>,
    native: bool,
) -> Result<ResumeGcImage<'owner, 'era>, RendezvousError> {
    owner.begin_restore_retry(native)
}

/// Peer-signed 18:40Z shape (a): typed scoped seam over the owner-private
/// admission word. Exactly the two states the probe stores.
pub enum AdmissionProbeState {
    InSlice(u32),
    Out,
}

impl AdmissionProbeState {
    fn word(self) -> u64 {
        match self {
            Self::InSlice(epoch) => super::admission::AdmissionWord::in_slice(epoch as u64),
            Self::Out => super::admission::AdmissionWord::OUT,
        }
    }
}

/// Restore-on-drop guard for [`MutatorPermit::hold_admission_for_test`]:
/// stores the PRIOR (quiescent) word back, so the deliberately inconsistent
/// state cannot outlive the tested transition.
#[must_use = "the guard restores the permit's prior admission word on drop"]
pub struct AdmissionProbeGuard<'a> {
    entry: &'a std::sync::Arc<super::MutatorEntry>,
    prior: u64,
}

impl Drop for AdmissionProbeGuard<'_> {
    fn drop(&mut self) {
        self.entry
            .admission
            .store(self.prior, std::sync::atomic::Ordering::Release);
    }
}

impl super::MutatorPermit<'_> {
    /// CAS-install a probe state from the current quiescent OUT word and
    /// return the restore-on-drop guard.
    ///
    /// # Safety
    /// No collector or catalog transition other than the one under test may
    /// observe this permit while the guard lives.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) unsafe fn hold_admission_for_test(
        &self,
        state: AdmissionProbeState,
    ) -> AdmissionProbeGuard<'_> {
        let prior = self
            .entry
            .admission
            .compare_exchange(
                super::admission::AdmissionWord::OUT,
                state.word(),
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .expect("admission probe requires a quiescent OUT word");
        AdmissionProbeGuard {
            entry: &self.entry,
            prior,
        }
    }
}

// ---------------------------------------------------------------------------
// Peer-signed MEMQ-TAIL-SHAPES (b) + 21:25Z clarification (i): closure-scoped,
// non-swappable collector-table access for the re-homed keeper-table scope
// test. All writes go through the typed authority-word seam below; there is no
// raw word setter.

use super::AllStopped;
use super::SealedTableAccess;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::value::TaggedValue;
use crate::value::LispHashTable;
use std::sync::atomic::{AtomicU64, Ordering};

/// Typed scoped seam states (peer 21:25Z (i)): exactly the words the
/// keeper-table test stores. `LeakedWriter` is the table_authority::WRITER
/// sentinel the test also stores; `Quiescent` restores the empty word.
pub enum AuthorityProbeState {
    LeakedReader { readers: u32 },
    LeakedWriter,
    Quiescent,
}

impl AuthorityProbeState {
    fn word(self) -> u64 {
        match self {
            Self::LeakedReader { readers } => readers as u64,
            Self::LeakedWriter => super::table_authority::WRITER,
            Self::Quiescent => 0,
        }
    }
}

/// Restore-on-drop guard for [`KeeperTableScope::hold_authority_word_for_test`]:
/// stores the PRIOR word back so the deliberately inconsistent state cannot
/// outlive the tested transition. Private fields; must_use.
#[must_use = "the guard restores the table authority word on drop"]
pub struct AuthorityProbeGuard<'a> {
    word: &'a AtomicU64,
    prior: u64,
}

impl Drop for AuthorityProbeGuard<'_> {
    fn drop(&mut self) {
        self.word.store(self.prior, Ordering::Release);
    }
}

/// Non-swappable borrow newtype over the stopped collector heap. Private
/// `&mut`; NO Deref/DerefMut. Forwards exactly the operations the keeper-table
/// test calls; `enter` records the table for the following operations so the
/// authority-word seam needs no lease argument (peer 21:25Z signature).
pub struct KeeperTableScope<'a, 'era> {
    heap: &'a mut TaggedHeap,
    stopped: &'a AllStopped<'era>,
    table: Option<TaggedValue>,
}

impl<'a, 'era> KeeperTableScope<'a, 'era> {
    /// Copied null check for the installed collector-tables slot.
    pub fn collector_tables_is_null(&self) -> bool {
        self.heap._satb_layout_reserve.collector_tables().is_null()
    }

    /// Validate and record the table the following operations act on.
    pub fn enter(&mut self, table: TaggedValue) -> Result<(), super::RegistrationError> {
        super::KeeperTableMut::enter(self.stopped, table).map(|lease| drop(lease))?;
        self.table = Some(table);
        Ok(())
    }

    /// Copied current authority word of the recorded table.
    pub fn authority_word(&self) -> u64 {
        let table = self.table.expect("enter a table before reading its word");
        let lease = super::KeeperTableMut::enter(self.stopped, table).expect("keeper lease");
        lease.authority_word().load(Ordering::Acquire)
    }

    /// Inspect the recorded table through the installed collector-tables
    /// bridge. The callback cannot leak a payload reference.
    ///
    /// # Safety
    /// The table must be a live rooted hash table of this stopped heap and the
    /// stopped-tables installation (this scope) must remain live.
    pub unsafe fn inspect(&self, f: impl for<'table> FnOnce(&'table LispHashTable)) {
        let table = self.table.expect("enter a table before inspecting it");
        let ptr = table
            .gc_veclike_ptr()
            .expect("rooted table")
            .cast_mut()
            .cast();
        // SAFETY: caller contract; the scope keeps the installation live.
        unsafe { self.heap.collector_tables().inspect(ptr, f) }
    }

    /// Typed scoped authority-word seam (peer 21:25Z (i)): CAS-installs from
    /// the quiescent word and a restore-on-drop guard puts the prior word back.
    ///
    /// # Safety
    /// No collector or reader other than the one under test observes the table
    /// while the guard lives.
    pub unsafe fn hold_authority_word_for_test(
        &mut self,
        state: AuthorityProbeState,
    ) -> AuthorityProbeGuard<'_> {
        let table = self.table.expect("enter a table before holding its word");
        let lease = super::KeeperTableMut::enter(self.stopped, table).expect("keeper lease");
        let word: *const AtomicU64 = lease.authority_word();
        // SAFETY: the rooted table of the stopped heap outlives the scope;
        // the returned guard is tied to &mut self, so no other operation on
        // this scope can run while the probe word is installed.
        let word: &AtomicU64 = unsafe { &*word };
        let prior = word
            .compare_exchange(0, state.word(), Ordering::AcqRel, Ordering::Acquire)
            .expect("authority word must be quiescent");
        AuthorityProbeGuard { word, prior }
    }
}

/// Peer-signed shape (b): closure-scoped stopped-tables installation. The
/// AllStopped must come from a real stop or an unsafe mint (the caller's
/// contract); the installation is cleared on return and on unwind.
///
/// # Safety
/// `stopped` must be a real stop of `era` (or an unsafe fence mint), the body
/// must not resume owners, and no other collector-table installation may be
/// live.
pub unsafe fn with_stopped_tables_for_test<'a, 'era, R, K: crate::tagged::gc::EraKeeper>(
    era: &'a ProofEra<'_, K>,
    stopped: &'a AllStopped<'era>,
    body: impl FnOnce(&mut KeeperTableScope<'_, 'era>) -> R,
) -> R {
    let mut slot = era.core.collector.try_borrow_mut().expect("live collector");
    let heap = slot
        .as_mut()
        .expect("live collector")
        .registration_heap_mut();
    assert!(
        heap._satb_layout_reserve.collector_tables().is_null(),
        "collector tables already installed"
    );
    let result = super::super::collector_tables::with_stopped_tables(heap, stopped, |heap| {
        body(&mut KeeperTableScope {
            heap,
            stopped,
            table: None,
        })
    })
    .expect("stopped-tables install");
    assert!(
        heap._satb_layout_reserve.collector_tables().is_null(),
        "collector tables leaked past the scope"
    );
    result
}

/// Counter reset for the A8 driver (peer MEMQ-TAIL-SHAPES (5)): a write, so
/// an unsafe fence.
///
/// # Safety
/// The caller has exclusive use of the profiling counters (serial probe); no
/// other thread records or reads them concurrently.
pub unsafe fn reset_catalog_waits_for_test() {
    // SAFETY: caller contract.
    unsafe { super::super::cons_storage::reset_catalog_waits_for_test() }
}
