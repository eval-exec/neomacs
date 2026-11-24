//! Direct qualification of numeric raw-view provenance in the real value heap.
//!
//! The full-World native rows cover canonical admission, charging and sealed
//! result homes. This bounded Miri row isolates the storage aliasing surface:
//! real stopped heap/catalog ownership, two confined raw owners, actual table
//! publication against the same CatalogState, and retained-frontier refreeze.

use super::MutatorKey;
use crate::tagged::gc::arena_pages::{ObjectArena, ObjectPage, PagedObject};
use crate::tagged::gc::cons_storage::ConsCatalog;
use crate::tagged::gc::{HeapIdentity, TaggedHeap};
use crate::tagged::header::{BignumObj, FloatObj, HashTableObj, VecLikeHeader, VecLikeType};
use crate::tagged::value::TaggedValue;
use crate::value::{HashTableTest, LispHashTable};
use malachite::integer::Integer;
use std::num::NonZeroU64;
use std::sync::atomic::Ordering;

fn key(heap: HeapIdentity, owner: u64) -> MutatorKey {
    // This is copied identity, not forged execution/World authority. The
    // fixture alone owns the stopped heap and exercises actual catalog checks.
    MutatorKey {
        heap,
        world: heap,
        owner,
        registration: NonZeroU64::new(owner).unwrap(),
        generation: NonZeroU64::MIN,
    }
}

fn publish_table(catalog: &ConsCatalog, owner: MutatorKey) {
    let object = Box::new(HashTableObj {
        header: VecLikeHeader::new(VecLikeType::HashTable),
        table: LispHashTable::new_with_options(HashTableTest::Eq, 4, None, 1.5, 0.8125),
    });
    let table = catalog.link_hash_table(owner, object).unwrap();
    assert_ne!(table, TaggedValue::NIL);
    // No collection can occur while the sole fixture owns live numeric runs.
    // This table intentionally has no root and is swept after stopped reunion.
}

fn frontiers<T: PagedObject>(arena: &ObjectArena<T>) -> Vec<usize> {
    arena
        .pages
        .iter()
        .map(|page| {
            arena
                .numeric_retained_page_frontier(page.base_addr())
                .unwrap()
        })
        .collect()
}

#[test]
fn two_owner_numeric_views_survive_catalog_mutation_and_retained_refreeze() {
    let pages_before = (
        FloatObj::live_page_counter().load(Ordering::SeqCst),
        BignumObj::live_page_counter().load(Ordering::SeqCst),
    );
    let mut heap = TaggedHeap::new();
    heap.gc_threshold = usize::MAX;
    let identity = heap.heap_identity();
    let catalog = ConsCatalog::new(identity);
    let a = key(identity, 1);
    let b = key(identity, 2);
    // SAFETY: the fixture is the sole stopped Rust owner. No heap TLS,
    // external mutator, callback, marker launch, run or page view exists.
    let mut collector = unsafe { heap.drain_to_quiescent(|_| {}) }.unwrap();
    // SAFETY: the sole quiescent owner installs actual storage and suspends
    // its original collector loan before any numeric claim can be issued.
    unsafe { catalog.install(collector.registration_heap_mut()) }.unwrap();
    let suspended = collector.suspend_cons_storage();
    for owner in [a, b] {
        assert!(catalog.claim_float(owner, None).unwrap().1.is_none());
        assert!(catalog.claim_bignum(owner, None).unwrap().1.is_none());
    }
    let requested = catalog.numeric_catalog_snapshot_for_test();
    assert_eq!(requested.pending_float_requests, 2);
    assert_eq!(requested.pending_bignum_requests, 2);
    // SAFETY: the empty claims lent no views. The sole stopped fixture
    // resumes the original current designator, then reunites before inspection.
    let mut collector = unsafe { suspended.resume() };
    // SAFETY: no owner view or callback exists, and placeholders are intact.
    unsafe { catalog.restore(collector.registration_heap_mut()) }.unwrap();
    // SAFETY: this is the matching sole stopped resume; actual pending requests
    // mint the unkeyed pools before the next raw view can be issued.
    unsafe { catalog.install(collector.registration_heap_mut()) }.unwrap();
    let suspended = collector.suspend_cons_storage();

    let (generation, mut fa) = catalog
        .claim_float(a, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    let (other_generation, mut fb) = catalog
        .claim_float(b, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    let (big_generation, mut ba) = catalog
        .claim_bignum(a, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    let (other_big_generation, mut bb) = catalog
        .claim_bignum(b, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    assert_eq!(generation, other_generation);
    assert_eq!(generation, big_generation);
    assert_eq!(generation, other_big_generation);
    assert_eq!(catalog.active_regions().unwrap(), 4);
    drop(fa.next_slot().unwrap().initialize(1.25));
    publish_table(&catalog, a);
    drop(
        bb.next_slot()
            .unwrap()
            .initialize(Integer::from(i64::MAX - 1)),
    );
    drop(fb.next_slot().unwrap().initialize(2.5));
    drop(ba.next_slot().unwrap().initialize(Integer::from(i64::MAX)));
    let before_refusal = catalog.numeric_catalog_snapshot_for_test();
    let (reason, mut fa) = catalog.settle_float(b, fa).unwrap_err();
    assert_eq!(reason, super::RegistrationError::WrongOwner);
    assert_eq!(catalog.numeric_catalog_snapshot_for_test(), before_refusal);
    fa.validate(a, generation).unwrap();
    // The failed metadata-only close returned the original still-live run;
    // both owners can continue writing their disjoint raw page fields.
    // Dropping these sealed initialized handoffs ends only their run borrow.
    // No safepoint is possible: the sole fixture cannot restore or collect
    // until every run settles. Values are gathered from real arena ownership
    // only after reunion below; no seal constructor or raw escape is added.
    drop(fa.next_slot().unwrap().initialize(4.5));
    drop(fa.next_slot().unwrap().initialize(6.25));
    // This dead tail will lower A's bignum frontier from two to one. A's
    // float slot one instead remains an interior hole below live slot two.
    drop(
        ba.next_slot()
            .unwrap()
            .initialize(Integer::from(i64::MAX - 5)),
    );
    catalog.settle_float(a, fa).unwrap();
    publish_table(&catalog, b);
    // A's close and both table publications reborrow CatalogState while B's
    // raw bitmap/occupancy fields remain live and writable.
    drop(fb.next_slot().unwrap().initialize(3.75));
    drop(
        bb.next_slot()
            .unwrap()
            .initialize(Integer::from(i64::MAX - 2)),
    );
    catalog.settle_float(b, fb).unwrap();
    catalog.settle_bignum(a, ba).unwrap();
    catalog.settle_bignum(b, bb).unwrap();
    assert_eq!(catalog.active_regions().unwrap(), 0);
    let interval = catalog.numeric_catalog_snapshot_for_test();
    assert_eq!(interval.current_float_slots, 5);
    assert_eq!(interval.current_bignum_slots, 4);

    // SAFETY: all four exclusive runs were consumed through their real close
    // paths. The sole stopped owner reconstructs the collector exactly once.
    let mut collector = unsafe { suspended.resume() };
    // SAFETY: there is no live page view, marker, callback or shell borrower.
    unsafe { catalog.restore(collector.registration_heap_mut()) }.unwrap();
    let (first_a, first_b, first_big, first_big_b, roots) = {
        let heap = collector.registration_heap_mut();
        let floats = heap.float_arena.collect_allocated_slots();
        let bignums = heap.bignum_arena.collect_allocated_slots();
        assert_eq!(floats.len(), 5);
        assert_eq!(bignums.len(), 4);
        let mut first_a = None;
        let mut first_b = None;
        let mut first_big = None;
        let mut first_big_b = None;
        let mut roots = Vec::new();
        for pointer in floats {
            // SAFETY: the real stopped arena enumerates allocated slots only;
            // each complete typed write preceded the corresponding set bit.
            let value = unsafe { TaggedValue::from_float_ptr(pointer) };
            match value.as_float().unwrap() {
                1.25 => {
                    first_a = Some(pointer as usize);
                    roots.push(value);
                }
                2.5 => {
                    first_b = Some(pointer as usize);
                    roots.push(value);
                }
                6.25 => roots.push(value),
                4.5 | 3.75 => {}
                other => panic!("unexpected initialized float {other}"),
            }
        }
        for pointer in bignums {
            // SAFETY: as above, the actual bignum arena owns initialized slots.
            let value = unsafe { TaggedValue::from_veclike_ptr(pointer.cast()) };
            if value.as_bignum().unwrap() == &Integer::from(i64::MAX) {
                first_big = Some(pointer as usize);
                roots.push(value);
            } else if value.as_bignum().unwrap() == &Integer::from(i64::MAX - 1) {
                first_big_b = Some(pointer as usize);
                roots.push(value);
            }
        }
        assert_eq!(roots.len(), 5);
        assert_eq!(frontiers(&heap.float_arena), [3, 2]);
        assert_eq!(frontiers(&heap.bignum_arena), [2, 2]);
        heap.collect_rooted(roots.iter().copied());
        assert_eq!(heap.float_arena.collect_allocated_slots().len(), 3);
        assert_eq!(heap.bignum_arena.collect_allocated_slots().len(), 2);
        assert_eq!(frontiers(&heap.float_arena), [3, 1]);
        assert_eq!(frontiers(&heap.bignum_arena), [1, 1]);

        // Exercise the real serial stopped paths. The reconciled free chains
        // serve B's dead slot one before its never-used slot two. Closing the
        // ordinary float reserve removes every prepaid unused suffix bit.
        let serial_float_one = heap.alloc_float(7.5);
        let serial_float_two = heap.alloc_float(8.5);
        let serial_big_one = heap.alloc_bignum(Integer::from(i64::MAX - 6));
        let serial_big_two = heap.alloc_bignum(Integer::from(i64::MAX - 7));
        heap.close_alloc_regions();
        assert_eq!(
            serial_float_one.as_float_ptr().unwrap() as usize,
            first_b.unwrap() + FloatObj::SLOT_BYTES
        );
        assert_eq!(
            serial_float_two.as_float_ptr().unwrap() as usize,
            first_b.unwrap() + 2 * FloatObj::SLOT_BYTES
        );
        assert_eq!(
            serial_big_one.as_bignum().unwrap(),
            &Integer::from(i64::MAX - 6)
        );
        assert_eq!(
            serial_big_two.as_bignum().unwrap(),
            &Integer::from(i64::MAX - 7)
        );
        roots.extend([
            serial_float_one,
            serial_float_two,
            serial_big_one,
            serial_big_two,
        ]);
        assert_eq!(frontiers(&heap.float_arena), [3, 3]);
        assert_eq!(frontiers(&heap.bignum_arena), [1, 3]);
        (
            first_a.unwrap(),
            first_b.unwrap(),
            first_big.unwrap(),
            first_big_b.unwrap(),
            roots,
        )
    };
    // A's float hole at one remains interior below live slot two. B's serial
    // allocations raised its bitmap frontier from the old saved two to three;
    // A's collected bignum tail lowered its frontier from two to one. Refreeze
    // must derive all four tails from these current initialized bitmaps.
    // SAFETY: the stopped collection completed, no descriptor/view survives,
    // and the sole owner suspends its heap loan again immediately after install.
    unsafe { catalog.install(collector.registration_heap_mut()) }.unwrap();
    let suspended = collector.suspend_cons_storage();
    let (new_generation, mut fa) = catalog
        .claim_float(a, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    let (_, mut fb) = catalog
        .claim_float(b, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    let (_, mut ba) = catalog
        .claim_bignum(a, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    let (_, mut bb) = catalog
        .claim_bignum(b, None)
        .map(|(generation, run)| (generation, run.unwrap()))
        .unwrap();
    assert_ne!(new_generation, generation);
    assert_eq!(fa.capacity(), ObjectPage::<FloatObj>::SLOTS - 3);
    assert_eq!(fb.capacity(), ObjectPage::<FloatObj>::SLOTS - 3);
    assert_eq!(ba.capacity(), ObjectPage::<BignumObj>::SLOTS - 1);
    assert_eq!(bb.capacity(), ObjectPage::<BignumObj>::SLOTS - 3);
    drop(fa.next_slot().unwrap().initialize(10.0));
    publish_table(&catalog, b);
    drop(fb.next_slot().unwrap().initialize(20.0));
    drop(
        ba.next_slot()
            .unwrap()
            .initialize(Integer::from(i64::MAX - 3)),
    );
    drop(
        bb.next_slot()
            .unwrap()
            .initialize(Integer::from(i64::MAX - 4)),
    );
    catalog.settle_float(a, fa).unwrap();
    publish_table(&catalog, a);
    catalog.settle_float(b, fb).unwrap();
    catalog.settle_bignum(a, ba).unwrap();
    catalog.settle_bignum(b, bb).unwrap();
    assert_eq!(catalog.active_regions().unwrap(), 0);
    // SAFETY: every run has settled, the sole fixture is stopped, and no raw
    // descriptor view or field reference crosses reconstruction of the loan.
    let mut collector = unsafe { suspended.resume() };
    // SAFETY: real checked thaw reconciles the settled initialized frontiers.
    unsafe { catalog.restore(collector.registration_heap_mut()) }.unwrap();
    {
        let heap = collector.registration_heap_mut();
        let mut observed = 0;
        for pointer in heap.float_arena.collect_allocated_slots() {
            // SAFETY: fresh enumeration by the reunited real stopped arena.
            let value = unsafe { TaggedValue::from_float_ptr(pointer) };
            match value.as_float().unwrap() {
                10.0 => {
                    assert_eq!(pointer as usize, first_a + 3 * FloatObj::SLOT_BYTES);
                    observed += 1;
                }
                20.0 => {
                    assert_eq!(pointer as usize, first_b + 3 * FloatObj::SLOT_BYTES);
                    observed += 1;
                }
                1.25 | 2.5 | 6.25 | 7.5 | 8.5 => {}
                other => panic!("unexpected post-refreeze float {other}"),
            }
        }
        for pointer in heap.bignum_arena.collect_allocated_slots() {
            // SAFETY: fresh enumeration by the reunited initialized arena.
            let value = unsafe { TaggedValue::from_veclike_ptr(pointer.cast()) };
            if value.as_bignum().unwrap() == &Integer::from(i64::MAX - 3) {
                assert_eq!(pointer as usize, first_big + BignumObj::SLOT_BYTES);
                observed += 1;
            } else if value.as_bignum().unwrap() == &Integer::from(i64::MAX - 4) {
                assert_eq!(pointer as usize, first_big_b + 3 * BignumObj::SLOT_BYTES);
                observed += 1;
            }
        }
        assert_eq!(observed, 4);
        assert_eq!(frontiers(&heap.float_arena), [4, 4]);
        assert_eq!(frontiers(&heap.bignum_arena), [2, 4]);
        drop(roots);
        heap.collect_rooted(std::iter::empty());
        assert!(heap.float_arena.collect_allocated_slots().is_empty());
        assert!(heap.bignum_arena.collect_allocated_slots().is_empty());
    }
    drop(collector);
    drop(catalog);
    drop(heap);
    assert_eq!(
        (
            FloatObj::live_page_counter().load(Ordering::SeqCst),
            BignumObj::live_page_counter().load(Ordering::SeqCst),
        ),
        pages_before
    );
}
