//! Composition of table refusal/poison with the actual canonical transaction.
use super::fixture::*;
use super::publication::name_through_table;
use super::*;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::intern::parallel::{
    fresh_registry_facts_for_test, identity_census_for_test, name_publication_for_test,
};
use crate::emacs_core::symbol::LispSymbol;
use crate::tagged::gc::mutator_registration::symbol_authority::TableCommitFault;
use crate::tagged::gc::symbol_table::publication::{
    RetiredTableCount, TableLimits, TestTableLimits,
};
use crate::tagged::gc::{
    InitialInternError, PlainSymbolBacking, RegistrationError, RendezvousError,
};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;

#[derive(Clone, Copy, Debug)]
enum Limit {
    Bytes,
    Retirees,
}

fn table_limit(limit: Limit) {
    fixture(
        PlainSymbolBacking::Unprojected,
        Query::TableGrowth,
        |run, homes| {
            let registry = run.owner.registry;
            let old = registry.symbols.tables.view();
            let table_before = registry.symbols.tables.stats().unwrap();
            let old_name = name_through_table(run, old.table(), &homes.old_handle, homes.old_id);
            let id = SymId(identity_census_for_test().1.try_into().unwrap());
            let world_before = run.initial_facts_for_test(id).unwrap();
            let heap = registry.heap;
            let name = homes.name.word_for_test(run);
            let spelling = String::from_utf8(
                Value::from_bits(name)
                    .as_lisp_string()
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
            let canonical_before = fresh_registry_facts_for_test(heap, id, name, &spelling);
            let limits = match limit {
                Limit::Bytes => {
                    TableLimits::new(table_before.retained_bytes, TableLimits::DEFAULT_RETIRED)
                }
                Limit::Retirees => {
                    TableLimits::new(TableLimits::DEFAULT_BYTES, RetiredTableCount::new(0))
                }
            };
            registry
                .symbols
                .tables
                .set_limits_for_test(TestTableLimits::Override(limits))
                .unwrap();
            let output = homes.dst.word_for_test(run);
            for _ in 0..2 {
                // This is a repeated semantic refusal, not a contention retry.
                let registry_before = fresh_registry_facts_for_test(heap, id, name, &spelling);
                let before = registry.symbols.tables.stats().unwrap();
                let charge = run.owner.local.as_ref().unwrap().hot.bytes_since_gc;
                assert_eq!(
                    intern_retry(run, &homes.name, &mut homes.dst),
                    Err(InitialInternError::Authority(RegistrationError::Capacity))
                );
                let canonical_after = fresh_registry_facts_for_test(heap, id, name, &spelling);
                let after = registry.symbols.tables.stats().unwrap();
                assert_eq!(
                    run.owner.local.as_ref().unwrap().hot.bytes_since_gc - charge,
                    canonical_after.bytes - registry_before.bytes + after.retained_bytes.get()
                        - before.retained_bytes.get()
                );
                assert_eq!(after.generation, table_before.generation);
                assert_eq!(after.retired, table_before.retired);
                assert!(std::ptr::eq(
                    old.table(),
                    registry.symbols.tables.view().table()
                ));
                assert_eq!(homes.dst.word_for_test(run), output);
                assert_eq!(
                    canonical_after,
                    crate::emacs_core::intern::parallel::FreshRegistryFacts {
                        bytes: canonical_after.bytes,
                        ..canonical_before
                    }
                );
                assert_eq!(
                    name_publication_for_test(heap, id, name),
                    (None, false, false, false)
                );
                let world = run.initial_facts_for_test(id).unwrap();
                assert_eq!(
                    (
                        world.members,
                        world.epoch,
                        world.slots,
                        world.chunks,
                        world.sides,
                        world.bytes,
                        world.present,
                        world.member
                    ),
                    (
                        world_before.members,
                        world_before.epoch,
                        world_before.slots,
                        world_before.chunks,
                        world_before.sides,
                        world_before.bytes,
                        world_before.present,
                        world_before.member
                    )
                );
                assert_eq!(
                    name_through_table(run, old.table(), &homes.old_handle, homes.old_id),
                    old_name
                );
            }
            // The retained-capacity charge is paid once; an ordinary retry after
            // restoring this ledger's production limits issues exactly one ID.
            registry
                .symbols
                .tables
                .set_limits_for_test(TestTableLimits::Production)
                .unwrap();
            intern_retry(run, &homes.name, &mut homes.dst).unwrap();
            let committed = fresh_registry_facts_for_test(heap, id, name, &spelling);
            assert_eq!(committed.epoch, canonical_before.epoch + 1);
            assert_eq!(committed.symbols, canonical_before.symbols + 1);
            let after = registry.symbols.tables.stats().unwrap();
            assert_eq!(after.retired.get(), table_before.retired.get() + 1);
            assert_eq!(
                after.generation,
                table_before.generation.checked_successor().unwrap()
            );
            eprintln!("NMJIT_N3_ACTUAL_TABLE_LIMIT {limit:?} no_world_or_registry_publication=1");
        },
    )
    .unwrap();
}

#[test]
fn nmjit_n3_actual_byte_limit_refuses_before_world_and_registry_publication() {
    table_limit(Limit::Bytes);
}

#[test]
fn nmjit_n3_actual_retiree_limit_refuses_before_world_and_registry_publication() {
    table_limit(Limit::Retirees);
}

#[test]
fn nmjit_n3_actual_post_table_publication_poison_retains_both_descriptors() {
    with_query_era(
        PlainSymbolBacking::Unprojected,
        Query::TableGrowth,
        |era, ids, target| {
            assert!(target.is_none());
            let old_handle = era.admit_plain_symbol(ids[1]).unwrap();
            let mut stopped = era.stop().unwrap();
            let inputs = era
                .with_stopped_heap_for_test(&stopped, |heap| unsafe {
                    // SAFETY: immediate seeds on the actual matching stopped heap.
                    [
                        SharedRoot::new(heap, Value::NIL),
                        SharedRoot::new(heap, Value::fixnum(23)),
                    ]
                })
                .unwrap();
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let mut owner = era
                .register(1, 4)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();
            let outcome = {
                let mut run = owner.run_access().unwrap();
                let mut name = run.root_slot(&inputs[0]).unwrap();
                let mut dst = run.root_slot(&inputs[1]).unwrap();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run.sym_value_into(&old_handle, &mut name).unwrap();
                    let registry = run.owner.registry;
                    let old = registry.symbols.tables.view();
                    let before = registry.symbols.tables.stats().unwrap();
                    let id = SymId(identity_census_for_test().1.try_into().unwrap());
                    let word = name.word_for_test(&run);
                    let spelling = String::from_utf8(
                        Value::from_bits(word)
                            .as_lisp_string()
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                    )
                    .unwrap();
                    let canonical_before =
                        fresh_registry_facts_for_test(registry.heap, id, word, &spelling);
                    let old_name = name_through_table(&mut run, old.table(), &old_handle, ids[1]);
                    let world_before = run.initial_facts_for_test(id).unwrap();
                    let charge = run.owner.local.as_ref().unwrap().hot.bytes_since_gc;
                    run.initial_table_commit_fault_for_test(
                        TableCommitFault::AfterDescriptorPublication,
                    )
                    .unwrap();
                    assert_eq!(
                        intern_retry(&mut run, &name, &mut dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    );
                    let now = registry.symbols.tables.view();
                    let after = registry.symbols.tables.stats().unwrap();
                    assert_eq!(
                        after.generation,
                        before.generation.checked_successor().unwrap()
                    );
                    assert_eq!(after.retired.get(), before.retired.get() + 1);
                    assert!(!std::ptr::eq(old.table(), now.table()));
                    assert_eq!(old.table().chunk(0.into()), now.table().chunk(0.into()));
                    assert!(now.table().slots().get() > old.table().slots().get());
                    // Poison refuses new leases. This terminal test has one owner;
                    // WRITER returned and no other actor can edit the retained World.
                    // SAFETY: original retained allocation root; the old initialized
                    // slot remains unchanged, no mutable cell or manager ref escapes.
                    let old_again = unsafe {
                        let slot = ids[1].0 as usize;
                        let span = old.table().chunk_slots().get();
                        let chunk = old.table().chunk((slot / span).into()).unwrap();
                        (&*chunk.as_ptr().cast::<LispSymbol>().add(slot % span)).name()
                    };
                    assert_eq!(old_again, old_name);
                    let canonical_after =
                        fresh_registry_facts_for_test(registry.heap, id, word, &spelling);
                    assert_eq!(
                        canonical_after,
                        crate::emacs_core::intern::parallel::FreshRegistryFacts {
                            bytes: canonical_after.bytes,
                            ..canonical_before
                        }
                    );
                    assert_eq!(
                        name_publication_for_test(registry.heap, id, word),
                        (None, false, false, false)
                    );
                    assert_eq!(dst.word_for_test(&run), Value::fixnum(23).bits());
                    assert!(
                        run.owner.local.as_ref().unwrap().hot.bytes_since_gc - charge
                            >= after.retained_bytes.get() - before.retained_bytes.get()
                    );
                    assert!(now.table().slots().get() > world_before.slots);
                    assert_eq!(
                        run.initial_intern_soft_into(&name, &mut dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    );
                    let (poisoned, quiescent, _prior_fault_receipt) =
                        run.initial_poison_receipt_for_test();
                    assert!(poisoned && quiescent);
                }));
                drop(name);
                drop(dst);
                // Total terminal teardown whether the fault or an assertion failed.
                match run.clear_root_slots() {
                    Ok(()) => {}
                    Err(RegistrationError::Poisoned) => {
                        run.clear_terminal_roots_for_test().unwrap()
                    }
                    Err(error) => panic!("N3 terminal root cleanup: {error:?}"),
                }
                outcome
            };
            drop(inputs);
            let run_refused = matches!(owner.run_access(), Err(RegistrationError::Poisoned));
            let stop_refused = matches!(era.stop(), Err(RendezvousError::Poisoned));
            drop(owner);
            let finish_reason = match era.finish() {
                Err(failure) => {
                    let reason = failure.reason;
                    drop(failure);
                    Some(reason)
                }
                Ok((collector, keeper)) => {
                    drop(collector);
                    drop(keeper);
                    None
                }
            };
            if let Err(assertion) = outcome {
                std::panic::resume_unwind(assertion);
            }
            assert!(run_refused && stop_refused);
            assert_eq!(finish_reason, Some(RegistrationError::Poisoned));
            eprintln!("NMJIT_N3_ACTUAL_POST_TABLE_POISON all_published_allocations_retained=1");
        },
    );
}
