//! A finite weak sweep permission cannot authorize new weak shells.
use super::*;

#[test]
fn a6_ht_b8_weak_copy_refuses_before_allocation_or_root_publication() {
    let mut context = Context::new();
    let table = context
        .eval_str("(make-hash-table :test 'eq :weakness 'key)")
        .unwrap();
    let (source, nil) = unsafe {
        (
            SharedRoot::new(&context.tagged_heap, table),
            SharedRoot::new(&context.tagged_heap, TaggedValue::NIL),
        )
    };
    context
        .admit_owned_parallel_with_weak_tables_for_test(&[table], |projection| {
            let era = projection.era();
            era.start_cons_storage().unwrap();
            let mut owner = era
                .register(10_800, 2)
                .unwrap()
                .activate(era.heap_identity(), 10_800)
                .unwrap();
            with_run(&mut owner, |run| {
                let src = run.root_slot(&source)?;
                let dst = run.root_slot(&nil)?;
                let hot = &run.owner.local.as_ref().unwrap().hot;
                let before = (
                    hot.allocated_count,
                    hot.bytes_since_gc,
                    hot.memory_use_counts,
                );
                assert_eq!(
                    run.copy_hash_table_into(&src, &dst),
                    Err(RegistrationError::Publication)
                );
                let hot = &run.owner.local.as_ref().unwrap().hot;
                assert_eq!(
                    (
                        hot.allocated_count,
                        hot.bytes_since_gc,
                        hot.memory_use_counts
                    ),
                    before
                );
                assert_eq!(dst.word_for_test(run), TaggedValue::NIL.bits());
                let value = TaggedValue::from_bits(src.word_for_test(run));
                let lease = TableRef::enter(run, value)?;
                assert_eq!(lease.authority_word().load(Ordering::Acquire), 1);
                assert_eq!(lease.user_test_depth(), 0);
                lease.read(|storage| assert_eq!(storage.len(), 0));
                drop(lease);
                run.clear_root_slots()?;
                Ok(())
            })
            .unwrap();
            owner.enter_native(&[]).unwrap();
            let mut stopped = era.stop().unwrap();
            era.return_cons_storage(&stopped).unwrap();
            era.collect_stopped(&mut stopped).unwrap();
            complete(stopped);
            owner.retire_rooted(&[]).unwrap();
            let (collector, keeper) = projection.finish().unwrap();
            (collector, keeper, ())
        })
        .unwrap();
}
