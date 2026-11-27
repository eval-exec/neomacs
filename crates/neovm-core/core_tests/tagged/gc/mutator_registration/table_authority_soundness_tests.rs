//! A8/A9 rows for the sealed hash-table authority word.
//!
//! A nested keeper lease on one table must refuse `Busy`. An edit that
//! replaces the `LispHashTable` payload must keep the authority word at
//! WRITER and keep the user-test depth; another owner's reader enter during
//! that edit must not succeed.

use super::era_fixtures::EraFixtureExt;
use super::{
    KeeperTableMut, Mutator, MutatorPermit, RegistrationError, RendezvousError, SealedTableAccess,
    TableMut, TableRef,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{HashTableTest, LispHashTable};
use crate::tagged::gc::HeapIdentity;
use crate::tagged::value::TaggedValue;
use neovm_eval::test_support::authority::baton_collect_parallel_world_roots;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const SPIN_WAIT: Duration = Duration::from_secs(30);
const READER_WINDOW: Duration = Duration::from_millis(50);
const WRITER: u64 = 1 << 63;

fn activate<'era>(mut permit: MutatorPermit<'era>, heap: HeapIdentity, id: u64) -> Mutator<'era> {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match permit.activate(heap, id) {
            Ok(owner) => return owner,
            Err(failure)
                if failure.reason == RegistrationError::Busy && Instant::now() < deadline =>
            {
                permit = failure.into_permit();
                thread::yield_now();
            }
            Err(failure) => panic!("activate: {:?}", failure.reason),
        }
    }
}

fn retire(mut owner: Mutator<'_>) {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match owner.retire_rooted(&[]) {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && Instant::now() < deadline =>
            {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire: {:?}", failure.reason),
        }
    }
}

fn finish_era(era: neovm_eval::test_support::authority::WorldProofEra<'_>) {
    let mut stopped = era.stop().expect("stop");
    era.collect_stopped(&mut stopped, baton_collect_parallel_world_roots)
        .expect("collect");
    stopped.complete().expect("complete");
    let (collector, keeper) = era.finish().expect("finish");
    drop(collector);
    drop(keeper);
}

/// Wakes a reader that is parked on `go`. `cancel` tells it to leave without
/// entering, so a panic in the writer still lets `thread::scope` join.
struct ReleaseReader {
    go: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

impl Drop for ReleaseReader {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.go.store(true, Ordering::Release);
    }
}

#[test]
fn nested_keeper_enter_on_one_table_returns_busy() {
    Context::with_test_collection_era(
        |context| {
            // SAFETY: serial test setup before the World is frozen; the
            // non-swappable view is used in place for this one allocation.
            let table = unsafe { context.tagged_heap_mut() }
                .alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
            context.set_variable("p2ht-a8-root", table);
            table.bits()
        },
        |era, table_bits| {
            let mut stopped = era.stop().expect("stop");
            // SAFETY: the rooted table survives the frozen World; the bits
            // came from the rooted setup.
            let table = unsafe { TaggedValue::from_bits(table_bits) };
            let first = KeeperTableMut::enter(&stopped, table).expect("first keeper lease");
            let second = KeeperTableMut::enter(&stopped, table);
            let refused = matches!(second, Err(RegistrationError::Busy));
            drop(second);
            drop(first);
            era.collect_stopped(&mut stopped, baton_collect_parallel_world_roots)
                .expect("collect");
            stopped.complete().expect("complete");
            let (collector, keeper) = era.finish().expect("finish");
            drop(collector);
            drop(keeper);
            assert!(
                refused,
                "a second keeper lease on the same table must refuse Busy"
            );
        },
    );
}

#[test]
fn replacing_hash_table_inside_edit_keeps_writer_and_depth() {
    Context::with_test_collection_era(
        |context| {
            // SAFETY: serial test setup before the World is frozen.
            let table = unsafe { context.tagged_heap_mut() }
                .alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
            context.set_variable("p2ht-a9-root", table);
            table.bits()
        },
        |era, table_bits| {
            let heap = era.heap_identity();
            let writer_permit = era.register(1, 4).expect("writer permit");
            let reader_permit = era.register(2, 4).expect("reader permit");
            let ready = Arc::new(AtomicBool::new(false));
            let go = Arc::new(AtomicBool::new(false));
            let cancel = Arc::new(AtomicBool::new(false));
            let entered = Arc::new(AtomicBool::new(false));
            let mut word_during = 0u64;
            let mut depth_during = 0u8;
            let mut entered_during = false;
            thread::scope(|scope| {
                let ready_reader = Arc::clone(&ready);
                let go_reader = Arc::clone(&go);
                let cancel_reader = Arc::clone(&cancel);
                let entered_reader = Arc::clone(&entered);
                scope.spawn(move || {
                    let mut owner = activate(reader_permit, heap, 2);
                    let mut run = owner.run_access().expect("reader run");
                    ready_reader.store(true, Ordering::Release);
                    while !go_reader.load(Ordering::Acquire) {
                        thread::yield_now();
                    }
                    if cancel_reader.load(Ordering::Acquire) {
                        drop(run);
                        retire(owner);
                        return;
                    }
                    // SAFETY: same rooted table bits as the writer.
                    let table = unsafe { TaggedValue::from_bits(table_bits) };
                    let lease = TableRef::enter(&mut run, table).expect("reader enter");
                    entered_reader.store(true, Ordering::Release);
                    drop(lease);
                    drop(run);
                    retire(owner);
                });
                while !ready.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                let mut owner = activate(writer_permit, heap, 1);
                let mut run = owner.run_access().expect("writer run");
                // SAFETY: same rooted table bits.
                let table = unsafe { TaggedValue::from_bits(table_bits) };
                let mut lease = TableMut::enter(&mut run, table).expect("writer lease");
                let word = std::ptr::from_ref(lease.authority_word());
                // Depth lives on the object, beside the payload `edit` replaces.
                let depth = unsafe {
                    let obj = (table.bits() & !neovm_value::tagged::value::TAG_MASK)
                        as *const neovm_value::tagged::header::HashTableObj;
                    std::ptr::from_ref(&(*obj).test_depth)
                };
                // SAFETY: the lease holds the table alive; the byte is the
                // user-test depth this row observes across the payload replace.
                unsafe { (*depth).store(2, Ordering::Release) };
                let release = ReleaseReader {
                    go: Arc::clone(&go),
                    cancel: Arc::clone(&cancel),
                };
                lease.edit(|ht| {
                    *ht = LispHashTable::new(HashTableTest::Eql);
                    // The reader is already at `TableRef::enter`. Publish the
                    // window, then sample whether it got in while WRITER
                    // should still be held.
                    cancel.store(false, Ordering::Release);
                    go.store(true, Ordering::Release);
                    let deadline = Instant::now() + READER_WINDOW;
                    while Instant::now() < deadline && !entered.load(Ordering::Acquire) {
                        thread::yield_now();
                    }
                    // SAFETY: both pointers address table metadata captured
                    // before the payload replace; the object stays rooted.
                    unsafe {
                        word_during = (*word).load(Ordering::Acquire);
                        depth_during = (*depth).load(Ordering::Acquire);
                    }
                    entered_during = entered.load(Ordering::Acquire);
                });
                drop(lease);
                drop(release);
                drop(run);
                retire(owner);
            });
            finish_era(era);
            assert_eq!(
                word_during, WRITER,
                "replacing the hash-table payload cleared the writer bit"
            );
            assert_eq!(
                depth_during, 2,
                "replacing the hash-table payload cleared the user-test depth"
            );
            assert!(
                !entered_during,
                "another owner entered while the writer lease was inside the edit"
            );
        },
    );
}

/// #876: when a tenured heap-allocated `Box<HashTableObj>` happens to sit
/// inside a non-contiguous `[dump_addr_lo, dump_addr_hi)` span created by
/// mapped image registrations, `TableMut::edit` treats it as boxed (`!mapped`)
/// and sets `header.remembered = Logged`. Stopped collection clearers
/// (`trace_collection_work_world_stopped` and `reset_generational_remembered_world_stopped`) must use
/// the same boxed authority (`!owner_is_mapped || flags.boxed()`), otherwise
/// the header stays stuck `Logged` after minor 1, the second `TableMut::edit`
/// fails to re-log the table, and minor 2 collects the live young child.
#[test]
fn table_mut_bracketed_boxed_table_clears_remembered_across_stopped_minors() {
    use neovm_eval::test_support::authority::admit_owned_proof_parallel_for_test;
    use neovm_value::tagged::gc::TaggedHeap;
    use neovm_value::tagged::header::{GcHeader, HashTableObj, VecLikeHeader};
    use neovm_value::test_support::authority::set_generational_enabled_for_test;
    use neovm_value::value::HashKey;

    // Declare `bracket_heap` BEFORE `context` so its allocated `HashTableObj`
    // shells strictly outlive `context`'s `TaggedHeap`. Allocating the mapped
    // bracket shells via `bracket_heap.alloc_hash_table(...)` routes through
    // the exact same `allocation.rs` `Box::new(HashTableObj::new(..))` callsite
    // as `context.tagged_heap_mut().alloc_hash_table(...)`, guaranteeing both
    // share the same size-class/callsite bucket even on callsite-segregated
    // allocators (such as macOS xzone libmalloc) and keeping the resulting
    // `[dump_addr_lo, dump_addr_hi)` bracket tightly around `table`.
    let mut bracket_heap = TaggedHeap::new();
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    unsafe { set_generational_enabled_for_test(context.tagged_heap_mut(), true) };

    let mut candidates = Vec::new();
    let mut heap_tables = Vec::new();
    let mut bracket: Option<(usize, usize, TaggedValue)> = None;
    for _round in 0..8 {
        for _ in 0..32 {
            candidates.push(bracket_heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq)));
            let ht = unsafe { context.tagged_heap_mut() }
                .alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
            heap_tables.push(ht);
        }
        candidates.sort_by_key(|ht| ht.as_veclike_ptr().expect("bracket ptr") as usize);
        let mut best: Option<(usize, usize, usize, TaggedValue)> = None;
        for &ht in &heap_tables {
            let addr = ht.as_veclike_ptr().expect("table ptr") as usize;
            let pos = candidates
                .partition_point(|b| (b.as_veclike_ptr().expect("bracket ptr") as usize) < addr);
            if pos > 0 && pos < candidates.len() {
                let lo_i = pos - 1;
                let hi_i = pos;
                let lo_a = candidates[lo_i].as_veclike_ptr().expect("lo ptr") as usize;
                let hi_a = candidates[hi_i].as_veclike_ptr().expect("hi ptr") as usize;
                let span = hi_a - lo_a;
                if best.is_none_or(|(best_span, _, _, _)| span < best_span) {
                    best = Some((span, lo_i, hi_i, ht));
                }
            }
        }
        if let Some((_, lo_i, hi_i, ht)) = best {
            bracket = Some((lo_i, hi_i, ht));
            break;
        }
    }
    let (lo_idx, hi_idx, table) =
        bracket.expect("layout could not produce a bracket around any heap table");
    let lo_ptr = candidates[lo_idx]
        .as_veclike_ptr()
        .expect("lo veclike")
        .cast_mut()
        .cast::<VecLikeHeader>();
    let hi_ptr = candidates[hi_idx]
        .as_veclike_ptr()
        .expect("hi veclike")
        .cast_mut()
        .cast::<VecLikeHeader>();

    // Root `table` and promote it to ordinary-old (tenured, non-permanent,
    // boxed) via a stopped minor BEFORE registering the mapped bracket so
    // `table` is genuinely tenured before its address enters `[dump_addr_lo,
    // dump_addr_hi)`.
    context.set_variable("b876-bracketed-table", table);
    admit_owned_proof_parallel_for_test(&mut context, |projection| {
        let era = projection.era();
        let mut stopped = era.stop().expect("stop for tenure minor");
        let report = era
            .collect_stopped_minor(&mut stopped, baton_collect_parallel_world_roots)
            .expect("tenure minor");
        assert!(!report.major, "setup tenure must be a minor cycle");
        stopped.complete().expect("complete tenure minor");
        let (collector, keeper) = projection.finish().expect("finish tenure era");
        (collector, keeper, ())
    })
    .expect("tenure era succeeds");

    // SAFETY: `lo_ptr` and `hi_ptr` belong to `bracket_heap` (declared before
    // `context`, never collected, dropped strictly after `context`), so both
    // stay live, aligned, and writable for `context`'s entire heap lifetime.
    unsafe {
        let heap = context.tagged_heap_mut();
        heap.register_mapped_veclike_object(lo_ptr, std::mem::size_of::<HashTableObj>());
        heap.register_mapped_veclike_object(hi_ptr, std::mem::size_of::<HashTableObj>());
    }

    let table_header = table
        .as_veclike_ptr()
        .expect("table veclike")
        .cast::<GcHeader>();
    unsafe {
        assert!((*table_header).flags.boxed(), "table must be heap-boxed");
        assert!((*table_header).tenured, "table must be tenured old");
        assert!(
            !(*table_header).generation.permanent(),
            "table must be ordinary-old, not permanent"
        );
        assert!(
            !(*table_header).is_remembered(),
            "table must start Unlogged before era 1"
        );
    }
    assert!(
        context.tagged_heap().mapped_image_owns_for_test(table),
        "table address must sit inside [dump_addr_lo, dump_addr_hi)"
    );

    // Helper: allocate a fresh young vector outside `[dump_addr_lo, dump_addr_hi)`.
    // `mark_value` (`incremental.rs:825`) classifies addresses inside the dump
    // span as mapped before checking arena ownership, so the young child itself
    // must lie in normal young arena memory outside the synthetic bracket.
    let alloc_young_outside_bracket = |ctx: &mut Context, tag: i64| -> TaggedValue {
        for _ in 0..64 {
            let v = unsafe { ctx.tagged_heap_mut() }.alloc_vector(vec![TaggedValue::fixnum(tag)]);
            if !ctx.tagged_heap().mapped_image_owns_for_test(v) {
                return v;
            }
        }
        panic!("could not allocate young vector outside synthetic mapped bracket");
    };

    // Era 1: write a young vector via parallel `TableMut::edit` (sets `Logged`),
    // then run a stopped minor collection.
    let young_first = alloc_young_outside_bracket(&mut context, 111);
    let table_bits = table.bits();
    let mut logged_before_minor_1 = false;
    admit_owned_proof_parallel_for_test(&mut context, |projection| {
        let era = projection.era();
        let mut owner = activate(
            era.register(1, 8).expect("register"),
            era.heap_identity(),
            1,
        );
        {
            let mut run = owner.run_access().expect("run access");
            let mut lease =
                TableMut::enter(&mut run, unsafe { TaggedValue::from_bits(table_bits) })
                    .expect("enter table");
            lease.edit(|ht| {
                ht.insert(HashKey::Int(1), TaggedValue::fixnum(1), young_first);
            });
            drop(lease);
        }
        logged_before_minor_1 = unsafe { (*table_header).is_remembered() };
        owner.enter_native(&[]).expect("enter native");
        let mut stopped = era.stop().expect("stop for minor 1");
        let report = era
            .collect_stopped_minor(&mut stopped, baton_collect_parallel_world_roots)
            .expect("stopped minor 1");
        assert!(!report.major, "minor 1 must be a minor cycle");
        stopped.complete().expect("complete minor 1");
        owner.leave_native().expect("leave native");
        retire(owner);
        let (collector, keeper) = projection.finish().expect("finish era 1");
        (collector, keeper, ())
    })
    .expect("era 1 succeeds");
    assert!(
        logged_before_minor_1,
        "TableMut::edit must set header.remembered = Logged on the tenured boxed table"
    );
    let remembered_after_minor_1 = unsafe { (*table_header).is_remembered() };

    // Era 2: allocate a brand-new YOUNG vector reachable ONLY through `table`,
    // insert it via parallel `TableMut::edit`, and run stopped minor 2.
    let young_second = alloc_young_outside_bracket(&mut context, 222);
    assert!(
        context.tagged_heap().owns_heap_value_for_test(young_second),
        "young_second must be owned before minor 2"
    );
    admit_owned_proof_parallel_for_test(&mut context, |projection| {
        let era = projection.era();
        let mut owner = activate(
            era.register(2, 8).expect("register"),
            era.heap_identity(),
            2,
        );
        {
            let mut run = owner.run_access().expect("run access");
            let mut lease =
                TableMut::enter(&mut run, unsafe { TaggedValue::from_bits(table_bits) })
                    .expect("enter table");
            lease.edit(|ht| {
                ht.insert(HashKey::Int(2), TaggedValue::fixnum(2), young_second);
            });
            drop(lease);
        }
        owner.enter_native(&[]).expect("enter native");
        let mut stopped = era.stop().expect("stop for minor 2");
        let report = era
            .collect_stopped_minor(&mut stopped, baton_collect_parallel_world_roots)
            .expect("stopped minor 2");
        assert!(!report.major, "minor 2 must be a minor cycle");
        stopped.complete().expect("complete minor 2");
        owner.leave_native().expect("leave native");
        retire(owner);
        let (collector, keeper) = projection.finish().expect("finish era 2");
        (collector, keeper, ())
    })
    .expect("era 2 succeeds");

    let young_second_survived = context.tagged_heap().owns_heap_value_for_test(young_second);
    assert!(
        !remembered_after_minor_1 && young_second_survived,
        "#876 regression: bracketed boxed table stayed Logged after minor 1 \
         (remembered_after_minor_1={remembered_after_minor_1}) and young_second \
         survived minor 2={young_second_survived}"
    );
    assert_eq!(
        context
            .eval_str("(list (aref (gethash 1 b876-bracketed-table) 0) (aref (gethash 2 b876-bracketed-table) 0))")
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("read surviving entries"),
        "(111 222)"
    );
    drop(context);
    drop(bracket_heap);
}
