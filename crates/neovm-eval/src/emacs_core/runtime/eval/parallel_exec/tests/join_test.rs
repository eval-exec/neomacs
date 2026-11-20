//! A sole cons survives a stop only when the owner scan is acknowledged.
//!
//! The five sources of one arena each hold one cons. Setup publishes that
//! scan while this thread is still the mutator. The owner thread then
//! acknowledges the published batch and drops it, so the collection's only
//! root for those conses is the batch `collect_stopped` already traces.

use super::super::owner_bindings::{BindingKind, Channel, SlotValue, UndoTarget};
use super::frames::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::{ParallelHeapEraWorldCollectExt, WorldProofEra};
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::tagged::gc::{
    AllStopped, HeapIdentity, MeasuredInterval, Mutator, MutatorKey, MutatorPermit,
    OwnerEraReserve, RegistrationError, RendezvousError, Request,
};
use neovm_value::tagged::header::ConsCell;
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::test_support::authority::begin_restore_retry_for_test;
use neovm_value::value::Value;
use std::mem::size_of;
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const OWNER: u64 = 7401;
const ADMISSION_DEADLINE: Duration = Duration::from_secs(30);

struct SoleRoots {
    scratch: Vec<SharedRoot>,
    keepers: Vec<SharedRoot>,
    frame: usize,
    operand: usize,
    handler: usize,
    binding: usize,
    registry: usize,
}

struct RefreshScan {
    first: Vec<SharedRoot>,
    second: Vec<SharedRoot>,
    keepers: Vec<SharedRoot>,
    old_bits: usize,
    new_bits: usize,
}

fn budget() -> PollBudget {
    PollBudget::new(NonZeroU32::new(8).expect("nonzero poll interval"))
}

fn cons(context: &mut Context, tag: i64) -> (Value, SharedRoot) {
    let value = context
        .tagged_heap
        .alloc_cons(Value::fixnum(tag), Value::NIL);
    // SAFETY: setup is the serial mutator of this context's heap. `value`
    // was allocated here, and the returned root keeps it reachable.
    let root = unsafe { SharedRoot::new(&context.tagged_heap, value) };
    (value, root)
}

fn publish_five(context: &mut Context) -> SoleRoots {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    let (frame, frame_root) = cons(context, 101);
    let (operand, operand_root) = cons(context, 102);
    let (handler, handler_root) = cons(context, 103);
    let (binding, binding_root) = cons(context, 104);
    let (registry, registry_root) = cons(context, 105);
    let mut arena = FrameArena::new(4);
    let id = arena
        .push_interp(InterpFrame::new(
            frame,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            1,
            HandlerMark::EMPTY,
            OwnerRecordMark::EMPTY,
            budget(),
        ))
        .expect("interpreter frame");
    arena.push_operand(operand);
    arena
        .push_vm(VmFrame::new(
            Value::fixnum(1),
            Value::fixnum(2),
            Pc::new(0),
            OperandCursor::new(0, 1),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            budget(),
        ))
        .expect("vm frame");
    arena.handlers_mut().push(HandlerRecord::new(
        HandlerKind::Catch,
        handler,
        Value::NIL,
        id,
        OwnerRecordMark::EMPTY,
    ));
    arena.bindings_mut().push(SlotRecord::new(
        NIL_SYM_ID,
        TlsIndex::VOID,
        Channel::Default,
        BindingKind::Let,
        UndoTarget::Previous(SlotValue::new(binding)),
        SlotGeneration::new(NonZeroU32::MIN),
    ));
    arena
        .roots_mut()
        .push(RootGroup::Misc, registry)
        .expect("registry root");
    let mut scratch = Vec::new();
    arena
        .publish_scan(&context.tagged_heap, &mut scratch)
        .expect("owner scan");
    SoleRoots {
        scratch,
        keepers: vec![
            frame_root,
            operand_root,
            handler_root,
            binding_root,
            registry_root,
        ],
        frame: frame.bits(),
        operand: operand.bits(),
        handler: handler.bits(),
        binding: binding.bits(),
        registry: registry.bits(),
    }
}

fn publish_refresh(context: &mut Context) -> RefreshScan {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    let (old, old_root) = cons(context, 201);
    let (replacement, new_root) = cons(context, 202);
    let mut arena = FrameArena::new(1);
    let handle = arena
        .roots_mut()
        .push(RootGroup::Misc, old)
        .expect("old root");
    let mut first = Vec::new();
    arena
        .publish_scan(&context.tagged_heap, &mut first)
        .expect("pre-escape scan");
    arena.roots_mut().pop(handle).expect("revoke the old root");
    arena
        .roots_mut()
        .push(RootGroup::Misc, replacement)
        .expect("replacement root");
    let mut second = Vec::new();
    arena
        .publish_scan(&context.tagged_heap, &mut second)
        .expect("post-escape scan");
    RefreshScan {
        first,
        second,
        keepers: vec![old_root, new_root],
        old_bits: old.bits(),
        new_bits: replacement.bits(),
    }
}

fn activate(mut permit: MutatorPermit<'_>, heap: HeapIdentity) -> Mutator<'_> {
    let start = Instant::now();
    loop {
        match permit.activate(heap, OWNER) {
            Ok(mutator) => return mutator,
            Err(failure)
                if failure.reason == RegistrationError::Busy
                    && start.elapsed() < ADMISSION_DEADLINE =>
            {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activate: {:?}", failure.reason),
        }
    }
}

fn request_epoch(rendezvous: neovm_value::tagged::gc::Rendezvous<'_>) -> std::num::NonZeroU64 {
    let start = Instant::now();
    loop {
        match rendezvous.request() {
            Ok(Request::Active(epoch)) => return epoch,
            Ok(Request::Queued) if start.elapsed() < ADMISSION_DEADLINE => thread::yield_now(),
            Ok(Request::Queued) => panic!("request: Queued"),
            Err(RendezvousError::Busy) if start.elapsed() < ADMISSION_DEADLINE => {
                thread::yield_now();
            }
            Err(error) => panic!("request: {error:?}"),
        }
    }
}

fn acknowledge(owner: &mut Mutator<'_>, epoch: std::num::NonZeroU64, roots: &[SharedRoot]) {
    let start = Instant::now();
    loop {
        match owner.acknowledge(epoch, roots) {
            Ok(()) => return,
            Err(RendezvousError::Busy) if start.elapsed() < ADMISSION_DEADLINE => {
                thread::yield_now();
            }
            Err(error) => panic!("acknowledge: {error:?}"),
        }
    }
}

fn restore(owner: &mut Mutator<'_>) {
    let mut image = begin_restore_retry_for_test(owner, false).expect("begin restore");
    let start = Instant::now();
    loop {
        match image.apply() {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy
                    && start.elapsed() < ADMISSION_DEADLINE =>
            {
                image = failure.into_image();
                thread::yield_now();
            }
            Err(failure) => panic!("restore: {:?}", failure.reason),
        }
    }
}

fn retire(mut owner: Mutator<'_>) {
    let start = Instant::now();
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy
                    && start.elapsed() < ADMISSION_DEADLINE =>
            {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire: {:?}", failure.reason),
        }
    }
}

fn stop<'era>(era: &'era WorldProofEra<'_>) -> AllStopped<'era> {
    retry_busy("stop", || era.stop())
}

/// The registry gate is a try-lock. The owner resumes concurrently, so one
/// attempt can observe `Busy` and the next attempt proceeds.
fn retry_busy<T>(label: &str, mut op: impl FnMut() -> Result<T, RendezvousError>) -> T {
    let start = Instant::now();
    loop {
        match op() {
            Ok(value) => return value,
            Err(RendezvousError::Busy) if start.elapsed() < ADMISSION_DEADLINE => {
                thread::yield_now();
            }
            Err(error) => panic!("{label}: {error:?}"),
        }
    }
}

fn reader_words(stopped: &AllStopped<'_>, key: MutatorKey) -> Vec<usize> {
    retry_busy("stopped root reader", || {
        let mut words = Vec::new();
        stopped.with_root_reader(key, |reader| {
            reader.append_words(&mut words);
            reader.finish().expect("finish the root reader");
        })?;
        Ok(words)
    })
}

fn owns(era: &WorldProofEra<'_>, stopped: &AllStopped<'_>, bits: usize) -> bool {
    retry_busy("stopped heap", || {
        era.with_stopped_heap_for_test(stopped, |heap| {
            heap.owns_heap_value_for_test(unsafe {
                // SAFETY: `bits` is `Value::bits` of a cons allocated in this
                // heap before the era was admitted.
                Value::from_bits(bits)
            })
        })
    })
}

fn finish(era: WorldProofEra<'_>) {
    let (collector, keeper) = era.finish().expect("finish the era");
    drop(collector);
    drop(keeper);
}

struct PublishedBatch {
    key: MutatorKey,
    words: Vec<usize>,
}

fn publish_and_ack(
    owner: &mut Mutator<'_>,
    rendezvous: neovm_value::tagged::gc::Rendezvous<'_>,
    scratch: Vec<SharedRoot>,
) -> PublishedBatch {
    let epoch = request_epoch(rendezvous);
    let words = scratch.iter().map(SharedRoot::parallel_word).collect();
    acknowledge(owner, epoch, &scratch);
    drop(scratch);
    PublishedBatch {
        key: owner.key(),
        words,
    }
}

#[test]
fn sole_root_in_each_owner_source_survives_only_through_the_joined_scan() {
    Context::with_test_collection_era(publish_five, |era, mut roots| {
        drop(roots.keepers);
        let heap = era.heap_identity();
        let rendezvous = era.rendezvous();
        let permit = era.register(OWNER, 0).expect("register the owner");
        let (tx, rx) = mpsc::channel();
        let scratch = std::mem::take(&mut roots.scratch);
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let published = publish_and_ack(&mut owner, rendezvous, scratch);
                tx.send(published).expect("owner acknowledged");
                restore(&mut owner);
                retire(owner);
            });
            let published = rx
                .recv_timeout(Duration::from_secs(30))
                .expect("owner reaches the acknowledgement");
            let mut stopped = stop(&era);
            let reader = reader_words(&stopped, published.key);
            let report = retry_busy("collect", || {
                ParallelHeapEraWorldCollectExt::collect_stopped(&era, &mut stopped)
            });
            assert!(report.major);
            let swept: Vec<&str> = [
                ("frame", roots.frame),
                ("operand", roots.operand),
                ("handler", roots.handler),
                ("binding", roots.binding),
                ("registry", roots.registry),
            ]
            .into_iter()
            .filter(|(_, bits)| !owns(&era, &stopped, *bits))
            .map(|(name, _)| name)
            .collect();
            assert!(
                swept.is_empty(),
                "sole roots swept despite the joined scan: {swept:?}"
            );
            assert_eq!(reader, published.words);
            stopped.complete().expect("complete");
        });
        finish(era);
    });
}

#[test]
fn refreshed_scan_is_acknowledged_after_the_escape_stand_in() {
    Context::with_test_collection_era_then(
        publish_refresh,
        |era, mut refresh| {
            drop(refresh.keepers);
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let permit = era.register(OWNER, 0).expect("register the owner");
            let (tx, rx) = mpsc::channel();
            let first = std::mem::take(&mut refresh.first);
            let second = std::mem::take(&mut refresh.second);
            let old_bits = refresh.old_bits;
            let new_bits = refresh.new_bits;
            thread::scope(|scope| {
                scope.spawn(move || {
                    let mut owner = activate(permit, heap);
                    let published = publish_and_ack(&mut owner, rendezvous, first);
                    tx.send(published).expect("first acknowledgement");
                    restore(&mut owner);
                    // The replacement was published before admission. This stop
                    // acknowledges that fresh scan and does not reuse the slice
                    // from before the stand-in escape.
                    let published = publish_and_ack(&mut owner, rendezvous, second);
                    tx.send(published).expect("second acknowledgement");
                    restore(&mut owner);
                    retire(owner);
                });
                let first = rx
                    .recv_timeout(ADMISSION_DEADLINE)
                    .expect("first acknowledgement");
                let mut stopped = stop(&era);
                let reader = reader_words(&stopped, first.key);
                assert_eq!(reader, first.words);
                assert!(reader.contains(&old_bits));
                retry_busy("first collection", || {
                    ParallelHeapEraWorldCollectExt::collect_stopped(&era, &mut stopped)
                });
                assert!(owns(&era, &stopped, old_bits));
                // `second` is still alive on the owner thread, so its root-table
                // lease keeps the replacement reachable across this collection.
                assert!(owns(&era, &stopped, new_bits));
                stopped.complete().expect("complete the first stop");

                let second = rx
                    .recv_timeout(ADMISSION_DEADLINE)
                    .expect("second acknowledgement");
                let mut stopped = stop(&era);
                let reader = reader_words(&stopped, second.key);
                assert_eq!(reader, second.words, "the second stop uses the fresh scan");
                assert!(reader.contains(&new_bits));
                assert!(
                    !reader.contains(&old_bits),
                    "the pre-escape slice was acknowledged again"
                );
                retry_busy("second collection", || {
                    ParallelHeapEraWorldCollectExt::collect_stopped(&era, &mut stopped)
                });
                assert!(owns(&era, &stopped, new_bits));
                // The previous batch stays in `retained_roots`, and the root
                // table traces every live batch, so the replaced cons stays
                // reachable. The current reader above is the fresh scan only.
                assert!(
                    owns(&era, &stopped, old_bits),
                    "the prior batch lease should keep the replaced cons"
                );
                stopped.complete().expect("complete the second stop");
            });
            // Retirement has returned. `EraCore::finish` drains `retained_roots`
            // with `try_finish` (mutator_registration.rs). The serial collection
            // below has no acknowledgement of the prior cons.
            let (collector, keeper) = era.finish().expect("finish the era");
            (collector, keeper, old_bits)
        },
        |context, old_bits| {
            // SAFETY: `old_bits` is `Value::bits` of the cons allocated in
            // this heap before the era was admitted.
            let prior = unsafe { Value::from_bits(old_bits) };
            assert!(
                context.tagged_heap.owns_heap_value_for_test(prior),
                "the prior cons was already gone before the post-drain collection"
            );
            context.gc_collect_exact();
            assert!(
                !context.tagged_heap.owns_heap_value_for_test(prior),
                "the prior cons stayed owned after its batch lease was finished"
            );
        },
    );
}

struct GrantedPair {
    scratch: Vec<SharedRoot>,
    kept: usize,
    control: usize,
    kept_tail: usize,
    control_tail: usize,
}

fn cons_at(addr: usize) -> Value {
    let cell = addr as *const ConsCell;
    // SAFETY: `addr` is a cell the owner's reserve just carved and has not
    // published. The stores finish before the tagged value is rooted.
    unsafe {
        (*cell).set_car(Value::NIL);
        (*cell).set_cdr(Value::NIL);
        Value::from_cons_ptr(cell)
    }
}

fn probe_bits(addr: usize) -> usize {
    let cell = addr as *const ConsCell;
    // SAFETY: the address is the cell after a handed cons in a block the
    // reserve still holds. The probe does not read or write the cell.
    unsafe { Value::from_cons_ptr(cell).bits() }
}

fn publish_two_owner_grants(context: &mut Context) -> GrantedPair {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.tagged_heap.close_alloc_regions();

    let mut reserve_a = OwnerEraReserve::new();
    let mut reserve_b = OwnerEraReserve::new();
    let mut arena_a = FrameArena::new(1);
    let mut arena_b = FrameArena::new(1);
    reserve_a
        .grant_at_stop(&mut context.tagged_heap, true, MeasuredInterval::zero())
        .expect("owner A grant");
    reserve_b
        .grant_at_stop(&mut context.tagged_heap, true, MeasuredInterval::zero())
        .expect("owner B grant");
    arena_a
        .carve_granted_cons(&mut context.tagged_heap, &mut reserve_a, None)
        .expect("carve A");
    arena_b
        .carve_granted_cons(&mut context.tagged_heap, &mut reserve_b, None)
        .expect("carve B");
    let addr_a = arena_a.take_cons_cell().expect("cell A");
    let addr_b = arena_b.take_cons_cell().expect("cell B");
    let kept = cons_at(addr_a);
    let control = cons_at(addr_b);
    let kept_tail = probe_bits(addr_a + size_of::<ConsCell>());
    let control_tail = probe_bits(addr_b + size_of::<ConsCell>());

    // An open book is not a stop publication, so this collection is not entered.
    let mut refused = Vec::new();
    assert_eq!(
        arena_a.publish_scan(&context.tagged_heap, &mut refused),
        Err(RendezvousError::Publication)
    );
    assert!(refused.is_empty(), "an open book must not publish a root");
    assert_eq!(
        arena_b.publish_scan(&context.tagged_heap, &mut refused),
        Err(RendezvousError::Publication)
    );
    assert!(refused.is_empty());

    arena_a.close_granted_reserve(&mut context.tagged_heap, &mut reserve_a);
    arena_b.close_granted_reserve(&mut context.tagged_heap, &mut reserve_b);
    reserve_a
        .return_unused_before_sweep(&mut context.tagged_heap)
        .expect("return A before the sweep");
    reserve_b
        .return_unused_before_sweep(&mut context.tagged_heap)
        .expect("return B before the sweep");
    assert!(!reserve_a.holds_reserve());
    assert!(!reserve_b.holds_reserve());
    assert!(context.tagged_heap.owns_heap_value_for_test(kept));
    assert!(context.tagged_heap.owns_heap_value_for_test(control));
    assert!(
        !context
            .tagged_heap
            .owns_heap_value_for_test(value_bits(kept_tail))
    );
    assert!(
        !context
            .tagged_heap
            .owns_heap_value_for_test(value_bits(control_tail))
    );
    assert_eq!(context.tagged_heap.jit_cons_window_for_test(), (0, 0));
    assert_eq!(context.tagged_heap.jit_float_window_for_test(), (0, 0));

    arena_a
        .roots_mut()
        .push(RootGroup::Misc, kept)
        .expect("joined root");
    let mut scratch = Vec::new();
    arena_a
        .publish_scan(&context.tagged_heap, &mut scratch)
        .expect("closed book publishes");
    assert_eq!(scratch.len(), 1);

    GrantedPair {
        scratch,
        kept: kept.bits(),
        control: control.bits(),
        kept_tail,
        control_tail,
    }
}

fn value_bits(bits: usize) -> Value {
    // SAFETY: `bits` is `Value::bits` of a cons address in this heap. The
    // ownership probe rejects a cell the reserve returned.
    unsafe { Value::from_bits(bits) }
}

#[test]
fn two_owner_regions_keep_only_the_joined_scan_and_refuse_an_open_book() {
    Context::with_test_collection_era(publish_two_owner_grants, |era, mut roots| {
        let heap = era.heap_identity();
        let rendezvous = era.rendezvous();
        let permit = era.register(OWNER, 0).expect("register the owner");
        let (tx, rx) = mpsc::channel();
        let scratch = std::mem::take(&mut roots.scratch);
        thread::scope(|scope| {
            scope.spawn(move || {
                let mut owner = activate(permit, heap);
                let published = publish_and_ack(&mut owner, rendezvous, scratch);
                tx.send(published).expect("owner acknowledged");
                restore(&mut owner);
                retire(owner);
            });
            let published = rx
                .recv_timeout(ADMISSION_DEADLINE)
                .expect("owner reaches the acknowledgement");
            let mut stopped = stop(&era);
            assert!(
                owns(&era, &stopped, roots.kept),
                "the handed cons was reclaimed before the stop"
            );
            assert!(
                owns(&era, &stopped, roots.control),
                "the control was reclaimed before the stop"
            );
            assert!(!owns(&era, &stopped, roots.kept_tail));
            assert!(!owns(&era, &stopped, roots.control_tail));
            let report = retry_busy("collect", || {
                ParallelHeapEraWorldCollectExt::collect_stopped(&era, &mut stopped)
            });
            assert!(report.major);
            assert!(
                owns(&era, &stopped, roots.kept),
                "the joined scan lost owner A's handed cons"
            );
            assert!(
                !owns(&era, &stopped, roots.control),
                "owner B's cons survived without a root"
            );
            assert!(
                !owns(&era, &stopped, roots.kept_tail),
                "the unhanded tail was treated as a live root"
            );
            assert!(!owns(&era, &stopped, roots.control_tail));
            let reader = reader_words(&stopped, published.key);
            assert_eq!(reader, published.words);
            assert!(reader.contains(&roots.kept));
            assert!(!reader.contains(&roots.control));
            stopped.complete().expect("complete");
        });
        finish(era);
    });
}
