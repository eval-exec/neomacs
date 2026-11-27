//! Generic publication owner.
//!
//! One current owner, a predecessor vector, a nonblocking metadata mutex, and
//! an atomic root. A noting replace may reclaim one predecessor when
//! `view_depth` is zero, before the quote. `NeedGrace` still refuses while a
//! view is outstanding or the quote does not fit, and that refusal keeps
//! every owner. The no-hook path never reclaims: symbol publication does not
//! call it. The core does not implement `Send` or `Sync`: an arbitrary
//! published address is not that proof.

use super::sync::{AtomicPtr, AtomicU32, Mutex, MutexGuard, Ordering, TryLockError, fence};
use super::{
    GraceReclaimReport, PayloadGeneration, PayloadPlan, PublicationChange, PublicationError,
    PublicationPayload, PublicationStatistics, RetainedBytes, RetentionLimit, RetentionLimits,
    RetiredCount,
};
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;

struct Metadata<P> {
    current: P,
    /// Generation the noting path appended for `current`.
    /// `None` on the no-hook path, which never reclaims.
    current_generation: Option<PayloadGeneration>,
    predecessors: Vec<P>,
    /// Parallel to `predecessors`: the generation each one appended.
    predecessor_generations: Vec<Option<PayloadGeneration>>,
    /// Test builds can tighten one owner's caps without process state.
    /// Absent means the limits stored at construction.
    #[cfg(any(test, feature = "test-support"))]
    limit_override: Option<RetentionLimits>,
}

/// Retention owner. Callers never borrow the ledger or the root directly.
pub(crate) struct PublicationLedger<P: PublicationPayload> {
    root: AtomicPtr<u8>,
    metadata: Mutex<Metadata<P>>,
    limits: RetentionLimits,
    /// Live [`PublicationView`]s of this ledger. Reclaim runs only at zero.
    /// `AtomicU32` is `Sync`, so a view's reference does not add a
    /// `Cell<u32>: Sync` note to the confinement fixture.
    /// Updated atomically with `fetch_add(AcqRel)` + `fence(SeqCst)` /
    /// `fetch_sub(Release)` so typed `Send + Sync` wrappers (such as
    /// `PublishedSymbolTables`) form a Dekker Store-Buffering pair with
    /// `root.store(Release)` (under `metadata`) + `fence(SeqCst)` +
    /// `view_depth.load(Acquire)`, preventing a concurrent grace drain from
    /// observing `view_depth == 0` behind a live view that loaded a retired
    /// root.
    view_depth: AtomicU32,
    // Keeps `Send`/`Sync` off this generic store. The typed publisher, not an
    // arbitrary pointer, is what may add that proof later.
    _not_send: PhantomData<Rc<()>>,
}

/// Acquire snapshot of the root. It keeps the address it loaded.
#[must_use = "retain this view until every use of the loaded address returns"]
pub(crate) struct PublicationView<'owner, P: PublicationPayload> {
    address: *mut u8,
    depth: &'owner AtomicU32,
    _owner: PhantomData<&'owner PublicationLedger<P>>,
    _thread: PhantomData<Rc<()>>,
}

/// Prepared publication. Dropping it publishes nothing.
#[must_use = "publish the prepared transaction or drop it without publishing"]
pub(crate) struct PreparedPublication<'owner, P: PublicationPayload> {
    store: &'owner PublicationLedger<P>,
    metadata: MutexGuard<'owner, Metadata<P>>,
    plan: PayloadPlan<P>,
    _thread: PhantomData<Rc<()>>,
}

/// One admission under the metadata lock. `admit` runs at most once.
pub(crate) struct Admission<'a, P: PublicationPayload> {
    metadata: &'a mut Metadata<P>,
    limits: RetentionLimits,
    view_depth: &'a AtomicU32,
}

/// What the caller commits after its own backing work. `Keep` neither
/// pushes nor stores. `Replace` is the initialized successor.
pub(crate) enum Commit<P> {
    Keep,
    Replace(P),
}

/// Open transaction. The metadata lock is held until `finish` or drop.
#[must_use = "finish the open transaction or drop it without publishing"]
pub(crate) struct OpenPublication<'owner, P: PublicationPayload, T> {
    store: &'owner PublicationLedger<P>,
    metadata: MutexGuard<'owner, Metadata<P>>,
    body: T,
    _thread: PhantomData<Rc<()>>,
}

/// Capacity growth is reported even when admission refuses. A dropped
/// transaction publishes nothing.
#[must_use = "publish the open transaction or drop it without publishing"]
pub(crate) struct Accounted<'owner, P: PublicationPayload, T> {
    pub(crate) result: Result<OpenPublication<'owner, P, T>, PublicationError<P::Error>>,
    pub(crate) charged: RetainedBytes,
}

/// Uninhabited witness naming a payload type for auto-trait assertions and
/// compile-fail fixtures without permitting a value to be constructed.
pub(crate) enum BoundWitness {}

// SAFETY: `BoundWitness` is uninhabited (`enum BoundWitness {}`), so no
// instance can exist and every method is vacuously sound.
unsafe impl PublicationPayload for BoundWitness {
    type Error = ();
    type Generation = u64;

    fn published_address(&self) -> *mut u8 {
        match *self {}
    }

    fn retained_bytes(&self) -> RetainedBytes {
        match *self {}
    }

    fn generation(&self) -> u64 {
        match *self {}
    }

    fn prepare(current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
        match *current {}
    }
}

static_assertions::assert_not_impl_any!(PublicationLedger<BoundWitness>: Send, Sync);
static_assertions::assert_not_impl_any!(PublicationView<'static, BoundWitness>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(PreparedPublication<'static, BoundWitness>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(OpenPublication<'static, BoundWitness, ()>: Send, Sync, Clone, Copy);

impl<P: PublicationPayload> PublicationView<'_, P> {
    pub(crate) fn published_address(&self) -> *mut u8 {
        self.address
    }
}

impl<P: PublicationPayload> Drop for PublicationView<'_, P> {
    fn drop(&mut self) {
        let prev = self.depth.fetch_sub(1, Ordering::Release);
        debug_assert!(prev > 0, "publication view depth underflow");
    }
}

impl<P: PublicationPayload> PublicationLedger<P> {
    /// Install `initial` before the first Release store. No predecessor exists.
    pub(crate) fn create(
        initial: P,
        limits: RetentionLimits,
    ) -> Result<Self, PublicationError<P::Error>> {
        if initial.retained_bytes().get() > limits.bytes.get() {
            return Err(PublicationError::NeedGrace(RetentionLimit::RetainedBytes));
        }
        let address = initial.published_address();
        let store = Self {
            root: AtomicPtr::new(std::ptr::null_mut()),
            metadata: Mutex::new(Metadata {
                current: initial,
                current_generation: None,
                predecessors: Vec::new(),
                predecessor_generations: Vec::new(),
                #[cfg(any(test, feature = "test-support"))]
                limit_override: None,
            }),
            limits,
            view_depth: AtomicU32::new(0),
            _not_send: PhantomData,
        };
        store.root.store(address, Ordering::Release);
        Ok(store)
    }

    /// [`Self::create`] plus `note_roots` on the payload that becomes current,
    /// before the Release store. A quote refusal does not call `note_roots`.
    /// `note_roots` returns the generation id it appended.
    ///
    /// `create` itself stays the no-hook path so symbol publication does not
    /// monomorphize a root callback.
    pub(crate) fn create_noting(
        initial: P,
        limits: RetentionLimits,
        note_roots: impl FnOnce(&P) -> PayloadGeneration,
    ) -> Result<Self, PublicationError<P::Error>> {
        if initial.retained_bytes().get() > limits.bytes.get() {
            return Err(PublicationError::NeedGrace(RetentionLimit::RetainedBytes));
        }
        let address = initial.published_address();
        let current_generation = note_roots(&initial);
        let store = Self {
            root: AtomicPtr::new(std::ptr::null_mut()),
            metadata: Mutex::new(Metadata {
                current: initial,
                current_generation: Some(current_generation),
                predecessors: Vec::new(),
                predecessor_generations: Vec::new(),
                #[cfg(any(test, feature = "test-support"))]
                limit_override: None,
            }),
            limits,
            view_depth: AtomicU32::new(0),
            _not_send: PhantomData,
        };
        store.root.store(address, Ordering::Release);
        Ok(store)
    }

    /// One Acquire snapshot of the root, ordered against concurrent grace drain
    /// by a `SeqCst` Dekker fence pair. Later publications do not change this
    /// view.
    ///
    /// Forms a Store-Buffering (Dekker) pair with [`Self::release_retained`]
    /// and [`Self::drain_predecessors_after_grace`]: the view executes
    /// `view_depth.fetch_add(1, AcqRel)` -> `fence(SeqCst)` ->
    /// `root.load(Acquire)`, while replacement executes `root.store(Release)`
    /// under `metadata` and grace drain executes `metadata` lock ->
    /// `fence(SeqCst)` -> `view_depth.load(Acquire)`. Thus no live view can
    /// observe a retired root while a concurrent drain observes `view_depth == 0`.
    /// Dropping the view decrements `view_depth` with `Release`, synchronizing
    /// with a subsequent `view_depth.load(Acquire)` before predecessors drop.
    pub(crate) fn view(&self) -> PublicationView<'_, P> {
        let prev = self.view_depth.fetch_add(1, Ordering::AcqRel);
        assert!(prev < u32::MAX, "publication view depth");
        fence(Ordering::SeqCst);
        PublicationView {
            address: self.root.load(Ordering::Acquire),
            depth: &self.view_depth,
            _owner: PhantomData,
            _thread: PhantomData,
        }
    }

    pub(crate) fn statistics(
        &self,
    ) -> Result<PublicationStatistics<P::Generation>, PublicationError<P::Error>> {
        let metadata = self.lock()?;
        Ok(PublicationStatistics {
            generation: metadata.current.generation(),
            retired: RetiredCount::new(metadata.predecessors.len()),
            retained_bytes: RetainedBytes::new(retained_quote(&metadata)?),
        })
    }

    pub(crate) fn prepare(&self) -> Result<PreparedPublication<'_, P>, PublicationError<P::Error>> {
        let mut metadata = self.lock()?;
        let limits = self.effective_limits(&metadata);
        let plan = P::prepare(&metadata.current).map_err(PublicationError::Payload)?;
        if let PayloadPlan::Replacement(next) = &plan {
            admit_replacement(&mut metadata, next, limits)?;
        }
        Ok(PreparedPublication {
            store: self,
            metadata,
            plan,
            _thread: PhantomData,
        })
    }

    /// The live root. Callers atomic-load this cell; they do not own it.
    pub(crate) fn slot(&self) -> &AtomicPtr<u8> {
        &self.root
    }

    /// Drain all retired predecessors (never `current`) once a stop-bound grace
    /// period guarantees no concurrent reader retains a pointer loaded before
    /// `current`.
    ///
    /// Exclusively for no-hook ledgers (created via [`Self::create`], such as
    /// `PublishedSymbolTables`). Refuses `Busy` if the metadata mutex is
    /// contended, if any tracked [`PublicationView`] is still live
    /// (`view_depth > 0`), or if this is a noting ledger carrying root-bank
    /// generations (`create_noting` / `finish_noting`). Refuses `Poisoned` if
    /// the metadata mutex is poisoned (retaining all owners).
    ///
    /// # Safety
    /// The caller must hold a valid stop-bound multi-mutator grace proof
    /// establishing that zero threads retain any descriptor pointer loaded from
    /// [`Self::slot`] prior to the current published root.
    pub(crate) unsafe fn drain_predecessors_after_grace(
        &self,
    ) -> Result<GraceReclaimReport, PublicationError<P::Error>> {
        let mut metadata = self.lock()?;
        // `metadata` lock synchronizes with prior `release_retained`'s
        // `root.store(Release)`, and this `fence(SeqCst)` + `load(Acquire)`
        // completes the Store-Buffering (Dekker) pair against `view()`'s
        // `view_depth.fetch_add(1, AcqRel)` -> `fence(SeqCst)` -> `root.load(Acquire)`.
        fence(Ordering::SeqCst);
        if self.view_depth.load(Ordering::Acquire) > 0 {
            return Err(PublicationError::Busy);
        }
        // Noting consumers manage root-bank generations via
        // `reclaim_oldest_predecessor`; never drop tracked generations here.
        if metadata.current_generation.is_some()
            || metadata.predecessor_generations.iter().any(Option::is_some)
        {
            return Err(PublicationError::Busy);
        }
        let reclaimed = metadata.predecessors.len();
        let before_reserved = reserved_capacity_bytes::<P>(&metadata);
        let mut payload_bytes = 0usize;
        for owner in &metadata.predecessors {
            payload_bytes = payload_bytes.saturating_add(owner.retained_bytes().get());
        }
        // Never drop `metadata.current` or alter `self.root`.
        metadata.predecessors.clear();
        metadata.predecessor_generations.clear();
        // Shrinking an empty Vec releases its backing allocation without
        // allocating, restoring `retained_quote` to `current` alone.
        metadata.predecessors.shrink_to_fit();
        metadata.predecessor_generations.shrink_to_fit();
        let freed_reserved =
            before_reserved.saturating_sub(reserved_capacity_bytes::<P>(&metadata));
        let freed_bytes = payload_bytes.saturating_add(freed_reserved);
        Ok(GraceReclaimReport {
            reclaimed: RetiredCount::new(reclaimed),
            freed_bytes: RetainedBytes::new(freed_bytes),
        })
    }

    /// Lock, let the caller decide, and reserve when it admits a quote.
    ///
    /// The closure builds whatever the adapter still needs (a shape check, an
    /// uninitialized successor). `admit` is the reserve and the byte quote.
    /// `finish` commits only after the caller's backing work.
    pub(crate) fn account<T>(
        &self,
        decide: impl FnOnce(&mut Admission<'_, P>) -> Result<T, PublicationError<P::Error>>,
    ) -> Accounted<'_, P, T> {
        let mut metadata = match self.lock() {
            Ok(metadata) => metadata,
            Err(error) => {
                return Accounted {
                    result: Err(error),
                    charged: RetainedBytes::new(0),
                };
            }
        };
        let before = reserved_capacity_bytes::<P>(&metadata);
        let limits = self.effective_limits(&metadata);
        let decided = {
            let mut admission = Admission {
                metadata: &mut metadata,
                limits,
                view_depth: &self.view_depth,
            };
            decide(&mut admission)
        };
        let charged =
            RetainedBytes::new(reserved_capacity_bytes::<P>(&metadata).saturating_sub(before));
        let result = match decided {
            Ok(body) => Ok(OpenPublication {
                store: self,
                metadata,
                body,
                _thread: PhantomData,
            }),
            Err(error) => Err(error),
        };
        Accounted { result, charged }
    }

    fn effective_limits(&self, metadata: &Metadata<P>) -> RetentionLimits {
        let _ = metadata;
        #[cfg(any(test, feature = "test-support"))]
        if let Some(limits) = metadata.limit_override {
            return limits;
        }
        self.limits
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_limit_override(
        &self,
        limits: Option<RetentionLimits>,
    ) -> Result<(), PublicationError<P::Error>> {
        self.lock()?.limit_override = limits;
        Ok(())
    }

    /// Blocking read of the current owner and every predecessor. A poisoned
    /// lock still yields the retained owners; it does not clear the flag.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn with_retained<R>(&self, body: impl FnOnce(&P, &[P]) -> R) -> R {
        let metadata = match self.metadata.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        body(&metadata.current, &metadata.predecessors)
    }

    /// Hold the metadata lock across `body`. A panic poisons the lock.
    #[cfg(test)]
    pub(crate) fn hold_metadata_for_test(&self, body: impl FnOnce()) {
        let _guard = match self.metadata.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        body();
    }

    fn lock(&self) -> Result<MutexGuard<'_, Metadata<P>>, PublicationError<P::Error>> {
        match self.metadata.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => Err(PublicationError::Busy),
            Err(TryLockError::Poisoned(_)) => Err(PublicationError::Poisoned),
        }
    }

    /// Release-store `pointer` only after `predecessors` already owns the
    /// address currently in the root.
    fn release_retained(&self, pointer: *mut u8, predecessors: &[P]) {
        // The test witness reads the predecessor slice before the store.
        // Production publishes the pointer and does not scan that slice.
        #[cfg(test)]
        {
            let previous = self.root.load(Ordering::Relaxed);
            let retained = predecessors
                .iter()
                .any(|owner| owner.published_address() == previous);
            note_release_retained(retained);
        }
        #[cfg(not(test))]
        let _ = predecessors;
        self.root.store(pointer, Ordering::Release);
    }
}

impl<P: PublicationPayload> PreparedPublication<'_, P> {
    pub(crate) fn publish(
        mut self,
    ) -> Result<PublicationChange<P::Generation>, PublicationError<P::Error>> {
        let next = match self.plan {
            PayloadPlan::Unchanged => {
                return Ok(PublicationChange::Unchanged {
                    generation: self.metadata.current.generation(),
                });
            }
            PayloadPlan::Replacement(next) => next,
        };
        Ok(commit_replacement(self.store, &mut self.metadata, next))
    }
}

impl<P: PublicationPayload> Admission<'_, P> {
    pub(crate) fn current(&self) -> &P {
        &self.metadata.current
    }

    pub(crate) fn admit(&mut self, extra: RetainedBytes) -> Result<(), PublicationError<P::Error>> {
        admit_quote(self.metadata, extra.get(), self.limits)
    }

    /// Drop the oldest predecessor when this replace is at the retiree cap
    /// and no view is alive.
    ///
    /// `reclaim` removes that generation's roots and no other, while the
    /// payload is still in `predecessors`. Current is never passed to it.
    /// A positive view depth, a count under the cap, a missing generation
    /// id, or a byte quote that would still refuse returns without
    /// dropping an owner, so `admit` keeps `NeedGrace`.
    #[cfg_attr(not(test), expect(dead_code, reason = "P10-b slices 2/3 wire refresh"))]
    pub(crate) fn reclaim_oldest_predecessor(
        &mut self,
        extra: usize,
        reclaim: impl FnOnce(PayloadGeneration),
    ) {
        fence(Ordering::SeqCst);
        if self.view_depth.load(Ordering::Acquire) > 0 {
            return;
        }
        if self.metadata.predecessors.len() < self.limits.retired.get() {
            return;
        }
        let Some(generation) = self
            .metadata
            .predecessor_generations
            .first()
            .copied()
            .flatten()
        else {
            return;
        };
        match quote_dropping_oldest(self.metadata, extra) {
            Ok(needed) if needed <= self.limits.bytes.get() => {}
            _ => return,
        }
        // Current is never reclaimed. This id is the oldest predecessor.
        // Other stored ids stay valid: they are not slots.
        reclaim(generation);
        self.metadata.predecessors.remove(0);
        self.metadata.predecessor_generations.remove(0);
    }
}

impl<P: PublicationPayload, T> OpenPublication<'_, P, T> {
    pub(crate) fn finish(
        self,
        decide: impl FnOnce(T, &P) -> Result<Commit<P>, PublicationError<P::Error>>,
    ) -> Result<PublicationChange<P::Generation>, PublicationError<P::Error>> {
        let OpenPublication {
            store,
            mut metadata,
            body,
            _thread,
        } = self;
        match decide(body, &metadata.current)? {
            Commit::Keep => Ok(PublicationChange::Unchanged {
                generation: metadata.current.generation(),
            }),
            Commit::Replace(next) => Ok(commit_replacement(store, &mut metadata, next)),
        }
    }

    /// [`Self::finish`] that notes the replacement before the Release store.
    /// `Commit::Keep` does not call `note_roots`. `note_roots` returns the
    /// generation id it appended for the new current payload.
    pub(crate) fn finish_noting(
        self,
        decide: impl FnOnce(T, &P) -> Result<Commit<P>, PublicationError<P::Error>>,
        note_roots: impl FnOnce(&P) -> PayloadGeneration,
    ) -> Result<PublicationChange<P::Generation>, PublicationError<P::Error>> {
        let OpenPublication {
            store,
            mut metadata,
            body,
            _thread,
        } = self;
        match decide(body, &metadata.current)? {
            Commit::Keep => Ok(PublicationChange::Unchanged {
                generation: metadata.current.generation(),
            }),
            Commit::Replace(next) => Ok(commit_replacement_noting(
                store,
                &mut metadata,
                next,
                note_roots,
            )),
        }
    }
}

fn commit_replacement<P: PublicationPayload>(
    store: &PublicationLedger<P>,
    metadata: &mut Metadata<P>,
    next: P,
) -> PublicationChange<P::Generation> {
    let charged = next.retained_bytes();
    let generation = next.generation();
    let pointer = next.published_address();
    let previous_generation = metadata.current_generation.take();
    let previous = mem::replace(&mut metadata.current, next);
    push_predecessor(metadata, previous, previous_generation);
    store.release_retained(pointer, &metadata.predecessors);
    PublicationChange::Published {
        generation,
        charged,
    }
}

/// [`commit_replacement`] after `note_roots` has seen the new current payload.
/// The note runs only on this replacement path, before the Release store.
/// `note_roots` returns the generation id appended for `next`.
fn commit_replacement_noting<P: PublicationPayload>(
    store: &PublicationLedger<P>,
    metadata: &mut Metadata<P>,
    next: P,
    note_roots: impl FnOnce(&P) -> PayloadGeneration,
) -> PublicationChange<P::Generation> {
    let charged = next.retained_bytes();
    let generation = next.generation();
    let pointer = next.published_address();
    let noted = note_roots(&next);
    let previous_generation = metadata.current_generation.replace(noted);
    let previous = mem::replace(&mut metadata.current, next);
    push_predecessor(metadata, previous, previous_generation);
    store.release_retained(pointer, &metadata.predecessors);
    PublicationChange::Published {
        generation,
        charged,
    }
}

fn push_predecessor<P>(
    metadata: &mut Metadata<P>,
    payload: P,
    generation: Option<PayloadGeneration>,
) {
    metadata.predecessors.push(payload);
    metadata.predecessor_generations.push(generation);
}

/// Retiree cap, then one reserved predecessor slot, then the byte quote.
/// A reserve that succeeds stays allocated when the byte cap refuses. The
/// candidate is not pushed and the root is not stored.
fn admit_replacement<P: PublicationPayload>(
    metadata: &mut Metadata<P>,
    next: &P,
    limits: RetentionLimits,
) -> Result<(), PublicationError<P::Error>> {
    admit_quote(metadata, next.retained_bytes().get(), limits)
}

fn admit_quote<P: PublicationPayload>(
    metadata: &mut Metadata<P>,
    extra: usize,
    limits: RetentionLimits,
) -> Result<(), PublicationError<P::Error>> {
    if metadata.predecessors.len() >= limits.retired.get() {
        return Err(PublicationError::NeedGrace(RetentionLimit::Retired));
    }
    // Reservation failure and quote overflow are capacity, not backpressure.
    // The two comparisons above and below are the only `NeedGrace` arms.
    if metadata.predecessors.try_reserve(1).is_err()
        || metadata.predecessor_generations.try_reserve(1).is_err()
    {
        return Err(PublicationError::Capacity);
    }
    let needed = retained_quote(metadata)?
        .checked_add(extra)
        .ok_or(PublicationError::Capacity)?;
    if needed > limits.bytes.get() {
        return Err(PublicationError::NeedGrace(RetentionLimit::RetainedBytes));
    }
    Ok(())
}

fn reserved_capacity_bytes<P: PublicationPayload>(metadata: &Metadata<P>) -> usize {
    // Vec capacity is bounded by `isize::MAX` bytes. Saturation keeps the
    // charge accessor total if that invariant changes. The quote uses
    // `checked_mul` and reports overflow as `Capacity`.
    metadata
        .predecessors
        .capacity()
        .saturating_mul(P::storage_overhead())
}

/// The quote `admit_quote` will see after the oldest predecessor is dropped.
/// Capacity does not shrink on remove, so the reservation term stays.
fn quote_dropping_oldest<P: PublicationPayload>(
    metadata: &Metadata<P>,
    extra: usize,
) -> Result<usize, PublicationError<P::Error>> {
    let mut total = metadata
        .predecessors
        .capacity()
        .checked_mul(P::storage_overhead())
        .ok_or(PublicationError::Capacity)?;
    total = total
        .checked_add(metadata.current.retained_bytes().get())
        .ok_or(PublicationError::Capacity)?;
    for owner in metadata.predecessors.iter().skip(1) {
        total = total
            .checked_add(owner.retained_bytes().get())
            .ok_or(PublicationError::Capacity)?;
    }
    total.checked_add(extra).ok_or(PublicationError::Capacity)
}

fn retained_quote<P: PublicationPayload>(
    metadata: &Metadata<P>,
) -> Result<usize, PublicationError<P::Error>> {
    let mut total = metadata
        .predecessors
        .capacity()
        .checked_mul(P::storage_overhead())
        .ok_or(PublicationError::Capacity)?;
    total = total
        .checked_add(metadata.current.retained_bytes().get())
        .ok_or(PublicationError::Capacity)?;
    for owner in &metadata.predecessors {
        total = total
            .checked_add(owner.retained_bytes().get())
            .ok_or(PublicationError::Capacity)?;
    }
    Ok(total)
}

#[cfg(test)]
fn note_release_retained(retained: bool) {
    RELEASE_RETAINED_PREDECESSOR.with(|cell| cell.set(Some(retained)));
}

#[cfg(test)]
std::thread_local! {
    static RELEASE_RETAINED_PREDECESSOR: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn take_release_order() -> Option<bool> {
    RELEASE_RETAINED_PREDECESSOR.with(|cell| cell.take())
}

#[cfg(test)]
mod tests {
    use super::{
        Commit, PayloadGeneration, PayloadPlan, PublicationChange, PublicationError,
        PublicationLedger, PublicationPayload, PublicationView, RetainedBytes, RetentionLimit,
        RetentionLimits, RetiredCount, take_release_order,
    };
    use std::cell::Cell;

    struct Toy {
        value: Box<u64>,
        bytes: usize,
        generation: u64,
    }

    impl Toy {
        fn new(value: u64, bytes: usize, generation: u64) -> Self {
            Self {
                value: Box::new(value),
                bytes,
                generation,
            }
        }
    }

    // The carrier TLS scan accepts only a cfg attached to this item.
    #[cfg(test)]
    thread_local! {
        static PLAN: Cell<Option<u64>> = const { Cell::new(None) };
    }

    // SAFETY: `self.value` is a heap-allocated `Box<u64>`, so its address
    // stays valid and unmoved across moves of `Toy` until drop.
    unsafe impl PublicationPayload for Toy {
        type Error = ();
        type Generation = u64;

        fn published_address(&self) -> *mut u8 {
            (&raw const *self.value).cast_mut().cast()
        }

        fn retained_bytes(&self) -> RetainedBytes {
            RetainedBytes::new(self.bytes)
        }

        fn generation(&self) -> u64 {
            self.generation
        }

        fn prepare(current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
            match PLAN.with(Cell::take) {
                Some(value) => Ok(PayloadPlan::Replacement(Toy::new(
                    value,
                    current.bytes,
                    current.generation.saturating_add(1),
                ))),
                None => Ok(PayloadPlan::Unchanged),
            }
        }
    }

    fn read(view: &PublicationView<'_, Toy>) -> u64 {
        // SAFETY: `Toy::published_address` is the heap `u64`. The ledger owns
        // that allocation, and this view borrows the ledger.
        unsafe { *view.published_address().cast::<u64>() }
    }

    fn ledger(value: u64, bytes: usize, limits: RetentionLimits) -> PublicationLedger<Toy> {
        PublicationLedger::create(Toy::new(value, bytes, 0), limits).unwrap()
    }

    fn plan(value: u64) {
        PLAN.with(|cell| cell.set(Some(value)));
    }

    #[test]
    fn view_taken_before_publish_keeps_the_old_value_after_two_publishes() {
        let store = ledger(10, 1, RetentionLimits::production());
        let first = store.view();
        assert_eq!(read(&first), 10);

        plan(20);
        let first_publish = store.prepare().unwrap().publish().unwrap();
        plan(30);
        let second_publish = store.prepare().unwrap().publish().unwrap();

        assert_eq!(
            first_publish,
            PublicationChange::Published {
                generation: 1,
                charged: RetainedBytes::new(1),
            }
        );
        assert_eq!(
            second_publish,
            PublicationChange::Published {
                generation: 2,
                charged: RetainedBytes::new(1),
            }
        );
        assert_eq!(read(&first), 10);
        assert_eq!(read(&store.view()), 30);
        let statistics = store.statistics().unwrap();
        assert_eq!(statistics.generation, 2);
        assert_eq!(statistics.retired, RetiredCount::new(2));
        assert_eq!(statistics.retained_bytes, RetainedBytes::new(3));
    }

    #[test]
    fn unchanged_does_not_push_or_store() {
        let store = ledger(10, 4, RetentionLimits::production());
        let before = store.view().published_address();
        assert_eq!(take_release_order(), None);

        let change = store.prepare().unwrap().publish().unwrap();

        assert_eq!(change, PublicationChange::Unchanged { generation: 0 });
        assert_eq!(change.charged(), RetainedBytes::new(0));
        assert_eq!(store.view().published_address(), before);
        assert_eq!(read(&store.view()), 10);
        assert_eq!(take_release_order(), None);
        let statistics = store.statistics().unwrap();
        assert_eq!(statistics.retired, RetiredCount::new(0));
        assert_eq!(statistics.generation, 0);
        assert_eq!(statistics.retained_bytes, RetainedBytes::new(4));
    }

    #[test]
    fn cap_refusal_keeps_every_owner() {
        let store = ledger(
            1,
            4,
            RetentionLimits::new(RetainedBytes::new(1_000), RetiredCount::new(1)),
        );
        let original = store.view();
        plan(2);
        assert!(matches!(
            store.prepare().unwrap().publish().unwrap(),
            PublicationChange::Published { generation: 1, .. }
        ));
        let published = store.view();
        plan(3);
        let Err(refused) = store.prepare() else {
            panic!("retiree cap kept a prepared transaction");
        };
        assert_eq!(
            refused,
            PublicationError::NeedGrace(RetentionLimit::Retired)
        );
        assert_eq!(read(&original), 1);
        assert_eq!(read(&published), 2);
        assert_eq!(read(&store.view()), 2);
        let statistics = store.statistics().unwrap();
        assert_eq!(statistics.retired, RetiredCount::new(1));
        assert_eq!(statistics.generation, 1);
        assert_eq!(statistics.retained_bytes, RetainedBytes::new(8));

        let store = ledger(
            1,
            10,
            RetentionLimits::new(RetainedBytes::new(20), RetiredCount::new(8)),
        );
        let original = store.view();
        plan(2);
        store.prepare().unwrap().publish().unwrap();
        let published = store.view();
        plan(3);
        let Err(refused) = store.prepare() else {
            panic!("byte cap kept a prepared transaction");
        };
        assert_eq!(
            refused,
            PublicationError::NeedGrace(RetentionLimit::RetainedBytes)
        );
        assert_eq!(read(&original), 1);
        assert_eq!(read(&published), 2);
        assert_eq!(read(&store.view()), 2);
        let statistics = store.statistics().unwrap();
        assert_eq!(statistics.retired, RetiredCount::new(1));
        assert_eq!(statistics.generation, 1);
        assert_eq!(statistics.retained_bytes, RetainedBytes::new(20));
    }

    #[test]
    fn predecessor_is_pushed_before_release_store() {
        let store = ledger(10, 1, RetentionLimits::production());
        assert_eq!(take_release_order(), None);
        plan(20);
        let change = store.prepare().unwrap().publish().unwrap();
        assert_eq!(
            change,
            PublicationChange::Published {
                generation: 1,
                charged: RetainedBytes::new(1),
            }
        );
        assert_eq!(take_release_order(), Some(true));
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
        assert_eq!(read(&store.view()), 20);
    }

    #[test]
    fn replacement_publishes_a_non_prefix_successor() {
        let store = ledger(10, 1, RetentionLimits::production());
        let old = store.view();
        let old_address = old.published_address();
        plan(20);
        let change = store.prepare().unwrap().publish().unwrap();
        let new_address = store.view().published_address();
        // The successor is a distinct allocation. Nothing in the core asks
        // the new address to extend the predecessor; that law is the adapter's.
        assert_ne!(old_address, new_address);
        assert_eq!(read(&old), 10);
        assert_eq!(read(&store.view()), 20);
        assert_eq!(
            change,
            PublicationChange::Published {
                generation: 1,
                charged: RetainedBytes::new(1),
            }
        );
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
    }

    #[test]
    fn byte_quote_overflow_is_capacity_not_need_grace() {
        let store = ledger(
            1,
            usize::MAX,
            RetentionLimits::new(RetainedBytes::new(usize::MAX), RetiredCount::new(8)),
        );
        let original = store.view();
        plan(2);
        let Err(refused) = store.prepare() else {
            panic!("an overflowing quote prepared a transaction");
        };
        assert_eq!(refused, PublicationError::Capacity);
        assert_eq!(read(&original), 1);
        assert_eq!(read(&store.view()), 1);
        let statistics = store.statistics().unwrap();
        assert_eq!(statistics.retired, RetiredCount::new(0));
        assert_eq!(statistics.generation, 0);
        assert_eq!(statistics.retained_bytes, RetainedBytes::new(usize::MAX));
    }

    #[test]
    fn e7p0_grace_drain_reclaims_retired_preserves_current_and_unblocks_need_grace() {
        let store = ledger(
            1,
            10,
            RetentionLimits::new(RetainedBytes::new(25), RetiredCount::new(1)),
        );
        plan(2);
        store.prepare().unwrap().publish().unwrap();
        let current_addr = store.view().published_address();
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
        plan(3);
        let Err(refused) = store.prepare() else {
            panic!("retiree cap kept a prepared transaction");
        };
        assert_eq!(
            refused,
            PublicationError::NeedGrace(RetentionLimit::Retired)
        );

        // SAFETY: test owns `store` exclusively and holds no live PublicationView.
        let report = unsafe { store.drain_predecessors_after_grace() }.unwrap();
        assert_eq!(report.reclaimed, RetiredCount::new(1));
        assert!(report.freed_bytes.get() >= 10);
        let after = store.statistics().unwrap();
        assert_eq!(after.retired, RetiredCount::new(0));
        assert_eq!(after.generation, 1);
        assert_eq!(after.retained_bytes, RetainedBytes::new(10));
        assert_eq!(store.view().published_address(), current_addr);
        assert_eq!(read(&store.view()), 2);

        plan(3);
        store.prepare().unwrap().publish().unwrap();
        assert_eq!(read(&store.view()), 3);
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
    }

    #[test]
    fn e7p0_grace_drain_refuses_busy_while_publication_view_is_live() {
        let store = ledger(1, 10, RetentionLimits::production());
        plan(2);
        store.prepare().unwrap().publish().unwrap();
        let live_view = store.view();
        // SAFETY: testing fail-closed refusal while a PublicationView is live.
        let refused = unsafe { store.drain_predecessors_after_grace() };
        assert_eq!(refused, Err(PublicationError::Busy));
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
        assert_eq!(read(&live_view), 2);
        drop(live_view);

        // SAFETY: `live_view` dropped; test owns `store` exclusively.
        let report = unsafe { store.drain_predecessors_after_grace() }.unwrap();
        assert_eq!(report.reclaimed, RetiredCount::new(1));
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(0));
    }

    #[test]
    fn e7p0_grace_drain_refuses_poisoned_and_retains_all_owners() {
        let store = ledger(1, 10, RetentionLimits::production());
        plan(2);
        store.prepare().unwrap().publish().unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.hold_metadata_for_test(|| panic!("poison metadata"));
        }));
        // SAFETY: testing fail-closed poison retention.
        let refused = unsafe { store.drain_predecessors_after_grace() };
        assert_eq!(refused, Err(PublicationError::Poisoned));
        store.with_retained(|current, predecessors| {
            assert_eq!(*current.value, 2);
            assert_eq!(predecessors.len(), 1);
            assert_eq!(*predecessors[0].value, 1);
        });
    }

    #[test]
    fn e7p0_grace_drain_refuses_busy_when_metadata_mutex_contended() {
        let store = ledger(1, 10, RetentionLimits::production());
        plan(2);
        store.prepare().unwrap().publish().unwrap();
        let held = store.account(|_| Ok(())).result.unwrap();
        // SAFETY: testing fail-closed refusal while the metadata mutex is contended.
        let refused = unsafe { store.drain_predecessors_after_grace() };
        assert_eq!(refused, Err(PublicationError::Busy));
        drop(held);
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
        // SAFETY: lock released and zero views are live.
        let report = unsafe { store.drain_predecessors_after_grace() }.unwrap();
        assert_eq!(report.reclaimed, RetiredCount::new(1));
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(0));
    }

    #[test]
    fn e7p0_grace_drain_refuses_noting_ledger_and_retains_generations() {
        let store = PublicationLedger::create_noting(
            Toy::new(1, 10, 0),
            RetentionLimits::production(),
            |_| PayloadGeneration::from_raw(1),
        )
        .unwrap();
        let open = store.account(|_| Ok(Toy::new(2, 10, 1))).result.unwrap();
        open.finish_noting(
            |next, _| Ok(Commit::Replace(next)),
            |_| PayloadGeneration::from_raw(2),
        )
        .unwrap();
        // SAFETY: testing fail-closed refusal on a noting ledger.
        let refused = unsafe { store.drain_predecessors_after_grace() };
        assert_eq!(refused, Err(PublicationError::Busy));
        assert_eq!(store.statistics().unwrap().retired, RetiredCount::new(1));
        store.with_retained(|current, predecessors| {
            assert_eq!(*current.value, 2);
            assert_eq!(predecessors.len(), 1);
            assert_eq!(*predecessors[0].value, 1);
        });
    }
}
