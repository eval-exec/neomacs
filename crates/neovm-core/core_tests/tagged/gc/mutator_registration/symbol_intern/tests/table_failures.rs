//! Composition of table refusal/poison with the actual canonical transaction.
use super::fixture::*;
use crate::intern::SymId;
use crate::intern::parallel::{identity_census_for_test, name_publication_for_test};
use crate::symbol::parallel::test_facts::{
    TableCommitFault, TableLimitProbe, fresh_registry_facts_for_test,
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
            let old = run.symbol_table_census_for_test().unwrap();
            let table_before = old;
            let old_name =
                run.symbol_table_name_at_for_test(old.address(), &homes.old_handle, homes.old_id);
            let id = unsafe {
                // SAFETY: the census length is the next id this process interner issues.
                SymId::from_raw(identity_census_for_test().1.try_into().unwrap())
            };
            let world_before = run.initial_facts_copy_for_test(id).unwrap();
            let heap = run.registry_heap_for_test();
            let name = homes.name.word_for_test(run);
            let spelling = String::from_utf8(
                unsafe { Value::from_bits(name) }
                    .as_lisp_string()
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap();
            let canonical_before = fresh_registry_facts_for_test(heap, id, name, &spelling);
            let probe = match limit {
                Limit::Bytes => TableLimitProbe::Bytes,
                Limit::Retirees => TableLimitProbe::Retirees,
            };
            run.override_symbol_table_limits_for_test(probe).unwrap();
            let output = homes.dst.word_for_test(run);
            for _ in 0..2 {
                // This is a repeated semantic refusal, not a contention retry.
                let registry_before = fresh_registry_facts_for_test(heap, id, name, &spelling);
                let before = run.symbol_table_census_for_test().unwrap();
                let charge = run.hot_bytes_since_gc_for_test().unwrap();
                assert_eq!(
                    intern_once(run, &homes.name, &mut homes.dst),
                    Err(InitialInternError::Authority(RegistrationError::Capacity))
                );
                let canonical_after = fresh_registry_facts_for_test(heap, id, name, &spelling);
                let after = run.symbol_table_census_for_test().unwrap();
                assert_eq!(
                    run.hot_bytes_since_gc_for_test().unwrap() - charge,
                    canonical_after.bytes() - registry_before.bytes() + after.retained_bytes()
                        - before.retained_bytes()
                );
                assert_eq!(after.generation(), table_before.generation());
                assert_eq!(after.retired(), table_before.retired());
                assert_eq!(after.address(), old.address());
                assert_eq!(homes.dst.word_for_test(run), output);
                assert_eq!(
                    canonical_after,
                    neovm_value::test_support::authority::fresh_registry_facts_with_bytes(
                        canonical_before,
                        canonical_after.bytes(),
                    )
                );
                assert_eq!(
                    name_publication_for_test(heap, id, name),
                    (None, false, false, false)
                );
                let world = run.initial_facts_copy_for_test(id).unwrap();
                assert_eq!(
                    (
                        world.members(),
                        world.epoch(),
                        world.slots(),
                        world.chunks(),
                        world.sides(),
                        world.bytes(),
                        world.present(),
                        world.member()
                    ),
                    (
                        world_before.members(),
                        world_before.epoch(),
                        world_before.slots(),
                        world_before.chunks(),
                        world_before.sides(),
                        world_before.bytes(),
                        world_before.present(),
                        world_before.member()
                    )
                );
                assert_eq!(
                    run.symbol_table_name_at_for_test(
                        old.address(),
                        &homes.old_handle,
                        homes.old_id
                    ),
                    old_name
                );
            }
            // The retained-capacity charge is paid once; an ordinary retry after
            // restoring this ledger's production limits issues exactly one ID.
            run.override_symbol_table_limits_for_test(TableLimitProbe::Production)
                .unwrap();
            intern_once(run, &homes.name, &mut homes.dst).unwrap();
            let committed = fresh_registry_facts_for_test(heap, id, name, &spelling);
            assert_eq!(committed.epoch(), canonical_before.epoch() + 1);
            assert_eq!(committed.symbols(), canonical_before.symbols() + 1);
            let after = run.symbol_table_census_for_test().unwrap();
            assert_eq!(after.retired(), table_before.retired() + 1);
            assert_eq!(
                after.generation(),
                table_before.generation_successor().unwrap()
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
                    let old = run.symbol_table_census_for_test().unwrap();
                    let before = old;
                    let id = unsafe {
                        // SAFETY: the census length is the next id this process interner issues.
                        SymId::from_raw(identity_census_for_test().1.try_into().unwrap())
                    };
                    let word = name.word_for_test(&run);
                    let spelling = String::from_utf8(
                        // SAFETY: `word` is the rooted name just read from the home slot.
                        unsafe { Value::from_bits(word) }
                            .as_lisp_string()
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                    )
                    .unwrap();
                    let heap = run.registry_heap_for_test();
                    let canonical_before = fresh_registry_facts_for_test(heap, id, word, &spelling);
                    let old_name =
                        run.symbol_table_name_at_for_test(old.address(), &old_handle, ids[1]);
                    let world_before = run.initial_facts_copy_for_test(id).unwrap();
                    let charge = run.hot_bytes_since_gc_for_test().unwrap();
                    run.arm_table_commit_fault_for_test(
                        TableCommitFault::AfterDescriptorPublication,
                    )
                    .unwrap();
                    assert_eq!(
                        intern_once(&mut run, &name, &mut dst),
                        Err(InitialInternError::Authority(RegistrationError::Poisoned))
                    );
                    let now = run.symbol_table_census_for_test().unwrap();
                    assert_eq!(now.generation(), before.generation_successor().unwrap());
                    assert_eq!(now.retired(), before.retired() + 1);
                    assert_ne!(old.address(), now.address());
                    assert_eq!(old.chunk0(), now.chunk0());
                    assert!(now.slots() > old.slots());
                    // Poison refuses new leases. This terminal test has one owner;
                    // WRITER returned and no other actor can edit the retained World.
                    // The name read does not take a cell reader.
                    let old_again = run.retained_symbol_name_for_test(old.address(), ids[1]);
                    assert_eq!(old_again, old_name);
                    let canonical_after = fresh_registry_facts_for_test(heap, id, word, &spelling);
                    assert_eq!(
                        canonical_after,
                        neovm_value::test_support::authority::fresh_registry_facts_with_bytes(
                            canonical_before,
                            canonical_after.bytes(),
                        )
                    );
                    assert_eq!(
                        name_publication_for_test(heap, id, word),
                        (None, false, false, false)
                    );
                    assert_eq!(dst.word_for_test(&run), Value::fixnum(23).bits());
                    assert!(
                        run.hot_bytes_since_gc_for_test().unwrap() - charge
                            >= now.retained_bytes() - before.retained_bytes()
                    );
                    assert!(now.slots() > world_before.slots());
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
                    let reason = failure.reason();
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

#[test]
fn e7p0_all_stopped_grace_receipt_reclaims_retired_symbol_tables_with_live_owner() {
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

            let heap = era.heap_identity();
            let permit = era.register(1, 4).unwrap();
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
            let (stop_epoch_tx, stop_epoch_rx) =
                std::sync::mpsc::sync_channel::<std::num::NonZeroU64>(1);

            let inputs_ref = &inputs;
            let (keeper_report, second_report) = std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let mut owner = permit.activate(heap, 1).unwrap();
                    let (old_name, after_growth) = {
                        let mut run = owner.run_access().unwrap();
                        let mut name = run.root_slot(&inputs_ref[0]).unwrap();
                        let mut dst = run.root_slot(&inputs_ref[1]).unwrap();
                        run.sym_value_into(&old_handle, &mut name).unwrap();
                        let before = run.symbol_table_census_for_test().unwrap();
                        let old_name = run.symbol_table_name_at_for_test(
                            before.address(),
                            &old_handle,
                            ids[1],
                        );
                        intern_once(&mut run, &name, &mut dst).unwrap();
                        let after_growth = run.symbol_table_census_for_test().unwrap();
                        assert_eq!(after_growth.retired(), before.retired() + 1);
                        assert_eq!(
                            after_growth.generation(),
                            before.generation_successor().unwrap()
                        );
                        assert_ne!(after_growth.address(), before.address());
                        assert!(after_growth.retained_bytes() > before.retained_bytes());
                        drop(name);
                        drop(dst);
                        run.clear_root_slots().unwrap();
                        (old_name, after_growth)
                    };

                    // Signal keeper to request stop, then park cooperatively at Mutator::poll.
                    ready_tx.send(()).unwrap();
                    let resumed_epoch = loop {
                        match owner.poll(|| Ok(Vec::new())).unwrap() {
                            crate::tagged::gc::PollOutcome::Continue => std::thread::yield_now(),
                            crate::tagged::gc::PollOutcome::Resumed { epoch } => break epoch,
                        }
                    };
                    let expected_epoch = stop_epoch_rx.recv().unwrap();
                    assert_eq!(resumed_epoch, expected_epoch);

                    // After grace reclamation: retired == 0, retained_bytes shrank,
                    // current table address & generation preserved and readable.
                    {
                        let mut run = loop {
                            match owner.run_access() {
                                Ok(run) => break run,
                                Err(RegistrationError::Busy) => std::thread::yield_now(),
                                Err(err) => panic!("post-grace run_access: {err:?}"),
                            }
                        };
                        let after_grace = run.symbol_table_census_for_test().unwrap();
                        assert_eq!(after_grace.retired(), 0);
                        assert_eq!(after_grace.address(), after_growth.address());
                        assert_eq!(after_grace.generation(), after_growth.generation());
                        assert!(after_grace.retained_bytes() < after_growth.retained_bytes());
                        assert_eq!(
                            run.symbol_table_name_at_for_test(
                                after_grace.address(),
                                &old_handle,
                                ids[1]
                            ),
                            old_name
                        );
                    }

                    owner.retire_rooted(&[]).unwrap();
                });

                ready_rx.recv().unwrap();
                let mut grace_stop = era.stop().unwrap();
                let stop_epoch = grace_stop.epoch();
                let mut receipt = grace_stop.mint_grace_receipt().unwrap();
                assert_eq!(receipt.epoch(), stop_epoch);
                assert_eq!(receipt.heap_identity(), heap);
                let first = receipt.reclaim_symbol_tables().unwrap();
                let second = receipt.reclaim_symbol_tables().unwrap();
                drop(receipt);
                grace_stop.trace(|_, _| Ok(())).unwrap();
                grace_stop.complete().unwrap();
                stop_epoch_tx.send(stop_epoch).unwrap();

                worker.join().unwrap();
                (first, second)
            });

            assert!(keeper_report.reclaimed >= 1);
            assert!(keeper_report.freed_bytes > 0);
            assert_eq!(second_report.reclaimed, 0);
            assert_eq!(second_report.freed_bytes, 0);

            drop(inputs);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn e7p0_grace_receipt_confinement_and_idempotent_empty_stop() {
    static_assertions::assert_not_impl_any!(
        crate::tagged::gc::EraGraceReceipt<'static, 'static>: Send,
        Sync,
        Clone,
        Copy
    );
    with_query_era(
        PlainSymbolBacking::Unprojected,
        Query::TableGrowth,
        |era, _ids, _target| {
            let mut stopped = era.stop().unwrap();
            let stopped_epoch = stopped.epoch();
            let mut receipt = stopped.mint_grace_receipt().unwrap();
            assert_eq!(receipt.epoch(), stopped_epoch);
            assert_eq!(receipt.heap_identity(), era.heap_identity());
            let report = receipt.reclaim_symbol_tables().unwrap();
            assert_eq!(report.reclaimed, 0);
            assert_eq!(report.freed_bytes, 0);
            drop(receipt);
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}

#[test]
fn e7p0_grace_receipt_refuses_poisoned_era_and_retains_tables() {
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
            {
                let mut run = owner.run_access().unwrap();
                let mut name = run.root_slot(&inputs[0]).unwrap();
                let mut dst = run.root_slot(&inputs[1]).unwrap();
                run.sym_value_into(&old_handle, &mut name).unwrap();
                let old = run.symbol_table_census_for_test().unwrap();
                let old_name =
                    run.symbol_table_name_at_for_test(old.address(), &old_handle, ids[1]);
                run.arm_table_commit_fault_for_test(TableCommitFault::AfterDescriptorPublication)
                    .unwrap();
                assert_eq!(
                    intern_once(&mut run, &name, &mut dst),
                    Err(InitialInternError::Authority(RegistrationError::Poisoned))
                );
                let after = run.symbol_table_census_for_test().unwrap();
                assert_ne!(after.address(), old.address());
                assert_eq!(after.retired(), old.retired() + 1);
                assert_eq!(
                    run.retained_symbol_name_for_test(old.address(), ids[1]),
                    old_name
                );
                drop(name);
                drop(dst);
                run.clear_terminal_roots_for_test().unwrap();
            }

            // On a poisoned era, `era.stop()` refuses `Poisoned`, making
            // `EraGraceReceipt` unconstructible so retired tables stay retained.
            assert_eq!(era.stop().err(), Some(RendezvousError::Poisoned));
            drop(inputs);
            drop(owner);
            let finish_err = era.finish().unwrap_err();
            assert_eq!(finish_err.reason(), RegistrationError::Poisoned);
            drop(finish_err);
        },
    );
}

#[test]
fn e7p0_grace_receipt_unblocks_real_era_need_grace_with_live_owner() {
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

            let heap = era.heap_identity();
            let permit = era.register(1, 4).unwrap();
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
            let inputs_ref = &inputs;

            let reclaimed = std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let mut owner = permit.activate(heap, 1).unwrap();
                    {
                        let mut run = owner.run_access().unwrap();
                        let mut name = run.root_slot(&inputs_ref[0]).unwrap();
                        let mut dst = run.root_slot(&inputs_ref[1]).unwrap();
                        run.sym_value_into(&old_handle, &mut name).unwrap();
                        run.override_symbol_table_limits_for_test(TableLimitProbe::Retirees)
                            .unwrap();
                        assert_eq!(
                            intern_once(&mut run, &name, &mut dst),
                            Err(InitialInternError::Authority(RegistrationError::Capacity))
                        );
                        run.override_symbol_table_limits_for_test(TableLimitProbe::Production)
                            .unwrap();
                        intern_once(&mut run, &name, &mut dst).unwrap();
                        let after_growth = run.symbol_table_census_for_test().unwrap();
                        assert!(after_growth.retired() >= 1);
                        drop(name);
                        drop(dst);
                        run.clear_root_slots().unwrap();
                    }

                    ready_tx.send(()).unwrap();
                    loop {
                        match owner.poll(|| Ok(Vec::new())).unwrap() {
                            crate::tagged::gc::PollOutcome::Continue => std::thread::yield_now(),
                            crate::tagged::gc::PollOutcome::Resumed { .. } => break,
                        }
                    }

                    {
                        let run = loop {
                            match owner.run_access() {
                                Ok(run) => break run,
                                Err(RegistrationError::Busy) => std::thread::yield_now(),
                                Err(err) => panic!("post-grace run_access: {err:?}"),
                            }
                        };
                        let after_grace = run.symbol_table_census_for_test().unwrap();
                        assert_eq!(after_grace.retired(), 0);
                    }
                    owner.retire_rooted(&[]).unwrap();
                });

                ready_rx.recv().unwrap();
                let mut grace_stop = era.stop().unwrap();
                let mut receipt = grace_stop.mint_grace_receipt().unwrap();
                let report = receipt.reclaim_symbol_tables().unwrap();
                drop(receipt);
                grace_stop.trace(|_, _| Ok(())).unwrap();
                grace_stop.complete().unwrap();
                worker.join().unwrap();
                report
            });

            assert!(reclaimed.reclaimed >= 1);
            assert!(reclaimed.freed_bytes > 0);
            drop(inputs);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}
