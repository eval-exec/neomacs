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
        let plan = P::prepare(&metadata.current).map_err(PublicationError::Payload)?;
        if let PayloadPlan::Replacement(next) = &plan {
            admit_replacement(&mut metadata, next, self.limits)?;
        }
        Ok(PreparedPublication {
            store: self,
            metadata,
            plan,
            _thread: PhantomData,
        })
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
        let charged = next.retained_bytes();
        let generation = next.generation();
        let pointer = next.published_address();
        let previous = mem::replace(&mut self.metadata.current, next);
        self.metadata.predecessors.push(previous);
        self.store
            .release_retained(pointer, &self.metadata.predecessors);
        Ok(PublicationChange::Published {
            generation,
            charged,
        })
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
    if metadata.predecessors.len() >= limits.retired.get() {
        return Err(PublicationError::NeedGrace(RetentionLimit::Retired));
    }
    if metadata.predecessors.try_reserve(1).is_err() {
        return Err(PublicationError::NeedGrace(RetentionLimit::Retired));
    }
    let needed = retained_quote(metadata)?
        .checked_add(next.retained_bytes().get())
        .ok_or(PublicationError::NeedGrace(RetentionLimit::RetainedBytes))?;
    if needed > limits.bytes.get() {
        return Err(PublicationError::NeedGrace(RetentionLimit::RetainedBytes));
    }
    Ok(())
}

fn retained_quote<P: PublicationPayload>(
    metadata: &Metadata<P>,
) -> Result<usize, PublicationError<P::Error>> {
    let mut total = metadata.current.retained_bytes().get();
    for owner in &metadata.predecessors {
        total = total
            .checked_add(owner.retained_bytes().get())
            .ok_or(PublicationError::NeedGrace(RetentionLimit::RetainedBytes))?;
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
}
