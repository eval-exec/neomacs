//! E4-P2 Slice 1 (C1) tests: `OwnerSlabHandle` and `OwnerSlabLease<'run, 'owner, 'era>`
//! stop-boundary exclusion, uniqueness, heap-identity validation, fail-closed
//! leak behavior, and real-thread barrier exclusion against `AllStopped`.

use crate::emacs_core::eval::owner_bindings::{
    BindingKind, Channel, OwnerSlabLeaseError, SlotState, SlotValue, UndoTarget,
};
use crate::emacs_core::eval::parallel_entry::owned_source::era_fixture::{
    KeeperPanicGuard, close, finish_fixture, keeper_serve, retire,
};
use crate::emacs_core::eval::world::{WorldProofEra, WorldProofEraExt};
use crate::emacs_core::eval::{Context, ThreadingMode};
use neovm_value::forward::TlsIndex;
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::gc::{PollOutcome, RendezvousError, StopAttempt};
use neovm_value::value::Value;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

fn finish_epoch() -> NonZeroU64 {
    NonZeroU64::new(1).expect("nonzero epoch")
}

fn fresh_context() -> Context {
    let mut ctx = Context::new_minimal_vm_harness(crate::test_utils::unit_runtime());
    ctx.setup_thread_locals();
    ctx
}

fn project_test_var(ctx: &mut Context, name: &str) -> (SymId, TlsIndex) {
    let sym = intern(name);
    ctx.set_variable(name, Value::fixnum(0));
    let tls = ctx
        .project_thread_variable_for_test(sym)
        .expect("project plain test variable")
        .index();
    (sym, tls)
}

fn open_proof_era<'h>(ctx: &'h mut Context) -> WorldProofEra<'h> {
    let keeper = ctx.issue_green_baton().expect("issue baton");
    let frozen = ctx
        .capture_parallel_legacy_roots(&keeper)
        .expect("capture frozen legacy roots");
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = ctx.tagged_heap.heap_root().as_ptr();
    // SAFETY: sole serial Context in test fixture; capture completed before
    // this heap loan, and `keeper` excludes concurrent mark.
    let collector = unsafe {
        (*heap).drain_to_quiescent(|heap| {
            ctx.seed_registered_mutator_roots_world_stopped(heap);
        })
    }
    .expect("drain to quiescent");
    WorldProofEra::admit_rooted(collector, keeper, frozen).expect("admit proof era")
}

#[test]
fn c1_owner_slab_handle_is_unique_and_refuses_second_extraction() {
    let mut ctx = fresh_context();
    let expected_heap = ctx.tagged_heap.heap_identity();
    let handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("first parallel handle extraction succeeds");
    assert_eq!(handle.heap_identity(), expected_heap);
    assert_eq!(
        ctx.take_parallel_owner_slab_handle().unwrap_err(),
        OwnerSlabLeaseError::HandleAlreadyIssued,
        "a second handle extraction on the same owner must refuse"
    );
}

#[test]
fn c1_owner_slab_lease_refuses_wrong_heap() {
    let mut ctx_a = fresh_context();
    let mut handle_a = ctx_a
        .take_parallel_owner_slab_handle()
        .expect("extract handle from world A");
    let _parked_a = ctx_a.park_for_test();

    let mut ctx_b = fresh_context();
    let heap_b = ctx_b.tagged_heap.heap_identity();
    assert_ne!(handle_a.heap_identity(), heap_b);

    let era_b = open_proof_era(&mut ctx_b);
    let permit = era_b.register(1, 8).expect("register owner on era B");
    let mut mutator = permit.activate(heap_b, 1).expect("activate owner on era B");
    {
        let mut run = mutator.run_access().expect("enter run slice on era B");
        let err = handle_a
            .lease(&mut run)
            .expect_err("lease against another heap's RunAccess must refuse");
        assert_eq!(err, OwnerSlabLeaseError::WrongHeap);
    }
    mutator.finish(finish_epoch()).expect("finish mutator");
    let (collector, keeper) = era_b.finish().expect("finish era B");
    drop(collector);
    drop(keeper);
}

#[test]
fn c1_owner_slab_lease_mutates_slab_and_survives_across_stretches() {
    let mut ctx = fresh_context();
    let heap = ctx.tagged_heap.heap_identity();
    let (sym, tls) = project_test_var(&mut ctx, "e4p2-c1-stretch-sym");
    let mut handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("extract handle");

    let era = open_proof_era(&mut ctx);
    let permit = era.register(1, 8).expect("register owner");
    let mut mutator = permit.activate(heap, 1).expect("activate owner");

    // Stretch 1: push a binding record and set slot value.
    let record_id = {
        let mut run = mutator.run_access().expect("run stretch 1");
        let mut lease = handle.lease(&mut run).expect("mint lease 1");
        assert_eq!(lease.heap_identity(), heap);
        assert_eq!(lease.run_access().heap_identity(), heap);
        assert_eq!(lease.run_access_mut().heap_identity(), heap);
        let (slab, _run_ref) = lease.parts_mut();
        let prev =
            slab.replace_default_slot(tls, SlotState::Bound(SlotValue::new(Value::fixnum(42))));
        assert!(matches!(prev, SlotState::NoBinding));
        slab.push_record(
            sym,
            tls,
            BindingKind::Let,
            Channel::Default,
            UndoTarget::Inherited { suspended: None },
        )
    };

    // Between stretches: poll cleanly with no lease live.
    assert_eq!(
        mutator
            .poll(|| Ok(Vec::new()))
            .expect("poll between stretches"),
        PollOutcome::Continue
    );

    // Stretch 2: observe the written value, update it, and pop the record.
    {
        let mut run = mutator.run_access().expect("run stretch 2");
        let mut lease = handle.lease(&mut run).expect("mint lease 2");
        let current = lease.slab().slot(tls, Channel::Default);
        assert_eq!(
            current.value().and_then(SlotValue::bound),
            Some(Value::fixnum(42))
        );
        let popped = lease
            .slab_mut()
            .take_top_record(record_id)
            .expect("pop top record");
        assert_eq!(popped.sym, sym);
        lease
            .slab_mut()
            .replace_default_slot(tls, SlotState::NoBinding);
    }

    mutator.finish(finish_epoch()).expect("finish mutator");
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);
}

#[test]
fn c1_owner_slab_lease_forgetting_lease_fails_closed() {
    let mut ctx = fresh_context();
    let heap = ctx.tagged_heap.heap_identity();
    let (sym, tls) = project_test_var(&mut ctx, "e4p2-c1-forget-sym");
    let mut handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("extract handle");

    let era = open_proof_era(&mut ctx);
    let permit = era.register(1, 8).expect("register owner");
    let mut mutator = permit.activate(heap, 1).expect("activate owner");

    {
        let mut run = mutator.run_access().expect("run stretch");
        let mut lease = handle.lease(&mut run).expect("mint lease");
        lease
            .slab_mut()
            .replace_default_slot(tls, SlotState::Bound(SlotValue::new(Value::fixnum(99))));
        lease.slab_mut().push_record(
            sym,
            tls,
            BindingKind::Let,
            Channel::Default,
            UndoTarget::Inherited { suspended: None },
        );
        // Deliberately leak the lease without dropping it.
        std::mem::forget(lease);
        // Re-leasing the same slab must fail closed (`ActiveLease`).
        assert_eq!(
            handle
                .lease(&mut run)
                .expect_err("forgotten lease fails closed"),
            OwnerSlabLeaseError::ActiveLease
        );
    }

    // Even after a forgotten lease, dropping `RunAccess` + entering native-stopped
    // + reaching `AllStopped` remains sound to trace because moving `lease` into
    // `mem::forget` ended all `&OwnerSlab`/`&mut OwnerSlab` references.
    mutator
        .enter_native(&[])
        .expect("enter native-stopped before same-thread stop");
    let mut traced = Vec::new();
    let mut stopped = era.stop().expect("stop after RunAccess dropped");
    era.collect_stopped(&mut stopped, |baton, stopped, out| {
        baton.trace_owner_slabs_stopped_for_test(stopped, &mut |v| {
            traced.push(v);
            out.push(v);
        })?;
        baton.collect_parallel_world_roots(stopped, out)
    })
    .expect("stopped collection after forgotten lease");
    stopped.complete().expect("complete stop");
    assert!(
        traced.contains(&Value::fixnum(99)),
        "stopped root walk observed the slab binding written before mem::forget(lease)"
    );

    mutator
        .leave_native()
        .expect("leave native-stopped before finish");
    retire(mutator);
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);
}

#[test]
#[should_panic(expected = "OwnerSlot accessed while a parallel OwnerSlabLease is active")]
fn c1_owner_slab_lease_active_forbids_serial_slot_access() {
    let mut ctx = fresh_context();
    let heap = ctx.tagged_heap.heap_identity();
    let mut handle = ctx
        .take_parallel_owner_slab_handle()
        .expect("extract handle");

    let era = open_proof_era(&mut ctx);
    let permit = era.register(1, 8).expect("register owner");
    let mut mutator = permit.activate(heap, 1).expect("activate owner");

    let mut run = mutator.run_access().expect("run stretch");
    let lease = handle.lease(&mut run).expect("mint active lease");
    // Forget the lease so `parallel_lease_active` stays pinned after the era
    // ends; this lets us invoke `ctx.owner.slab_mut()` in completely safe Rust
    // after `era`'s `&mut ctx` borrow ends, with zero raw-pointer aliasing.
    std::mem::forget(lease);
    drop(run);
    mutator.finish(finish_epoch()).expect("finish mutator");
    let (collector, keeper) = era.finish().expect("finish era");
    drop(collector);
    drop(keeper);

    // Safe serial access must panic BEFORE materializing `&mut OwnerSlab`.
    let _ = ctx.owner.slab_mut();
}

#[test]
fn c1_forgotten_run_access_pins_in_slice_and_blocks_all_stopped() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    {
        crate::emacs_core::jit::force_jit_off_for_test(true);
        crate::emacs_core::jit::cache::reset_carrier_tls();
    }
    crate::emacs_core::eval::parallel_entry::source_entry::install_inactive_source_module_probe_for_test();

    let mut source = Box::new(Context::new_with_threading(
        ThreadingMode::Green,
        crate::test_utils::unit_runtime(),
    ));
    source.setup_thread_locals();
    let heap = source.tagged_heap.heap_identity();
    let (_sym, tls) = project_test_var(&mut source, "e4p2-c1-forget-run-sym");
    source.gc_collect_exact();
    source.tagged_heap.close_alloc_regions();
    let bootstrap = source.capture_green_bootstrap_for_test();

    let (_shell, start) = close(source).prepare_era().expect("prepare parallel era");
    let outcome = keeper_serve(start, |keeper| {
        let _panic_guard = KeeperPanicGuard { keeper };
        keeper
            .keeper_baton_for_test()
            .install_stowed_green_runtime_for_test(bootstrap);
        let world_handle = keeper.keeper_baton_for_test().test_world_handle_for_test();
        let permit = keeper.register(1, 8).expect("register owner");

        let forgotten_barrier = Arc::new(Barrier::new(2));
        let checked_barrier = Arc::new(Barrier::new(2));

        std::thread::scope(|scope| {
            let forgotten_o = Arc::clone(&forgotten_barrier);
            let checked_o = Arc::clone(&checked_barrier);
            let owner_thread = scope.spawn(move || {
                let (mut owner_ctx, retention) = world_handle.spawn_seated_context_for_test(1);
                let mut handle = owner_ctx
                    .take_parallel_owner_slab_handle()
                    .expect("extract handle");
                let mut mutator = permit.activate(heap, 1).expect("activate owner");

                let mut run = mutator.run_access().expect("enter run stretch");
                {
                    let mut lease = handle.lease(&mut run).expect("mint lease");
                    lease.slab_mut().replace_default_slot(
                        tls,
                        SlotState::Bound(SlotValue::new(Value::fixnum(7))),
                    );
                }
                // Forget `RunAccess`: `AdmissionWord::IN_SLICE` stays pinned so
                // the keeper cannot reach `AllStopped`.
                std::mem::forget(run);
                assert!(
                    !mutator.admission_is_out_for_test(),
                    "forgotten RunAccess keeps IN_SLICE pinned"
                );
                forgotten_o.wait();
                checked_o.wait();

                // Dropping an unfinished mutator poisons the era cleanly.
                drop(mutator);
                drop(handle);
                drop(retention);
                owner_ctx.retire_green_owner();
            });

            forgotten_barrier.wait();
            match keeper.stop_within(Duration::from_millis(25)) {
                Ok(StopAttempt::Pending(_)) => {}
                other => panic!("forgotten RunAccess must block AllStopped, got: {other:?}"),
            }
            checked_barrier.wait();
            owner_thread.join().expect("join owner thread");
        });

        keeper
            .keeper_baton_for_test()
            .uninstall_green_runtime_for_test();
    });
    assert!(
        outcome.is_err(),
        "forgetting RunAccess and dropping unfinished Mutator poisons the era"
    );
}

/// Real two-thread barrier row (and TSan race check) for E4-P2 C1:
///
/// - Owner thread (originating OS thread) owns `OwnerSlabHandle` and activates
///   `Mutator`, minting `RunAccess` + `OwnerSlabLease` and mutating the slab.
/// - Keeper thread (real spawned OS thread) owns the `WorldProofEra` and
///   requests a stop WHILE the owner thread holds `OwnerSlabLease` and writes
///   to the slab.
/// - Barrier + atomic phase checks prove the keeper NEVER enters `AllStopped`
///   or calls `OwnerSlabRegistry::trace_roots_for_heap` until the owner drops
///   `OwnerSlabLease`, drops `RunAccess`, and ACKs the stop via `Mutator::poll`.
/// - Multi-round loop exercises repeated lease -> stop -> trace -> resume
///   handoffs across real OS threads under TSan with zero data races.
#[test]
fn c1_real_thread_stop_during_slab_lease_stretch_excludes_collector_until_drop() {
    const ROUNDS: usize = 8;

    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    {
        crate::emacs_core::jit::force_jit_off_for_test(true);
        crate::emacs_core::jit::cache::reset_carrier_tls();
    }
    crate::emacs_core::eval::parallel_entry::source_entry::install_inactive_source_module_probe_for_test();

    let mut source = Box::new(Context::new_with_threading(
        ThreadingMode::Green,
        crate::test_utils::unit_runtime(),
    ));
    source.setup_thread_locals();
    let heap = source.tagged_heap.heap_identity();
    let (sym, tls) = project_test_var(&mut source, "e4p2-c1-tsan-sym");
    let private = Value::string("e4p2-c1-tsan-private");
    source.lexenv = private;
    source.gc_collect_exact();
    source.tagged_heap.close_alloc_regions();
    let bootstrap = source.capture_green_bootstrap_for_test();

    let (shell, start) = close(source).prepare_era().expect("prepare parallel era");

    let keeper_traced_sum = Arc::new(AtomicUsize::new(0));
    let keeper_traced_sum_outer = Arc::clone(&keeper_traced_sum);

    let (end, ()) = keeper_serve(start, |keeper| {
        let _panic_guard = KeeperPanicGuard { keeper };
        keeper
            .keeper_baton_for_test()
            .install_stowed_green_runtime_for_test(bootstrap);
        let world_handle = keeper.keeper_baton_for_test().test_world_handle_for_test();
        let permit = keeper.register(1, 16).expect("register owner mutator");

        let lease_live_barrier = Arc::new(Barrier::new(2));
        let round_done_barrier = Arc::new(Barrier::new(2));
        let collector_entered = Arc::new(AtomicBool::new(false));

        std::thread::scope(|scope| {
            let lease_live_o = Arc::clone(&lease_live_barrier);
            let round_done_o = Arc::clone(&round_done_barrier);
            let collector_entered_o = Arc::clone(&collector_entered);

            let owner_thread = scope.spawn(move || {
                let (mut owner_ctx, retention) = world_handle.spawn_seated_context_for_test(1);
                let mut handle = owner_ctx
                    .take_parallel_owner_slab_handle()
                    .expect("extract unique owner slab handle on real owner thread");
                let mut mutator = permit
                    .activate(heap, 1)
                    .expect("activate mutator on real owner thread");

                for round in 0..ROUNDS {
                    collector_entered_o.store(false, Ordering::Release);

                    // Enter admitted RunAccess stretch and mint exclusive OwnerSlabLease.
                    let record_id = {
                        let mut run = mutator.run_access().expect("run_access for round");
                        let mut lease = handle.lease(&mut run).expect("mint OwnerSlabLease");

                        // Initial slab mutation before notifying keeper.
                        lease.slab_mut().replace_default_slot(
                            tls,
                            SlotState::Bound(SlotValue::new(Value::fixnum(
                                (round as i64 + 1) * 100,
                            ))),
                        );
                        let rec = lease.slab_mut().push_record(
                            sym,
                            tls,
                            BindingKind::Let,
                            Channel::Default,
                            UndoTarget::Inherited { suspended: None },
                        );

                        // Signal keeper to call `keeper.stop()` while this lease is STILL live.
                        lease_live_o.wait();

                        // Perform additional slab mutations while the keeper is concurrently
                        // requesting a stop. The collector MUST NOT enter AllStopped yet.
                        for step in 1..=7i64 {
                            assert!(
                                !collector_entered_o.load(Ordering::Acquire),
                                "round {round} step {step}: collector entered AllStopped while OwnerSlabLease was live!"
                            );
                            lease.slab_mut().replace_default_slot(
                                tls,
                                SlotState::Bound(SlotValue::new(Value::fixnum(
                                    (round as i64 + 1) * 100 + step,
                                ))),
                            );
                            if step == 3 {
                                std::thread::yield_now();
                            }
                        }
                        assert!(
                            !collector_entered_o.load(Ordering::Acquire),
                            "round {round}: collector entered AllStopped before OwnerSlabLease dropped!"
                        );
                        rec
                        // `lease` drops here, then `run` drops here (clearing IN_SLICE with Dekker fence).
                    };

                    // Now ACK the keeper's stop request via `Mutator::poll`.
                    let resumed_epoch = loop {
                        match mutator.poll(|| Ok(Vec::new())).expect("owner poll") {
                            PollOutcome::Resumed { epoch } => break epoch,
                            PollOutcome::Continue => std::thread::yield_now(),
                        }
                    };
                    assert!(resumed_epoch.get() > 0);
                    assert!(
                        collector_entered_o.load(Ordering::Acquire),
                        "round {round}: keeper must have traced under AllStopped before poll resumed"
                    );

                    // Wait for keeper to finish completing the stop epoch, then pop the record in
                    // a clean post-stop stretch.
                    round_done_o.wait();
                    {
                        let mut run = mutator.run_access().expect("post-stop run_access");
                        let mut lease = handle.lease(&mut run).expect("post-stop lease");
                        let popped = lease
                            .slab_mut()
                            .take_top_record(record_id)
                            .expect("pop round record");
                        assert_eq!(popped.sym, sym);
                        lease
                            .slab_mut()
                            .replace_default_slot(tls, SlotState::NoBinding);
                    }
                }

                retire(mutator);
                drop(handle);
                drop(retention);
                owner_ctx.retire_green_owner();
            });

            for round in 0..ROUNDS {
                // Wait until the owner thread is inside its active OwnerSlabLease stretch.
                lease_live_barrier.wait();

                // Request a stop while the owner is STILL holding OwnerSlabLease and mutating
                // the OwnerSlab. `keeper.stop()` MUST block until the owner drops
                // `OwnerSlabLease`, drops `RunAccess`, and calls `mutator.poll(...)`.
                let mut stopped = loop {
                    match keeper.stop() {
                        Ok(stopped) => break stopped,
                        Err(RendezvousError::Busy) => std::thread::yield_now(),
                        Err(other) => panic!("round {round}: keeper stop failed: {other:?}"),
                    }
                };
                collector_entered.store(true, Ordering::Release);

                let expected_val = Value::fixnum((round as i64 + 1) * 100 + 7);
                let mut saw_expected = false;
                keeper
                    .keeper_baton_for_test()
                    .trace_owner_slabs_stopped_for_test(&stopped, &mut |v| {
                        if v == expected_val {
                            saw_expected = true;
                        }
                    })
                    .expect("AllStopped slab trace");
                assert!(
                    saw_expected,
                    "round {round}: AllStopped slab trace must see the final write {expected_val:?}"
                );
                keeper_traced_sum.fetch_add(1, Ordering::AcqRel);

                stopped.trace_serial().expect("trace serial stop");
                stopped.complete().expect("complete stopped epoch");
                round_done_barrier.wait();
            }

            owner_thread.join().expect("join real owner thread");
        });

        keeper
            .keeper_baton_for_test()
            .uninstall_green_runtime_for_test();
    })
    .unwrap_or_else(|failure| panic!("keeper era refused: {:?}", failure.notice()));

    assert_eq!(keeper_traced_sum_outer.load(Ordering::Acquire), ROUNDS);
    let returned = shell.accept_return(end).expect("accept era return");
    finish_fixture(returned, private);
}
