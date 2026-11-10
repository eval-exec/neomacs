//! Fence delegates that must be written inside the `crate::tagged::gc` module
//! tree because their signatures name crate-visible registry/resume types.
//!
//! The module is `pub(crate)`; `neovm_value::test_support::authority` is the
//! only public re-export path, so a compile-fail fixture that names an
//! original item keeps failing. Each item has one AUTHORITY-ITEMS row.
//! The module also carries two VIS re-exports (the catalog-waits observer fn
//! and its snapshot type); each public route site has a TEST-SUPPORT-VIS
//! `use` row, and no second fn definition exists.

use super::Mutator;
use super::rendezvous::{RendezvousError, ResumeGcImage};
use crate::tagged::gc::ProofEra;

/// FENCE: hand the sealed legacy-root census out of a proof era before its
/// explicit finish. The re-homed execution fixture re-admits the same World
/// through `ParallelHeapEra::admit_rooted_with`, which requires the ACTUAL
/// census (finishing setup does not authorize fabricating a new one). The
/// `EraCore` field stays private; the in-crate fixture is the only other
/// consumer, and it already uses this exact take.
pub fn take_legacy_roots_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &mut ProofEra<'heap, K>,
) -> Option<crate::tagged::gc::FrozenLegacyRoots> {
    era.core.legacy_roots.take()
}

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

/// Name and public route for the peer-approved MEMQ-TAIL-SHAPES (5) copied
/// catalog-waits observer. `cons_storage` is a private module, so without this
/// re-export the A'-widened items have no cross-crate path. Same items and
/// rows as `tagged::gc::cons_storage::{catalog_waits_for_test,
/// CatalogWaitsSnapshot}` (VIS-VALUE rows); nothing is widened here.
///
/// The re-export is cfg-split because the A' widening only exists in the
/// test-support build: in value's own cfg(test) build both items are still
/// `pub(crate)`, and a `pub use` of them would be E0365. Both arms name the
/// same items, so there is exactly ONE definition of each (no second fn) —
/// peer 23:01Z condition (a) on the catalog route.
#[cfg(feature = "test-support")]
pub use super::super::cons_storage::{CatalogWaitsSnapshot, catalog_waits_for_test};
#[cfg(not(feature = "test-support"))]
pub(crate) use super::super::cons_storage::{CatalogWaitsSnapshot, catalog_waits_for_test};

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

    /// Typed scoped authority-word seam (peer 21:25Z (i); &self receiver per
    /// peer 22:30Z): CAS-installs from the quiescent word and a restore-on-drop
    /// guard puts the prior word back. The guard borrows the scope immutably,
    /// so the caller may make &self reads (notably [`Self::inspect`]) while the
    /// probe word is installed; `enter` still needs &mut self and is excluded
    /// by the borrow for the guard's lifetime.
    ///
    /// # Safety
    /// No collector or reader other than the one under test observes the table
    /// while the guard lives.
    pub unsafe fn hold_authority_word_for_test(
        &self,
        state: AuthorityProbeState,
    ) -> AuthorityProbeGuard<'_> {
        let table = self.table.expect("enter a table before holding its word");
        let lease = super::KeeperTableMut::enter(self.stopped, table).expect("keeper lease");
        let word: *const AtomicU64 = lease.authority_word();
        // SAFETY: the rooted table of the stopped heap outlives the scope;
        // the returned guard is tied to &self, so only further &self reads on
        // this scope (not enter) can run while the probe word is installed.
        let word: &AtomicU64 = unsafe { &*word };
        let prior = word
            .compare_exchange(0, state.word(), Ordering::AcqRel, Ordering::Acquire)
            .expect("authority word must be quiescent");
        AuthorityProbeGuard { word, prior }
    }

    /// Typed nested-install probe (peer 22:30Z (ii)): while this scope's
    /// stopped-tables installation is live, attempt a second installation over
    /// the same heap. The refusal is returned as a copied result so the
    /// caller's assertion stays `matches!(.., Err(RendezvousError::Busy))`.
    /// No state changes on refusal.
    pub fn try_nested_install_for_test(&mut self) -> Result<(), super::RendezvousError> {
        super::super::collector_tables::with_stopped_tables(self.heap, self.stopped, |_| ())
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

/// FENCE (peer-signed A composite; peer #1482 condition 3 audit): wait on
/// the registry's resume-debt event, then hand the body a run admitted under
/// the same predicate gate. The Mutator's registry field, the park ticket,
/// the registry guard, the live phase word and the resume wait never cross;
/// the caller sees only `RunAccess`. This is a verbatim move of the fixture's
/// historical `with_run`; every error string and the close/panic ordering are
/// preserved.
pub fn with_registry_admission_gate_for_test<'era, R>(
    owner: &mut super::Mutator<'era>,
    mut body: impl FnMut(
        &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
    ) -> Result<R, String>,
) -> Result<R, String> {
    use super::rendezvous::Phase;

    fn failure(payload: Box<dyn std::any::Any + Send>) -> String {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_else(|| "non-string cohort assertion".into())
    }

    let registry = owner.registry;
    let _park = registry.park_ticket();
    let mut gate = registry
        .lock_blocking()
        .map_err(|error| format!("cohort admission gate: {error:?}"))?;
    loop {
        match gate.rendezvous.phase {
            Phase::Idle => break,
            Phase::DrainingResumes => {
                // Check the specified non-blocking refusal while its actual
                // predicate is locked. Busy in any other phase is not retried.
                match owner.run_access() {
                    Err(crate::tagged::gc::mutator_registration::RegistrationError::Busy) => {}
                    Err(error) => return Err(format!("cohort drain refusal: {error:?}")),
                    Ok(run) => {
                        drop(gate);
                        drop(run);
                        return Err("cohort admitted during DrainingResumes".into());
                    }
                }
                // The final ResumeGcImage::try_apply -> finish_debt opens Idle
                // and notifies this same condvar. wait atomically releases the
                // predicate mutex, including the product's poison wake path.
                gate = registry
                    .wait(gate)
                    .map_err(|error| format!("cohort resume-debt wait: {error:?}"))?;
            }
            Phase::Stopping(_) | Phase::Tracing(_) | Phase::Poisoned => {
                return Err(format!(
                    "cohort unexpected admission phase: {:?}",
                    gate.rendezvous.phase
                ));
            }
        }
    }
    // Admit under the predicate gate so another owner cannot start restoring
    // between the Idle observation and run_access. The body takes no gate.
    let admitted = owner.run_access();
    drop(gate);
    drop(_park);
    let mut run = admitted.map_err(|error| format!("cohort run admission: {error:?}"))?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut run)));
    // A close error is terminal evidence, never a reason to repeat the body or
    // the close. Normal local-region closure requires no registry mutex.
    let closed = run.close_cons_region();
    drop(run);
    let result = match result {
        Ok(result) => result,
        Err(payload) => Err(failure(payload)),
    };
    match closed {
        Ok(()) => result,
        Err(error) => Err(match result {
            Ok(_) => format!("cohort close: {error:?}"),
            Err(body) => format!("cohort close: {error:?}; body={body}"),
        }),
    }
}

/// FENCE (copied read; peer #1482 design rubric 7): the retained collector
/// heap's generational mode, so the fixture keeps a REAL pre-admission mode
/// observation without widening the collector's heap accessor.
pub fn collector_generational_enabled_for_test(
    collector: &mut crate::tagged::gc::QuiescentCollector<'_>,
) -> bool {
    collector
        .registration_heap_mut()
        .tagged_heap_generational_enabled()
}

/// FENCE (re-homed execution fixtures): construct a HOF singleton on the
/// calling owner. The constructor reserves root homes and cons storage, so
/// it is a WRITE; the original `HofSingleton::new` stays `pub(crate)`.
pub fn hof_singleton_new_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
    element: crate::tagged::gc::mutator_registration::ImmediateResult,
) -> Result<
    crate::tagged::gc::mutator_registration::HofSingleton<'era>,
    crate::tagged::gc::mutator_registration::RegistrationError,
> {
    crate::tagged::gc::mutator_registration::HofSingleton::new(run, element)
}

/// FENCE (re-homed execution fixtures): construct the resident HOF scratch
/// homes. Root allocation is a WRITE; the original stays `pub(crate)`.
pub fn hof_scratch_new_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
) -> Result<
    crate::tagged::gc::mutator_registration::HofScratch<'era>,
    crate::tagged::gc::mutator_registration::HofError,
> {
    crate::tagged::gc::mutator_registration::HofScratch::new(run)
}

/// FENCE (re-homed execution fixture): prepare the canonical HOF builtin
/// under the era's checked function-write authority. The preparation takes
/// the exclusive admission writer, so the original `pub(crate)` era method
/// stays crate-private and this fence is the cross-crate route.
pub fn era_prepare_hof_builtin_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &crate::tagged::gc::ParallelHeapEra<'heap, K>,
    operation: crate::tagged::gc::HofOperation,
    key: &crate::tagged::gc::mutator_registration::SymbolHandle,
) -> Result<
    crate::tagged::gc::PreparedHofBuiltin,
    crate::tagged::gc::mutator_registration::FunctionError,
> {
    era.prepare_hof_builtin(operation, key)
}

/// FENCE (re-homed execution fixture): prepare a unary function symbol under
/// the same checked write authority. The original stays `pub(crate)`.
pub fn era_prepare_unary_function_symbol_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &crate::tagged::gc::ParallelHeapEra<'heap, K>,
    key: &crate::tagged::gc::mutator_registration::SymbolHandle,
) -> Result<
    crate::tagged::gc::PreparedUnaryFunction,
    crate::tagged::gc::mutator_registration::FunctionError,
> {
    era.prepare_unary_function_symbol(key)
}

/// FENCE (copied read; peer #1482 condition 3 audit): the retained
/// registry's published-table shape and statistics for the re-homed
/// execution fixtures. Only Copy scalars cross; the registry, the sidecar,
/// the publication view and the backing table stay internal. The stats
/// snapshot keeps its historical expect point and message.
pub fn registry_publication_facts_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &crate::tagged::gc::ParallelHeapEra<'heap, K>,
) -> (
    crate::tagged::gc::symbol_table::SymbolSlotCount,
    crate::tagged::gc::symbol_table::SymbolChunkCount,
    crate::tagged::gc::symbol_table::SymbolChunkSlots,
    crate::tagged::gc::symbol_table::publication::TableStatistics,
) {
    let registry = era.core.registry();
    let tables = &registry.symbols.tables;
    let view = tables.view();
    let table = view.table();
    (
        table.slots(),
        table.chunk_count(),
        table.chunk_slots(),
        tables
            .stats()
            .expect("publication initial table statistics"),
    )
}

/// FENCE (copied read): the registry-side address word of the published
/// table's first chunk, captured by the fixture at prepare time. No chunk,
/// pointer descriptor or table crosses; `None` means no first chunk.
pub fn registry_first_chunk_word_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &crate::tagged::gc::ParallelHeapEra<'heap, K>,
) -> Option<usize> {
    let registry = era.core.registry();
    registry
        .symbols
        .tables
        .view()
        .table()
        .chunk(0.into())
        .map(|chunk| chunk.as_ptr().addr())
}

/// FENCE (copied read): the activation-side address word of the published
/// table's first chunk, for the consumer's identity-retained assertion. The
/// private chunk-address type and the view never cross; only the word does.
pub fn table_first_chunk_word_for_test(
    activation: &crate::tagged::gc::mutator_registration::ExecutionActivation<'_, '_, '_>,
) -> Option<usize> {
    let view = activation.table_view();
    let table = view.table();
    table.chunk(0.into()).map(|chunk| chunk.as_ptr().addr())
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
