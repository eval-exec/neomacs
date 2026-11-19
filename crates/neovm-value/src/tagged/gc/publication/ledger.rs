//! Generic publication owner.
//!
//! One current owner, a predecessor vector, a nonblocking metadata mutex, and
//! an atomic root. There is no `reclaim` method. `NeedGrace` refuses and keeps
//! every owner. The core does not implement `Send` or `Sync`: an arbitrary
//! published address is not that proof.

use super::sync::{AtomicPtr, Mutex, MutexGuard, Ordering, TryLockError};
use super::{
    PayloadPlan, PublicationChange, PublicationError, PublicationPayload, PublicationStatistics,
    RetainedBytes, RetentionLimit, RetentionLimits, RetiredCount,
};
use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;

struct Metadata<P> {
    current: P,
    predecessors: Vec<P>,
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
    // Keeps `Send`/`Sync` off this generic store. The typed publisher, not an
    // arbitrary pointer, is what may add that proof later.
    _not_send: PhantomData<Rc<()>>,
}

/// Acquire snapshot of the root. It keeps the address it loaded.
#[must_use = "retain this view until every use of the loaded address returns"]
pub(crate) struct PublicationView<'owner, P: PublicationPayload> {
    address: *mut u8,
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

/// Names a payload for the auto-trait assertions and the compile-fail fixture.
pub(crate) struct BoundWitness;

impl PublicationPayload for BoundWitness {
    type Error = ();
    type Generation = u64;

    fn published_address(&self) -> *mut u8 {
        std::ptr::null_mut()
    }

    fn retained_bytes(&self) -> RetainedBytes {
        RetainedBytes::new(0)
    }

    fn generation(&self) -> u64 {
        0
    }

    fn prepare(_current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
        Ok(PayloadPlan::Unchanged)
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
                predecessors: Vec::new(),
                #[cfg(any(test, feature = "test-support"))]
                limit_override: None,
            }),
            limits,
            _not_send: PhantomData,
        };
        store.root.store(address, Ordering::Release);
        Ok(store)
    }

    /// [`Self::create`] plus `note_roots` on the payload that becomes current,
    /// before the Release store. A quote refusal does not call `note_roots`.
    ///
    /// `create` itself stays the no-hook path so symbol publication does not
    /// monomorphize a root callback.
    pub(crate) fn create_noting(
        initial: P,
        limits: RetentionLimits,
        note_roots: impl FnOnce(&P),
    ) -> Result<Self, PublicationError<P::Error>> {
        if initial.retained_bytes().get() > limits.bytes.get() {
            return Err(PublicationError::NeedGrace(RetentionLimit::RetainedBytes));
        }
        let address = initial.published_address();
        note_roots(&initial);
        let store = Self {
            root: AtomicPtr::new(std::ptr::null_mut()),
            metadata: Mutex::new(Metadata {
                current: initial,
                predecessors: Vec::new(),
                #[cfg(any(test, feature = "test-support"))]
                limit_override: None,
            }),
            limits,
            _not_send: PhantomData,
        };
        store.root.store(address, Ordering::Release);
        Ok(store)
    }

    /// One Acquire load. Later publications do not change this view.
    pub(crate) fn view(&self) -> PublicationView<'_, P> {
        PublicationView {
            address: self.root.load(Ordering::Acquire),
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
    /// `Commit::Keep` does not call `note_roots`.
    pub(crate) fn finish_noting(
        self,
        decide: impl FnOnce(T, &P) -> Result<Commit<P>, PublicationError<P::Error>>,
        note_roots: impl FnOnce(&P),
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
    let previous = mem::replace(&mut metadata.current, next);
    metadata.predecessors.push(previous);
    store.release_retained(pointer, &metadata.predecessors);
    PublicationChange::Published {
        generation,
        charged,
    }
}

/// [`commit_replacement`] after `note_roots` has seen the new current payload.
/// The note runs only on this replacement path, before the Release store.
fn commit_replacement_noting<P: PublicationPayload>(
    store: &PublicationLedger<P>,
    metadata: &mut Metadata<P>,
    next: P,
    note_roots: impl FnOnce(&P),
) -> PublicationChange<P::Generation> {
    let charged = next.retained_bytes();
    let generation = next.generation();
    let pointer = next.published_address();
    note_roots(&next);
    let previous = mem::replace(&mut metadata.current, next);
    metadata.predecessors.push(previous);
    store.release_retained(pointer, &metadata.predecessors);
    PublicationChange::Published {
        generation,
        charged,
    }
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
    if metadata.predecessors.try_reserve(1).is_err() {
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
        PayloadPlan, PublicationChange, PublicationError, PublicationLedger, PublicationPayload,
        PublicationView, RetainedBytes, RetentionLimit, RetentionLimits, RetiredCount,
        take_release_order,
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

    impl PublicationPayload for Toy {
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
}
