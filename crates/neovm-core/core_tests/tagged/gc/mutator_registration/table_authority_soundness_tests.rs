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
