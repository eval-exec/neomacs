//! Reference-free GEN1 bootstrap handoff. This is not a mutator registration
//! surface, a reusable stopped interval, or permission to move a collector.
//!
//! Value does not name an evaluator `Context`. The unsafe constructor states
//! the precondition (the source shell is already consumed and no collector
//! frame is live). Eval's adapter is what makes that precondition true.

use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::thread::ThreadId;

use crate::tagged::collection_reads::{SourceObservationError, SourceObservationsDetached};
use crate::tagged::transport::{PreparedRootBatch, RootBatchLease, SharedRoot};
use crate::tagged::value::TaggedValue;

use super::super::{
    EraWorldLoan, FrozenFunctionRoots, FrozenLegacyRoots, Gen1Admission, HeapIdentity, WorldLoan,
    clear_tagged_heap_if_installed, tagged_heap_is_installed,
};
use super::QuiescentCollector;

/// The original allocation root, never a collector-reference-derived pointer.
///
/// Identity metadata alone cannot construct this provenance-bearing designator.
/// The only constructor is unsafe, the field is private, and the value is not
/// `Send`, `Sync`, `Clone`, or `Copy`.
#[doc(hidden)]
pub struct HeapRootDesignator(NonNull<super::super::TaggedHeap>);

impl HeapRootDesignator {
    /// # Safety
    /// `root` is the original `AliasableBox` provenance root, retained by the
    /// matching [`EraWorldLoan`]. No reference derived from another root may
    /// be retained across a reconstruction made from this designator.
    #[doc(hidden)]
    pub unsafe fn from_original(root: NonNull<super::super::TaggedHeap>) -> Self {
        Self(root)
    }

    #[doc(hidden)]
    pub fn as_non_null(&self) -> NonNull<super::super::TaggedHeap> {
        self.0
    }
}

static_assertions::assert_not_impl_any!(HeapRootDesignator: Send, Sync, Clone, Copy);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransferGeneration(NonZeroU64);

static NEXT_TRANSFER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransferIdentity {
    heap: HeapIdentity,
    generation: TransferGeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum TransferPhase {
    Prepared = 1,
    Local = 2,
    Returned = 3,
    Restored = 4,
    Poisoned = 5,
}

/// Typed private metadata word. Unknown encodings fail closed; no observed
/// phase alone grants a heap loan or a source restoration capability.
struct TransferPhaseWord(AtomicU8);

impl TransferPhaseWord {
    fn new(phase: TransferPhase) -> Self {
        Self(AtomicU8::new(phase as u8))
    }

    fn decode(raw: u8) -> TransferPhase {
        match raw {
            1 => TransferPhase::Prepared,
            2 => TransferPhase::Local,
            3 => TransferPhase::Returned,
            4 => TransferPhase::Restored,
            _ => TransferPhase::Poisoned,
        }
    }

    fn load(&self) -> TransferPhase {
        Self::decode(self.0.load(Ordering::Acquire))
    }

    fn poison(&self) {
        self.0
            .store(TransferPhase::Poisoned as u8, Ordering::Release);
    }

    fn abandon_pending(&self) {
        let _ = self.0.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |raw| match Self::decode(raw) {
                TransferPhase::Restored => None,
                TransferPhase::Prepared
                | TransferPhase::Local
                | TransferPhase::Returned
                | TransferPhase::Poisoned => Some(TransferPhase::Poisoned as u8),
            },
        );
    }

    fn transition(
        &self,
        expected: TransferPhase,
        next: TransferPhase,
    ) -> Result<(), TransferError> {
        self.0
            .compare_exchange(
                expected as u8,
                next as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| TransferError::Phase)
    }
}

struct ReturnControl {
    identity: TransferIdentity,
    source: ThreadId,
    phase: TransferPhaseWord,
}

impl ReturnControl {
    fn poison(&self) {
        self.phase.poison();
    }

    fn transition(
        &self,
        expected: TransferPhase,
        next: TransferPhase,
    ) -> Result<(), TransferError> {
        self.phase.transition(expected, next)
    }
}

/// Why a reference-free handoff refused. The inputs stay with the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum TransferError {
    WrongHeap,
    WrongRoot,
    WrongThread,
    WrongReturn,
    InstalledTls,
    Observations(SourceObservationError),
    GenerationalRequired,
    Exhausted,
    Capacity,
    Roots,
    Phase,
}

/// Opaque words are never decoded off-owner. The accompanying existing root
/// table lease pins every managed word; immediates need no tracing slot.
struct OpaqueRootWord(usize);

struct Backing {
    owner: WorldLoan,
    publication: RootBatchLease,
    words: Box<[OpaqueRootWord]>,
    functions: FrozenFunctionRoots,
}

/// Source-side receipt. It contains no heap access and cannot travel.
///
/// No public constructor. [`PreparedHeapTransfer::prepare_detached`] is the
/// only producer. The value is not `Send`, `Sync`, `Clone`, or `Copy`.
#[must_use]
#[doc(hidden)]
pub struct PendingHeapReturn {
    control: Arc<ReturnControl>,
    observations: Option<SourceObservationsDetached>,
    local: PhantomData<Rc<()>>,
}

impl Drop for PendingHeapReturn {
    fn drop(&mut self) {
        self.control.phase.abandon_pending();
    }
}

impl PendingHeapReturn {
    #[doc(hidden)]
    pub fn is_prepared(&self) -> bool {
        self.control.phase.load() == TransferPhase::Prepared
    }

    #[doc(hidden)]
    pub fn is_poisoned(&self) -> bool {
        self.control.phase.load() == TransferPhase::Poisoned
    }

    #[doc(hidden)]
    pub fn retains_observations(&self) -> bool {
        self.observations.is_some()
    }

    /// Pair this receipt with the returned right on the source thread.
    #[doc(hidden)]
    pub fn restore(
        mut self,
        mut returned: ReturnedHeapTransfer,
    ) -> Result<RestoredHeapBacking, RestoreFailure> {
        let reason = if self.control.source != std::thread::current().id() {
            Some(TransferError::WrongThread)
        } else if !Arc::ptr_eq(&self.control, &returned.transfer.control)
            || self.control.identity != returned.transfer.control.identity
        {
            Some(TransferError::WrongReturn)
        } else {
            self.control
                .transition(TransferPhase::Returned, TransferPhase::Restored)
                .err()
        };
        if let Some(reason) = reason {
            self.control.poison();
            returned.transfer.control.poison();
            return Err(RestoreFailure {
                reason,
                pending: self,
                returned,
            });
        }
        let backing = returned
            .transfer
            .backing
            .take()
            .expect("affine returned backing");
        let restored = RestoredHeapBacking {
            backing: Some(backing),
            observations: self.observations.take(),
            heap: self.control.identity.heap,
            control: Arc::clone(&self.control),
            local: PhantomData,
        };
        // Keep a retention-on-unwind owner around even while the trusted
        // infallible holder hook runs. A future bad hook cannot drop backing.
        EraWorldLoan::adopt_current_thread(
            &restored.backing.as_ref().expect("restored backing").owner,
        );
        Ok(restored)
    }
}

static_assertions::assert_not_impl_any!(PendingHeapReturn: Send, Sync, Clone, Copy);

/// Unique retained bootstrap right. Its source collector reference has ended.
///
/// Unfinished Drop or `mem::forget` retains backing and the rooted publication.
/// `Send` is an explicit unsafe impl: the fields do not imply it. The value is
/// not `Sync`, `Clone`, or `Copy`. The only constructor is unsafe.
#[must_use]
#[doc(hidden)]
pub struct PreparedHeapTransfer {
    root: HeapRootDesignator,
    control: Arc<ReturnControl>,
    backing: Option<Backing>,
}

// SAFETY: `prepare_detached` consumes the actual GEN1 collector, requires the
// source's inaccessible sole-owner shell and detached host/TLS state, and ends
// that reference before constructing this object. The original root and the
// unique `Send` `WorldLoan` retain the exact allocation. Scoped receive never
// returns a borrowed collector; the actual collector must return before this
// affine right can move again. `RootBatchLease` and opaque words contain no
// local references. Canonical observation ownership is checked at preparation and
// after every local callback. The source Rc was replaced, and the source-local
// receipt contains only a placeholder. No existing `!Send` loan gains `Send`.
unsafe impl Send for PreparedHeapTransfer {}

static_assertions::assert_not_impl_any!(PreparedHeapTransfer: Sync, Clone, Copy);
static_assertions::assert_impl_all!(PreparedHeapTransfer: Send);

impl Drop for PreparedHeapTransfer {
    fn drop(&mut self) {
        if let Some(backing) = self.backing.take() {
            self.control.poison();
            std::mem::forget(backing);
        }
    }
}

/// `prepare_detached` refused before publishing a right. The caller still owns
/// the admission, the loan, and the census. Fields stay private.
#[must_use]
#[doc(hidden)]
pub struct PrepareFailure<'heap> {
    reason: TransferError,
    admission: Gen1Admission<'heap>,
    owner: WorldLoan,
    roots: FrozenLegacyRoots,
    observations: Option<SourceObservationsDetached>,
}

impl<'heap> PrepareFailure<'heap> {
    #[doc(hidden)]
    pub fn reason(&self) -> TransferError {
        self.reason
    }

    /// Return the inputs the constructor did not consume.
    #[doc(hidden)]
    pub fn into_parts(
        self,
    ) -> (
        TransferError,
        Gen1Admission<'heap>,
        WorldLoan,
        FrozenLegacyRoots,
        Option<SourceObservationsDetached>,
    ) {
        (
            self.reason,
            self.admission,
            self.owner,
            self.roots,
            self.observations,
        )
    }

    /// Detach refused before a receipt existed. The caller still owns every input.
    #[doc(hidden)]
    pub fn from_observation_detach(
        reason: SourceObservationError,
        admission: Gen1Admission<'heap>,
        owner: WorldLoan,
        roots: FrozenLegacyRoots,
    ) -> Self {
        Self {
            reason: TransferError::Observations(reason),
            admission,
            owner,
            roots,
            observations: None,
        }
    }
}

impl std::fmt::Debug for PrepareFailure<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrepareFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(PrepareFailure<'static>: Send, Sync, Clone, Copy);

/// A local loan or a return refused, and this value still owns the transfer.
#[must_use]
#[doc(hidden)]
pub struct RetainedTransferFailure {
    reason: TransferError,
    transfer: PreparedHeapTransfer,
    local: PhantomData<Rc<()>>,
}

impl RetainedTransferFailure {
    #[doc(hidden)]
    pub fn reason(&self) -> TransferError {
        self.reason
    }

    #[doc(hidden)]
    pub fn is_poisoned(&self) -> bool {
        self.transfer.control.phase.load() == TransferPhase::Poisoned
    }

    #[doc(hidden)]
    pub fn retains_backing(&self) -> bool {
        self.transfer.backing.is_some()
    }
}

impl std::fmt::Debug for RetainedTransferFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RetainedTransferFailure")
            .field(&self.reason)
            .finish()
    }
}

static_assertions::assert_not_impl_any!(RetainedTransferFailure: Send, Sync, Clone, Copy);

/// Closed completion vocabulary: metadata or an existing retained root handle.
///
/// # Safety
/// Implement this only for `()`, [`HeapIdentity`], [`TransferError`],
/// [`SharedRoot`], and `Result` of those. A completion must not carry a Lisp
/// word, a collector loan, or a local borrow. The trait is `unsafe` because a
/// private supertrait in this crate cannot seal a type that another crate
/// names in the `with_collector` bound. An external `unsafe impl` is a
/// contract violation, the same shape as [`EraWorldLoan`].
#[doc(hidden)]
pub unsafe trait HeapTransferCompletion: Send + 'static {}

macro_rules! completion_types {
    ($($ty:ty),+ $(,)?) => {$(
        unsafe impl HeapTransferCompletion for $ty {}
    )+};
}

completion_types!((), HeapIdentity, TransferError, SharedRoot);

unsafe impl<T: HeapTransferCompletion, E: HeapTransferCompletion> HeapTransferCompletion
    for Result<T, E>
{
}

static_assertions::assert_not_impl_any!(TaggedValue: HeapTransferCompletion);

/// Retains the actual transfer until every callback and collector loan has
/// ended. Drop then detaches any restored ordinary store before clearing heap
/// TLS. The callback lives in `run_local`'s inner frame, so this cleanup also
/// runs after its collector destructor on unwind; no sibling heap loan survives.
struct LocalHeapScope {
    transfer: Option<PreparedHeapTransfer>,
    local: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(LocalHeapScope: Send, Sync, Clone, Copy);

impl LocalHeapScope {
    fn retire_tls(&self) -> Result<(), TransferError> {
        let transfer = self.transfer.as_ref().expect("local retained transfer");
        retire_transfer_tls(transfer.root.as_non_null())
    }

    fn into_transfer(mut self) -> (PreparedHeapTransfer, Result<(), TransferError>) {
        let retired = self.retire_tls();
        (
            self.transfer.take().expect("local retained transfer"),
            retired,
        )
    }
}

impl Drop for LocalHeapScope {
    fn drop(&mut self) {
        if self.transfer.is_some() {
            // An unwind can never release a Send right. A failed cleanup leaves
            // the retained backing poisoned by the transfer field's Drop.
            let _ = self.retire_tls();
        }
        // The actual transfer still owns backing throughout cleanup, including
        // unwinding. Its field Drop then poisons and retains unfinished backing.
    }
}

/// Only after the complete collector or era frame has returned and every
/// participant has retired. A refusal that can still hold a heap loan clears
/// TLS by address and does not call this.
fn retire_transfer_tls(root: NonNull<super::super::TaggedHeap>) -> Result<(), TransferError> {
    let installed = super::super::TAGGED_HEAP.with(|slot| slot.get() == root.as_ptr());
    // SAFETY: the caller's complete callback, collector, or era frame has
    // returned, or unwound. The scope retains the sole original backing and
    // no heap reference. The fresh loan ends before a Send right can escape.
    let heap = unsafe { root.as_ref() };
    if installed {
        heap.preflight_source_store_retirement()
            .map_err(TransferError::Observations)?;
        clear_tagged_heap_if_installed(heap);
    }
    heap.retire_source_observation_membership()
        .map_err(TransferError::Observations)
}

/// Rebuild the exclusive loan from the retained provenance root.
///
/// # Safety
/// `root` is that transfer's original root. No collector loan, installed TLS
/// slot, or sibling heap borrow is outstanding. `body` drops the collector
/// before it returns; the higher-ranked bound stops the loan from escaping.
unsafe fn with_retained_collector<R>(
    root: NonNull<super::super::TaggedHeap>,
    body: impl for<'heap> FnOnce(QuiescentCollector<'heap>) -> R,
) -> R {
    // SAFETY: the caller holds the sole retained root. This frame ends when
    // `body` returns, so the raw loan does not escape it.
    let collector = unsafe { QuiescentCollector::from_retained_root(&mut *root.as_ptr()) };
    body(collector)
}

/// Armed before preparation may call a retention or publication hook.
/// An unwind retains every owning input. Drop does not recover them.
struct RetainedBootstrapParts {
    owner: std::mem::ManuallyDrop<WorldLoan>,
    roots: std::mem::ManuallyDrop<FrozenLegacyRoots>,
    observations: std::mem::ManuallyDrop<SourceObservationsDetached>,
}

impl RetainedBootstrapParts {
    fn new(
        owner: WorldLoan,
        roots: FrozenLegacyRoots,
        observations: SourceObservationsDetached,
    ) -> Self {
        Self {
            owner: std::mem::ManuallyDrop::new(owner),
            roots: std::mem::ManuallyDrop::new(roots),
            observations: std::mem::ManuallyDrop::new(observations),
        }
    }

    fn into_parts(self) -> (WorldLoan, FrozenLegacyRoots, SourceObservationsDetached) {
        let Self {
            owner,
            roots,
            observations,
        } = self;
        (
            std::mem::ManuallyDrop::into_inner(owner),
            std::mem::ManuallyDrop::into_inner(roots),
            std::mem::ManuallyDrop::into_inner(observations),
        )
    }
}

struct RetainedPreparation<'heap> {
    admission: std::mem::ManuallyDrop<Gen1Admission<'heap>>,
    parts: RetainedBootstrapParts,
}

impl<'heap> RetainedPreparation<'heap> {
    fn into_parts(
        self,
    ) -> (
        Gen1Admission<'heap>,
        WorldLoan,
        FrozenLegacyRoots,
        SourceObservationsDetached,
    ) {
        let Self { admission, parts } = self;
        let (owner, roots, observations) = parts.into_parts();
        (
            std::mem::ManuallyDrop::into_inner(admission),
            owner,
            roots,
            observations,
        )
    }
}

pub(crate) mod production_era;
pub(crate) mod source_preparation;

#[doc(hidden)]
pub use crate::tagged::gc::mutator_registration::custody_brand::{
    PublishedOwner, StopScope, StoppedInterval, UnenteredReservation, with_stop_scope,
};
#[doc(hidden)]
pub use production_era::{ScopedEraError, ScopedEraFailure};
#[doc(hidden)]
pub use source_preparation::ClosedSourcePrepareFailure;

impl PreparedHeapTransfer {
    /// # Safety
    /// The caller has consumed the actual sole serial owner into an
    /// inaccessible suspended shell, retaining all manager allocations. It has
    /// ended every host, projection, and manager view and uninstalled ordinary
    /// heap and World TLS. `roots` is that shell's fresh complete census, with
    /// all inputs rooted until publication. No legacy access can resume except
    /// by consuming the matching [`PendingHeapReturn`] and returned right.
    /// Forgetting either must not destroy or reactivate the shell.
    ///
    /// This constructor does not establish those eval adapter facts. It is
    /// dormant until that consuming adapter calls it, and this function takes
    /// no evaluator context.
    ///
    /// Every enclosing source frame with a reference-bearing collector or era
    /// argument must return before the resulting transfer is used or sent:
    /// dropping that argument does not end its protected call-frame borrow.
    /// Return the prepared right out of the source loan scope first.
    ///
    /// Borrowed and raw store frames are excluded by that same contract. `Rc`
    /// counts cannot discover them. Observation ownership is checked here from
    /// the receipt the caller detached after its own frames returned. That
    /// receipt stays in [`PendingHeapReturn`], never in the `Send` transfer.
    ///
    /// The admission, loan, census, and receipt are armed before any retention
    /// or publication hook. A panic in that hook retains them. A typed refusal
    /// returns them to the caller.
    #[doc(hidden)]
    pub unsafe fn prepare_detached<'heap>(
        admission: Gen1Admission<'heap>,
        owner: WorldLoan,
        roots: FrozenLegacyRoots,
        observations: SourceObservationsDetached,
    ) -> Result<(PendingHeapReturn, Self), PrepareFailure<'heap>> {
        let mut armed = RetainedPreparation {
            admission: std::mem::ManuallyDrop::new(admission),
            parts: RetainedBootstrapParts::new(owner, roots, observations),
        };
        let validation = (|| {
            let heap = armed.admission.collector_mut().registration_heap_mut();
            if heap.heap_identity() != armed.parts.owner.parallel_heap_identity()
                || armed.parts.roots.heap_identity() != heap.heap_identity()
            {
                return Err(TransferError::WrongHeap);
            }
            if !armed.parts.owner.held_here() {
                return Err(TransferError::WrongThread);
            }
            if tagged_heap_is_installed() {
                return Err(TransferError::InstalledTls);
            }
            let root = armed.parts.owner.heap_root_designator();
            if root.as_non_null().as_ptr() != std::ptr::from_mut(heap) {
                return Err(TransferError::WrongRoot);
            }
            heap.validate_source_observation_receipt(&armed.parts.observations)
                .map_err(TransferError::Observations)?;
            let generation = NEXT_TRANSFER
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| TransferError::Exhausted)?;
            let generation = NonZeroU64::new(generation).ok_or(TransferError::Exhausted)?;
            let mut words = Vec::new();
            words
                .try_reserve_exact(armed.parts.roots.words().len())
                .map_err(|_| TransferError::Capacity)?;
            words.extend(
                armed
                    .parts
                    .roots
                    .words()
                    .iter()
                    .map(|value| OpaqueRootWord(value.bits())),
            );
            let epoch = armed
                .parts
                .owner
                .parallel_epoch_retention()
                .map_err(|_| TransferError::Roots)?;
            let prepared = PreparedRootBatch::reserve_retained(heap, epoch, words.len())
                .map_err(|_| TransferError::Roots)?;
            // SAFETY: the consumed sole owner and the actual GEN1 admission
            // retain every original input. No collection or callback occurs
            // during publication.
            let finalized = unsafe { prepared.finalize(armed.parts.roots.words()) }
                .map_err(|_| TransferError::Roots)?;
            let publication = finalized
                .publish_finalized_root_batch()
                .map_err(|_| TransferError::Roots)?;
            Ok((
                root,
                TransferGeneration(generation),
                publication,
                words.into_boxed_slice(),
            ))
        })();
        let (root, generation, publication, words) = match validation {
            Ok(value) => value,
            Err(reason) => {
                let (admission, owner, roots, observations) = armed.into_parts();
                return Err(PrepareFailure {
                    reason,
                    admission,
                    owner,
                    roots,
                    observations: Some(observations),
                });
            }
        };
        let (admission, owner, roots, observations) = armed.into_parts();
        let identity = TransferIdentity {
            heap: admission.heap_identity(),
            generation,
        };
        let control = Arc::new(ReturnControl {
            identity,
            source: std::thread::current().id(),
            phase: TransferPhaseWord::new(TransferPhase::Prepared),
        });
        let functions = roots.function_roots();
        // Source TLS is absent, so Drop cannot restore the installed store.
        // No reference-bearing collector is retained in the outbound right.
        drop(admission.into_collector());
        drop(roots);
        let pending = PendingHeapReturn {
            control: Arc::clone(&control),
            observations: Some(observations),
            local: PhantomData,
        };
        Ok((
            pending,
            Self {
                root,
                control,
                backing: Some(Backing {
                    owner,
                    publication,
                    words,
                    functions,
                }),
            },
        ))
    }

    fn refuse(self, reason: TransferError) -> RetainedTransferFailure {
        self.control.poison();
        RetainedTransferFailure {
            reason,
            transfer: self,
            local: PhantomData,
        }
    }

    /// Test-only traversal of the real local collector boundary. No heap word
    /// or collector capability is exposed to the boxed-source fixture.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn source_roundtrip_for_test(self) -> Result<Self, RetainedTransferFailure> {
        self.with_collector(|admission| {
            let identity = admission.heap_identity();
            (admission.into_collector(), identity)
        })
        .map(|(transfer, _identity)| transfer)
    }

    /// The actual local collector, not merely an arbitrary result, must come
    /// back. The higher-ranked bound excludes borrowed views. The completion
    /// bound excludes unrooted Lisp words, which have no borrow lifetime.
    #[doc(hidden)]
    pub fn with_collector<R: HeapTransferCompletion>(
        self,
        body: impl for<'heap> FnOnce(Gen1Admission<'heap>) -> (QuiescentCollector<'heap>, R),
    ) -> Result<(Self, R), RetainedTransferFailure> {
        if tagged_heap_is_installed() {
            return Err(self.refuse(TransferError::InstalledTls));
        }
        let mut scope = LocalHeapScope {
            transfer: Some(self),
            local: PhantomData,
        };
        let result = scope
            .transfer
            .as_mut()
            .expect("local retained transfer")
            .run_local(body);
        // `run_local`'s entire reference-bearing frame ended before cleanup.
        let (transfer, retired) = scope.into_transfer();
        match (result, retired) {
            (Ok(result), Ok(())) => {
                match transfer
                    .control
                    .transition(TransferPhase::Local, TransferPhase::Prepared)
                {
                    Ok(()) => Ok((transfer, result)),
                    Err(reason) => Err(transfer.refuse(reason)),
                }
            }
            (Err(reason), _) | (Ok(_), Err(reason)) => Err(transfer.refuse(reason)),
        }
    }

    fn run_local<R: HeapTransferCompletion>(
        &mut self,
        body: impl for<'heap> FnOnce(Gen1Admission<'heap>) -> (QuiescentCollector<'heap>, R),
    ) -> Result<R, TransferError> {
        self.control
            .transition(TransferPhase::Prepared, TransferPhase::Local)?;
        let backing = self.backing.as_ref().expect("affine live transfer");
        EraWorldLoan::adopt_current_thread(&backing.owner);
        if !backing.owner.held_here() {
            return Err(TransferError::WrongThread);
        }
        // SAFETY: this unique transfer owns the original retained root. The
        // source and all previous local collector references ended. No TLS is
        // installed and no other capability can access this allocation.
        let collector = unsafe {
            QuiescentCollector::from_retained_root(&mut *self.root.as_non_null().as_ptr())
        };
        let admission = match Gen1Admission::check(collector) {
            Ok(admission) => admission,
            Err(failure) => {
                let (_reason, collector) = failure.into_parts();
                drop(collector);
                return Err(TransferError::GenerationalRequired);
            }
        };
        let (mut collector, result) = body(admission);
        let correct = collector.heap_identity() == self.control.identity.heap
            && std::ptr::from_mut(collector.registration_heap_mut())
                == self.root.as_non_null().as_ptr();
        let tls_absent = !tagged_heap_is_installed();
        drop(collector);
        if !correct {
            return Err(TransferError::WrongRoot);
        }
        if !tls_absent {
            return Err(TransferError::InstalledTls);
        }
        // The Local phase stays armed until the outer scope retires TLS and
        // revalidates the observation owner after this frame returns.
        Ok(result)
    }

    #[doc(hidden)]
    pub fn return_to_source(self) -> Result<ReturnedHeapTransfer, RetainedTransferFailure> {
        if let Err(reason) = self
            .control
            .transition(TransferPhase::Prepared, TransferPhase::Returned)
        {
            return Err(self.refuse(reason));
        }
        Ok(ReturnedHeapTransfer { transfer: self })
    }

    #[doc(hidden)]
    pub fn is_prepared(&self) -> bool {
        self.control.phase.load() == TransferPhase::Prepared
    }

    #[doc(hidden)]
    pub fn is_poisoned(&self) -> bool {
        self.control.phase.load() == TransferPhase::Poisoned
    }

    #[doc(hidden)]
    pub fn retains_backing(&self) -> bool {
        self.backing.is_some()
    }

    #[doc(hidden)]
    pub fn publication_root_count(&self) -> usize {
        self.backing
            .as_ref()
            .expect("affine live transfer")
            .publication
            .root_count()
    }

    #[doc(hidden)]
    pub fn heap_identity(&self) -> HeapIdentity {
        self.control.identity.heap
    }

    /// Address of the original provenance root. Tests use it only to name the
    /// existing heap slot. It is not a new thread-local.
    #[doc(hidden)]
    pub fn root_ptr(&self) -> *mut super::super::TaggedHeap {
        self.root.as_non_null().as_ptr()
    }
}

/// The reference-free right, back on a path to its source receipt.
///
/// No public constructor. [`PreparedHeapTransfer::return_to_source`] is the
/// only producer. `Send` follows the inner transfer. The value is not `Sync`,
/// `Clone`, or `Copy`.
#[must_use]
#[doc(hidden)]
pub struct ReturnedHeapTransfer {
    transfer: PreparedHeapTransfer,
}

static_assertions::assert_impl_all!(ReturnedHeapTransfer: Send);
static_assertions::assert_not_impl_any!(ReturnedHeapTransfer: Sync, Clone, Copy);

/// Restore refused. Both the receipt and the returned right stay owned here.
#[must_use]
#[doc(hidden)]
pub struct RestoreFailure {
    reason: TransferError,
    pending: PendingHeapReturn,
    returned: ReturnedHeapTransfer,
}

impl RestoreFailure {
    #[doc(hidden)]
    pub fn reason(&self) -> TransferError {
        self.reason
    }

    #[doc(hidden)]
    pub fn pending_is_poisoned(&self) -> bool {
        self.pending.is_poisoned()
    }

    #[doc(hidden)]
    pub fn returned_retains_backing(&self) -> bool {
        self.returned.transfer.retains_backing()
    }
}

impl std::fmt::Debug for RestoreFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RestoreFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(RestoreFailure: Send, Sync, Clone, Copy);

/// Source-thread backing after a matched restore. Not a heap loan.
///
/// No public constructor. [`PendingHeapReturn::restore`] is the only producer.
/// The value is not `Send`, `Sync`, `Clone`, or `Copy`. Recovering the census
/// is unsafe and does not rebuild an evaluator shell.
#[must_use]
#[doc(hidden)]
pub struct RestoredHeapBacking {
    backing: Option<Backing>,
    observations: Option<SourceObservationsDetached>,
    heap: HeapIdentity,
    control: Arc<ReturnControl>,
    local: PhantomData<Rc<()>>,
}

impl Drop for RestoredHeapBacking {
    fn drop(&mut self) {
        if let Some(backing) = self.backing.take() {
            self.control.poison();
            std::mem::forget(backing);
        }
    }
}

impl RestoredHeapBacking {
    fn restore_observations(&mut self) -> Result<(), TransferError> {
        let root = self
            .backing
            .as_ref()
            .expect("retained source backing")
            .owner
            .heap_root_designator();
        let receipt = self
            .observations
            .take()
            .expect("affine source observation effect");
        // SAFETY: `PendingHeapReturn` validated the original thread, the unique
        // returned control, and the heap generation. Destination frames have
        // ended. This object retains the original backing and there is no
        // other loan.
        let mut collector =
            unsafe { QuiescentCollector::from_retained_root(&mut *root.as_non_null().as_ptr()) };
        match collector.restore_source_observations(receipt) {
            Ok(()) => Ok(()),
            Err(failure) => {
                self.observations = Some(failure.receipt);
                Err(TransferError::Observations(failure.reason))
            }
        }
    }

    /// # Safety
    /// Only the original consuming source-shell adapter may recover these
    /// parts. Restore its actual rooted frame before retiring publication.
    /// No other heap or World loan may start during that recovery. This
    /// function does not mint [`FrozenLegacyRoots`]: the adapter under
    /// eval/world does, where that constructor is allowed.
    #[doc(hidden)]
    pub unsafe fn into_source_parts(
        mut self,
    ) -> Result<
        (
            WorldLoan,
            HeapIdentity,
            Vec<TaggedValue>,
            FrozenFunctionRoots,
            RootBatchLease,
        ),
        SourceRecoveryFailure,
    > {
        let mut decoded = Vec::new();
        let backing = self.backing.as_ref().expect("affine restored backing");
        if decoded.try_reserve_exact(backing.words.len()).is_err() {
            return Err(SourceRecoveryFailure {
                reason: TransferError::Capacity,
                restored: self,
            });
        }
        decoded.extend(backing.words.iter().map(|word| {
            // SAFETY: these bits were copied from the same census at
            // preparation, and the publication lease has pinned them since.
            unsafe { TaggedValue::from_bits(word.0) }
        }));
        // Finish allocation and decoding before restoring the source effect.
        // The publication stays live across this whole transaction.
        if let Err(reason) = self.restore_observations() {
            return Err(SourceRecoveryFailure {
                reason,
                restored: self,
            });
        }
        let Backing {
            owner,
            publication,
            words,
            functions,
        } = self.backing.take().expect("affine restored backing");
        drop(words);
        Ok((owner, self.heap, decoded, functions, publication))
    }
}

static_assertions::assert_not_impl_any!(RestoredHeapBacking: Send, Sync, Clone, Copy);

/// `into_source_parts` refused and still owns the restored backing.
#[must_use]
#[doc(hidden)]
pub struct SourceRecoveryFailure {
    reason: TransferError,
    /// Owned so dropping the error still runs the backing's poison drop.
    /// [`Self::into_restored`] is the only way to take it back.
    restored: RestoredHeapBacking,
}

impl SourceRecoveryFailure {
    #[doc(hidden)]
    pub fn reason(&self) -> TransferError {
        self.reason
    }

    #[doc(hidden)]
    pub fn retains_backing(&self) -> bool {
        self.restored.backing.is_some()
    }

    #[doc(hidden)]
    pub fn retains_observations(&self) -> bool {
        self.restored.observations.is_some()
    }

    /// Return the restored backing so the source can retry after the refusal
    /// that still owns this effect. No public constructor mints a new one.
    #[doc(hidden)]
    pub fn into_restored(self) -> (TransferError, RestoredHeapBacking) {
        (self.reason, self.restored)
    }
}

impl std::fmt::Debug for SourceRecoveryFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceRecoveryFailure")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(SourceRecoveryFailure: Send, Sync, Clone, Copy);

/// Address-only fault injection for the transfer tests. Writes the existing
/// heap slot. This is not a new thread-local and not a production entry.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn poke_installed_heap_address(root: *mut super::super::TaggedHeap) {
    super::super::TAGGED_HEAP.with(|slot| slot.set(root));
}
