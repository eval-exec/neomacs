//! Fence negatives use serial setup or the real keeper stop, never a fake stop.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{HashTableTest, HashTableWeakness, LispHashTable};
use crate::tagged::header::ImageResident;

fn clean_context() -> Context {
    let mut context = Context::new();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    context.gc_collect_exact();
    context
}

fn weak(context: &mut Context) -> TaggedValue {
    let value = context
        .tagged_heap
        .alloc_hash_table(LispHashTable::new_with_options(
            HashTableTest::Eq,
            4,
            Some(HashTableWeakness::Key),
            1.5,
            0.8,
        ));
    context.set_variable("p2ht-b8-policy-table", value);
    value
}

#[test]
fn a6_ht_b8_fence_requires_explicit_complete_weak_inventory() {
    let mut context = clean_context();
    let table = weak(&mut context);
    context.with_weak_sweep_keeper_for_test(|context, keeper| {
        let none = WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[]).unwrap();
        assert!(supported(&context.tagged_heap).is_err());
        assert!(supported_with_weak(&context.tagged_heap, Some(&none)).is_err());
        let permit = WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[table]).unwrap();
        supported_with_weak(&context.tagged_heap, Some(&permit)).unwrap();
        assert!(!permit.covers(&context.tagged_heap, TaggedValue::NIL.bits()));
    });
}

#[test]
fn a6_ht_b8_fence_refuses_foreign_and_non_table_designators() {
    let mut first = clean_context();
    let table = weak(&mut first);
    first.with_weak_sweep_keeper_for_test(|first, first_keeper| {
        let mut second = clean_context();
        second.with_weak_sweep_keeper_for_test(|second, second_keeper| {
            assert!(matches!(
                WeakSweepPermit::mint(&second.tagged_heap, &first_keeper, &[table]),
                Err(RegistrationError::WrongHeap)
            ));
            assert!(matches!(
                WeakSweepPermit::mint(&second.tagged_heap, &second_keeper, &[table]),
                Err(RegistrationError::Publication)
            ));
            assert!(matches!(
                WeakSweepPermit::mint(&first.tagged_heap, &first_keeper, &[TaggedValue::fixnum(1)]),
                Err(RegistrationError::Publication)
            ));
        });
    });
}

#[test]
fn a6_ht_b8_fence_refuses_pending_active_test_and_storage_authority() {
    let mut context = clean_context();
    let table = weak(&mut context);
    context.with_weak_sweep_keeper_for_test(|context, keeper| {
        let ptr = table
            .as_veclike_ptr()
            .unwrap()
            .cast::<HashTableObj>()
            .cast_mut();
        // SAFETY: this test is the sole serial keeper, with a rooted live shell;
        // it deliberately constructs refused states without entering any owner.
        unsafe {
            (*ptr).table.test_depth.store(1, Ordering::Release);
            assert!(WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[table]).is_err());
            (*ptr).table.test_depth.store(0, Ordering::Release);
            for word in [1, super::super::table_authority::WRITER] {
                (*ptr).table.data.authority.store(word, Ordering::Release);
                assert!(WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[table]).is_err());
            }
            (*ptr).table.data.authority.store(0, Ordering::Release);
            (*ptr).table.set_pending_dump_entries(Vec::new());
            assert!(WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[table]).is_err());
            assert!(
                (*ptr).table.needs_hydration(),
                "refusal must not hydrate storage"
            );
            (*ptr).table.hydrate_pending();
        }
    });
}

#[test]
fn a6_ht_b8_fence_refuses_permanent_mapped_and_callback_payloads() {
    let mut context = clean_context();
    let table = weak(&mut context);
    context.with_weak_sweep_keeper_for_test(|context, keeper| {
        let ptr = table
            .as_veclike_ptr()
            .unwrap()
            .cast::<HashTableObj>()
            .cast_mut();
        // SAFETY: rooted owned header, exclusive serial keeper. Restore the
        // temporary generation marker before any real collection/destruction.
        unsafe {
            let header = &mut (*ptr).header.gc;
            let generation = header.generation;
            let tenured = header.tenured;
            header.make_permanent(&ImageResident::for_test());
            assert!(WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[table]).is_err());
            header.generation = generation;
            header.tenured = tenured;
        }
        let permit = WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[table]).unwrap();
        // A decoded image owns its shell, not this heap's owned-object catalog.
        let mapped = Box::new(HashTableObj {
            header: VecLikeHeader::new(VecLikeType::HashTable),
            table: LispHashTable::new_with_options(
                HashTableTest::Eq,
                4,
                Some(HashTableWeakness::Key),
                1.5,
                0.8,
            ),
        });
        let mapped_ptr = std::ptr::from_ref(&*mapped).cast_mut();
        let mapped_value = unsafe { TaggedValue::from_veclike_ptr(mapped_ptr.cast()) };
        assert!(WeakSweepPermit::mint(&context.tagged_heap, &keeper, &[mapped_value]).is_err());
        context.tagged_heap.mapped_veclike_objects.push(
            super::super::super::arena_pages::MappedVecLikeObject::new(
                mapped_ptr.cast(),
                std::mem::size_of::<HashTableObj>(),
            ),
        );
        assert!(supported_with_weak(&context.tagged_heap, Some(&permit)).is_err());
        context.tagged_heap.mapped_veclike_objects.pop();
        context
            .tagged_heap
            .alloc_user_ptr(std::ptr::null_mut(), None);
        assert!(supported_with_weak(&context.tagged_heap, Some(&permit)).is_err());
    });
}

#[test]
fn a6_ht_b8_stopped_fence_refuses_before_collection_mutation_and_recovers() {
    let mut context = clean_context();
    let table = weak(&mut context);
    context
        .admit_owned_parallel_with_weak_tables_for_test(&[table], |projection| {
            let era = projection.era();
            let mut stopped = era.stop().unwrap();
            let cycle = era
                .with_stopped_heap_for_test(&stopped, TaggedHeap::gc_collections)
                .unwrap();
            let lease = KeeperTableMut::enter(&stopped, table).unwrap();
            let authority = std::ptr::from_ref(lease.authority_word());
            let depth = std::ptr::from_ref(&lease.guarded_table().test_depth);
            drop(lease);
            for word in [1, super::super::table_authority::WRITER] {
                // SAFETY: fault injection into the rooted shell while the actual
                // stop remains held; no storage borrow or owner exists here.
                unsafe { (*authority).store(word, Ordering::Release) };
                assert!(era.collect_stopped(&mut stopped).is_err());
                unsafe { (*authority).store(0, Ordering::Release) };
            }
            unsafe { (*depth).store(1, Ordering::Release) };
            assert!(era.collect_stopped(&mut stopped).is_err());
            unsafe { (*depth).store(0, Ordering::Release) };
            assert_eq!(
                era.with_stopped_heap_for_test(&stopped, TaggedHeap::gc_collections)
                    .unwrap(),
                cycle
            );
            era.collect_stopped(&mut stopped).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = projection.finish().unwrap();
            (collector, keeper, ())
        })
        .unwrap();
}
