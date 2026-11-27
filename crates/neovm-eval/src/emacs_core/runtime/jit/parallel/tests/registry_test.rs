//! E7-P.1 tests: `NmjitCodeRegistry` stop-bound grace reclamation and
//! multi-owner concurrent leaf execution across real stopped collections.
use super::execution_fixtures::ExecutionTestHomes;
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::{ParallelHeapEraWorldExt, WorldEra};
use neovm_value::tagged::gc::{Gen1Admission, PlainSymbolBacking, PollOutcome};
use neovm_value::tagged::value::{Fixnum, TaggedValue};
use neovm_value::test_support::authority::{nil_root_for_test, take_legacy_roots_for_test};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

fn retry_busy_registry(
    mut op: impl FnMut() -> Result<(), NmjitRegistryError>,
) -> Result<(), NmjitRegistryError> {
    for _ in 0..10_000 {
        match op() {
            Ok(()) => return Ok(()),
            Err(
                NmjitRegistryError::Busy
                | NmjitRegistryError::Entry(
                    ParallelEntryError::Authority(RegistrationError::Busy)
                    | ParallelEntryError::CellRead(CellReadError::Global(
                        neovm_value::tagged::gc::GlobalError::Authority(RegistrationError::Busy),
                    )),
                ),
            ) => std::thread::yield_now(),
            Err(err) => return Err(err),
        }
    }
    op()
}

struct Gen1ModePin {
    previous: Option<std::ffi::OsString>,
}

impl Gen1ModePin {
    fn new() -> Self {
        let previous = std::env::var_os("NEOVM_GC_GENERATIONAL");
        // SAFETY: `cargo nextest` executes one test per process; restored on drop.
        unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "1") };
        Self { previous }
    }
}

impl Drop for Gen1ModePin {
    fn drop(&mut self) {
        // SAFETY: see `new`.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
            }
        }
    }
}

#[test]
fn e7p1_nmjit_registry_multi_owner_concurrent_leaf_across_two_stops_and_grace_reclaim() {
    let _harness = crate::test_utils::unit_harness();
    let _gen1 = Gen1ModePin::new();
    let outcome = Context::try_with_test_symbol_backing_init_era(
        PlainSymbolBacking::Unprojected,
        |obarray, id| {
            obarray.seed_value_id(id, TaggedValue::fixnum(91));
        },
        |mut proof, id| {
            let frozen = take_legacy_roots_for_test(&mut proof).expect("rooted setup evidence");
            let (collector, keeper) = proof.finish().expect("setup return");
            let admitted = match Gen1Admission::check(collector) {
                Ok(admitted) => admitted,
                Err(failure) => {
                    let (reason, collector) = failure.into_parts();
                    drop(collector);
                    drop(frozen);
                    drop(keeper);
                    panic!("GEN1 required: {reason:?}");
                }
            };

            let registry = Arc::new(NmjitCodeRegistry::new(&admitted));
            let c1 = registry.compile_scalar(
                &admitted,
                ScalarProgram::Constant(ImmediateResult::Fixnum(Fixnum::try_from(42).unwrap())),
            );
            let c2 = registry.compile_scalar(&admitted, ScalarProgram::Argument);
            let c3 = registry.compile_cell(&admitted, CellRead::GlobalValue);

            let era = WorldEra::admit_rooted(admitted, keeper, frozen).unwrap_or_else(|failure| {
                let (reason, collector, keeper) = failure.into_parts();
                drop(collector);
                drop(keeper);
                panic!("admit_rooted: {reason:?}");
            });

            if !cfg!(feature = "jit") {
                assert!(matches!(
                    c1,
                    Err(NmjitRegistryError::Compile(
                        ParallelCompileError::BackendUnavailable
                    ))
                ));
                assert!(matches!(
                    c2,
                    Err(NmjitRegistryError::Compile(
                        ParallelCompileError::BackendUnavailable
                    ))
                ));
                assert!(matches!(
                    c3,
                    Err(NmjitRegistryError::Compile(
                        ParallelCompileError::BackendUnavailable
                    ))
                ));
                let mut stopped = era.stop().unwrap();
                let mut receipt = stopped.mint_grace_receipt().unwrap();
                let report = registry.reclaim_with_grace(&mut receipt).unwrap();
                assert_eq!(report.reclaimed, 0);
                assert_eq!(report.remaining_active, 0);
                drop(receipt);
                stopped.trace(|_, _| Ok(())).unwrap();
                stopped.complete().unwrap();
                let (collector, keeper) = era.finish().unwrap();
                drop(collector);
                drop(keeper);
                return;
            }

            let leaf_const = c1.expect("jit scalar const");
            let leaf_arg = c2.expect("jit scalar arg");
            let leaf_cell = c3.expect("jit cell read");
            assert_eq!(registry.stats().unwrap().active, 3);
            assert_eq!(registry.stats().unwrap().retired, 0);

            let symbol = era.admit_plain_symbol(id).expect("plain symbol handle");
            let admission = era.execution_admission();
            let heap = era.heap_identity();
            const OWNERS: usize = 4;
            let permits: Vec<_> = (0..OWNERS)
                .map(|idx| era.register((idx + 1) as u64, 4).unwrap())
                .collect();
            let (p1_tx, p1_rx) = mpsc::sync_channel::<usize>(OWNERS);
            let (p2_tx, p2_rx) = mpsc::sync_channel::<usize>(OWNERS);
            let abort = Arc::new(AtomicBool::new(false));

            struct AbortOnDrop(Arc<AtomicBool>);
            impl Drop for AbortOnDrop {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }

            let (reclaim1, reclaim2, idempotent2) = std::thread::scope(|scope| {
                let _abort_guard = AbortOnDrop(Arc::clone(&abort));
                let mut handles = Vec::with_capacity(OWNERS);
                for (idx, permit) in permits.into_iter().enumerate() {
                    let reg = Arc::clone(&registry);
                    let adm = &admission;
                    let sym = &symbol;
                    let p1 = p1_tx.clone();
                    let p2 = p2_tx.clone();
                    let ab = Arc::clone(&abort);
                    handles.push(scope.spawn(move || {
                        let mut permit = permit;
                        let mut owner = loop {
                            match permit.activate(heap, (idx + 1) as u64) {
                                Ok(owner) => break owner,
                                Err(failure) if failure.reason == RegistrationError::Busy => {
                                    permit = failure.into_permit();
                                    std::thread::yield_now();
                                }
                                Err(failure) => panic!("activate: {failure:?}"),
                            }
                        };
                        let expected_arg = ImmediateResult::Fixnum(
                            Fixnum::try_from((idx as i64) * 11 + 3).unwrap(),
                        );
                        // Phase 1: all 4 owners concurrently run all 3 shared leaves.
                        {
                            let mut run = loop {
                                match owner.run_access() {
                                    Ok(run) => break run,
                                    Err(RegistrationError::Busy) => std::thread::yield_now(),
                                    Err(err) => panic!("phase 1 run_access: {err:?}"),
                                }
                            };
                            let dst = nil_root_for_test(&mut run).unwrap();
                            for _ in 0..8 {
                                retry_busy_registry(|| {
                                    reg.run_scalar(
                                        leaf_const,
                                        adm,
                                        &mut run,
                                        &dst,
                                        ImmediateResult::Nil,
                                    )
                                })
                                .unwrap();
                                assert_eq!(
                                    ExecutionTestHomes::read(&dst, &run),
                                    ImmediateResult::Fixnum(Fixnum::try_from(42).unwrap())
                                );
                                retry_busy_registry(|| {
                                    reg.run_scalar(leaf_arg, adm, &mut run, &dst, expected_arg)
                                })
                                .unwrap();
                                assert_eq!(ExecutionTestHomes::read(&dst, &run), expected_arg);
                                retry_busy_registry(|| {
                                    reg.run_cell(leaf_cell, adm, &mut run, &dst, sym)
                                })
                                .unwrap();
                                assert_eq!(
                                    ExecutionTestHomes::read(&dst, &run),
                                    ImmediateResult::Fixnum(Fixnum::try_from(91).unwrap())
                                );
                            }
                            run.clear_root_slots().unwrap();
                        }
                        p1.send(idx).unwrap();
                        loop {
                            if ab.load(Ordering::Acquire) {
                                return;
                            }
                            match owner.poll(|| Ok(Vec::new())) {
                                Ok(PollOutcome::Continue)
                                | Err(neovm_value::tagged::gc::RendezvousError::Busy) => {
                                    std::thread::yield_now();
                                }
                                Ok(PollOutcome::Resumed { .. }) => break,
                                Err(err) => panic!("phase 1 poll: {err:?}"),
                            }
                        }

                        // Phase 2: `leaf_const` retired & reclaimed in Stop #1;
                        // `leaf_arg` and `leaf_cell` remain active.
                        {
                            let mut run = loop {
                                match owner.run_access() {
                                    Ok(run) => break run,
                                    Err(RegistrationError::Busy) => std::thread::yield_now(),
                                    Err(err) => panic!("phase 2 run_access: {err:?}"),
                                }
                            };
                            let dst = nil_root_for_test(&mut run).unwrap();
                            assert!(matches!(
                                reg.run_scalar(
                                    leaf_const,
                                    adm,
                                    &mut run,
                                    &dst,
                                    ImmediateResult::Nil
                                ),
                                Err(NmjitRegistryError::RetiredOrMissing)
                            ));
                            retry_busy_registry(|| {
                                reg.run_scalar(leaf_arg, adm, &mut run, &dst, expected_arg)
                            })
                            .unwrap();
                            assert_eq!(ExecutionTestHomes::read(&dst, &run), expected_arg);
                            retry_busy_registry(|| {
                                reg.run_cell(leaf_cell, adm, &mut run, &dst, sym)
                            })
                            .unwrap();
                            assert_eq!(
                                ExecutionTestHomes::read(&dst, &run),
                                ImmediateResult::Fixnum(Fixnum::try_from(91).unwrap())
                            );
                            run.clear_root_slots().unwrap();
                        }
                        p2.send(idx).unwrap();
                        loop {
                            if ab.load(Ordering::Acquire) {
                                return;
                            }
                            match owner.poll(|| Ok(Vec::new())) {
                                Ok(PollOutcome::Continue)
                                | Err(neovm_value::tagged::gc::RendezvousError::Busy) => {
                                    std::thread::yield_now();
                                }
                                Ok(PollOutcome::Resumed { .. }) => break,
                                Err(err) => panic!("phase 2 poll: {err:?}"),
                            }
                        }

                        // Phase 3: `leaf_cell` retired & reclaimed in Stop #2;
                        // `leaf_arg` remains active and callable.
                        {
                            let mut run = loop {
                                match owner.run_access() {
                                    Ok(run) => break run,
                                    Err(RegistrationError::Busy) => std::thread::yield_now(),
                                    Err(err) => panic!("phase 3 run_access: {err:?}"),
                                }
                            };
                            let dst = nil_root_for_test(&mut run).unwrap();
                            assert!(matches!(
                                reg.run_cell(leaf_cell, adm, &mut run, &dst, sym),
                                Err(NmjitRegistryError::RetiredOrMissing)
                            ));
                            retry_busy_registry(|| {
                                reg.run_scalar(leaf_arg, adm, &mut run, &dst, expected_arg)
                            })
                            .unwrap();
                            assert_eq!(ExecutionTestHomes::read(&dst, &run), expected_arg);
                            run.clear_root_slots().unwrap();
                        }
                        loop {
                            match owner.retire_rooted(&[]) {
                                Ok(()) => break,
                                Err(failure)
                                    if failure.reason
                                        == neovm_value::tagged::gc::RendezvousError::Busy =>
                                {
                                    owner = failure.into_owner();
                                    std::thread::yield_now();
                                }
                                Err(failure) => panic!("retire_rooted: {failure:?}"),
                            }
                        }
                    }));
                }

                // Stop #1: retire `leaf_const`, stop all 4 owners, mint EraGraceReceipt, reclaim.
                for _ in 0..OWNERS {
                    p1_rx
                        .recv_timeout(Duration::from_secs(10))
                        .expect("phase 1 worker signal");
                }
                registry.retire(leaf_const).unwrap();
                assert_eq!(registry.stats().unwrap().retired, 1);
                let mut stop1 = era.stop().unwrap();
                let mut receipt1 = stop1.mint_grace_receipt().unwrap();
                let r1 = registry.reclaim_with_grace(&mut receipt1).unwrap();
                let _sym1 = receipt1.reclaim_symbol_tables().unwrap();
                drop(receipt1);
                stop1.trace(|_, _| Ok(())).unwrap();
                stop1.complete().unwrap();

                // Stop #2: retire `leaf_cell`, stop all 4 owners, mint EraGraceReceipt, reclaim twice.
                for _ in 0..OWNERS {
                    p2_rx
                        .recv_timeout(Duration::from_secs(10))
                        .expect("phase 2 worker signal");
                }
                registry.retire(leaf_cell).unwrap();
                let mut stop2 = era.stop().unwrap();
                let mut receipt2 = stop2.mint_grace_receipt().unwrap();
                let r2 = registry.reclaim_with_grace(&mut receipt2).unwrap();
                let r2_again = registry.reclaim_with_grace(&mut receipt2).unwrap();
                drop(receipt2);
                stop2.trace(|_, _| Ok(())).unwrap();
                stop2.complete().unwrap();

                for h in handles {
                    h.join().unwrap();
                }
                (r1, r2, r2_again)
            });

            assert_eq!(
                reclaim1,
                NmjitGraceReclaim {
                    reclaimed: 1,
                    remaining_active: 2
                }
            );
            assert_eq!(
                reclaim2,
                NmjitGraceReclaim {
                    reclaimed: 1,
                    remaining_active: 1
                }
            );
            assert_eq!(
                idempotent2,
                NmjitGraceReclaim {
                    reclaimed: 0,
                    remaining_active: 1
                }
            );
            let final_stats = registry.stats().unwrap();
            assert_eq!(final_stats.active, 1);
            assert_eq!(final_stats.retired, 0);
            assert_eq!(final_stats.active_calls, 0);

            drop(admission);
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
            drop(registry);
        },
    );
    assert!(outcome.is_ok(), "era setup failed: {outcome:?}");
}

#[test]
fn e7p1_nmjit_registry_refuses_wrong_heap_busy_and_retains_on_poison() {
    let _harness = crate::test_utils::unit_harness();
    let _gen1 = Gen1ModePin::new();
    Context::try_with_test_symbol_backing_init_era(
        PlainSymbolBacking::Unprojected,
        |_, _| {},
        |mut proof_a, _| {
            let frozen_a = take_legacy_roots_for_test(&mut proof_a).unwrap();
            let (collector_a, keeper_a) = proof_a.finish().unwrap();
            let admitted_a = Gen1Admission::check(collector_a).unwrap();
            let registry_a = NmjitCodeRegistry::new(&admitted_a);
            let compiled_a = if cfg!(feature = "jit") {
                let id1 = registry_a
                    .compile_scalar(&admitted_a, ScalarProgram::Argument)
                    .unwrap();
                let id2 = registry_a
                    .compile_scalar(&admitted_a, ScalarProgram::Constant(ImmediateResult::True))
                    .unwrap();
                registry_a.retire(id2).unwrap();
                Some((id1, id2))
            } else {
                None
            };
            let era_a = WorldEra::admit_rooted(admitted_a, keeper_a, frozen_a).unwrap();

            Context::try_with_test_symbol_backing_init_era(
                PlainSymbolBacking::Unprojected,
                |_, _| {},
                |mut proof_b, _| {
                    let frozen_b = take_legacy_roots_for_test(&mut proof_b).unwrap();
                    let (collector_b, keeper_b) = proof_b.finish().unwrap();
                    let admitted_b = Gen1Admission::check(collector_b).unwrap();
                    assert!(matches!(
                        registry_a.compile_scalar(&admitted_b, ScalarProgram::Argument),
                        Err(NmjitRegistryError::WrongHeap)
                    ));
                    let era_b = WorldEra::admit_rooted(admitted_b, keeper_b, frozen_b).unwrap();
                    let admission_b = era_b.execution_admission();
                    if let Some((id1, _)) = compiled_a {
                        let mut owner_b = era_b
                            .register(1, 4)
                            .unwrap()
                            .activate(era_b.heap_identity(), 1)
                            .unwrap();
                        {
                            let mut run_b = owner_b.run_access().unwrap();
                            let dst_b = nil_root_for_test(&mut run_b).unwrap();
                            assert!(matches!(
                                registry_a.run_scalar(
                                    id1,
                                    &admission_b,
                                    &mut run_b,
                                    &dst_b,
                                    ImmediateResult::Nil
                                ),
                                Err(NmjitRegistryError::WrongHeap)
                            ));
                            run_b.clear_root_slots().unwrap();
                        }
                        owner_b.retire_rooted(&[]).unwrap();
                    }
                    drop(admission_b);

                    let mut stop_b = era_b.stop().unwrap();
                    let mut receipt_b = stop_b.mint_grace_receipt().unwrap();
                    assert!(matches!(
                        registry_a.reclaim_with_grace(&mut receipt_b),
                        Err(NmjitRegistryError::WrongHeap)
                    ));
                    drop(receipt_b);
                    stop_b.trace(|_, _| Ok(())).unwrap();
                    stop_b.complete().unwrap();
                    let (collector_b, keeper_b) = era_b.finish().unwrap();
                    drop(collector_b);
                    drop(keeper_b);
                },
            )
            .unwrap();

            let mut stop_a = era_a.stop().unwrap();
            let mut receipt_a = stop_a.mint_grace_receipt().unwrap();
            let busy = registry_a
                .hold_active_call_for_test(|| registry_a.reclaim_with_grace(&mut receipt_a));
            assert!(matches!(busy, Err(NmjitRegistryError::Busy)));
            let mutex_busy = registry_a
                .hold_state_lock_for_test(|| registry_a.reclaim_with_grace(&mut receipt_a));
            assert!(matches!(mutex_busy, Err(NmjitRegistryError::Busy)));
            if compiled_a.is_some() {
                assert_eq!(registry_a.stats().unwrap().retired, 1);
            }

            registry_a.poison_for_test();
            assert!(matches!(
                registry_a.reclaim_with_grace(&mut receipt_a),
                Err(NmjitRegistryError::Poisoned)
            ));
            let (retained_active, retained_retired) =
                registry_a.retained_counts_ignoring_poison_for_test();
            if compiled_a.is_some() {
                assert_eq!((retained_active, retained_retired), (1, 1));
            } else {
                assert_eq!((retained_active, retained_retired), (0, 0));
            }

            drop(receipt_a);
            stop_a.trace(|_, _| Ok(())).unwrap();
            stop_a.complete().unwrap();
            let (collector_a, keeper_a) = era_a.finish().unwrap();
            drop(collector_a);
            drop(keeper_a);
        },
    )
    .unwrap();
}
