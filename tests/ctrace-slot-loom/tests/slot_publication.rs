//! One serialized mutator publishes a slot while the marker reads it.
//! See README.md for the exact ordering correspondence and scope limits.
#![forbid(unsafe_code)]

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicUsize, Ordering, fence};
use loom::thread;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use static_assertions::{assert_impl_all, assert_not_impl_any};

// Opaque model identities, never real Values or pointer addresses.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
enum SlotWord {
    Immediate = 0,
    OldChild = 1,
    ConstructedChild = 2,
}

assert_impl_all!(SlotWord: Send, Sync, Copy, Clone, fmt::Debug, Into<usize>, TryFrom<usize>);
const _: () = {
    assert!(std::mem::size_of::<SlotWord>() == std::mem::size_of::<usize>());
    assert!(std::mem::align_of::<SlotWord>() == std::mem::align_of::<usize>());
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarrierOrder {
    BeforeSlot,
    AfterSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConstructorTiming {
    DuringMutation,
    BeforeThreads,
}

mod sealed {
    pub trait Sealed {}
}

trait Protocol: sealed::Sealed + Send + Sync + fmt::Debug + 'static {
    const PUBLICATION_FENCE: Option<Ordering>;
    const BARRIER: BarrierOrder;
}

macro_rules! protocol {
    ($name:ident, $fence:expr, $barrier:expr) => {
        #[derive(Debug)]
        struct $name;
        impl sealed::Sealed for $name {}
        impl Protocol for $name {
            const PUBLICATION_FENCE: Option<Ordering> = $fence;
            const BARRIER: BarrierOrder = $barrier;
        }
        assert_impl_all!($name: Send, Sync, fmt::Debug);
    };
}

protocol!(
    RequiredOrdering,
    Some(Ordering::Release),
    BarrierOrder::BeforeSlot
);
protocol!(OmitPublicationFence, None, BarrierOrder::BeforeSlot);
protocol!(
    MoveBarrierAfterStore,
    Some(Ordering::Release),
    BarrierOrder::AfterSlot
);

const UNINITIALIZED: usize = 0;
const CONSTRUCTED_PAYLOAD: usize = 37;

#[derive(Debug)]
struct SharedSlot {
    slot: AtomicUsize,
    constructed_payload: AtomicUsize,
    // One pending preimage models queue membership, not a queue algorithm.
    satb_preimage: AtomicUsize,
}

assert_impl_all!(SharedSlot: Send, Sync, fmt::Debug);
assert_not_impl_any!(SharedSlot: Copy, Clone);

/// Created on its own mutator thread; it never crosses threads afterward.
#[derive(Debug)]
struct SlotWriter<M: Protocol> {
    shared: Arc<SharedSlot>,
    mode: PhantomData<M>,
    confined: PhantomData<Rc<()>>,
}

/// The sole publication entry borrows the writer exclusively and consumes
/// this capability. Neither representation nor construction is public.
#[derive(Debug)]
struct SlotWrite<'writer, M: Protocol> {
    writer: &'writer mut SlotWriter<M>,
}

/// Constructor acknowledgment, confined with the writer until publication.
#[derive(Debug)]
struct PreparedWord {
    word: SlotWord,
    confined: PhantomData<Rc<()>>,
}

#[derive(Debug)]
struct MarkerReader {
    shared: Arc<SharedSlot>,
}

assert_not_impl_any!(SlotWriter<RequiredOrdering>: Send, Sync, Copy, Clone);
assert_impl_all!(SlotWriter<RequiredOrdering>: fmt::Debug);
assert_not_impl_any!(SlotWrite<'static, RequiredOrdering>: Send, Sync, Copy, Clone);
assert_impl_all!(SlotWrite<'static, RequiredOrdering>: fmt::Debug);
assert_not_impl_any!(PreparedWord: Send, Sync, Copy, Clone);
assert_impl_all!(PreparedWord: fmt::Debug);
assert_impl_all!(MarkerReader: Send, Sync, fmt::Debug);
assert_not_impl_any!(MarkerReader: Copy, Clone);

impl<M: Protocol> SlotWriter<M> {
    fn prepare(&mut self, word: SlotWord, timing: ConstructorTiming) -> PreparedWord {
        if word == SlotWord::ConstructedChild && timing == ConstructorTiming::DuringMutation {
            // Model ordinary constructor writes with an independent Relaxed
            // word. Marker creation must not publish this initialization.
            self.shared
                .constructed_payload
                .store(CONSTRUCTED_PAYLOAD, Ordering::Relaxed);
        }
        PreparedWord {
            word,
            confined: PhantomData,
        }
    }

    fn begin(&mut self) -> SlotWrite<'_, M> {
        SlotWrite { writer: self }
    }
}

impl<M: Protocol> SlotWrite<'_, M> {
    fn publish(self, prepared: PreparedWord) {
        let shared = &self.writer.shared;
        let old = SlotWord::try_from(shared.slot.load(Ordering::Relaxed))
            .expect("private slot contains a declared model word");
        if M::BARRIER == BarrierOrder::BeforeSlot {
            shared
                .satb_preimage
                .store(usize::from(old), Ordering::Relaxed);
        }
        if let Some(ordering) = M::PUBLICATION_FENCE {
            fence(ordering);
        }
        // The one slot store: a Release fence before this Relaxed store
        // synchronizes with the marker's Acquire load that observes it.
        shared
            .slot
            .store(usize::from(prepared.word), Ordering::Relaxed);
        // A scheduling point adds no happens-before edge. In particular the
        // marker may run between a replacement and an incorrectly late barrier.
        thread::yield_now();
        if M::BARRIER == BarrierOrder::AfterSlot {
            shared
                .satb_preimage
                .store(usize::from(old), Ordering::Relaxed);
        }
    }
}

impl MarkerReader {
    fn trace_one_attempt(&self, prejoin_stats: Option<&ExplorationStats>) {
        let word = SlotWord::try_from(self.shared.slot.load(Ordering::Acquire))
            .expect("private slot contains a declared model word");
        if let Some(stats) = prejoin_stats {
            let counter = if word == SlotWord::OldChild {
                &stats.prejoin_old
            } else {
                &stats.prejoin_replaced
            };
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if word == SlotWord::ConstructedChild {
            // This independent field has no queue or join synchronization.
            assert_eq!(
                self.shared.constructed_payload.load(Ordering::Relaxed),
                CONSTRUCTED_PAYLOAD,
                "marker observed a slot before its child constructor was published"
            );
        }
        // The marker must retain the preimage either through the old slot
        // observation or the already-enqueued SATB root when it sees new/nil.
        let old_in_satb =
            self.shared.satb_preimage.load(Ordering::Relaxed) == usize::from(SlotWord::OldChild);
        assert!(
            word == SlotWord::OldChild || old_in_satb,
            "slot replacement preceded its SATB preimage"
        );
    }
}

// Host-only counters do not use Loom primitives, introduce a scheduling
// point, or publish any modeled field. They attest that the reader exercised
// an actual replacement before join rather than only checking joined state.
#[derive(Debug, Default)]
struct ExplorationStats {
    iterations: std::sync::atomic::AtomicUsize,
    prejoin_old: std::sync::atomic::AtomicUsize,
    prejoin_replaced: std::sync::atomic::AtomicUsize,
}

struct ExplorationReport<'stats, M: Protocol> {
    stats: &'stats ExplorationStats,
    mode: PhantomData<M>,
}

impl<M: Protocol> Drop for ExplorationReport<'_, M> {
    fn drop(&mut self) {
        let read = |counter: &std::sync::atomic::AtomicUsize| {
            counter.load(std::sync::atomic::Ordering::Relaxed)
        };
        eprintln!(
            "loom-exploration protocol={} iterations={} prejoin_old={} prejoin_replaced={}",
            std::any::type_name::<M>(),
            read(&self.stats.iterations),
            read(&self.stats.prejoin_old),
            read(&self.stats.prejoin_replaced),
        );
    }
}

fn model<M: Protocol>(target: SlotWord, timing: ConstructorTiming) {
    let stats = std::sync::Arc::new(ExplorationStats::default());
    let _report = ExplorationReport::<M> {
        stats: &stats,
        mode: PhantomData,
    };
    let captured_stats = stats.clone();
    let mut builder = loom::model::Builder::new();
    builder.max_threads = 2;
    builder.max_branches = 128;
    builder.max_permutations = None;
    builder.max_duration = None;
    builder.preemption_bound = None;
    builder.checkpoint_file = None;
    builder.expect_explicit_explore = false;
    builder.location = true;
    builder.log = false;
    builder.check(move || {
        captured_stats
            .iterations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let initial_payload = match timing {
            ConstructorTiming::BeforeThreads => CONSTRUCTED_PAYLOAD,
            ConstructorTiming::DuringMutation => UNINITIALIZED,
        };
        let shared = Arc::new(SharedSlot {
            slot: AtomicUsize::new(usize::from(SlotWord::OldChild)),
            constructed_payload: AtomicUsize::new(initial_payload),
            satb_preimage: AtomicUsize::new(usize::from(SlotWord::Immediate)),
        });
        let reader = MarkerReader {
            shared: shared.clone(),
        };
        let marker_stats = captured_stats.clone();
        let worker = thread::spawn(move || {
            reader.trace_one_attempt(Some(&marker_stats));
        });
        // The confined writer stays on the model's original mutator thread.
        // Construction occurs AFTER marker spawn; no readiness flag or join
        // publishes the constructor before the marker's racing attempt.
        let mut writer = SlotWriter::<M> {
            shared: shared.clone(),
            mode: PhantomData,
            confined: PhantomData,
        };
        let prepared = writer.prepare(target, timing);
        writer.begin().publish(prepared);
        // Propagate the marker's original panic so controls must match their
        // property assertion rather than a generic join-error expectation.
        if let Err(failure) = worker.join() {
            std::panic::resume_unwind(failure);
        }
        let reader = MarkerReader {
            shared: shared.clone(),
        };
        assert_eq!(shared.slot.load(Ordering::Acquire), usize::from(target));
        assert_eq!(
            shared.satb_preimage.load(Ordering::Relaxed),
            usize::from(SlotWord::OldChild),
        );
        reader.trace_one_attempt(None);
    });
    assert!(
        stats
            .prejoin_replaced
            .load(std::sync::atomic::Ordering::Relaxed)
            > 0,
        "model never exercised a replacement before join",
    );
}

#[test]
fn release_fence_relaxed_slot_publishes_constructor_before_marker_load() {
    model::<RequiredOrdering>(
        SlotWord::ConstructedChild,
        ConstructorTiming::DuringMutation,
    );
}

#[test]
fn satb_preimage_precedes_replacement_with_an_immediate() {
    model::<RequiredOrdering>(SlotWord::Immediate, ConstructorTiming::DuringMutation);
}

#[test]
fn preinitialized_child_remains_visible_with_the_required_slot_protocol() {
    // Initialization predates both threads. Keep the required fence: it
    // also publishes pending SATB membership in this queue abstraction.
    model::<RequiredOrdering>(SlotWord::ConstructedChild, ConstructorTiming::BeforeThreads);
}

#[test]
#[should_panic(expected = "marker observed a slot before its child constructor was published")]
fn negative_control_missing_release_fence_exposes_uninitialized_child() {
    model::<OmitPublicationFence>(
        SlotWord::ConstructedChild,
        ConstructorTiming::DuringMutation,
    );
}

#[test]
#[should_panic(expected = "slot replacement preceded its SATB preimage")]
fn negative_control_barrier_after_store_loses_preimage() {
    model::<MoveBarrierAfterStore>(SlotWord::Immediate, ConstructorTiming::DuringMutation);
}
