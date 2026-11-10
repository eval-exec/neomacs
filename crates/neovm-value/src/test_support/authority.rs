//! Signed Prep-D R6 TA2 authority fence (peer rulings 05:5xZ-b and 06:0xZ).
//!
//! This module may be named only from `crates/neovm-core/core_tests/**`; the
//! architecture TA2 guard rejects every `src/**` or `tests/**` reference.
//! Each FENCE item here is one row of `AUTHORITY-ITEMS.tsv`, signed before
//! code. The fence re-exports existing drivers and hooks with their bodies and
//! contracts unchanged, and never widens an original path, so a compile-fail
//! fixture that names the original item keeps failing. This module also
//! carries VIS re-exports (the catalog-waits observer fn and its snapshot
//! type): those are TEST-SUPPORT-VIS rows, not AUTHORITY rows, and each public
//! route site has its own `use` row (peer 23:01Z condition (a)).
//!
//! Row 1: `mark_collection_observed` (existing GC observation driver).
//!
//! THE TEST HEAP ROUTE (LEAD 15:1xZ, amended 15:2xZ): a re-homed test that
//! allocates Lisp Values WITHOUT a Context must open a [`TestHeapScope`]. The
//! scope installs a fresh `TaggedHeap` on this thread through the production
//! root-install path and restores the PREVIOUS slot value on drop (panic-safe;
//! nested scopes restore LIFO). The scope heap is NOT freed on drop: Values
//! are Copy tagged words with no borrow of their heap, so a Value that escapes
//! the scope (returned from a helper, parked in a fixture, or read by a later
//! assertion) would otherwise dangle into freed memory. Drop retires the heap
//! into a thread-local graveyard freed only at thread exit, preserving the
//! pre-split thread-lifetime fallback semantics. Tests that own a Context use
//! the Context route instead. Direct `TAGGED_HEAP`/`set_tagged_heap`
//! installation in test code is the pattern this route replaces (a guard arm
//! flags it once landed).

thread_local! {
    /// Heaps retired by [`TestHeapScope`], freed only at thread exit
    /// (LEAD 15:2xZ). Values are Copy tagged words and may outlive the scope
    /// that allocated them, so no scope may free its heap while the thread
    /// still runs; this is the pre-split thread-lifetime fallback semantics.
    /// Each `TaggedHeap::drop` at thread teardown detaches the store protocol
    /// and pointer-check-clears the slot if it still names that heap.
    static TEST_HEAP_GRAVEYARD: std::cell::RefCell<
        Vec<crate::tagged::gc::AliasableBox<crate::tagged::gc::TaggedHeap>>,
    > = const { std::cell::RefCell::new(Vec::new()) };
}

/// THE canonical no-Context test heap (LEAD 15:1xZ, amended 15:2xZ):
/// RAII, `!Send`.
///
/// `install` boxes a fresh `TaggedHeap` and installs its root into the
/// thread's allocation slot via `set_tagged_heap_root` (the production
/// root-owning installer). `Drop` restores the previous slot value in LIFO
/// order (a panic inside the scope therefore restores it during unwinding)
/// and RETIRES the scope heap into this thread's graveyard; the graveyard
/// frees retired heaps only when the thread exits. Freeing at scope end would
/// leave every Value allocated in the scope dangling, because a Value is a
/// Copy tagged word that does not borrow its heap. Out-of-order drops are
/// outside the contract (the LIFO chain is the documented use).
///
/// GC isolation rule: a collection runs against exactly ONE heap. A collect
/// issued inside a nested scope (the inner heap installed) must not trace,
/// mark, sweep or move the OUTER heap's objects: every mark/probe path
/// classifies an address by this heap's block/arena ownership, so a foreign
/// address is ignored. An outer Value therefore survives any inner collect.
///
/// Cross-heap store rule: storing an outer-heap Value inside an inner-heap
/// object is NOT allowed. Allocation does not reject it (a store is just
/// words), and the runtime enforces the rule at the collector boundary: the
/// inner heap never traces, marks or moves the foreign object, so that
/// object's lifetime remains governed by its own heap. The registered
/// mutator and owned-slot boundaries additionally reject cross-heap
/// identities with `RegistrationError::WrongHeap`. A foreign word is safe to
/// carry, but it never roots the foreign object in this heap.
///
/// The TLS store is the fence's authority: AUTHORITY-ITEMS row
/// `TestHeapScope::install`. `set_tagged_heap` itself gains no new row.
pub struct TestHeapScope {
    /// The slot value before this scope installed; `None` means the thread had
    /// no heap (the common case in a re-homed test).
    previous: Option<std::ptr::NonNull<crate::tagged::gc::TaggedHeap>>,
    /// The owned heap. `AliasableBox` gives the stable provenance root the
    /// TLS slot names. `ManuallyDrop` is load-bearing: `Drop` never frees the
    /// heap, it retires it into [`TEST_HEAP_GRAVEYARD`].
    heap: std::mem::ManuallyDrop<crate::tagged::gc::AliasableBox<crate::tagged::gc::TaggedHeap>>,
    /// The TLS slot is thread-bound: moving the scope to another thread would
    /// restore the wrong thread's slot in `Drop`.
    _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl TestHeapScope {
    /// Install a fresh heap on this thread and return its scope.
    #[doc(hidden)]
    pub fn install() -> Self {
        let heap = crate::tagged::gc::AliasableBox::new(crate::tagged::gc::TaggedHeap::new());
        let root = heap.root();
        let previous = crate::tagged::gc::installed_tagged_heap_root_for_test_scope();
        crate::tagged::gc::set_tagged_heap_root(root);
        Self {
            previous,
            heap: std::mem::ManuallyDrop::new(heap),
            _not_send: std::marker::PhantomData,
        }
    }

    /// The installed scope heap's identity, for the allocation-placement
    /// assertion (`current_tagged_heap_identity()` must equal this inside the
    /// scope). No heap reference escapes.
    #[doc(hidden)]
    pub fn identity(&self) -> usize {
        self.heap.tagged_heap_identity()
    }
}

impl Drop for TestHeapScope {
    fn drop(&mut self) {
        match self.previous {
            // LIFO: the slot goes back to the outer/default installation,
            // re-deriving the TLS mirrors from that live root.
            Some(previous) => crate::tagged::gc::set_tagged_heap_root(previous),
            // No previous heap: uninstall this scope's heap through the
            // detach-then-pointer-checked-clear path, so the slot and every
            // hot-path mirror stop naming it. The heap stays allocated.
            None => crate::tagged::gc::clear_tagged_heap_if_installed(&self.heap),
        }
        // SAFETY: `install` initialized the field and this is its only move.
        let heap = unsafe { std::mem::ManuallyDrop::take(&mut self.heap) };
        retire_to_graveyard(heap);
    }
}

/// Park a retired scope heap until thread exit; never free it here
/// (LEAD 15:2xZ).
fn retire_to_graveyard(heap: crate::tagged::gc::AliasableBox<crate::tagged::gc::TaggedHeap>) {
    // `ManuallyDrop` guards the access-failure path: if the thread-local is
    // already destroyed (a scope dropped from another TLS destructor during
    // thread exit) the closure is never called, and the heap is deliberately
    // leaked rather than freed under a Value another destructor may still
    // read. A re-entrant graveyard borrow leaks the same way.
    let mut pending = std::mem::ManuallyDrop::new(heap);
    let _ = TEST_HEAP_GRAVEYARD.try_with(|graveyard| {
        if let Ok(mut graveyard) = graveyard.try_borrow_mut() {
            // SAFETY: `pending` is initialized and moved exactly once.
            graveyard.push(unsafe { std::mem::ManuallyDrop::take(&mut pending) });
        }
    });
}

static_assertions::assert_not_impl_any!(TestHeapScope: Send, Sync);

/// FENCE (N2 static store): arm the ByteCode allocation backtrace hook for
/// the next `n` ByteCode-kind allocations.  The raw `pub(crate)` fn stays
/// un-widened; this fence is its only cross-crate route.
pub fn arm_bytecode_backtraces_for_test(n: usize) {
    crate::tagged::gc::alloc_probe::arm_bytecode_backtraces(n);
}

/// Publish an exact sticky collection-observation mark for a live heap owner,
/// without observing a pointer projection or consulting the active heap. The
/// caller gates this API on observed mode and publishes its own JIT address
/// envelope before any subsequent compiled mutation. Returns whether the
/// shared mark is new.
///
/// This is the existing `pub(crate)` driver under the signed fence spelling;
/// body and contract are unchanged.
pub fn mark_collection_observed(bits: usize) -> bool {
    crate::tagged::gc::mark_collection_observed(bits)
}

/// FENCE forge (LEAD 12:3xZ/14:0xZ): hand the caller the exact shared
/// observation byte cell of a live owner header so a race fixture can
/// `fetch_or` it from scoped workers. The field stays `pub(crate)`; this
/// forge is the only cross-crate route, and the returned capability is the
/// real production atomic (no copy, no stand-in). Reads and writes through
/// the cell are N2 authority operations.
pub fn collection_observed_byte_for_test(
    header: &crate::tagged::header::GcHeader,
) -> &std::sync::atomic::AtomicU8 {
    &header.collection_observed
}

/// FENCE (N2 write): clear a live owner's shared observation byte and its
/// collection metadata under the production exclusion contract. The original
/// `GcHeader::clear_collection_observed` stays `pub(crate)` with no A-prime;
/// this fence is its only cross-crate route.
pub fn clear_collection_observed_for_test(header: &crate::tagged::header::GcHeader) {
    header.clear_collection_observed();
}

/// FENCE (N2 counter reset): drop the thread-local function-cell lookup
/// counter for the re-homed executor tests. The original stays `pub(crate)`
/// with no A' widening; this fence is its only cross-crate route.
pub fn reset_function_cell_lookup_count_for_test() {
    crate::symbol::reset_function_cell_lookup_count();
}

/// FENCE (N2 counter reset): sibling of the above for the byte-code data
/// projection counter. The original stays `pub(crate)`.
pub fn reset_bytecode_data_access_count_for_test() {
    crate::value::reset_bytecode_data_access_count();
}

/// The restricted proof-era type used by the re-homed era-fixture tests.
/// Same item as `tagged::gc::GenericProofEra`; the original path stays
/// `cfg(any(test, feature = "test-support"))` + `pub(crate)`.
pub type ProofEra<'heap, K> = crate::tagged::gc::ProofEra<'heap, K>;

/// Finish refusal of [`ProofEra`]; same item as
/// `tagged::gc::GenericProofEraFinishFailure`.
pub type ProofEraFinishFailure<'heap, K> = crate::tagged::gc::ProofEraFinishFailure<'heap, K>;

/// C5 intern-growth AUTHORITY fence (LEAD 05:3xZ; peer B2 host ruling).
///
/// The real two-actor intern-growth transaction is value's test-support
/// testkit (`c5_intern_testkit`); that module stays crate-private, so this
/// fence is the item's only public path. `epoch_probe` is the caller's
/// per-actor function-epoch observation hook: value has no evaluator counter
/// type and never inspects the returned value, so the upper layer attributes
/// a bump on the actual committing actor thread. `None` is the historic
/// non-jit shape. The testkit is absent under gc-memory-telemetry (the
/// original history machinery's own exclusion) and in value's own `cfg(test)`
/// build (`not(test)`: the test-support dependency build is the only consumer;
/// value's unit-test graph enables the feature through unification but has no
/// consumer for this fence), so this fence matches both.
#[cfg(all(not(test), not(feature = "gc-memory-telemetry")))]
pub fn c5_intern_growth_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &ProofEra<'heap, K>,
    holders: [crate::intern::SymId; 2],
    epoch_probe: Option<fn() -> u64>,
) -> Result<C5InternGrowthReceipt, String> {
    crate::tagged::gc::mutator_registration::c5_intern_testkit::c5_intern_growth_for_test(
        era,
        holders,
        epoch_probe,
    )
}

/// C5 relocating bridge: first force the qualified clean reservation refusal
/// to replace the pointer spine; its reserved capacity covers the later win.
#[cfg(all(not(test), not(feature = "gc-memory-telemetry")))]
pub fn c5_intern_relocating_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &ProofEra<'heap, K>,
    holders: [crate::intern::SymId; 2],
    epoch_probe: Option<fn() -> u64>,
) -> Result<C5InternGrowthReceipt, String> {
    crate::tagged::gc::mutator_registration::c5_intern_testkit::c5_intern_relocating_for_test(
        era,
        holders,
        epoch_probe,
    )
}

/// Copied receipt of the C5 intern-growth transaction.
#[cfg(all(not(test), not(feature = "gc-memory-telemetry")))]
pub use crate::tagged::gc::mutator_registration::c5_intern_testkit::C5InternGrowthReceipt;

/// FENCE observer (copied identity): the published table's address as seen
/// through the activation's publication view and through its own table view.
/// Equal addresses prove both borrows name the same published table; the
/// views, the descriptor type and the backing table stay internal.
pub fn publication_table_identity_for_test(
    activation: &crate::tagged::gc::mutator_registration::ExecutionActivation<'_, '_, '_>,
) -> (usize, usize) {
    let publication = activation.publication();
    let observed = publication.load();
    let checked = activation.table_view();
    (
        observed.table() as *const _ as usize,
        checked.table() as *const _ as usize,
    )
}

/// FENCE observer (copied read): the published symbol table's shape for the
/// re-homed cell test's slot-metadata assertions. Returns plain scalars
/// `(slots, chunk_slots, chunk_count)`; the view, its descriptor type and the
/// backing table stay internal.
pub fn table_shape_for_test(
    activation: &crate::tagged::gc::mutator_registration::ExecutionActivation<'_, '_, '_>,
) -> (usize, usize, usize) {
    let view = activation.table_view();
    let table = view.table();
    (
        table.slots().get(),
        table.chunk_slots().get(),
        table.chunk_count().get(),
    )
}

/// FENCE (re-homed execution fixture): take the sealed legacy-root census out
/// of a proof era before its explicit finish. The fixture re-admits the same
/// World through `ParallelHeapEra::admit_rooted_with`, which requires the
/// actual census; the `EraCore` field stays private and the original path has
/// no A-prime.
pub fn take_legacy_roots_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &mut ProofEra<'heap, K>,
) -> Option<crate::tagged::gc::FrozenLegacyRoots> {
    crate::tagged::gc::mutator_registration::test_fences::take_legacy_roots_for_test(era)
}

/// Fence-only symbol-authority word occupancy (peer 09:2xZ): the word itself
/// never leaves value. Callers get a copied u64 reading or an RAII guard whose
/// Drop restores quiescence; the reader/writer sentinel stays private.
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::symbol_authority::occupancy::{
    OccupiedSymbolWord, SymbolWordOccupancy,
};

/// Fence-only registration-wait helpers (REGWAIT-GAP): test timing and retry
/// classification only; they add no authority path.
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::retry::{
    Deadline, Retry, SPIN_WAIT, retry, retry_owned,
};
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::symbol_functions::retry::rendezvous;
/// Fence-only registration-wait CLASSIFIERS (REGWAIT-GAP, peer-signed
/// 18:40Z): pure error-to-Retry mappings the retry drivers take as their
/// third argument. The originals stay crate-private; this fence is their
/// only public route.
#[doc(hidden)]
pub fn registration(error: &crate::tagged::gc::mutator_registration::RegistrationError) -> Retry {
    crate::tagged::gc::mutator_registration::symbol_functions::retry::registration(error)
}

#[doc(hidden)]
pub fn initial_intern(
    error: &crate::tagged::gc::mutator_registration::InitialInternError,
) -> Retry {
    crate::tagged::gc::mutator_registration::symbol_functions::retry::initial_intern(error)
}

#[doc(hidden)]
pub fn global(error: &crate::tagged::gc::mutator_registration::GlobalError) -> Retry {
    crate::tagged::gc::mutator_registration::symbol_functions::retry::global(error)
}

/// Fence delegates for the re-homed root-slot tests (peer-signed rows): the
/// registry poison hook and the REGWAIT-GAP restore-retry begin. The originals
/// stay restricted; the module is crate-visible, so this re-export is their
/// only public path.
pub use crate::tagged::gc::mutator_registration::test_fences::{
    AdmissionProbeGuard, AdmissionProbeState, AuthorityProbeGuard, AuthorityProbeState,
    KeeperTableScope, begin_restore_retry_for_test, poison_registry_for_test,
    reset_catalog_waits_for_test,
};

/// Peer-signed MEMQ-TAIL-SHAPES (b): closure-scoped stopped-tables install.
/// The scope is non-swappable; the authority-word seam is on the scope and is
/// reachable only inside this closure.
///
/// # Safety
/// `stopped` must be a real stop of `era` (or an unsafe fence mint); the body
/// must not resume owners; no other collector-table installation may be live.
#[doc(hidden)]
pub unsafe fn with_stopped_tables_for_test<'a, 'era, R, K: crate::tagged::gc::EraKeeper>(
    era: &'a crate::tagged::gc::ProofEra<'_, K>,
    stopped: &'a crate::tagged::gc::AllStopped<'era>,
    body: impl FnOnce(&mut KeeperTableScope<'_, 'era>) -> R,
) -> R {
    unsafe {
        crate::tagged::gc::mutator_registration::test_fences::with_stopped_tables_for_test(
            era, stopped, body,
        )
    }
}
/// Public route for the peer-approved MEMQ-TAIL-SHAPES (5) copied catalog-waits
/// observer. `cons_storage` stays a private module, so without this re-export
/// the A'-widened item has no cross-crate path. Same item and rows as
/// `tagged::gc::cons_storage::catalog_waits_for_test` / `CatalogWaitsSnapshot`
/// (VIS-VALUE rows); this re-export adds no widening.
#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::test_fences::{
    CatalogWaitsSnapshot, catalog_waits_for_test,
};

/// Peer-signed 18:40Z shape (a): the typed scoped admission-word seam.
/// CAS-installs the probe state from the quiescent OUT word; the returned
/// `#[must_use]` guard restores the prior word on drop. `AdmissionWord` and
/// the entry field stay private.
///
/// # Safety
/// no collector or catalog transition other than the one under test observes
/// this permit while the guard lives
#[doc(hidden)]
pub unsafe fn hold_admission_for_test<'a>(
    permit: &'a crate::tagged::gc::mutator_registration::MutatorPermit<'_>,
    state: AdmissionProbeState,
) -> AdmissionProbeGuard<'a> {
    permit.hold_admission_for_test(state)
}

/// C1 FENCE for the re-homed lifecycle tests: mint this owner's young worker
/// loan through the signed delegate. The original method stays `pub(crate)`;
/// the returned `YoungWorkerLoan` is already the public token type. The test
/// deliberately forgets the loan to exercise the refusal path.
pub fn take_young_loan_for_test<'era>(
    owner: &mut crate::tagged::gc::mutator_registration::Mutator<'era>,
) -> Result<
    crate::tagged::gc::mutator_registration::YoungWorkerLoan<'era>,
    crate::tagged::gc::mutator_registration::RegistrationError,
> {
    owner.take_young_loan()
}

/// Value-FENCE delegates (class C3) for the re-homed lifecycle tests: typed
/// seeds into the owner's GC run state. The originals stay `pub(crate)
/// unsafe`; these delegates are their only cross-crate route (no A' on the
/// originals).
///
/// # Safety
/// The caller is the owner thread of this exclusive run access, no collection
/// or GC transition runs concurrently, and every seeded value is a same-heap
/// immediate (or remains reachable from the world roots).
#[doc(hidden)]
pub unsafe fn push_root_scratch_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
    value: crate::tagged::value::TaggedValue,
) {
    unsafe { run.push_root_scratch_for_test(value) }
}

/// Sibling of [`push_root_scratch_for_test`]; same class and contract.
///
/// # Safety
/// Same owner-exclusive serial-fixture contract as
/// [`push_root_scratch_for_test`].
#[doc(hidden)]
pub unsafe fn set_promoted_since_major_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
    promoted: usize,
) {
    unsafe { run.set_promoted_since_major_for_test(promoted) }
}

/// Sibling of [`push_root_scratch_for_test`]; same class and contract.
///
/// # Safety
/// Same owner-exclusive serial-fixture contract as
/// [`push_root_scratch_for_test`].
#[doc(hidden)]
pub unsafe fn push_remset_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
    value: crate::tagged::value::TaggedValue,
) {
    unsafe { run.push_remset_for_test(value) }
}

/// Fence delegate for the deterministic host-side root-table contention
/// scope. The original stays `cfg(any(test, feature = "test-support"))`
/// `pub(crate)`, so this delegate is its only cross-crate route.
pub fn with_root_table_lock_for_test(heap: crate::tagged::gc::HeapIdentity, op: impl FnOnce()) {
    crate::tagged::transport::with_root_table_lock_for_test(heap, op)
}

/// Typed fence hook (AUTHORITY row): exhaust the owner's root-slot
/// generation to force the overflow refusal path. There is deliberately no
/// general generation setter; the state field stays private.
#[cfg(any(test, feature = "test-support"))]
pub fn exhaust_root_slot_generation_for_test(run: &mut crate::tagged::gc::RunAccess<'_, '_>) {
    run.exhaust_root_slot_generation_for_test();
}

/// # Safety
/// prepare must return the original retained keeper's epoch and backing,
/// without executing Lisp, resuming owners, or leaking a manager reference.
#[cold]
pub unsafe fn admit_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    collector: crate::tagged::gc::QuiescentCollector<'heap>,
    keeper: K,
    prepare: impl FnOnce(
        &K,
    ) -> Result<
        (
            crate::tagged::gc::FacadeEpochRetention,
            crate::symbol::parallel::SymbolBacking,
        ),
        crate::tagged::gc::RegistrationError,
    >,
) -> Result<ProofEra<'heap, K>, crate::tagged::gc::EraAdmissionFailure<'heap, K>> {
    // SAFETY: the caller contract is identical to `ProofEra::admit_with`.
    unsafe { ProofEra::admit_with(collector, keeper, prepare) }
}

/// # Safety
/// prepare must return the original retained keeper's epoch and backing,
/// without executing Lisp, resuming owners, or leaking a manager reference.
#[cold]
pub unsafe fn admit_rooted_for_test<'heap, K: crate::tagged::gc::EraKeeper>(
    collector: crate::tagged::gc::QuiescentCollector<'heap>,
    keeper: K,
    roots: crate::tagged::gc::FrozenLegacyRoots,
    prepare: impl FnOnce(
        &K,
    ) -> Result<
        (
            crate::tagged::gc::FacadeEpochRetention,
            crate::symbol::parallel::SymbolBacking,
        ),
        crate::tagged::gc::RegistrationError,
    >,
) -> Result<ProofEra<'heap, K>, crate::tagged::gc::EraAdmissionFailure<'heap, K>> {
    // SAFETY: the caller contract is identical to `ProofEra::admit_rooted_with`.
    unsafe { ProofEra::admit_rooted_with(collector, keeper, roots, prepare) }
}

/// P1-HOOKS-2 FENCE (peer 16:4xZ): issue a thread projection. Identical
/// signature to `ThreadProjectionTable::project_for_test`; the method stays
/// `pub(crate)` and `cfg(any(test, feature = "test-support"))`, and the table
/// is the only authority that maps a header back to its descriptor.
pub fn project_for_test(
    table: &mut crate::forward::ThreadProjectionTable,
    arm: crate::forward::CanonicalArm,
    plain: crate::value::Value,
    host_projected: bool,
) -> &'static crate::forward::ThreadProjectionDescriptor {
    table.project_for_test(arm, plain, host_projected)
}

/// P1-HOOKS-2 FENCE (peer 16:4xZ): write a descriptor's canonical plain word.
/// Same body as `ThreadProjectionDescriptor::set_plain_value`, whose note-the-
/// overwritten-word contract is unchanged; the method stays `pub(crate)`.
pub fn set_plain_value(
    descriptor: &crate::forward::ThreadProjectionDescriptor,
    value: crate::value::Value,
) {
    descriptor.set_plain_value(value);
}

/// P1-HOOKS-2 unsafe FENCE (peer 16:4xZ): rebuild a cell handle from a raw
/// header address, exactly as `FwdRef::from_address`.
///
/// # Safety
/// The caller must pass an address that names a live descriptor header (the
/// cell only stores addresses that came from `FwdRef::of`); the conversion is
/// infallible by that invariant.
pub unsafe fn from_address(address: usize) -> crate::forward::FwdRef {
    // SAFETY: the caller contract is identical to `FwdRef::from_address`'s.
    unsafe { crate::forward::FwdRef::from_address(address) }
}

/// P1-HOOKS-2 unsafe FENCE (peer 16:4xZ): promote a header to the permanent
/// generation through the `ImageResident` capability.
///
/// # Safety
/// `header` must belong to an object live in this heap, and the caller must
/// hold the world-stopped guarantee: no collection is in progress and no
/// mutator is running. The permanent generation stays reserved for the dump
/// image; `resident` must be the real image-row proof (or its `for_test`
/// capability in a fixture).
pub unsafe fn make_permanent_for_test(
    header: &mut crate::tagged::header::GcHeader,
    resident: &crate::tagged::header::ImageResident,
) {
    header.make_permanent(resident);
}

/// FENCE (N2 arming knob): toggle post-mark ownership verification for THIS
/// thread. The original stays `pub(crate)`; this is its only cross-crate
/// route, with the same debug-only availability.
#[cfg(debug_assertions)]
pub fn set_verify_marked_objects_for_test(on: bool) {
    crate::tagged::gc::set_verify_marked_objects_for_test(on);
}

/// FENCE (closure-scoped knob): run `f` with the card-tag filter forced to
/// `on`, restoring the prior thread-local value on exit. The original stays
/// `pub(crate)`; no state or guard escapes.
pub fn with_card_tag_filter_for_test<R>(on: bool, f: impl FnOnce() -> R) -> R {
    crate::tagged::gc::with_card_tag_filter_for_test(on, f)
}

/// N2 ambient-state write (LEAD 10:1xZ): learn one exclusion in the
/// mutator-local observed envelope. The journal State is not owned by the
/// caller, so the original write stays pub(crate) with no A-prime widening;
/// this fence is its only cross-crate route.
pub fn observed_exclude_unobserved_for_test(bits: usize) -> bool {
    crate::tagged::collection_reads::observed_exclude_unobserved_for_test(bits)
}

/// P1-HOOKS-2 FENCE (peer 16:4xZ): opaque copy of an obarray's function-cell
/// publication clock. The value can only be minted by
/// [`snapshot_function_epoch_for_test`] from a live obarray and restored into
/// the same obarray; it is not a general epoch setter (forging the clock can
/// make stale dispatch caches look valid).
#[derive(Clone, Copy, Debug)]
pub struct FunctionEpochSnapshot(u64);

/// The snapshot half of the signed snapshot/restore fence.
pub fn snapshot_function_epoch_for_test(obarray: &crate::symbol::Obarray) -> FunctionEpochSnapshot {
    FunctionEpochSnapshot(obarray.function_epoch_snapshot_raw())
}

/// The restore half of the signed snapshot/restore fence: store back a clock
/// this same obarray read earlier.
pub fn restore_function_epoch_for_test(
    obarray: &mut crate::symbol::Obarray,
    snapshot: FunctionEpochSnapshot,
) {
    obarray.restore_function_epoch_raw(snapshot.0);
}

/// B5/R6 privacy re-narrow: set one symbol's trapped-write flag for the moved
/// symbol tests. `Obarray::get_mut_by_id` is crate-private; this fence is the
/// only test route to that flag write.
pub fn set_symbol_trapped_write_for_test(
    obarray: &mut crate::symbol::Obarray,
    id: crate::intern::SymId,
    trapped_write: crate::symbol::SymbolTrappedWrite,
) -> bool {
    let Some(symbol) = obarray.get_mut_by_id(id) else {
        return false;
    };
    symbol.set_trapped_write(trapped_write);
    true
}

/// B5/R6 privacy re-narrow: read the `debug-on-next-call` descriptor through
/// the signed test route. The raw resolver stays crate-private in every build
/// (the lazy-resolver compile contract names it).
pub fn debug_on_next_call_bool_fwd_for_test(
    obarray: &mut crate::symbol::Obarray,
    id: crate::intern::SymId,
) -> Option<&'static crate::forward::LispBoolFwd> {
    obarray.debug_on_next_call_bool_fwd(id)
}
/// Value-FENCE row (peer 16:44:46Z): typed serial-fixture hook arming or
/// disarming the concurrent SATB barrier without a GC thread. The original
/// method is `pub(crate) unsafe fn`; this delegate keeps its contract.
///
/// # Safety
/// No concurrent marker has been launched on this heap and none can run
/// during the call (serial synthetic fixture only). `on = false` disables the
/// SATB barrier, so a live marker would miss and free reachable objects.
#[cfg(any(test, feature = "test-support"))]
pub unsafe fn set_concurrent_active_for_test(heap: &mut crate::tagged::gc::TaggedHeap, on: bool) {
    // SAFETY: the caller upholds the serial-fixture contract above.
    unsafe { heap.set_concurrent_active_for_test(on) }
}

/// Value-FENCE row (peer 16:44:46Z): typed serial-fixture hook consuming the
/// published SATB handoff. The original method is `pub(crate) unsafe fn`.
///
/// # Safety
/// No concurrent marker has been launched on this heap and none can run
/// during the call; the published SATB handoff belongs to this serial
/// synthetic fixture and is consumed exactly once.
#[cfg(any(test, feature = "test-support"))]
pub unsafe fn take_satb_shared_for_test(
    heap: &mut crate::tagged::gc::TaggedHeap,
) -> Vec<crate::tagged::value::TaggedValue> {
    // SAFETY: the caller upholds the serial-fixture contract above.
    unsafe { heap.take_satb_shared_for_test() }
}
/// Value-FENCE row (Ruling P1-VALUE-CLASSES C1): owner-side admission witness
/// constructor for the relocated parallel transport tests. The witness is
/// metadata only and the original method stays crate-internal.
pub fn owner_root_admission_for_test<'a>(
    identity: crate::tagged::transport::ParallelRootIdentity,
    roots: &'a [crate::tagged::transport::SharedRoot],
) -> crate::tagged::gc::mutator_registration::OwnerRootAdmission<'a> {
    crate::tagged::gc::mutator_registration::OwnerRootAdmission::for_test(identity, roots)
}

/// Value-FENCE row (Ruling P1-VALUE-CLASSES C1): stopped-side witness
/// constructor. The original method stays crate-internal.
///
/// # Safety
/// The returned witness gates stopped-world root reads: the caller must hold
/// the stopped `AllStopped` world for this exact identity and epoch, and no
/// other owner may resume while the witness lives.
pub unsafe fn stopped_root_admission_for_test<'a>(
    identity: crate::tagged::transport::ParallelRootIdentity,
    epoch: u64,
) -> crate::tagged::gc::mutator_registration::StoppedRootAdmission<'a> {
    // SAFETY: the caller upholds the identical stopped-world contract above.
    unsafe {
        crate::tagged::gc::mutator_registration::StoppedRootAdmission::for_test(identity, epoch)
    }
}

/// Value-FENCE row (peer 17:27:27Z C1 TOKEN): the epoch-retention mints a
/// `FacadeEpochRetention` token (it retains exclusion through the final
/// publication or reader obligation), so it is fenced rather than VIS. Safe:
/// a retention only delays; it cannot unsound anything.
pub fn test_root_epoch_retention<'heap, K: crate::tagged::gc::EraKeeper>(
    era: &ProofEra<'heap, K>,
) -> crate::tagged::gc::FacadeEpochRetention {
    era.test_root_epoch_retention()
}

/// P1-HOOKS-2 FENCE (peer 17:04:43Z pre-signed): record a change of a thread
/// projection's logical redirect. Identical signature to
/// `ThreadProjectionDescriptor::set_arm`; the method stays `pub(crate)` and
/// the `Cell` write stays inside this crate.
pub fn set_arm(
    descriptor: &crate::forward::ThreadProjectionDescriptor,
    arm: crate::forward::CanonicalArm,
) {
    descriptor.set_arm(arm);
}
/// Value-FENCE row (peer 17:15Z fn_stamps split): the thread-local fn-stamps
/// policy switch for the relocated Context tests. Safe: it toggles only the
/// test override and touches no heap state.
pub fn force_fn_stamps_for_test(on: Option<bool>) {
    crate::symbol::force_fn_stamps_for_test(on);
}

/// Value-FENCE row (C1/C3): assertion-only plant of a same-heap immediate
/// into the original owner's root scratch slot for the relocated
/// symbol_globals transition fixtures.
///
/// # Safety
/// `value` must be an immediate (fixnum, nil or t) allocated in this owner's
/// heap, `root` must name one of this owner's rooted slots, and the owner must
/// be exclusively borrowed by `run`.
pub unsafe fn plant_scalar_root_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
    root: &crate::tagged::gc::mutator_registration::OwnerRootSlot<'_>,
    value: crate::tagged::value::TaggedValue,
) {
    run.plant_scalar_root_for_test(root, value);
}

/// Value-FENCE row (REGWAIT-GAP): retrying activation for the relocated
/// symbol_globals history fixtures; the same Busy-retry loop as the in-crate
/// test helper and no new authority.
pub fn activate_retry_for_test<'era>(
    mut permit: crate::tagged::gc::mutator_registration::MutatorPermit<'era>,
    heap: crate::tagged::gc::HeapIdentity,
    owner: u64,
) -> crate::tagged::gc::mutator_registration::Mutator<'era> {
    loop {
        match permit.activate(heap, owner) {
            Ok(token) => return token,
            Err(failed) if failed.reason == crate::tagged::gc::RegistrationError::Busy => {
                permit = failed.into_permit();
                std::thread::yield_now();
            }
            Err(failed) => panic!("activation failed: {failed:?}"),
        }
    }
}

/// Value-FENCE row (REGWAIT-GAP): retrying retirement for the same fixtures.
pub fn finish_retry_for_test(
    mut token: crate::tagged::gc::mutator_registration::Mutator<'_>,
    epoch: std::num::NonZeroU64,
) {
    loop {
        match token.finish(epoch) {
            Ok(()) => return,
            Err(failed) if failed.reason == crate::tagged::gc::RegistrationError::Busy => {
                token = failed.into_owner();
                std::thread::yield_now();
            }
            Err(failed) => panic!("retirement failed: {failed:?}"),
        }
    }
}

/// FENCE (N2 typed exhaustion, same shape as
/// `RunAccess::exhaust_root_slot_generation_for_test`): force the canonical
/// function epoch to the exact pre-wrap value the wrap-skip executor fixtures
/// use. There is deliberately no general epoch setter.
pub fn exhaust_function_epoch_for_test(obarray: &mut crate::symbol::Obarray) {
    obarray.set_function_epoch_for_test(u64::MAX - 1);
}

/// FENCE (C3 write): set the symbol-authority poison bit on this live owner.
/// The original stays `pub(crate)`; every subsequent symbol admission is
/// refused while the bit is set, exactly as the fixture requires.
pub fn set_symbol_poison_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
    poisoned: bool,
) {
    run.set_symbol_poison_for_test(poisoned);
}

/// FENCE (C1 forge): turn a bytecode function into the saved-image marker
/// shape the bytecode fixture needs: no resident ops and the sealed marker
/// set. The production state stays private; this typed forge reproduces the
/// exact fixture state without a general field setter.
pub fn forge_pdump_marker_ops_for_test(
    function: &mut crate::tagged::bytecode::chunk::ByteCodeFunction,
) {
    function.forge_pdump_marker_ops_for_test();
}

/// Value-FENCE row (C1): a forged-index copy of a root home for the relocated
/// transition fixtures. Safe: every use still goes through the keeper's
/// binding check, so an invalid index can only be refused.
pub fn owner_root_slot_with_index_for_test<'era>(
    root: &crate::tagged::gc::mutator_registration::OwnerRootSlot<'era>,
    index: usize,
) -> crate::tagged::gc::mutator_registration::OwnerRootSlot<'era> {
    root.with_index_for_test(index)
}

/// Value-FENCE row (C5 mint, LEAD 10:4xZ SHAPE (B)): reserve one NIL census
/// home on the live owner. The original `RunAccess::private_nil_root` stays
/// `pub(crate)`; this delegate is its only cross-crate route, so the
/// ExecutionTestHomes adapter hosted in eval can mint its operand home.
pub fn nil_root_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
) -> Result<
    crate::tagged::gc::mutator_registration::OwnerRootSlot<'era>,
    crate::tagged::gc::RegistrationError,
> {
    run.private_nil_root()
}

/// Value-FENCE row (N2 counter reset): zero the position-conversion scan-step
/// counter for the moved eval/corpus fixtures. The counter itself stays a
/// thread-local in `emacs_char`; this typed reset is the only cross-crate
/// route, and every increment call site is under the same
/// `cfg(any(test, feature = "test-support"))` gate.
pub fn reset_position_conversion_scan_steps_for_test() {
    crate::emacs_char::reset_position_conversion_scan_steps_for_test()
}

/// Value-FENCE row (N2 counter reset): zero the interval-balance call counter.
pub fn reset_interval_balance_calls_for_test() {
    crate::value::text_props::reset_interval_balance_calls_for_test()
}

/// Value-FENCE row (N2 counter reset): zero the text-property table clone
/// counter.
pub fn reset_text_property_table_clones_for_test() {
    crate::value::text_props::reset_text_property_table_clones_for_test()
}

/// Value-FENCE row (N2 counter reset): zero the bytecode-function clone
/// counter. Both increment sites (`Clone` and the in-place instance writer)
/// sit under the same instrumentation gate.
pub fn reset_bytecode_function_clone_count_for_test() {
    crate::tagged::bytecode::chunk::reset_bytecode_function_clone_count_for_test()
}

/// Value-FENCE row (N2 counter reset): zero the plain-value slot-visit counter.
/// All five increment sites are under the same instrumentation gate.
pub fn reset_plain_value_slot_visits() {
    crate::symbol::reset_plain_value_slot_visits()
}

/// Value-FENCE row (P1-CLASSES-ALL-CRATES A/N2): test-mode Tier-0 pin write
/// reached from eval's signed JIT fence delegate. The Atomic field stays
/// private and is never handed out. Deliberately a bare setter rather than a
/// scoped guard: the JIT A/B fixtures move the owning `ByteCodeFunction` into
/// a `Value` immediately after pinning, so a borrow-carrying guard cannot
/// outlive the move; the pin is per-function state and never shared heap
/// state, so a panicking test can leak only its own function's pin.
pub fn set_force_interpret_for_test(state: &crate::value::jit_runtime::RuntimeState, on: bool) {
    state.set_force_interpret_for_test(on);
}
/// Value-FENCE row (P1-CLASSES-ALL-CRATES C3): typed GEN1 policy write for the
/// re-homed rows-authority fixture.
///
/// # Safety
/// The heap view must be the owning mutator's exclusive view, obtained through
/// the Context's sanctioned accessor, and no collector or compiled leaf may run
/// across the barrier switch.
#[cfg(any(test, feature = "test-support"))]
pub unsafe fn set_generational_enabled_for_test(
    heap: &mut crate::tagged::gc::TaggedHeap,
    enabled: bool,
) {
    // SAFETY: the caller upholds the exclusive-quiescent-heap contract above.
    unsafe { heap.set_generational_enabled(enabled) }
}

/// Value-FENCE row (PC1 ruling 11:5xZ): read-only GEN1 policy observation
/// for the re-homed rows fixtures. App-crate tests use this instead of the
/// fenced hot-path getter, so the name-keyed fence has no app-crate callers.
#[cfg(any(test, feature = "test-support"))]
pub fn generational_enabled_for_test(heap: &crate::tagged::gc::TaggedHeap) -> bool {
    heap.tagged_heap_generational_enabled()
}

/// Value-FENCE row (P1-CLASSES-ALL-CRATES C3): start a real mark cycle so the
/// re-homed rows oracle can observe the collector-busy refusal.
///
/// # Safety
/// The heap must be exclusively borrowed and quiescent with no concurrent
/// marker running; the call asserts that precondition and starts a major cycle.
#[cfg(any(test, feature = "test-support"))]
pub unsafe fn begin_collection_for_test(heap: &mut crate::tagged::gc::TaggedHeap) {
    // SAFETY: the caller upholds the quiescent-heap contract above.
    unsafe { heap.begin_collection() }
}

/// Value-FENCE row (C1 write): one cold batch grant attempt for the relocated
/// cons protocol tests. Both the fence and the original are unsafe.
///
/// # Safety
/// The caller owns the granted batch (or drops the tails) before any further
/// allocation or barrier-relevant write.
pub unsafe fn cons_probe_tail_batch_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
) -> Result<(), crate::tagged::gc::RegistrationError> {
    // SAFETY: the caller upholds the granted-batch contract above.
    unsafe { run.cons_probe_tail_batch_for_test() }
}

/// Value-FENCE row (C1 write): drops every held tail without publishing; the
/// catalog refuses afterwards. Both the fence and the original are unsafe.
///
/// # Safety
/// The caller must expect no further storage work from this owner.
pub unsafe fn cons_drop_tails_for_test(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, '_>,
) {
    // SAFETY: the caller upholds the no-further-storage-work contract above.
    unsafe { run.cons_drop_tails_for_test() }
}

/// Value-FENCE row (C3): republish the write-barrier owner window after a
/// serial fixture mutates one of its inputs. Both the fence and the original
/// are unsafe.
///
/// # Safety
/// The heap is exclusively borrowed by the executing mutator (or the collector
/// holds the stopped-world handshake), and no other mutator or compiled leaf
/// may run while the window inputs change.
pub unsafe fn publish_barrier_window_for_test(heap: &mut crate::tagged::gc::TaggedHeap) {
    // SAFETY: the caller upholds the identical publication contract above.
    unsafe { heap.publish_barrier_window_for_test() }
}

/// VALUE-A3-COLLAPSE row (peer 00:36:00Z (ii)): the mint over the private
/// `LispString::from_borrowed_bytes` constructor. It builds a `LispString`
/// over caller-supplied raw memory, so it is an authority (mint) capability,
/// not a copied read; the mis-classed VIS row is removed.
///
/// # Safety
/// Same pointer/length/validation contract as the private constructor:
/// `ptr` must point to a live, NUL-terminated byte buffer of at least `len`
/// bytes (`ptr.add(len) == 0`), the bytes must remain mapped and unmodified
/// for the returned string's lifetime (and for the process lifetime when
/// `static_rodata` is true), and `size`/`size_byte` must describe the buffer
/// exactly (`len == payload_len_for(size, size_byte)`; `size_byte` is
/// negative for unibyte storage and otherwise the byte count, with `size` the
/// decoded character count for a multibyte payload).
pub unsafe fn heap_types__LispString__from_borrowed_bytes(
    ptr: *const u8,
    len: usize,
    size: usize,
    size_byte: i64,
    static_rodata: bool,
) -> crate::heap_types::LispString {
    // SAFETY: the caller upholds the constructor's contract above.
    unsafe {
        crate::heap_types::__test_support::lisp_string_from_borrowed_bytes(
            ptr,
            len,
            size,
            size_byte,
            static_rodata,
        )
    }
}

/// Construct a legacy-GC fixture with the knob selected before heap creation,
/// restoring the caller's environment on return or unwind.  The knob is
/// process-global state that the production heap constructor reads, so this
/// closure-scoped write is an AUTHORITY seam, not a VIS read; the body in
/// `test_utils` stays `pub(crate)`, making this fence the sole public route.
pub fn with_legacy_gc<T>(create: impl FnOnce() -> T) -> T {
    crate::test_utils::with_legacy_gc(create)
}

// ---------------------------------------------------------------------------
// FENCES (LEAD 18:1xZ, VIS-FREE-FN-RESET-ALLOWLIST 14 -> 0): every free-fn
// reset of a TLS/static counter is a write, so its owning crate's signed
// authority module is the only cross-crate route. The originals stay
// pub(crate) with no A-prime; these delegates are their sole public path.
// ---------------------------------------------------------------------------

/// FENCE (N2 TLS/static reset): zero the overlay-storage visit counter on the
/// calling thread. The original stays pub(crate) in the cfg-gated overlay
/// instrumentation module.
pub fn reset_overlays_at_node_visit_count() {
    crate::heap_types::overlay_storage::instrumentation::reset_overlays_at_node_visit_count();
}

/// FENCE (N2 TLS/static reset): zero the overlay-iterator frame-push counter.
pub fn reset_overlay_iterator_frame_push_count() {
    crate::heap_types::overlay_storage::instrumentation::reset_overlay_iterator_frame_push_count();
}

/// FENCE (N2 TLS/static reset): zero the overlay shift-node visit counter.
pub fn reset_overlay_shift_node_visit_count() {
    crate::heap_types::overlay_storage::instrumentation::reset_overlay_shift_node_visit_count();
}

/// FENCE (N2 TLS/static reset): zero the endpoint search node-visit counter.
pub fn reset_endpoint_search_node_visit_count() {
    crate::heap_types::overlay_storage::instrumentation::reset_endpoint_search_node_visit_count();
}

/// FENCE (N2 TLS/static reset): zero the endpoint search summary-shift counter.
pub fn reset_endpoint_search_summary_shift_count() {
    crate::heap_types::overlay_storage::instrumentation::reset_endpoint_search_summary_shift_count(
    );
}

/// FENCE (N2 TLS/static reset): zero the overlay full-enumeration visit counter.
pub fn reset_overlay_full_enumeration_visit_count() {
    crate::heap_types::overlay_storage::instrumentation::reset_overlay_full_enumeration_visit_count(
    );
}

/// FENCE (N2 TLS reset): zero the plist-get walk counter on the calling
/// thread. The original stays pub(crate) in symbol::plist_core.
pub fn reset_plist_get_walks() {
    crate::symbol::plist_core::reset_plist_get_walks();
}

/// FENCE (N2 TLS reset): zero the symbol-with-position plist comparison
/// counter. The original stays pub(crate) in symbol::plist_core.
pub fn reset_symbol_with_pos_plist_comparisons() {
    crate::symbol::plist_core::reset_symbol_with_pos_plist_comparisons();
}

/// FENCE (peer #297): install the calling thread's probe-scope suppression
/// guard. The guard writes thread-local state and is an unnameable
/// `!Send`/`!Sync` RAII value. The original stays `pub(crate)`; this fence
/// is its only public path.
#[must_use = "hold this guard for the fixture's entire probe-free scope"]
pub fn suppress_for_test() -> impl Drop {
    crate::tagged::gc::mutator_registration::a8_probes::suppress_for_test()
}

/// FENCE (lead #694 L-b): hold the process symbol-registry write lock for
/// `body`. The original `with_registry_writer_for_test` stays `pub(crate)`.
#[doc(hidden)]
pub fn with_symbol_registry_writer_for_test(body: impl FnOnce()) {
    crate::intern::parallel::with_registry_writer_for_test(body);
}

/// FENCE (lead #694 L-b): prepare a C6 name capability and return its symbol
/// handle. The original `prepare_name_symbol` stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn prepare_name_symbol_for_test<K: crate::tagged::gc::mutator_registration::EraKeeper>(
    era: &crate::tagged::gc::ProofEra<'_, K>,
    key: &crate::tagged::gc::mutator_registration::SymbolHandle,
) -> Result<
    crate::tagged::gc::mutator_registration::SymbolHandle,
    crate::tagged::gc::mutator_registration::FunctionError,
> {
    crate::tagged::gc::mutator_registration::name_ops::prepare_name_symbol(era, key)
}

/// FENCE (lead #694 L-b): `RunAccess::sym_name_fset_into` for a handle from
/// [`prepare_name_symbol_for_test`]. The original stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn sym_name_fset_into_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
    symbol: crate::tagged::gc::mutator_registration::SymbolHandle,
    src: &crate::tagged::gc::mutator_registration::OwnerRootSlot<'era>,
    dst: &mut crate::tagged::gc::mutator_registration::OwnerRootSlot<'era>,
) -> Result<(), crate::tagged::gc::mutator_registration::FunctionError> {
    crate::tagged::gc::mutator_registration::name_ops::sym_name_fset_into(run, symbol, src, dst)
}

/// FENCE (lead #694 L-b): `RunAccess::sym_name_call_begin` for a handle from
/// [`prepare_name_symbol_for_test`]. The original stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn sym_name_call_begin_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
    symbol: crate::tagged::gc::mutator_registration::SymbolHandle,
    cache: &mut crate::tagged::gc::mutator_registration::FunctionCallCache<'era>,
    activation: &mut crate::tagged::gc::mutator_registration::FunctionActivation<'era>,
) -> Result<(), crate::tagged::gc::mutator_registration::FunctionError> {
    crate::tagged::gc::mutator_registration::name_ops::sym_name_call_begin(
        run, symbol, cache, activation,
    )
}

/// FENCE (lead #694 L-b): handoff resume with the control gate uncontended.
/// The caller supplies the era. Assertions stay in the value body. The
/// original `handoff_resume_uncontended` stays `pub(crate)`.
#[cfg(all(not(feature = "gc-memory-telemetry"), not(miri)))]
#[doc(hidden)]
pub fn handoff_resume_uncontended_for_test<
    K: crate::tagged::gc::mutator_registration::EraKeeper,
>(
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    crate::tagged::gc::mutator_registration::restricted_lisp::globals::handoff::worker::contention_tests::handoff_resume_uncontended(era);
}

/// FENCE (lead #694 L-b): handoff resume when the control gate is held past
/// the yield budget. The original `handoff_resume_outlasts_yield_budget`
/// stays `pub(crate)`.
#[cfg(all(not(feature = "gc-memory-telemetry"), not(miri)))]
#[doc(hidden)]
pub fn handoff_resume_outlasts_yield_budget_for_test<
    K: crate::tagged::gc::mutator_registration::EraKeeper,
>(
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    crate::tagged::gc::mutator_registration::restricted_lisp::globals::handoff::worker::contention_tests::handoff_resume_outlasts_yield_budget(era);
}

/// FENCE: one iteration of a re-homed function-executor test. The core shell
/// admits the era. The original dispatcher stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn run_functions_executor_test_for_test<
    K: crate::tagged::gc::mutator_registration::EraKeeper,
>(
    name: &str,
    era: crate::tagged::gc::ProofEra<'_, K>,
    ids: &[crate::intern::SymId],
    kind: crate::tagged::gc::PlainSymbolBacking,
    row: usize,
) {
    crate::tagged::gc::mutator_registration::restricted_lisp::run_functions_executor_test(
        name, era, ids, kind, row,
    );
}

/// FENCE: one iteration of a re-homed function-handoff test. `world_roots`
/// is the caller's World walk (the eval baton walker). The original
/// dispatcher stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn run_function_handoff_test_for_test<K, W>(
    name: &str,
    era: crate::tagged::gc::ProofEra<'_, K>,
    ids: &[crate::intern::SymId],
    kind: crate::tagged::gc::PlainSymbolBacking,
    count: usize,
    world_roots: W,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    crate::tagged::gc::mutator_registration::restricted_lisp::run_function_handoff_test(
        name,
        era,
        ids,
        kind,
        count,
        world_roots,
    );
}

/// FENCE: the peer-young collection body. The core shell admits the era and
/// passes the serial cons graph. The original dispatcher stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn run_peer_young_collection_test_for_test<K, W>(
    name: &str,
    era: crate::tagged::gc::ProofEra<'_, K>,
    root: crate::tagged::transport::SharedRoot,
    words: [usize; 2],
    world_roots: W,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    crate::tagged::gc::mutator_registration::run_peer_young_collection_test(
        name,
        era,
        root,
        words,
        world_roots,
    );
}

/// FENCE: one re-homed restricted-lisp test. The core shell boots the
/// evaluator, shares planted words, admits the era, and walks World roots.
/// The original dispatcher stays `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn run_restricted_lisp_test_for_test<K, W>(
    name: &str,
    eval: &mut dyn FnMut(&str) -> Result<crate::tagged::value::TaggedValue, String>,
    share: &mut dyn FnMut(
        crate::tagged::value::TaggedValue,
    ) -> crate::tagged::transport::SharedRoot,
    admit: &mut dyn FnMut(&str, &str, &mut dyn for<'e> FnMut(&crate::tagged::gc::ProofEra<'e, K>)),
    bytes_since_gc: &mut dyn FnMut() -> usize,
    generational: bool,
    world_roots: W,
    switch_on: bool,
    switch_sites: &mut dyn FnMut() -> usize,
    switch_compile: &mut dyn FnMut(
        &[crate::tagged::bytecode::opcode::Op],
        &[crate::tagged::value::TaggedValue],
        &[crate::tagged::bytecode::chunk::GnuByteOffsetMapEntry],
    ),
    switch_call: &mut dyn FnMut(&[crate::tagged::value::TaggedValue]) -> (u8, usize, String),
    switch_flow: &mut dyn FnMut() -> Option<crate::error::Flow>,
    switch_print: &mut dyn FnMut(&crate::tagged::value::TaggedValue) -> String,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    crate::tagged::gc::mutator_registration::restricted_lisp::run_restricted_lisp_test(
        name,
        eval,
        share,
        admit,
        bytes_since_gc,
        generational,
        world_roots,
        switch_on,
        switch_sites,
        switch_compile,
        switch_call,
        switch_flow,
        switch_print,
    );
}

/// FENCE: one re-homed intern-executor test. The core shell opens the symbol
/// era and passes the baton World walk. The original dispatcher stays
/// `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn run_intern_executor_test_for_test<K, W>(
    name: &str,
    name_inputs: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &str,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    name_inputs_init: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &str,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    intern_race: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &[String],
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    world_roots: W,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    crate::tagged::gc::mutator_registration::restricted_lisp::run_intern_executor_test(
        name,
        name_inputs,
        name_inputs_init,
        intern_race,
        world_roots,
    );
}

/// FENCE (lead #768): the only cross-crate construction of
/// [`FreshRegistryFacts`](crate::symbol::parallel::test_facts::FreshRegistryFacts).
/// Copies the snapshot and replaces `bytes`. The fields stay
/// pub(crate); cross-crate reads use the Copy getters.
#[doc(hidden)]
pub fn fresh_registry_facts_with_bytes(
    facts: crate::symbol::parallel::test_facts::FreshRegistryFacts,
    bytes: usize,
) -> crate::symbol::parallel::test_facts::FreshRegistryFacts {
    crate::symbol::parallel::test_facts::FreshRegistryFacts { bytes, ..facts }
}

/// The fence's own behavioural tests are a DECLARED CHILD of the authority
/// module, not a physical sibling: TA2's definition surface is this module
/// tree, so the tests' `crate::test_support::authority` references stay
/// inside the signed fence. A plain child declaration keeps the rustc and
/// guard resolvers identical (`test_support/authority/`).
#[cfg(test)]
mod test_heap_scope_tests;

/// Setter fence for `SignalData::search_complete` (peer #816 write arm): the
/// moved signal-payload test rewrites the payload's resume-search state in
/// place. The field stays a read surface.
pub fn set_signal_search_complete_for_test(
    signal: &mut crate::error::SignalData,
    search_complete: bool,
) {
    signal.set_search_complete_for_test(search_complete);
}

/// Setter fence for `SignalData::selected_resume` (peer #816 write arm): the
/// moved signal-payload test records the selected resume target in place. The
/// field stays a read surface.
pub fn set_signal_selected_resume_for_test(
    signal: &mut crate::error::SignalData,
    selected_resume: Option<crate::value::flow_vocab::ResumeTarget>,
) {
    signal.set_selected_resume_for_test(selected_resume);
}

/// Canonical symbol write for moved tests. `Obarray::set_internal` stays
/// `pub(crate)` under test-support; this fence is the only public path.
pub fn set_obarray_internal_for_test(
    obarray: &mut crate::symbol::Obarray,
    id: crate::intern::SymId,
    value: crate::value::Value,
) {
    obarray.set_internal(id, value);
}

/// Setter fence for `HashTableStorage::switch_epoch` (VIS read-arm series,
/// lead #1052): the field stays pub(crate) for the crate's own writers
/// (tagged/mutate.rs, the JIT offset) and the moved switch-plan oracle forces
/// an epoch change through this fence instead of the field.
pub fn set_hash_table_switch_epoch_for_test(
    storage: &mut crate::value::HashTableStorage,
    value: u64,
) {
    storage.switch_epoch = value;
}

/// FENCE: one re-homed global-executor test. The core shell opens the symbol
/// era and passes the baton World walk. The original dispatcher stays
/// `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn run_globals_executor_test_for_test<K, W>(
    name: &str,
    symbol_era: &mut dyn FnMut(
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, crate::intern::SymId),
    ),
    symbol_backing: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, crate::intern::SymId),
    ),
    symbol_backings: &mut dyn FnMut(
        &[crate::tagged::gc::PlainSymbolBacking],
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    eval_then_admit: &mut dyn FnMut(
        &str,
        &mut dyn for<'e> FnMut(&crate::tagged::gc::ProofEra<'e, K>),
    ),
    world_roots: W,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    crate::tagged::gc::mutator_registration::restricted_lisp::run_globals_executor_test(
        name,
        symbol_era,
        symbol_backing,
        symbol_backings,
        eval_then_admit,
        world_roots,
    );
}

/// FENCE: one re-homed combined-executor test. The core shell opens the symbol
/// era and passes the baton World walk. The original dispatcher stays
/// `pub(crate)`.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn run_combined_executor_test_for_test<K, W>(
    name: &str,
    symbol_backing: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, crate::intern::SymId),
    ),
    symbol_functions: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    symbol_functions_init: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    symbol_name_inputs_init: &mut dyn FnMut(
        crate::tagged::gc::PlainSymbolBacking,
        &str,
        &mut dyn FnMut(&mut crate::symbol::Obarray, &[crate::intern::SymId]),
        &mut dyn for<'e> FnMut(crate::tagged::gc::ProofEra<'e, K>, Vec<crate::intern::SymId>),
    ),
    world_roots: W,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
    W: Copy
        + for<'k, 's, 'e, 'o> Fn(
            &'k K,
            &'s mut crate::tagged::gc::mutator_registration::AllStopped<'e>,
            &'o mut Vec<crate::tagged::value::TaggedValue>,
        ) -> Result<
            Vec<*mut *mut crate::tagged::header::MarkerObj>,
            crate::tagged::gc::mutator_registration::RendezvousError,
        >,
{
    crate::tagged::gc::mutator_registration::restricted_lisp::run_combined_executor_test(
        name,
        symbol_backing,
        symbol_functions,
        symbol_functions_init,
        symbol_name_inputs_init,
        world_roots,
    );
}

/// FENCE: one re-homed NMJIT construction test. The core shell opens the
/// symbol era and passes the World keeper preparation. The original
/// dispatcher stays `pub(crate)`.
#[doc(hidden)]
pub fn run_construction_nmjit_test_for_test<K>(
    name: &str,
    with_symbol_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ProofEra<'e, K>)),
    prepare: &mut dyn FnMut(
        &K,
    ) -> Result<
        (
            crate::tagged::gc::FacadeEpochRetention,
            crate::symbol::parallel::SymbolBacking,
        ),
        crate::tagged::gc::RegistrationError,
    >,
    held_here: &dyn Fn(&K) -> bool,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper,
{
    crate::tagged::gc::mutator_registration::run_construction_nmjit_test(
        name,
        with_symbol_era,
        prepare,
        held_here,
    );
}

/// FENCE: one re-homed mutator-registration era test. The core shell opens
/// the mutator era and passes the World keeper preparation. The original
/// dispatcher stays `pub(crate)`. The generation-counter test stays in value.
#[doc(hidden)]
pub fn run_mutator_registration_era_test_for_test<K>(
    name: &str,
    with_mutator_era: &mut dyn FnMut(&mut dyn for<'e> FnMut(ProofEra<'e, K>)),
    prepare: &mut dyn FnMut(
        &K,
    ) -> Result<
        (
            crate::tagged::gc::FacadeEpochRetention,
            crate::symbol::parallel::SymbolBacking,
        ),
        crate::tagged::gc::RegistrationError,
    >,
) where
    K: crate::tagged::gc::mutator_registration::EraKeeper + std::fmt::Debug,
{
    crate::tagged::gc::mutator_registration::run_mutator_registration_era_test(
        name,
        with_mutator_era,
        prepare,
    );
}

/// FENCE: one re-homed generational major test. The core shell installs the
/// eval scratch-root adapters, the JIT cons stores, and the edit marker-chain
/// host. The original dispatcher stays `pub(crate)`.
#[doc(hidden)]
pub fn run_generational_major_test_for_test(
    name: &str,
    save: fn() -> usize,
    push: fn(crate::value::Value) -> usize,
    set: fn(usize, crate::value::Value),
    restore: fn(usize),
    jit_setcar: unsafe extern "C" fn(*mut u8, i64, i64) -> i64,
    jit_setcdr: unsafe extern "C" fn(*mut u8, i64, i64) -> i64,
    new_marker_state: fn() -> *mut (),
    marker_splice: unsafe fn(*mut (), *mut crate::tagged::header::MarkerObj),
    marker_head: unsafe fn(*mut ()) -> *mut *mut crate::tagged::header::MarkerObj,
    marker_walk: unsafe fn(*mut ()) -> Vec<*mut crate::tagged::header::MarkerObj>,
    marker_drop: unsafe fn(*mut ()),
) {
    crate::tagged::gc::run_generational_major_test(
        name,
        save,
        push,
        set,
        restore,
        jit_setcar,
        jit_setcdr,
        new_marker_state,
        marker_splice,
        marker_head,
        marker_walk,
        marker_drop,
    );
}

/// FENCE: one re-homed ownership test that needs an upper adapter. The core
/// shell passes the edit `clear-string` builtin and the eval subr-metadata
/// mint. The original dispatcher stays `pub(crate)`. The other ownership
/// rows stay `#[test]` in value.
#[doc(hidden)]
pub fn run_ownership_upper_test_for_test(
    name: &str,
    clear_string: fn(Vec<crate::value::Value>) -> crate::error::EvalResult,
    leaked_subr: fn() -> (
        crate::value::Value,
        *mut crate::tagged::header::SubrMetadata,
    ),
) {
    crate::tagged::gc::run_ownership_upper_test(name, clear_string, leaked_subr);
}

/// FENCE: synthetic concurrent-mark scope for a serial fixture. The guard used
/// to lend an evaluator `Context`, which this crate cannot name. The caller
/// passes the heap pointer and the inhibit depth it already read; the closure
/// runs while the SATB barrier is armed, and drop clears it, including on
/// panic. The original flag sequence stays in the value guard.
///
/// # Safety
/// `heap` must stay live for this call and name the heap whose inhibit depth
/// was passed. No concurrent marker has been launched on it and none can run
/// during the call. The closure may mutably use that heap; the pointer stays
/// dormant until the guard drops. Disarming the barrier (`on = false`) on a
/// live marker would miss stores and free reachable objects.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub unsafe fn with_synthetic_concurrent_barrier_for_test(
    heap: *mut crate::tagged::gc::TaggedHeap,
    gc_inhibit_depth: usize,
    body: impl FnOnce(),
) {
    // SAFETY: the caller upholds the serial-fixture contract above.
    let _guard = unsafe {
        crate::tagged::gc::test_heap::SyntheticConcurrentBarrier::enter(heap, gc_inhibit_depth)
    };
    body();
}

/// FENCE: process collection-read configuration. The original stays
/// `pub(crate)`. This is the only public path; it writes the process gate.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn initialize_collection_reads_for_test() {
    crate::tagged::collection_reads::initialize_for_test();
}

/// FENCE: zero the observation-access counter. The original stays
/// `pub(crate)`. This is the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn reset_observation_state_accesses_for_test() {
    crate::tagged::collection_reads::reset_observation_state_accesses_for_test();
}

/// FENCE: hold the cold capture state across `body`. The original stays
/// `pub(crate)`. This is the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn with_cold_capture_for_test<R>(body: impl FnOnce() -> R) -> R {
    crate::tagged::collection_reads::with_cold_state_held(body)
}

/// FENCE: write a cons car through the store. The original stays
/// `pub(crate)`. This is the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn set_cons_car_in_store_for_test(
    store: &crate::tagged::collection_reads::StoreState,
    cell: crate::tagged::value::TaggedValue,
    value: crate::tagged::value::TaggedValue,
) -> bool {
    crate::tagged::mutate::set_cons_car_in_store_for_test(store, cell, value)
}

/// FENCE: write a record slot through the store. The original stays
/// `pub(crate)`. This is the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn set_record_slot_in_store_for_test(
    store: &crate::tagged::collection_reads::StoreState,
    value: crate::tagged::value::TaggedValue,
    index: usize,
    item: crate::tagged::value::TaggedValue,
) -> bool {
    crate::tagged::mutate::set_record_slot_in_store_for_test(store, value, index, item)
}

/// FENCE: write a string byte through the store. The original stays
/// `pub(crate)`. This is the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn set_string_byte_same_char_count_in_store_for_test(
    store: &crate::tagged::collection_reads::StoreState,
    value: crate::tagged::value::TaggedValue,
    byte_pos: usize,
    byte: u8,
) -> bool {
    crate::tagged::mutate::set_string_byte_same_char_count_in_store_for_test(
        store, value, byte_pos, byte,
    )
}

/// FENCE: write a vector slot through the store. The original stays
/// `pub(crate)`. This is the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn set_vector_slot_in_store_for_test(
    store: &crate::tagged::collection_reads::StoreState,
    value: crate::tagged::value::TaggedValue,
    index: usize,
    item: crate::tagged::value::TaggedValue,
) -> bool {
    crate::tagged::mutate::set_vector_slot_in_store_for_test(store, value, index, item)
}

/// FENCE: mutate bytecode data. The method stays `pub(crate)`. This is
/// the only public path.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn with_bytecode_data_mut_for_test<R>(
    value: crate::tagged::value::TaggedValue,
    f: impl FnOnce(&mut crate::tagged::bytecode::chunk::ByteCodeFunction) -> R,
) -> Option<R> {
    value.with_bytecode_data_mut_for_test(f)
}
