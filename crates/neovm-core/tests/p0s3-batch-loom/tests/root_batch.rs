//! An independent model of the root-table batch protocol, not VM code.
//!
//! Atomic model words make a missing publication edge an exact stale-payload
//! assertion rather than hiding it behind a model-only mutex. The production
//! payload is immutable host storage, not these atomics. No raw Lisp pointers
//! or unsafe operations are used here.

#![forbid(unsafe_code)]

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use loom::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum Phase {
    Reserved,
    Live,
    Closing,
    Retired,
    Cancelled,
}

assert_impl_all!(Phase: Send, Sync, Copy, fmt::Debug);
const _: () = {
    assert!(std::mem::size_of::<Phase>() == std::mem::size_of::<u8>());
    assert!(std::mem::align_of::<Phase>() == std::mem::align_of::<u8>());
};

mod sealed {
    pub trait Protocol {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationLayer {
    /// Production also installs a OnceLock payload while holding the table
    /// lock. Both supply synchronization independent of the state word.
    TableAndOnce,
    /// A separate experiment isolates just the state publication edge.
    StateOnly,
}

trait Protocol: sealed::Protocol + Send + Sync + fmt::Debug + 'static {
    const PUBLICATION_LAYER: PublicationLayer = PublicationLayer::TableAndOnce;
    const PUBLISH: Ordering = Ordering::Release;
    const OBSERVE: Ordering = Ordering::Acquire;
    const CHECK_ACTIVE_READERS: bool = true;
    const CHECK_ABANDONED_READERS: bool = true;
    const ABANDONMENT_FIRST: bool = false;
}

macro_rules! protocol {
    ($name:ident $(, $field:ident = $value:expr)*) => {
        #[derive(Debug)]
        struct $name;
        impl sealed::Protocol for $name {}
        impl Protocol for $name {
            $(const $field: protocol!(@type $field) = $value;)*
        }
        assert_impl_all!($name: Send, Sync, fmt::Debug);
    };
    (@type PUBLISH) => { Ordering };
    (@type OBSERVE) => { Ordering };
    (@type PUBLICATION_LAYER) => { PublicationLayer };
    (@type CHECK_ACTIVE_READERS) => { bool };
    (@type CHECK_ABANDONED_READERS) => { bool };
    (@type ABANDONMENT_FIRST) => { bool };
}

protocol!(Production);
protocol!(
    StatePublicationOnly,
    PUBLICATION_LAYER = PublicationLayer::StateOnly
);
protocol!(
    RelaxedPublication,
    PUBLICATION_LAYER = PublicationLayer::StateOnly,
    PUBLISH = Ordering::Relaxed
);
protocol!(
    RelaxedObservation,
    PUBLICATION_LAYER = PublicationLayer::StateOnly,
    OBSERVE = Ordering::Relaxed
);
protocol!(PrematureRetirement, CHECK_ACTIVE_READERS = false);
protocol!(DropCountsAsFinish, CHECK_ABANDONED_READERS = false);
protocol!(WrongCounterOrder, ABANDONMENT_FIRST = true);

/// The independent semantic oracle includes an uninterned symbol. Filtering
/// only heap-object pointers would incorrectly omit that root.
#[derive(Clone, Copy, Debug)]
enum InputValue {
    HeapObject,
    UninternedSymbol,
    Nil,
}

impl InputValue {
    fn root_word(self) -> Option<usize> {
        match self {
            Self::HeapObject => Some(11),
            Self::UninternedSymbol => Some(23),
            Self::Nil => None,
        }
    }
}

const INPUT: [InputValue; 3] = [
    InputValue::HeapObject,
    InputValue::UninternedSymbol,
    InputValue::Nil,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Generation {
    First,
    Second,
}

/// Instrument the held-exclusion obligation independently of the slot's
/// storage Arc. A retained slot/node is not itself an exclusion owner.
#[derive(Debug)]
struct Epoch {
    held: AtomicUsize,
    admission: Mutex<()>,
}

assert_impl_all!(Epoch: Send, Sync, fmt::Debug);
assert_not_impl_any!(Epoch: Clone, Copy);

impl Epoch {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            held: AtomicUsize::new(0),
            admission: Mutex::new(()),
        })
    }

    fn may_launch(&self) -> bool {
        let _admission = self.admission.lock().unwrap();
        self.held.load(Ordering::Acquire) == 0
    }
}

#[derive(Debug)]
struct Exclusion {
    epoch: Arc<Epoch>,
    _thread: PhantomData<Rc<()>>,
}

assert_not_impl_any!(Exclusion: Send, Sync, Clone, Copy);
assert_impl_all!(Exclusion: fmt::Debug);

impl Exclusion {
    fn acquire(epoch: &Arc<Epoch>) -> Self {
        let _admission = epoch.admission.lock().unwrap();
        epoch.held.fetch_add(1, Ordering::Release);
        Self {
            epoch: Arc::clone(epoch),
            _thread: PhantomData,
        }
    }
}

impl Drop for Exclusion {
    fn drop(&mut self) {
        self.epoch.held.fetch_sub(1, Ordering::Release);
    }
}

#[derive(Debug)]
struct Slot<P> {
    phase: AtomicU8,
    /// The production slot/table lock serializes publication, explicit reader
    /// admission, close and retirement. State-only experiments omit this lock
    /// ONLY on publication, leaving the reader/retirement protocol unchanged.
    admission: Mutex<()>,
    readers: AtomicUsize,
    abandoned: AtomicUsize,
    words: [AtomicUsize; 3],
    /// Release/Acquire initialization models the OnceLock publication edge,
    /// without pretending the model contains a real payload pointer.
    payload_initialized: AtomicBool,
    generation: Generation,
    /// Observational only: this Arc does not increment `epoch.held`.
    epoch: Arc<Epoch>,
    _protocol: PhantomData<P>,
}

assert_impl_all!(Slot<Production>: Send, Sync, fmt::Debug);
assert_not_impl_any!(Slot<Production>: Clone, Copy);

impl<P: Protocol> Slot<P> {
    fn phase(&self, ordering: Ordering) -> Phase {
        Phase::try_from(self.phase.load(ordering)).expect("closed model phase")
    }

    fn store_phase(&self, phase: Phase, ordering: Ordering) {
        self.phase.store(phase.into(), ordering);
    }

    fn assert_words(&self) {
        if P::PUBLICATION_LAYER == PublicationLayer::TableAndOnce {
            assert!(
                self.payload_initialized.load(Ordering::Acquire),
                "accepted unpublished batch payload"
            );
        }
        for (word, expected) in self.words.iter().zip(INPUT) {
            assert_eq!(
                word.load(Ordering::Relaxed),
                expected.root_word().unwrap_or(0),
                "accepted unpublished batch payload"
            );
        }
    }

    /// The common GC root walk keeps Closing visible even though new explicit
    /// readers are forbidden. Its lock protects payload unlink/recycling.
    fn trace(&self) -> Vec<usize> {
        let _admission = self.admission.lock().unwrap();
        match self.phase(P::OBSERVE) {
            Phase::Live | Phase::Closing => {
                self.assert_words();
                self.words
                    .iter()
                    .map(|word| word.load(Ordering::Relaxed))
                    .filter(|word| *word != 0)
                    .collect()
            }
            Phase::Reserved | Phase::Cancelled | Phase::Retired => Vec::new(),
        }
    }

    /// The low-level counter protocol is explored independently of Rust's
    /// outer lease borrow, which further restricts reachable API executions.
    fn reader(slot: &Arc<Self>) -> Option<Reader<P>> {
        let _admission = slot.admission.lock().unwrap();
        if slot.phase(P::OBSERVE) != Phase::Live {
            return None;
        }
        slot.readers.fetch_add(1, Ordering::AcqRel);
        slot.assert_words();
        Some(Reader {
            slot: Some(Arc::clone(slot)),
            _thread: PhantomData,
        })
    }

    fn close_and_finish(&self) -> Result<(), FinishError> {
        let _admission = self.admission.try_lock().map_err(|_| FinishError::Busy)?;
        match self.phase(Ordering::Acquire) {
            Phase::Live => self.store_phase(Phase::Closing, Ordering::Release),
            Phase::Closing => {}
            Phase::Reserved | Phase::Cancelled | Phase::Retired => {
                return Err(FinishError::WrongPhase);
            }
        }

        // Drop publishes abandonment before decrementing the active count.
        // Observing the final zero with Acquire must precede reading abandoned;
        // otherwise the closer could read old zero followed by the new zero.
        let (readers, abandoned) = if P::ABANDONMENT_FIRST {
            let abandoned = self.abandoned.load(Ordering::Acquire);
            let readers = self.readers.load(Ordering::Acquire);
            (readers, abandoned)
        } else {
            let readers = self.readers.load(Ordering::Acquire);
            let abandoned = self.abandoned.load(Ordering::Acquire);
            (readers, abandoned)
        };
        if P::CHECK_ACTIVE_READERS && readers != 0 {
            return Err(FinishError::ActiveReaders);
        }
        if P::CHECK_ABANDONED_READERS && abandoned != 0 {
            return Err(FinishError::AbandonedReader);
        }
        self.store_phase(Phase::Retired, Ordering::Release);
        Ok(())
    }
}

/// One counted retained epoch. Its destructor models the logical obligation:
/// Live/Closing abandonment retains the count permanently; terminal states
/// release it. Production retains ownership (including the actual Arc), not
/// merely this model's counter. The table's observational Slot Arc is separate.
#[derive(Debug)]
struct Retention<P: Protocol> {
    slot: Arc<Slot<P>>,
}

assert_impl_all!(Retention<Production>: Send, Sync, fmt::Debug);
assert_not_impl_any!(Retention<Production>: Clone, Copy);

impl<P: Protocol> Drop for Retention<P> {
    fn drop(&mut self) {
        match self.slot.phase(Ordering::Acquire) {
            Phase::Cancelled | Phase::Retired => {
                self.slot.epoch.held.fetch_sub(1, Ordering::Release);
            }
            Phase::Reserved | Phase::Live | Phase::Closing => {}
        }
    }
}

#[derive(Debug)]
struct Prepared<P: Protocol> {
    retention: Option<Retention<P>>,
    _thread: PhantomData<Rc<()>>,
}

assert_not_impl_any!(Prepared<Production>: Send, Sync, Clone, Copy);
assert_impl_all!(Prepared<Production>: fmt::Debug);

impl<P: Protocol> Prepared<P> {
    fn reserve(exclusion: &Exclusion, generation: Generation) -> Self {
        exclusion.epoch.held.fetch_add(1, Ordering::Relaxed);
        let slot = Arc::new(Slot {
            phase: AtomicU8::new(Phase::Reserved.into()),
            admission: Mutex::new(()),
            readers: AtomicUsize::new(0),
            abandoned: AtomicUsize::new(0),
            words: std::array::from_fn(|_| AtomicUsize::new(0)),
            payload_initialized: AtomicBool::new(false),
            generation,
            epoch: Arc::clone(&exclusion.epoch),
            _protocol: PhantomData,
        });
        Self {
            retention: Some(Retention { slot }),
            _thread: PhantomData,
        }
    }

    fn finalize(mut self) -> Finalized<P> {
        let retention = self.retention.take().unwrap();
        for (word, value) in retention.slot.words.iter().zip(INPUT) {
            word.store(value.root_word().unwrap_or(0), Ordering::Relaxed);
        }
        Finalized {
            retention: Some(retention),
            _thread: PhantomData,
        }
    }
}

impl<P: Protocol> Drop for Prepared<P> {
    fn drop(&mut self) {
        if let Some(retention) = &self.retention {
            retention
                .slot
                .store_phase(Phase::Cancelled, Ordering::Release);
        }
    }
}

#[derive(Debug)]
struct Finalized<P: Protocol> {
    retention: Option<Retention<P>>,
    _thread: PhantomData<Rc<()>>,
}

assert_not_impl_any!(Finalized<Production>: Send, Sync, Clone, Copy);
assert_impl_all!(Finalized<Production>: fmt::Debug);

impl<P: Protocol> Finalized<P> {
    fn publish(mut self) -> Lease<P> {
        let retention = self.retention.take().unwrap();
        match P::PUBLICATION_LAYER {
            PublicationLayer::TableAndOnce => {
                let _admission = retention.slot.admission.lock().unwrap();
                retention
                    .slot
                    .payload_initialized
                    .store(true, Ordering::Release);
                retention.slot.store_phase(Phase::Live, P::PUBLISH);
            }
            PublicationLayer::StateOnly => {
                retention.slot.store_phase(Phase::Live, P::PUBLISH);
            }
        }
        Lease { retention }
    }
}

impl<P: Protocol> Drop for Finalized<P> {
    fn drop(&mut self) {
        if let Some(retention) = &self.retention {
            retention
                .slot
                .store_phase(Phase::Cancelled, Ordering::Release);
        }
    }
}

#[derive(Debug)]
struct Lease<P: Protocol> {
    retention: Retention<P>,
}

assert_impl_all!(Lease<Production>: Send, Sync, fmt::Debug);
assert_not_impl_any!(Lease<Production>: Clone, Copy);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishError {
    Busy,
    ActiveReaders,
    AbandonedReader,
    WrongPhase,
}

#[derive(Debug)]
struct FinishFailure<P: Protocol> {
    lease: Lease<P>,
    error: FinishError,
}

assert_impl_all!(FinishFailure<Production>: Send, Sync, fmt::Debug);
assert_not_impl_any!(FinishFailure<Production>: Clone, Copy);

#[derive(Clone, Copy, Debug)]
struct Retired {
    generation: Generation,
}

assert_impl_all!(Retired: Send, Sync, Clone, Copy, fmt::Debug);

impl<P: Protocol> Lease<P> {
    fn try_finish(self) -> Result<Retired, FinishFailure<P>> {
        match self.retention.slot.close_and_finish() {
            Ok(()) => Ok(Retired {
                generation: self.retention.slot.generation,
            }),
            Err(error) => Err(FinishFailure { lease: self, error }),
        }
    }
}

#[derive(Debug)]
struct Reader<P: Protocol> {
    slot: Option<Arc<Slot<P>>>,
    _thread: PhantomData<Rc<()>>,
}

assert_not_impl_any!(Reader<Production>: Send, Sync, Clone, Copy);
assert_impl_all!(Reader<Production>: fmt::Debug);

impl<P: Protocol> Reader<P> {
    fn read(&self) {
        let slot = self.slot.as_ref().unwrap();
        assert!(
            matches!(slot.phase(Ordering::Acquire), Phase::Live | Phase::Closing),
            "retired a batch with an active reader"
        );
        slot.assert_words();
    }

    fn finish(mut self) {
        let slot = self.slot.take().unwrap();
        slot.readers.fetch_sub(1, Ordering::Release);
    }
}

impl<P: Protocol> Drop for Reader<P> {
    fn drop(&mut self) {
        if let Some(slot) = &self.slot {
            slot.abandoned.fetch_add(1, Ordering::Release);
            slot.readers.fetch_sub(1, Ordering::Release);
        }
    }
}

fn check(model: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 2;
    builder.max_branches = 256;
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.preemption_bound = None;
    builder.checkpoint_file = None;
    builder.expect_explicit_explore = false;
    builder.location = true;
    builder.log = false;
    builder.check(model);
}

fn live<P: Protocol>() -> (Arc<Epoch>, Arc<Slot<P>>, Lease<P>) {
    let epoch = Epoch::new();
    let exclusion = Exclusion::acquire(&epoch);
    let prepared = Prepared::<P>::reserve(&exclusion, Generation::First);
    let slot = Arc::clone(&prepared.retention.as_ref().unwrap().slot);
    let lease = prepared.finalize().publish();
    drop(exclusion);
    (epoch, slot, lease)
}

fn publication<P: Protocol>() {
    check(|| {
        let epoch = Epoch::new();
        let exclusion = Exclusion::acquire(&epoch);
        let prepared = Prepared::<P>::reserve(&exclusion, Generation::First);
        let slot = Arc::clone(&prepared.retention.as_ref().unwrap().slot);
        // Prepared/Finalized stay on their creating thread. The shared table
        // slot reaches the scanner before publication, without a join edge.
        let scanned = Arc::clone(&slot);
        let reader = thread::spawn(move || {
            if let Some(reader) = Slot::reader(&scanned) {
                reader.read();
                reader.finish();
            }
        });
        let lease = prepared.finalize().publish();
        drop(exclusion);
        reader.join().unwrap();
        assert_eq!(slot.trace(), vec![11, 23]);
        assert!(!epoch.may_launch());
        lease.try_finish().unwrap();
        assert!(epoch.may_launch());
    });
}

#[test]
fn release_acquire_publishes_the_complete_immutable_batch() {
    publication::<Production>();
}

#[test]
fn state_only_experiment_retains_release_acquire_without_extra_publication_edges() {
    publication::<StatePublicationOnly>();
}

#[test]
fn cancelling_reserved_and_finalized_batches_releases_only_their_epoch() {
    check(|| {
        let epoch = Epoch::new();
        for finalize in [false, true] {
            let exclusion = Exclusion::acquire(&epoch);
            let prepared = Prepared::<Production>::reserve(&exclusion, Generation::First);
            let slot = Arc::clone(&prepared.retention.as_ref().unwrap().slot);
            drop(exclusion);
            // Reserved/Finalized must retain exclusion independently of the
            // admission supplied to reserve, not merely borrow its lifetime.
            assert!(!epoch.may_launch());
            if finalize {
                let finalized = prepared.finalize();
                assert!(!epoch.may_launch());
                drop(finalized);
            } else {
                drop(prepared);
            }
            assert_eq!(slot.phase(Ordering::Acquire), Phase::Cancelled);
            assert!(Slot::reader(&slot).is_none());
            assert!(slot.trace().is_empty());
            assert!(epoch.may_launch());
        }
        let exclusion = Exclusion::acquire(&epoch);
        drop(Prepared::<Production>::reserve(
            &exclusion,
            Generation::First,
        ));
        // Cancelling a batch cannot release its caller's separate admission.
        assert!(!epoch.may_launch());
        drop(exclusion);
        assert!(epoch.may_launch());
    });
}

#[test]
fn close_blocks_new_readers_but_keeps_existing_readers_and_gc_roots() {
    check(|| {
        let (epoch, slot, lease) = live::<Production>();
        let reader = Slot::reader(&slot).unwrap();
        let failure = lease.try_finish().unwrap_err();
        assert_eq!(failure.error, FinishError::ActiveReaders);
        assert_eq!(slot.phase(Ordering::Acquire), Phase::Closing);
        assert!(Slot::reader(&slot).is_none());
        assert_eq!(slot.trace(), vec![11, 23]);
        reader.read();
        assert!(!epoch.may_launch());
        reader.finish();
        let retired = failure.lease.try_finish().unwrap();
        assert_eq!(retired.generation, Generation::First);
        assert!(slot.trace().is_empty());
        // Retaining the retired node's storage does not retain the exclusion.
        assert!(epoch.may_launch());
    });
}

#[test]
fn reader_drop_is_abandonment_and_cannot_complete_retirement() {
    check(|| {
        let (epoch, slot, lease) = live::<Production>();
        drop(Slot::reader(&slot).unwrap());
        let failure = lease.try_finish().unwrap_err();
        assert_eq!(failure.error, FinishError::AbandonedReader);
        assert_eq!(slot.phase(Ordering::Acquire), Phase::Closing);
        assert_eq!(slot.trace(), vec![11, 23]);
        drop(failure.lease);
        assert_eq!(slot.trace(), vec![11, 23]);
        assert!(!epoch.may_launch());
    });
}

#[test]
fn unfinished_lease_drop_keeps_live_roots_and_exclusion() {
    check(|| {
        let (epoch, slot, lease) = live::<Production>();
        drop(lease);
        assert_eq!(slot.phase(Ordering::Acquire), Phase::Live);
        assert_eq!(slot.trace(), vec![11, 23]);
        assert!(!epoch.may_launch());
    });
}

fn abandonment_race<P: Protocol>() {
    check(|| {
        let (epoch, slot, lease) = live::<P>();
        let worker_slot = Arc::clone(&slot);
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let reader = Slot::reader(&worker_slot).unwrap();
            ready_tx.send(()).unwrap();
            drop(reader);
        });
        ready_rx.recv().unwrap();
        let result = lease.try_finish();
        worker.join().unwrap();
        assert!(
            slot.phase(Ordering::Acquire) != Phase::Retired,
            "retired a batch without explicit reader finish"
        );
        let failure = result.unwrap_err();
        assert!(matches!(
            failure.error,
            FinishError::ActiveReaders | FinishError::AbandonedReader
        ));
        assert_eq!(slot.trace(), vec![11, 23]);
        drop(failure.lease);
        assert!(!epoch.may_launch());
    });
}

#[test]
fn acquire_reader_zero_orders_the_prior_abandonment_store() {
    abandonment_race::<Production>();
}

#[test]
fn explicit_reader_finish_racing_close_preserves_failed_finish_ownership() {
    check(|| {
        let (epoch, slot, lease) = live::<Production>();
        let worker_slot = Arc::clone(&slot);
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let reader = Slot::reader(&worker_slot).unwrap();
            reader.read();
            ready_tx.send(()).unwrap();
            reader.finish();
        });
        ready_rx.recv().unwrap();
        let result = lease.try_finish();
        worker.join().unwrap();
        let retired = match result {
            Ok(retired) => retired,
            Err(failure) => {
                assert_eq!(failure.error, FinishError::ActiveReaders);
                assert_eq!(slot.phase(Ordering::Acquire), Phase::Closing);
                assert_eq!(slot.trace(), vec![11, 23]);
                failure.lease.try_finish().unwrap()
            }
        };
        assert_eq!(retired.generation, Generation::First);
        assert!(slot.trace().is_empty());
        assert!(epoch.may_launch());
    });
}

#[test]
fn busy_retirement_preserves_the_live_lease_and_exclusion_for_retry() {
    check(|| {
        let (epoch, slot, lease) = live::<Production>();
        let worker_slot = Arc::clone(&slot);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let admission = worker_slot.admission.lock().unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            drop(admission);
        });
        ready_rx.recv().unwrap();
        let failure = lease.try_finish().unwrap_err();
        assert_eq!(failure.error, FinishError::Busy);
        assert_eq!(slot.phase(Ordering::Acquire), Phase::Live);
        assert!(!epoch.may_launch());
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(slot.trace(), vec![11, 23]);
        failure.lease.try_finish().unwrap();
        assert!(slot.trace().is_empty());
        assert!(epoch.may_launch());
    });
}

#[test]
fn recycling_uses_a_new_control_and_generation() {
    check(|| {
        let epoch = Epoch::new();
        let exclusion = Exclusion::acquire(&epoch);
        let first = Prepared::<Production>::reserve(&exclusion, Generation::First);
        let old_control = Arc::clone(&first.retention.as_ref().unwrap().slot);
        drop(first);
        let second = Prepared::<Production>::reserve(&exclusion, Generation::Second);
        let new_control = Arc::clone(&second.retention.as_ref().unwrap().slot);
        assert!(!Arc::ptr_eq(&old_control, &new_control));
        let lease = second.finalize().publish();
        drop(exclusion);
        assert_eq!(old_control.phase(Ordering::Acquire), Phase::Cancelled);
        assert_eq!(new_control.phase(Ordering::Acquire), Phase::Live);
        assert_eq!(new_control.trace(), vec![11, 23]);
        assert!(!epoch.may_launch());
        let retired = lease.try_finish().unwrap();
        assert_eq!(retired.generation, Generation::Second);
        assert!(epoch.may_launch());
    });
}

#[test]
#[should_panic(expected = "accepted unpublished batch payload")]
fn negative_control_relaxed_publication_exposes_unpublished_words() {
    publication::<RelaxedPublication>();
}

#[test]
#[should_panic(expected = "accepted unpublished batch payload")]
fn negative_control_relaxed_observation_exposes_unpublished_words() {
    publication::<RelaxedObservation>();
}

#[test]
#[should_panic(expected = "retired a batch with an active reader")]
fn negative_control_premature_retirement_removes_an_active_root() {
    check(|| {
        let (_, slot, lease) = live::<PrematureRetirement>();
        let reader = Slot::reader(&slot).unwrap();
        let _retired = lease.try_finish().unwrap();
        reader.read();
        reader.finish();
    });
}

#[test]
#[should_panic(expected = "retired a batch without explicit reader finish")]
fn negative_control_drop_must_not_count_as_explicit_finish() {
    abandonment_race::<DropCountsAsFinish>();
}

#[test]
#[should_panic(expected = "retired a batch without explicit reader finish")]
fn negative_control_abandonment_first_can_accept_two_unrelated_zeroes() {
    abandonment_race::<WrongCounterOrder>();
}
