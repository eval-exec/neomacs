//! E7-P.2 RED-first regression & invariant tests:
//! - D1: typed `CommandLoopBaseline` witness (refuses absent, nested, minibuffer, drifted specpdl)
//! - D2: mid-command `PendingUpgrade` fallback (first thread stays Baton, never runtime-upgraded;
//!       post-boundary thread admitted parallel; `--batch --eval` keeps E6-P.4 G4)
//! - D3: `Driver::take_stop` warn-once per stop epoch (`5s` production, `100ms` test, non-poisoning)
//! - Q-A / Q-B: compile-time quantum > slice >= floor invariants, burst-yield on `RingWaiter::raised()`,
//!       and pure-compute fairness slice releasing `WorldTurn` to `GroupTurn::Idle`
//! - D4 & zero-cost default: `Inactive` no-opt-in identity row + Main legacy JIT withdrawal while era open

use super::{
    ActivationRefusal, BATON_BURST_QUANTUM, EraOpenErrorClass, MIN_FAIRNESS_SLICE_FLOOR,
    PARALLEL_FAIRNESS_SLICE, ParallelActivationTag, classify_era_open_error,
};
use crate::emacs_core::eval::data::minibuffer_entry::{
    MinibufferManagerExt, RecursiveMinibufferPolicy,
};
use crate::emacs_core::eval::parallel_entry::driver_core::{
    Attempt, Batch, BatchVisitor, BreachOf, CustodyRefused, Driver, Head, KeeperMsg, KeeperOps,
    Refusal, Settle,
};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use crate::emacs_core::threads::{
    with_parallel_admission_disabled_for_test, with_parallel_admission_enabled_for_test,
};
use neovm_value::intern::intern;
use neovm_value::value::Value;
use std::hint::black_box;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn green_gen1_context() -> Context {
    let mut ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    // SAFETY: fresh serial test fixture before any concurrent activity.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut ctx.tagged_heap,
            true,
        );
    }
    ctx.setup_thread_locals();
    ctx.start_green_threads()
        .expect("start Green threads on Main");
    ctx
}

fn eval_printed(ctx: &mut Context, src: &str) -> String {
    let val = ctx
        .eval_str(src)
        .unwrap_or_else(|err| panic!("eval failed for `{src}`: {err:?}"));
    print_value(&val)
}

/// Atomically bind GNU's two outer `recursive_edit_1` baseline symbols and
/// simulate entering top-level interactive `run_exit_wrapped_command_loop`.
fn enter_simulated_top_level_command_loop(
    ctx: &mut Context,
) -> (usize, super::CommandLoopBaselineGuard) {
    ctx.set_variable("noninteractive", Value::NIL);
    ctx.command_loop_mut().running = true;
    ctx.command_loop_mut().recursive_depth = 1;
    let outer = ctx.specpdl().len();
    let guard = ctx
        .enter_command_loop_baseline_scope(outer, true)
        .expect("specbind canonical baseline symbols")
        .expect("D1: top-level command loop with baseline bindings must produce witness");
    (outer, guard)
}

/// Build a pure lexical bytecode loop computing sum(1..=n) using only
/// parallel-executor-safe arithmetic & stack opcodes (zero World turns).
fn make_pure_sum_bytecode() -> Value {
    use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
    use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
    let mut bc = ByteCodeFunction::new(neovm_value::value::LambdaParams {
        required: vec![intern("n")],
        optional: Vec::new(),
        rest: None,
    });
    bc.set_lexical(true);
    bc.set_ops(vec![
        Op::Constant(0),
        Op::StackRef(1),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(11),
        Op::StackRef(1),
        Op::Add,
        Op::StackRef(1),
        Op::Sub1,
        Op::StackSet(2),
        Op::Goto(1),
        Op::Return,
    ]);
    bc.set_constants(vec![Value::fixnum(0)].into());
    bc.set_max_stack(StackDepth::for_test(4));
    Value::make_bytecode(bc)
}

#[test]
fn e7p2_d1_command_loop_baseline_witness_refuses_absent_nested_and_minibuf() {
    let mut ctx = green_gen1_context();
    let control = ctx.green_control();

    // 1. Without a CommandLoopBaseline witness, PendingUpgrade MUST NOT activate.
    ctx.arm_pending_upgrade_for_test();
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::PendingUpgrade
    );
    ctx.parallel_between_commands_boundary()
        .expect("boundary without witness refuses cleanly");
    assert!(
        !control.era_is_open(),
        "D1: absent CommandLoopBaseline must refuse BetweenCommands activation"
    );
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::PendingUpgrade
    );

    // 2. Nested recursive-edit (recursive_depth == 2) and increment_depth == false
    //    must NOT produce a top-level witness or activate (D1 / HIGH #5).
    ctx.set_variable("noninteractive", Value::NIL);
    ctx.command_loop_mut().running = true;
    ctx.command_loop_mut().recursive_depth = 2;
    let nested_outer = ctx.specpdl().len();
    assert!(
        ctx.enter_command_loop_baseline_scope(nested_outer, true)
            .unwrap()
            .is_none(),
        "D1: nested recursive-edit must not mint top-level CommandLoopBaseline"
    );
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(!control.era_is_open());
    ctx.unbind_to(nested_outer);

    ctx.command_loop_mut().recursive_depth = 1;
    let no_inc_outer = ctx.specpdl().len();
    assert!(
        ctx.enter_command_loop_baseline_scope(no_inc_outer, false)
            .unwrap()
            .is_none(),
        "HIGH #5: increment_depth == false must not mint top-level CommandLoopBaseline"
    );
    ctx.unbind_to(no_inc_outer);

    // 3. Top-level witness present: nested depth, active minibuffer,
    //    noninteractive, executing_kbd_macro, active Lisp frame, and drifted
    //    specpdl must ALL defer PendingUpgrade cleanly (D1 / HIGH #3 / MED #6).
    let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

    // 3a. Nested recursive_depth == 2.
    ctx.command_loop_mut().recursive_depth = 2;
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        !control.era_is_open(),
        "D1: recursive_depth > 1 must defer PendingUpgrade"
    );
    ctx.command_loop_mut().recursive_depth = 1;

    // 3b. Active minibuffer (minibuffers().depth() == 1).
    let buf_id = ctx
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let permit = ctx
        .minibuffers()
        .prepare_entry(RecursiveMinibufferPolicy::Allow)
        .expect("minibuffer entry permit");
    let prompt = neovm_value::heap_types::LispString::from_utf8("M-x ");
    let _ = ctx
        .minibuffers_mut()
        .enter_with_permit(permit, buf_id, &prompt, None, None);
    assert_eq!(ctx.minibuffers().depth(), 1);
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        !control.era_is_open(),
        "D1/MED #6: active minibuffer (depth > 0) must defer PendingUpgrade"
    );
    let _ = ctx.minibuffers_mut().exit_minibuffer();
    assert_eq!(ctx.minibuffers().depth(), 0);

    // 3c. Noninteractive mode.
    ctx.set_variable("noninteractive", Value::T);
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        !control.era_is_open(),
        "D1/MED #6: noninteractive == t must defer PendingUpgrade"
    );
    ctx.set_variable("noninteractive", Value::NIL);

    // 3d. Active kbd-macro playback (HIGH #3).
    let kmacro_snap = ctx.snapshot_executing_kbd_macro_runtime();
    ctx.begin_executing_kbd_macro_runtime(vec![Value::fixnum('x' as i64)]);
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        !control.era_is_open(),
        "HIGH #3: active execute-kbd-macro must defer PendingUpgrade"
    );
    ctx.restore_executing_kbd_macro_runtime(kmacro_snap);

    // 3e. Active Lisp evaluation frame (depth > 0, HIGH #3).
    ctx.depth = 1;
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        !control.era_is_open(),
        "HIGH #3: active Lisp/VM frame (depth > 0) must defer PendingUpgrade"
    );
    ctx.depth = 0;

    // 3f. Drifted specpdl above baseline.
    let extra = ctx.specpdl().len();
    ctx.try_specbind_or_unwind_to(extra, intern("e7p2-d1-extra-var"), Value::T)
        .unwrap();
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        !control.era_is_open(),
        "D1: drifted specpdl above baseline must defer PendingUpgrade"
    );
    ctx.unbind_to(extra);

    // 4. Quiescent top-level with exact baseline activates ONCE.
    ctx.parallel_between_commands_boundary().unwrap();
    assert!(
        control.era_is_open(),
        "D1: quiescent top-level with valid CommandLoopBaseline must activate"
    );
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Active
    );

    drop(guard);
    ctx.unbind_to(outer);
    ctx.command_loop_mut().recursive_depth = 0;
    ctx.command_loop_mut().running = false;
    ctx.green_teardown();
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Inactive
    );
    assert!(!ctx.parallel_activation_fast_for_test());
    assert!(
        ctx.legacy_jit_withdrawn_for_test(),
        "P2: session-lifetime D4 withdrawal remains true across terminal green_teardown"
    );
}

#[test]
fn e7p2_d2_mid_command_pending_upgrade_stays_baton_then_post_boundary_is_parallel() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_gen1_context();
        let control = ctx.green_control();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Inactive
        );
        assert!(!control.era_is_open());

        // Mid-command #1: first `neomacs-make-parallel-thread` MUST fall back to
        // Baton (hint nil), NEVER open the era mid-command, and arm PendingUpgrade.
        let first = eval_printed(
            &mut ctx,
            "(progn
               (setq e7p2-first-m (make-mutex \"e7p2-first-m\"))
               (setq e7p2-first-c (make-condition-variable e7p2-first-m \"e7p2-first-c\"))
               (setq e7p2-first-ready nil)
               (setq e7p2-first-th
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (mutex-lock e7p2-first-m)
                        (setq e7p2-first-ready t)
                        (condition-wait e7p2-first-c)
                        (mutex-unlock e7p2-first-m)
                        111)
                      \"e7p2-first-mid-cmd\"))
               (while (null e7p2-first-ready) (thread-yield))
               (neomacs-thread-parallel-p e7p2-first-th))",
        );
        assert_eq!(
            first, "nil",
            "D2: first mid-command opt-in thread must stay Baton (hint nil)"
        );
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade,
            "D2: first mid-command opt-in must arm PendingUpgrade"
        );
        assert!(
            !control.era_is_open(),
            "D2: mid-command opt-in must not open era before BetweenCommands"
        );

        // Next quiescent top-level BetweenCommands boundary opens the era ONCE.
        ctx.parallel_between_commands_boundary()
            .expect("BetweenCommands activates PendingUpgrade");
        assert!(control.era_is_open(), "D2: BetweenCommands must open era");
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Active
        );
        assert!(
            ctx.legacy_jit_withdrawn_for_test(),
            "D4: legacy serial JIT must be withdrawn once era opens"
        );

        // First mid-command Baton thread MUST stay Baton even after era opens (never upgraded).
        let first_after = eval_printed(
            &mut ctx,
            "(progn
               (mutex-lock e7p2-first-m)
               (condition-notify e7p2-first-c)
               (mutex-unlock e7p2-first-m)
               (list (thread-join e7p2-first-th)
                     (neomacs-thread-parallel-p e7p2-first-th)))",
        );
        assert_eq!(
            first_after, "(111 nil)",
            "D2: first mid-command thread must NEVER be runtime-upgraded to parallel"
        );

        // Post-boundary command #2: subsequent opt-in thread IS admitted parallel!
        let second = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 200 22)) \"e7p2-second\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(
            second, "(222 t)",
            "D2: post-BetweenCommands opt-in thread must be admitted parallel (hint t)"
        );

        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
    });

    // Verify `--batch --eval` (no interactive CommandLoopBaseline) preserves E6-P.4 G4.
    with_parallel_admission_enabled_for_test(|| {
        let mut batch_ctx = green_gen1_context();
        let out = eval_printed(
            &mut batch_ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 40 2)) \"e7p2-batch-g4\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(
            out, "(42 t)",
            "D2: --batch --eval without CommandLoopBaseline must keep E6-P.4 G4 immediate admission"
        );
        batch_ctx.green_teardown();
    });
}

#[derive(Clone, Copy)]
struct SlowStopOps {
    pending_before_stop: &'static AtomicUsize,
    stop_calls: &'static AtomicUsize,
    group_queued: &'static AtomicUsize,
    next_epoch: &'static AtomicUsize,
}

struct DummyBatch {
    ops: SlowStopOps,
}
struct DummyHeld(SlowStopOps);
struct DummyWorldHeld(SlowStopOps);

impl Batch<SlowStopOps> for DummyBatch {
    type Ready = u64;
    type Grant = ();
    type Held = DummyHeld;
    type WorldHeld = DummyWorldHeld;

    fn head(&self) -> Result<Option<Head<u64>>, ()> {
        if self.ops.group_queued.load(Ordering::Relaxed) > 0 {
            Ok(Some(Head::BatonGroup))
        } else {
            Ok(None)
        }
    }
    fn ready(&self, receipt: u64) -> Result<Self::Ready, ()> {
        Ok(receipt)
    }
    fn grant_for(&mut self, _ready: &Self::Ready) -> Result<Option<Self::Grant>, ()> {
        Ok(Some(()))
    }
    fn grant_group(&mut self) -> Result<Option<Self::Grant>, ()> {
        if self.ops.group_queued.load(Ordering::Relaxed) > 0 {
            Ok(Some(()))
        } else {
            Ok(None)
        }
    }
    fn lend(self, _ready: Self::Ready) -> Result<((), Self::Held), CustodyRefused> {
        Ok(((), DummyHeld(self.ops)))
    }
    fn settle(held: Self::Held, _returned: (), _grant: Self::Grant) -> Result<Self, Settle<()>> {
        Ok(DummyBatch { ops: held.0 })
    }
    fn lend_world(self) -> Result<((), Self::WorldHeld), CustodyRefused> {
        Ok(((), DummyWorldHeld(self.ops)))
    }
    fn settle_world(
        held: Self::WorldHeld,
        _returned: (),
        _grant: Self::Grant,
    ) -> Result<Self, Settle<()>> {
        held.0.group_queued.store(0, Ordering::Relaxed);
        Ok(DummyBatch { ops: held.0 })
    }
    fn complete(self) -> Result<(), ()> {
        Ok(())
    }
}

impl KeeperOps for SlowStopOps {
    type Key = u64;
    type Receipt = u64;
    type Ticket = ();
    type Returned = ();
    type Reason = ();
    type WorldTurn = ();
    type WorldReturned = ();
    type Error = ();
    type Stopped = NonZeroU64;
    type Pending = u64;
    type Report = ();

    fn refusal(_error: ()) -> Refusal {
        Refusal::Other
    }
    fn owner(key: Self::Key) -> u64 {
        key
    }
    fn generation(receipt: &Self::Receipt) -> NonZeroU64 {
        NonZeroU64::new(*receipt).unwrap_or(NonZeroU64::MIN)
    }
    fn enqueue(self, _key: Self::Key, _generation: NonZeroU64) -> Result<(), ()> {
        Ok(())
    }
    fn cancel(self, _key: Self::Key) -> Result<(), ()> {
        Ok(())
    }
    fn enqueue_group(self) -> Result<(), ()> {
        self.group_queued.store(1, Ordering::Relaxed);
        Ok(())
    }
    fn cancel_group(self) -> Result<(), ()> {
        self.group_queued.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn requeue_group_at_head(self) -> Result<(), ()> {
        self.group_queued.store(1, Ordering::Relaxed);
        Ok(())
    }
    fn requeue_owner_at_head(self, _key: Self::Key, _generation: NonZeroU64) -> Result<(), ()> {
        Ok(())
    }
    fn stop_within(self, bound: Duration) -> Result<Attempt<Self::Stopped, Self::Pending>, ()> {
        self.stop_calls.fetch_add(1, Ordering::Relaxed);
        let remaining = self.pending_before_stop.load(Ordering::Relaxed);
        if remaining > 0 {
            self.pending_before_stop
                .store(remaining - 1, Ordering::Relaxed);
            std::thread::sleep(bound.min(Duration::from_millis(5)));
            Ok(Attempt::Pending(1))
        } else {
            let epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed) + 1;
            Ok(Attempt::Stopped(NonZeroU64::new(epoch as u64).unwrap()))
        }
    }
    fn gate_holder(self) -> Option<NonZeroU64> {
        None
    }
    fn poison(self) {}
    fn epoch(stopped: &Self::Stopped) -> NonZeroU64 {
        *stopped
    }
    fn trace(_stopped: &mut Self::Stopped) -> Result<(), ()> {
        Ok(())
    }
    fn should_collect(self, _stopped: &mut Self::Stopped) -> Result<bool, ()> {
        Ok(false)
    }
    fn collect_stopped(self, _stopped: &mut Self::Stopped) -> Result<(), ()> {
        Ok(())
    }
    fn complete_empty(_stopped: Self::Stopped) -> Result<(), ()> {
        Ok(())
    }
    fn ring_len(_stopped: &Self::Stopped) -> Result<usize, ()> {
        Ok(1)
    }
    fn with_batch<V: BatchVisitor<Self>>(
        self,
        _stopped: Self::Stopped,
        visitor: V,
    ) -> Result<V::Output, BreachOf<Self>> {
        visitor.visit(DummyBatch { ops: self })
    }
}

#[test]
fn e7p2_d3_driver_stop_overdue_warns_once_per_epoch_without_poisoning() {
    static PENDING: AtomicUsize = AtomicUsize::new(0);
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static GROUP_QUEUED: AtomicUsize = AtomicUsize::new(0);
    static NEXT_EPOCH: AtomicUsize = AtomicUsize::new(0);
    PENDING.store(0, Ordering::Relaxed);
    CALLS.store(0, Ordering::Relaxed);
    GROUP_QUEUED.store(0, Ordering::Relaxed);
    NEXT_EPOCH.store(0, Ordering::Relaxed);

    let ops = SlowStopOps {
        pending_before_stop: &PENDING,
        stop_calls: &CALLS,
        group_queued: &GROUP_QUEUED,
        next_epoch: &NEXT_EPOCH,
    };

    // 1. Production default `stop_bound` MUST be 5 seconds (D3).
    let (_tx_default, rx_default) = mpsc::channel();
    let default_driver = Driver::new(ops, rx_default);
    assert_eq!(
        default_driver.stop_bound_for_test(),
        Duration::from_secs(5),
        "D3: production Driver::stop_bound must be 5s"
    );

    // 2. Test bound (100ms): 3 overdue retries in Stop Epoch 1 + 2 overdue retries
    //    in Stop Epoch 2 must emit EXACTLY 1 warning per epoch (2 total), never poison.
    let (tx, rx) = mpsc::channel();
    let pending_observed = Arc::new(AtomicUsize::new(0));
    let pending_observed_in = Arc::clone(&pending_observed);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);

    let keeper = std::thread::spawn(move || {
        let mut driver = Driver::new(ops, rx).observe_pending(
            Duration::from_millis(100),
            move |_pending, _queued| {
                pending_observed_in.fetch_add(1, Ordering::Relaxed);
            },
        );
        let warnings = driver.overdue_warnings_counter();
        let link = driver.enroll_group();
        ready_tx.send((link, warnings)).unwrap();
        driver.run()
    });
    let (link, warnings) = ready_rx.recv().unwrap();
    let (world_rx, _waiter) = link.into_parts();

    // Stop Epoch 1: 3 pending attempts before stop completes.
    PENDING.store(3, Ordering::Relaxed);
    tx.send(KeeperMsg::BatonRequest).unwrap();
    let grant1 = world_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let epoch1 = match grant1 {
        crate::emacs_core::eval::parallel_entry::driver_core::GroupMsg::World { epoch, .. } => {
            epoch
        }
        crate::emacs_core::eval::parallel_entry::driver_core::GroupMsg::Poisoned => {
            panic!("expected World grant for epoch 1, got Poisoned")
        }
    };
    assert_eq!(
        warnings.load(Ordering::Relaxed),
        1,
        "D3: 3 pending retries in epoch 1 must emit exactly 1 warn-once"
    );
    assert_eq!(pending_observed.load(Ordering::Relaxed), 3);
    tx.send(KeeperMsg::WorldReturn {
        returned: (),
        end: crate::emacs_core::eval::parallel_entry::driver_core::IntervalEnd::Done,
    })
    .unwrap();
    assert!(epoch1 >= NonZeroU64::MIN);

    // Stop Epoch 2: 2 pending attempts before stop completes.
    PENDING.store(2, Ordering::Relaxed);
    tx.send(KeeperMsg::BatonRequest).unwrap();
    let grant2 = world_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let epoch2 = match grant2 {
        crate::emacs_core::eval::parallel_entry::driver_core::GroupMsg::World { epoch, .. } => {
            epoch
        }
        crate::emacs_core::eval::parallel_entry::driver_core::GroupMsg::Poisoned => {
            panic!("expected World grant for epoch 2, got Poisoned")
        }
    };
    assert_eq!(
        warnings.load(Ordering::Relaxed),
        2,
        "D3: second overdue stop epoch must warn once again (total 2)"
    );
    assert_eq!(pending_observed.load(Ordering::Relaxed), 5);
    tx.send(KeeperMsg::WorldReturn {
        returned: (),
        end: crate::emacs_core::eval::parallel_entry::driver_core::IntervalEnd::Done,
    })
    .unwrap();
    tx.send(KeeperMsg::GroupExit).unwrap();
    drop(tx);

    let end = keeper.join().expect("keeper thread clean");
    assert_eq!(
        end,
        crate::emacs_core::eval::parallel_entry::driver_core::DriverEnd::Drained,
        "D3: overdue stops must never poison the era"
    );
    assert!(epoch2 > NonZeroU64::MIN);
}

#[test]
fn e7p2_zero_cost_default_and_make_thread_identity() {
    // Single-thread (no Green started) identity row.
    let mut solo_ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    solo_ctx.setup_thread_locals();
    let (solo_outer, solo_guard) = enter_simulated_top_level_command_loop(&mut solo_ctx);
    for _ in 0..1000 {
        solo_ctx.parallel_between_commands_boundary().unwrap();
    }
    assert_eq!(
        solo_ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Inactive
    );
    assert!(!solo_ctx.legacy_jit_withdrawn_for_test());
    assert!(
        solo_ctx.green_seat().is_none(),
        "no-opt-in single-thread session must never start Green or spawn keeper"
    );
    drop(solo_guard);
    solo_ctx.unbind_to(solo_outer);
    drop(solo_ctx);

    // `make-thread`-only Green session identity row under default production
    // admission (`PRODUCTION_ADMISSION_ENABLED == true` on 77u) AND under
    // explicit admission-disabled mode.
    for force_disabled in [false, true] {
        let run_identity = || {
            let mut ctx = green_gen1_context();
            let control = ctx.green_control();
            control.reset_ring_waiter_loads_for_test();
            let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

            let out = eval_printed(
                &mut ctx,
                "(let ((th (make-thread (lambda () (+ 10 32)) \"ordinary-baton\")))
                   (list (thread-join th) (neomacs-thread-parallel-p th)))",
            );
            assert_eq!(out, "(42 nil)");
            for _ in 0..1000 {
                ctx.parallel_between_commands_boundary().unwrap();
            }
            assert_eq!(
                ctx.parallel_activation_tag_for_test(),
                ParallelActivationTag::Inactive
            );
            assert!(!control.era_is_open());
            assert!(!ctx.legacy_jit_withdrawn_for_test());
            assert_eq!(
                control.ring_waiter_loads_for_test(),
                0,
                "Q-A: Baton-only Inactive path must perform zero RingWaiter loads"
            );
            drop(guard);
            ctx.unbind_to(outer);
            ctx.command_loop_mut().recursive_depth = 0;
            ctx.command_loop_mut().running = false;
            ctx.green_teardown();
        };
        if force_disabled {
            with_parallel_admission_disabled_for_test(run_identity);
        } else {
            run_identity();
        }
    }
}

#[test]
fn e7p2_qb_compile_time_constants_and_burst_yield_unblocks_queued_owner() {
    assert!(BATON_BURST_QUANTUM > PARALLEL_FAIRNESS_SLICE);
    assert!(PARALLEL_FAIRNESS_SLICE >= MIN_FAIRNESS_SLICE_FLOOR);
    assert!(MIN_FAIRNESS_SLICE_FLOOR > Duration::ZERO);

    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_gen1_context();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        // Activate via D2: arm PendingUpgrade + BetweenCommands boundary.
        ctx.arm_pending_upgrade_for_test();
        ctx.parallel_between_commands_boundary().unwrap();
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Active
        );

        // Spawn an admitted parallel owner that requests a serial turn (modifying a
        // Lisp global variable), which raises `RingWaiter::raised() == true` while
        // Main holds `WorldTurn`.
        let hint = eval_printed(
            &mut ctx,
            "(progn
               (setq e7p2-burst-done nil)
               (setq e7p2-burst-th
                     (neomacs-make-parallel-thread
                      (lambda () (setq e7p2-burst-done 'unblocked) 77)
                      \"e7p2-burst-owner\"))
               (neomacs-thread-parallel-p e7p2-burst-th))",
        );
        assert_eq!(hint, "t");

        // Wait until the keeper enrolls the owner and its serial-turn request raises
        // RingWaiter (or the fairness slice lets it reach its serial turn), then verify
        // a non-blocking command burst via `parallel_between_commands_boundary()` unblocks
        // it without calling `thread-yield` or blocking in `read_key_sequence`.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut unblocked = false;
        while Instant::now() < deadline {
            ctx.parallel_between_commands_boundary().unwrap();
            if eval_printed(&mut ctx, "e7p2-burst-done") == "unblocked" {
                unblocked = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            unblocked,
            "Q-A/Q-B: BetweenCommands boundary must unblock queued parallel owner during non-blocking burst"
        );
        let (burst_yields, fairness_releases) = ctx
            .between_commands_counters_for_test()
            .expect("Active activation counters");
        assert!(
            burst_yields + fairness_releases >= 1,
            "expected at least one BetweenCommands yield/fairness release (burst={burst_yields}, fairness={fairness_releases})"
        );
        assert_eq!(eval_printed(&mut ctx, "(thread-join e7p2-burst-th)"), "77");

        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
    });
}

/// Q-B + HIGH #4 + MEDIUM #6 + B2: During a saturated Main command burst with a
/// live quiet parallel owner (`parallel_count == 1`, `RingWaiter == false`) AND
/// a constantly runnable sibling cooperative Baton `make-thread`, Main MUST still
/// release `WorldTurn` to `GroupTurn::Idle` (advancing `idle_epoch` and spending
/// real measured wall-clock time in `PARALLEL_FAIRNESS_SLICE`), and after the
/// parallel owner completes normally and is joined `Control::parallel_count` MUST
/// drop to 0 with zero further fairness releases.
///
/// Note: in E6-P.4 (`live_admission.rs:1167`), admitted Lisp owners run Lisp
/// inside `LiveSerial` and sit in `Phase::Idle` with `RingWaiter == false` while
/// parked (E8-P moves pure-compute bytecode out of `LiveSerial`); parking the
/// owner after startup is the exact E7-P.2 state where `parallel_count == 1`
/// and `RingWaiter == false`.
#[test]
fn e7p2_qb_pure_compute_fairness_slice_unblocks_cpu_bound_owner_during_burst() {
    let mut ctx = green_gen1_context();
    let control = ctx.green_control();
    let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

    ctx.arm_pending_upgrade_for_test();
    ctx.parallel_between_commands_boundary().unwrap();
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Active
    );

    // Spawn an admitted parallel owner that parks quietly in `Phase::Idle`
    // (`parallel_count == 1`, `RingWaiter == false`), AND a constantly runnable
    // sibling cooperative Baton `make-thread` (HIGH #4).
    let setup = eval_printed(
        &mut ctx,
        "(progn
           (setq e7p2-cpu-m (make-mutex \"e7p2-cpu-m\"))
           (setq e7p2-cpu-c (make-condition-variable e7p2-cpu-m \"e7p2-cpu-c\"))
           (setq e7p2-cpu-ready nil)
           (setq e7p2-cpu-th
                 (neomacs-make-parallel-thread
                  (lambda ()
                    (mutex-lock e7p2-cpu-m)
                    (setq e7p2-cpu-ready t)
                    (condition-wait e7p2-cpu-c)
                    (mutex-unlock e7p2-cpu-m)
                    1999000)
                  \"e7p2-pure-cpu\"))
           (while (null e7p2-cpu-ready) (thread-yield))
           (setq e7p2-h4-run t)
           (setq e7p2-h4-ticks 0)
           (setq e7p2-h4-baton
                 (make-thread
                  (lambda ()
                    (while e7p2-h4-run
                      (setq e7p2-h4-ticks (1+ e7p2-h4-ticks))
                      (thread-yield))
                    'baton-done)
                  \"e7p2-h4-baton-sibling\"))
           (neomacs-thread-parallel-p e7p2-cpu-th))",
    );
    assert_eq!(setup, "t");
    let ticks_before: i64 = eval_printed(&mut ctx, "e7p2-h4-ticks")
        .parse()
        .expect("numeric e7p2-h4-ticks before burst");
    assert_eq!(
        control.live_parallel_participant_count_fast(),
        1,
        "parallel owner must remain live during the saturated burst"
    );
    assert!(
        !control.era_ring_waiter_raised_fast(),
        "RingWaiter must be false so FairnessSlice (not BurstYield) fires"
    );

    ctx.set_fairness_overrides_for_test(Some(BATON_BURST_QUANTUM), Some(PARALLEL_FAIRNESS_SLICE));
    let (stale_waiter, idle_epoch, _pcount) = control
        .era_between_commands_handles()
        .expect("open era handles");
    let idle_before = idle_epoch.load(Ordering::Relaxed);
    let (burst_before, fairness_before) = ctx
        .between_commands_counters_for_test()
        .expect("Active activation counters");

    // Hold Main in a saturated non-blocking burst (zero thread-yield, zero
    // blocking input wait) while the runnable sibling Baton thread is present.
    let mut prev_f = fairness_before;
    let mut measured_yield_ns = 0u128;
    let burst_start = Instant::now();
    // Loop until at least 2 fairness slices have fired AND the burst has run its
    // 25ms floor, bounded by a 1s ceiling. A fixed 25ms wall-clock window makes
    // the slice count depend on how many CPU cycles the runner grants Main: under
    // a saturated CI runner only 1 slice fired in 25ms even though the mechanism
    // was correct (the same load-sensitive class as the d4 duty-cycle probe fixed
    // in 35d7350f84). The 25ms floor still guarantees a saturated burst; the 1s
    // ceiling makes a real mechanism regression fail fast rather than hang. The
    // idle-epoch and MEDIUM #6 checks below still tie every fire to a WorldTurn
    // release, so widening the window does not weaken what is asserted.
    loop {
        let call_t0 = Instant::now();
        ctx.parallel_between_commands_boundary().unwrap();
        let call_dt = call_t0.elapsed().as_nanos();
        let (_b, cur_f) = ctx
            .between_commands_counters_for_test()
            .expect("Active activation counters");
        if cur_f > prev_f {
            measured_yield_ns += call_dt;
            prev_f = cur_f;
        }
        let fired = prev_f - fairness_before;
        if (fired >= 2 && burst_start.elapsed() >= Duration::from_millis(25))
            || burst_start.elapsed() >= Duration::from_secs(1)
        {
            break;
        }
    }
    let burst_ms = burst_start.elapsed().as_secs_f64() * 1000.0;
    let (burst_after, fairness_after) = ctx
        .between_commands_counters_for_test()
        .expect("Active activation counters");
    let fires = fairness_after - fairness_before;
    assert!(
        fires >= 2,
        "Q-B / HIGH #4: >= 2 fairness slices must fire even with a runnable Baton sibling (fires={fires} in {burst_ms:.1}ms)"
    );
    assert_eq!(
        burst_after, burst_before,
        "Q-B: quiet burst must trigger FairnessSlice rather than RingWaiter BurstYield"
    );
    let idle_after = idle_epoch.load(Ordering::Relaxed);
    assert!(
        idle_after >= idle_before + fires as u64,
        "Q-B / HIGH #4: every FairnessSlice must release WorldTurn to GroupTurn::Idle even with a runnable Baton sibling (before={idle_before}, after={idle_after}, fires={fires})"
    );
    let measured_yield_ms = measured_yield_ns as f64 / 1_000_000.0;
    assert!(
        measured_yield_ms >= 1.5,
        "MEDIUM #6: empirical wall-clock yield time across {fires} slices must be >= 1.5ms (got {measured_yield_ms:.2}ms)"
    );
    let ticks_after: i64 = eval_printed(&mut ctx, "e7p2-h4-ticks")
        .parse()
        .expect("numeric e7p2-h4-ticks after burst");
    assert!(
        ticks_after > ticks_before,
        "HIGH #4 / review #4.2: runnable sibling Baton worker must make forward progress during the burst before join (before={ticks_before}, after={ticks_after})"
    );

    // Wake and join both the parallel owner and the runnable Baton sibling.
    assert_eq!(
        eval_printed(
            &mut ctx,
            "(progn
               (setq e7p2-h4-run nil)
               (mutex-lock e7p2-cpu-m)
               (condition-notify e7p2-cpu-c)
               (mutex-unlock e7p2-cpu-m)
               (list (thread-join e7p2-cpu-th) (thread-join e7p2-h4-baton)))"
        ),
        "(1999000 baton-done)"
    );

    // B2 regression check: after normal parallel owner completion + join,
    // `live_parallel_participant_count_fast()` MUST be 0 and an 8ms burst MUST
    // fire ZERO spurious fairness slices.
    assert_eq!(
        control.live_parallel_participant_count_fast(),
        0,
        "B2: publish_parallel_owner_exit must decrement Control::parallel_count to 0"
    );
    ctx.set_fairness_overrides_for_test(Some(BATON_BURST_QUANTUM), Some(PARALLEL_FAIRNESS_SLICE));
    let (_b2_b0, b2_f0) = ctx.between_commands_counters_for_test().unwrap();
    let b2_start = Instant::now();
    while b2_start.elapsed() < Duration::from_millis(8) {
        ctx.parallel_between_commands_boundary().unwrap();
    }
    let (_b2_b1, b2_f1) = ctx.between_commands_counters_for_test().unwrap();
    assert_eq!(
        b2_f1, b2_f0,
        "B2: zero fairness releases may fire after all parallel owners exited (before={b2_f0}, after={b2_f1})"
    );

    drop(guard);
    ctx.unbind_to(outer);
    ctx.command_loop_mut().recursive_depth = 0;
    ctx.command_loop_mut().running = false;
    ctx.green_teardown();
    // Review #2: `close_era` marks `era_waiter_reused = true` so
    // `era_ring_waiter_raised_fast()` cannot observe a closed era's stale waiter.
    stale_waiter.set_for_test(true);
    assert!(
        !control.era_ring_waiter_raised_fast(),
        "review #2: era_ring_waiter_raised_fast must return false after close_era even if stale waiter is raised"
    );
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Inactive
    );
    assert!(!ctx.parallel_activation_fast_for_test());
    assert!(
        ctx.legacy_jit_withdrawn_for_test(),
        "P2: session-lifetime D4 withdrawal remains true across terminal green_teardown"
    );
}

#[cfg(feature = "jit")]
fn reset_legacy_jit_for_d4_test() {
    use crate::emacs_core::jit;
    jit::force_jit_off_for_test(false);
    jit::stats::force_observe_for_test(jit::stats::ObserveOverride {
        stats: true,
        naming: false,
        entry_count: false,
    });
    let _ = jit::cache::clear_with_serial_receipt();
    jit::compile::shared::reset_carrier_tls();
    jit::stats::reset_compile_stats();
}

#[test]
fn e7p2_d4_legacy_jit_withdrawn_and_baton_window_exclusion() {
    with_parallel_admission_enabled_for_test(|| {
        #[cfg(feature = "jit")]
        reset_legacy_jit_for_d4_test();

        let mut ctx = green_gen1_context();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        #[cfg(feature = "jit")]
        let warm_calls = crate::emacs_core::jit::hot_threshold() as i64 + 200;
        #[cfg(not(feature = "jit"))]
        let warm_calls = 60i64;

        ctx.obarray
            .set_symbol_function("e7p2-hot-fn", make_pure_sum_bytecode());
        ctx.obarray
            .set_symbol_function("e7p2-hot-fn-post", make_pure_sum_bytecode());

        // Warm the hot bytecode function past `hot_threshold()` BEFORE era open
        // (default `Inactive` path) so legacy serial JIT compiles and enters natively.
        let before = eval_printed(
            &mut ctx,
            &format!(
                "(let ((k 0) (r 0))
                   (while (< k {warm_calls})
                     (setq r (e7p2-hot-fn 50))
                     (setq k (1+ k)))
                   r)"
            ),
        );
        assert_eq!(before, "1275");
        assert!(!ctx.legacy_jit_withdrawn_for_test());
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit;
            let pre_stats = jit::stats::compile_stats_snapshot();
            assert!(
                pre_stats.total_compiles >= 1,
                "D4 pre-era warm-up must compile via legacy JIT (got {pre_stats:?})"
            );
            assert!(
                pre_stats.native_entries >= 1,
                "D4 pre-era warm-up must enter legacy native code (got {pre_stats:?})"
            );
            assert!(
                jit::cache::compiled_entry_count_for_test() > 0,
                "D4 pre-era warm-up must populate legacy COMPILED cache"
            );
            assert!(
                jit::compile::shared::shared_jit_is_installed_for_test(),
                "D4 pre-era warm-up must install legacy SHARED_JIT"
            );
        }

        // Activate parallel era at BetweenCommands: withdraws legacy serial JIT.
        ctx.arm_pending_upgrade_for_test();
        ctx.parallel_between_commands_boundary().unwrap();
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Active
        );
        assert!(
            ctx.legacy_jit_withdrawn_for_test(),
            "D4: legacy serial JIT must be withdrawn for the open era's lifetime"
        );
        #[cfg(feature = "jit")]
        let snap_after_open = {
            use crate::emacs_core::jit;
            assert_eq!(
                jit::cache::compiled_entry_count_for_test(),
                0,
                "D4: legacy COMPILED cache must be empty immediately after era open"
            );
            assert_eq!(
                jit::cache::osr_entry_count_for_test(),
                0,
                "D4: legacy OSR cache must be empty immediately after era open"
            );
            assert!(
                !jit::compile::shared::shared_jit_is_installed_for_test(),
                "D4: legacy SHARED_JIT must be None immediately after era open"
            );
            jit::stats::compile_stats_snapshot()
        };

        // Keep an admitted parallel owner live while Main executes both the
        // previously-hot and a fresh cold bytecode function past `hot_threshold()`:
        // results match identically and ZERO new compiles / native entries occur.
        let after = eval_printed(
            &mut ctx,
            &format!(
                "(progn
                   (setq e7p2-d4-m (make-mutex \"e7p2-d4-m\"))
                   (setq e7p2-d4-c (make-condition-variable e7p2-d4-m \"e7p2-d4-c\"))
                   (setq e7p2-d4-ready nil)
                   (setq e7p2-d4-th
                         (neomacs-make-parallel-thread
                          (lambda ()
                            (mutex-lock e7p2-d4-m)
                            (setq e7p2-d4-ready t)
                            (condition-wait e7p2-d4-c)
                            (mutex-unlock e7p2-d4-m)
                            'done)
                          \"e7p2-d4-owner\"))
                   (while (null e7p2-d4-ready) (thread-yield))
                   (let ((k 0) (r1 0) (r2 0))
                     (while (< k {warm_calls})
                       (setq r1 (e7p2-hot-fn 50))
                       (setq r2 (e7p2-hot-fn-post 50))
                       (setq k (1+ k)))
                     (mutex-lock e7p2-d4-m)
                     (condition-notify e7p2-d4-c)
                     (mutex-unlock e7p2-d4-m)
                     (list r1 r2 (thread-join e7p2-d4-th) (neomacs-thread-parallel-p e7p2-d4-th))))"
            ),
        );
        assert_eq!(after, "(1275 1275 done t)");
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit;
            let snap_after_post = jit::stats::compile_stats_snapshot();
            assert_eq!(
                snap_after_post.total_compiles, snap_after_open.total_compiles,
                "D4: ZERO legacy JIT compiles allowed after era open (before={snap_after_open:?}, after={snap_after_post:?})"
            );
            assert_eq!(
                snap_after_post.native_entries, snap_after_open.native_entries,
                "D4: ZERO legacy native entries allowed after era open (before={snap_after_open:?}, after={snap_after_post:?})"
            );
            assert_eq!(
                jit::cache::compiled_entry_count_for_test(),
                0,
                "D4: legacy COMPILED cache must remain empty after post-era calls"
            );
            assert_eq!(
                jit::cache::osr_entry_count_for_test(),
                0,
                "D4: legacy OSR cache must remain empty after post-era calls"
            );
            assert!(
                !jit::compile::shared::shared_jit_is_installed_for_test(),
                "D4: legacy SHARED_JIT must remain None after post-era calls"
            );
            assert_eq!(
                jit::cache::native_depth(),
                0,
                "D4: zero legacy serial JIT frames may remain active during an open parallel era"
            );
        }

        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
    });
}

/// Paired D4 (Main interpreter-vs-legacy-JIT delta) and Q-B (`Inactive` vs
/// `Active(count==0)` vs `Active(count>0)` boundary cost + 4:1 burst duty cycle)
/// measurement probe (`E7P-DESIGN.md` §1.6, §1.7, §4 D4).
#[test]
fn e7p2_d4_and_qb_measurement_probe() {
    with_parallel_admission_enabled_for_test(|| {
        #[cfg(feature = "jit")]
        reset_legacy_jit_for_d4_test();

        let mut ctx = green_gen1_context();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        // 1. Zero-cost `Inactive` boundary overhead (100_000 iterations,
        //    `black_box` prevents LLVM loop-invariant DCE — MEDIUM #6).
        let inactive_iters = 100_000u64;
        let t0 = Instant::now();
        for _ in 0..inactive_iters {
            let res = black_box(&mut ctx)
                .parallel_between_commands_boundary()
                .unwrap();
            black_box(res);
        }
        let inactive_ns = t0.elapsed().as_nanos() as f64 / inactive_iters as f64;

        // 2. Build a representative hot bytecode command: sum(1..=500).
        ctx.obarray
            .set_symbol_function("e7p2-bench-sum", make_pure_sum_bytecode());

        // Warm up legacy serial JIT past `hot_threshold()` on default `Inactive`
        // path and prove it compiled & entered natively before timing steady-state.
        #[cfg(feature = "jit")]
        let warm_iters = (crate::emacs_core::jit::hot_threshold() as u64).max(500) + 200;
        #[cfg(not(feature = "jit"))]
        let warm_iters = 500u64;
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((k 0) (r 0)) (while (< k {warm_iters}) (setq r (e7p2-bench-sum 500)) (setq k (1+ k))) r)"
                )
            ),
            "125250"
        );
        #[cfg(feature = "jit")]
        let pre_measure_stats = {
            use crate::emacs_core::jit;
            let s = jit::stats::compile_stats_snapshot();
            assert!(
                s.total_compiles >= 1 && s.native_entries >= 1,
                "D4 probe: pre-era warm-up must compile and enter legacy JIT (got {s:?})"
            );
            assert!(
                jit::cache::compiled_entry_count_for_test() > 0
                    && jit::compile::shared::shared_jit_is_installed_for_test(),
                "D4 probe: legacy JIT cache and backend must be live pre-era"
            );
            s
        };
        let cmd_iters = 500u64;
        let t_jit = Instant::now();
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((k 0) (r 0)) (while (< k {cmd_iters}) (setq r (e7p2-bench-sum 500)) (setq k (1+ k))) r)"
                )
            ),
            "125250"
        );
        let jit_us_per_call = t_jit.elapsed().as_nanos() as f64 / (cmd_iters as f64 * 1000.0);
        #[cfg(feature = "jit")]
        let jit_measured_native_entries = {
            let s = crate::emacs_core::jit::stats::compile_stats_snapshot();
            let delta = s.native_entries - pre_measure_stats.native_entries;
            assert!(
                delta >= cmd_iters,
                "D4 probe: pre-era timed window must actually enter native JIT (expected >= {cmd_iters}, got {delta})"
            );
            delta
        };
        #[cfg(not(feature = "jit"))]
        let jit_measured_native_entries = 0u64;

        // Activate parallel era via BetweenCommands (withdraws legacy serial JIT).
        ctx.arm_pending_upgrade_for_test();
        ctx.parallel_between_commands_boundary().unwrap();
        assert!(ctx.legacy_jit_withdrawn_for_test());
        #[cfg(feature = "jit")]
        let interp_before_stats = {
            use crate::emacs_core::jit;
            assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
            assert_eq!(jit::cache::osr_entry_count_for_test(), 0);
            assert!(!jit::compile::shared::shared_jit_is_installed_for_test());
            jit::stats::compile_stats_snapshot()
        };

        // Measure Main command after legacy JIT withdrawal (enforced interpreter-only).
        let t_interp = Instant::now();
        assert_eq!(
            eval_printed(
                &mut ctx,
                &format!(
                    "(let ((k 0) (r 0)) (while (< k {cmd_iters}) (setq r (e7p2-bench-sum 500)) (setq k (1+ k))) r)"
                )
            ),
            "125250"
        );
        let interp_us_per_call = t_interp.elapsed().as_nanos() as f64 / (cmd_iters as f64 * 1000.0);
        #[cfg(feature = "jit")]
        let (interp_compiles_delta, interp_native_delta) = {
            use crate::emacs_core::jit;
            let s = jit::stats::compile_stats_snapshot();
            let cd = s.total_compiles - interp_before_stats.total_compiles;
            let nd = s.native_entries - interp_before_stats.native_entries;
            assert_eq!(cd, 0, "D4 probe: ZERO compiles allowed post-era");
            assert_eq!(nd, 0, "D4 probe: ZERO native entries allowed post-era");
            assert_eq!(jit::cache::compiled_entry_count_for_test(), 0);
            assert_eq!(jit::cache::osr_entry_count_for_test(), 0);
            assert!(!jit::compile::shared::shared_jit_is_installed_for_test());
            (cd, nd)
        };
        #[cfg(not(feature = "jit"))]
        let (interp_compiles_delta, interp_native_delta) = (0u64, 0u64);

        // 3. Active `count == 0` lock-free Q-A boundary cost (1 Acquire load on
        //    RingWaiter + 1 Acquire load on parallel_count, zero mutex locks,
        //    zero clock read; `black_box` prevents DCE).
        let active0_iters = 100_000u64;
        let t_a0 = Instant::now();
        for _ in 0..active0_iters {
            let res = black_box(&mut ctx)
                .parallel_between_commands_boundary()
                .unwrap();
            black_box(res);
        }
        let active0_ns = t_a0.elapsed().as_nanos() as f64 / active0_iters as f64;

        // 4. Active `count > 0` clock-read cost + empirically measured burst
        //    duty cycle with a live quiet parallel owner (`RingWaiter == false`).
        let setup = eval_printed(
            &mut ctx,
            "(progn
               (setq e7p2-m-m (make-mutex \"e7p2-m-m\"))
               (setq e7p2-m-c (make-condition-variable e7p2-m-m \"e7p2-m-c\"))
               (setq e7p2-m-ready nil)
               (setq e7p2-m-th
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (mutex-lock e7p2-m-m)
                        (setq e7p2-m-ready t)
                        (condition-wait e7p2-m-c)
                        (mutex-unlock e7p2-m-m)
                        0)
                      \"e7p2-measure-owner\"))
               (while (null e7p2-m-ready) (thread-yield))
               (neomacs-thread-parallel-p e7p2-m-th))",
        );
        assert_eq!(setup, "t");
        // Measure short non-yielding Active(count>0) clock-read overhead (< 4ms quantum).
        ctx.set_fairness_overrides_for_test(
            Some(Duration::from_secs(10)),
            Some(PARALLEL_FAIRNESS_SLICE),
        );
        let active1_iters = 20_000u64;
        let t_a1 = Instant::now();
        for _ in 0..active1_iters {
            let res = black_box(&mut ctx)
                .parallel_between_commands_boundary()
                .unwrap();
            black_box(res);
        }
        let active1_ns = t_a1.elapsed().as_nanos() as f64 / active1_iters as f64;

        // Restore real 4ms / 1ms knobs and measure the saturated-burst duty
        // cycle.  The burst is bounded by the fairness slices it OBSERVES, not
        // by a fixed wall-clock window: under full-suite load this thread can be
        // descheduled for most of any fixed window, and a slice count asserted
        // against wall clock then measures the host scheduler instead of the
        // fairness mechanism (77z-r2 gate: 1 slice in 25 ms on both attempts, on
        // a tree whose only changes were display-input ones).  The 25 ms floor
        // keeps the duty-cycle percentage describing a saturated burst; the 1 s
        // ceiling keeps a real mechanism regression failing fast.
        ctx.set_fairness_overrides_for_test(
            Some(BATON_BURST_QUANTUM),
            Some(PARALLEL_FAIRNESS_SLICE),
        );
        let (_b0, mut prev_f) = ctx.between_commands_counters_for_test().unwrap();
        let f0 = prev_f;
        let duty_start = Instant::now();
        let mut duty_iters = 0u64;
        let mut measured_yield_ns = 0u128;
        while duty_start.elapsed() < Duration::from_secs(1) {
            let call_t0 = Instant::now();
            ctx.parallel_between_commands_boundary().unwrap();
            let call_dt = call_t0.elapsed().as_nanos();
            let (_b, cur_f) = ctx.between_commands_counters_for_test().unwrap();
            if cur_f > prev_f {
                measured_yield_ns += call_dt;
                prev_f = cur_f;
            }
            duty_iters += 1;
            if prev_f - f0 >= 4 && duty_start.elapsed() >= Duration::from_millis(25) {
                break;
            }
        }
        let duty_ms = duty_start.elapsed().as_secs_f64() * 1000.0;
        let fairness_fires = prev_f - f0;
        assert!(
            fairness_fires >= 2,
            "expected >= 2 fairness slices while a live parallel owner waited \
             (got {fairness_fires} in {duty_ms:.1} ms / {duty_iters} boundary calls)"
        );
        let measured_yield_ms = measured_yield_ns as f64 / 1_000_000.0;
        let measured_main_pct = ((duty_ms - measured_yield_ms) / duty_ms).clamp(0.0, 1.0) * 100.0;

        eprintln!(
            "E7P2_MEASURE d4_jit_us_per_call={jit_us_per_call:.2} d4_interp_us_per_call={interp_us_per_call:.2} d4_ratio={:.2}x d4_jit_native_entries={jit_measured_native_entries} d4_interp_compiles={interp_compiles_delta} d4_interp_native_entries={interp_native_delta}",
            interp_us_per_call / jit_us_per_call.max(0.001)
        );
        eprintln!(
            "E7P2_MEASURE qb_inactive_ns={inactive_ns:.2} qb_active_count0_ns={active0_ns:.2} qb_active_count1_ns={active1_ns:.2}"
        );
        eprintln!(
            "E7P2_MEASURE qb_duty_window_ms={duty_ms:.2} qb_duty_iters={duty_iters} qb_fairness_fires={fairness_fires} qb_measured_yield_ms={measured_yield_ms:.2} qb_measured_main_pct={measured_main_pct:.1}"
        );

        assert_eq!(
            eval_printed(
                &mut ctx,
                "(progn
                   (mutex-lock e7p2-m-m)
                   (condition-notify e7p2-m-c)
                   (mutex-unlock e7p2-m-m)
                   (thread-join e7p2-m-th))"
            ),
            "0"
        );
        assert_eq!(
            ctx.green_control().live_parallel_participant_count_fast(),
            0,
            "B2: parallel_count must return to 0 after joining measurement owner"
        );
        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Inactive,
            "T2: green_teardown must reset ParallelActivationState to Inactive"
        );
        assert!(
            !ctx.parallel_activation_fast_for_test(),
            "T2: green_teardown must clear parallel_activation_fast"
        );
        assert!(
            ctx.legacy_jit_withdrawn_for_test(),
            "T2/P2: session-lifetime D4 withdrawal stays true after terminal green_teardown"
        );
    });
}

fn green_gen0_context() -> Context {
    let mut ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    // SAFETY: fresh serial fixture before TLS install or worker threads.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut ctx.tagged_heap,
            false,
        );
    }
    ctx.setup_thread_locals();
    ctx.start_green_threads()
        .expect("start Green runtime for E7-P.2 GEN0 test");
    ctx
}

/// T1: Permanent `BetweenCommands` refusal (`PendingUpgrade -> Refused`) on
/// unreachable session invariants (`NotGreenWorld` and `GenerationalDisabled`)
/// falls back to Baton with hint `nil` and clears `parallel_activation_fast`
/// so later boundaries do not re-probe.
#[test]
fn e7p2_t1_permanent_refusal_falls_back_to_baton_without_reprobe() {
    with_parallel_admission_enabled_for_test(|| {
        // 1. NotGreenWorld (Sim mode with generational enabled).
        let mut sim_ctx = Context::new(crate::test_utils::unit_runtime());
        // SAFETY: fresh serial fixture before TLS install or worker threads.
        unsafe {
            neovm_value::test_support::authority::set_generational_enabled_for_test(
                &mut sim_ctx.tagged_heap,
                true,
            );
        }
        sim_ctx.setup_thread_locals();
        let (sim_outer, sim_guard) = enter_simulated_top_level_command_loop(&mut sim_ctx);
        // Sim worlds bypass `spawn_parallel_opt_in_thread` in the creator, so
        // arm `PendingUpgrade` explicitly to exercise the `NotGreenWorld`
        // permanent-refusal arm at `BetweenCommands`.
        sim_ctx.arm_pending_upgrade_for_test();
        assert_eq!(
            sim_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade
        );
        assert!(sim_ctx.parallel_activation_fast_for_test());

        sim_ctx
            .parallel_between_commands_boundary()
            .expect("BetweenCommands records permanent NotGreenWorld refusal");
        assert_eq!(
            sim_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Refused
        );
        assert_eq!(
            sim_ctx.parallel_activation_refusal_for_test(),
            Some(ActivationRefusal::NotGreenWorld)
        );
        assert!(!sim_ctx.parallel_activation_fast_for_test());
        assert!(!sim_ctx.legacy_jit_withdrawn_for_test());

        // Subsequent boundary and creator calls stay Refused without re-probing.
        sim_ctx
            .parallel_between_commands_boundary()
            .expect("Refused fast path is a no-op");
        let second_sim = eval_printed(
            &mut sim_ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 41 2)) \"t1-sim-2\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(second_sim, "(43 nil)");
        assert_eq!(
            sim_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Refused
        );
        assert!(!sim_ctx.parallel_activation_fast_for_test());
        drop(sim_guard);
        sim_ctx.unbind_to(sim_outer);
        sim_ctx.command_loop_mut().recursive_depth = 0;
        sim_ctx.command_loop_mut().running = false;
        drop(sim_ctx);

        // 2. GenerationalDisabled (Green GEN0 world).
        let mut gen0_ctx = green_gen0_context();
        let (gen0_outer, gen0_guard) = enter_simulated_top_level_command_loop(&mut gen0_ctx);
        // `admit_parallel` already refuses GEN0 before `spawn_parallel_opt_in_thread`,
        // so arm `PendingUpgrade` explicitly to exercise the `GenerationalDisabled`
        // permanent-refusal arm at `BetweenCommands`.
        gen0_ctx.arm_pending_upgrade_for_test();
        assert_eq!(
            gen0_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade
        );

        gen0_ctx
            .parallel_between_commands_boundary()
            .expect("BetweenCommands records permanent GenerationalDisabled refusal");
        assert_eq!(
            gen0_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Refused
        );
        assert_eq!(
            gen0_ctx.parallel_activation_refusal_for_test(),
            Some(ActivationRefusal::GenerationalDisabled)
        );
        assert!(!gen0_ctx.parallel_activation_fast_for_test());
        assert!(!gen0_ctx.legacy_jit_withdrawn_for_test());
        assert!(!gen0_ctx.green_control().era_is_open());

        // Subsequent boundary and creator calls fall back to Baton without re-probing.
        gen0_ctx
            .parallel_between_commands_boundary()
            .expect("Refused fast path is a no-op");
        let closure = gen0_ctx
            .eval_str("(lambda () (+ 41 2))")
            .expect("eval closure");
        assert!(
            matches!(
                gen0_ctx.spawn_parallel_opt_in_thread(&[closure]),
                Err(super::super::live_admission::SpawnParallelError::Refused(
                    super::super::live_admission::LiveEraOpenError::ActivationRefused
                ))
            ),
            "T1: Refused state must short-circuit spawn_parallel_opt_in_thread with ActivationRefused without re-probing"
        );
        let second_gen0 = eval_printed(
            &mut gen0_ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 41 2)) \"t1-gen0-2\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(second_gen0, "(43 nil)");
        assert_eq!(
            gen0_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Refused
        );
        assert!(!gen0_ctx.green_control().era_is_open());

        drop(gen0_guard);
        gen0_ctx.unbind_to(gen0_outer);
        gen0_ctx.command_loop_mut().recursive_depth = 0;
        gen0_ctx.command_loop_mut().running = false;
        gen0_ctx.green_teardown();
        assert_eq!(
            gen0_ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Inactive
        );
        assert!(!gen0_ctx.parallel_activation_fast_for_test());
        assert!(!gen0_ctx.legacy_jit_withdrawn_for_test());
    });
}

/// R1 + R2 + T2: Transient era-open failures (`FailKeeperSpawn` ->
/// `KeeperStartFailed`, and `FailAfterOpenBeforeAcquire` rollback before
/// legacy JIT withdrawal) keep `PendingUpgrade` without permanently downgrading
/// the session or detaching Main's legacy JIT, succeed on the next quiescent
/// `BetweenCommands` boundary, and reset cleanly across `green_teardown`.
#[test]
fn e7p2_r2_transient_failures_retry_without_permanent_refusal_and_t2_teardown() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_gen1_context();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        let first = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 40 2)) \"r2-first\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(first, "(42 nil)");
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade
        );

        // 1. Transient keeper OS-thread spawn refusal (EAGAIN/ENOMEM class).
        super::super::live_admission::set_era_open_fault_for_test(Some(
            super::super::live_admission::LiveEraOpenFault::FailKeeperSpawn,
        ));
        ctx.parallel_between_commands_boundary()
            .expect("transient KeeperStartFailed keeps PendingUpgrade");
        super::super::live_admission::set_era_open_fault_for_test(None);
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade,
            "R2: KeeperStartFailed must keep PendingUpgrade instead of Refused"
        );
        assert!(ctx.parallel_activation_fast_for_test());
        assert!(!ctx.legacy_jit_withdrawn_for_test());
        assert!(!ctx.green_control().era_is_open());

        // 2. Rollback after open_era_on_control (before final JIT withdrawal).
        super::super::live_admission::set_era_open_fault_for_test(Some(
            super::super::live_admission::LiveEraOpenFault::FailAfterOpenBeforeAcquire,
        ));
        ctx.parallel_between_commands_boundary()
            .expect("rollback before commit keeps PendingUpgrade");
        super::super::live_admission::set_era_open_fault_for_test(None);
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade,
            "R1/R2: rollback before commit must keep PendingUpgrade and leave legacy JIT intact"
        );
        assert!(ctx.parallel_activation_fast_for_test());
        assert!(!ctx.legacy_jit_withdrawn_for_test());
        assert!(!ctx.green_control().era_is_open());

        // 3. Next quiescent boundary succeeds cleanly.
        ctx.parallel_between_commands_boundary()
            .expect("retry succeeds on next BetweenCommands boundary");
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Active
        );
        assert!(ctx.parallel_activation_fast_for_test());
        assert!(ctx.legacy_jit_withdrawn_for_test());
        assert!(ctx.green_control().era_is_open());

        let second = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 40 2)) \"r2-second\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(second, "(42 t)");

        // 4. T2: post-teardown state reset assertions.
        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Inactive,
            "T2: post-teardown state must be Inactive"
        );
        assert!(
            !ctx.parallel_activation_fast_for_test(),
            "T2: post-teardown parallel_activation_fast must be false"
        );
        assert!(
            ctx.legacy_jit_withdrawn_for_test(),
            "T2/P2: post-teardown legacy_jit_withdrawn stays true once an era opened"
        );
    });
}

/// HIGH #3: A non-Main Baton `make-thread` worker invoking
/// `parallel_between_commands_boundary()` (or `enter_command_loop_baseline_scope`)
/// must NEVER open the era, NEVER permanently refuse the session, and NEVER
/// mutate Main's active fairness counters.
#[test]
fn e7p2_h3_non_main_baton_worker_cannot_activate_or_refuse_or_mutate_fairness() {
    use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
    use neovm_value::error::EvalResult;

    fn worker_boundary_probe(ctx: &mut Context, _args: Vec<Value>) -> EvalResult {
        // Attempt to mint a baseline witness from a non-Main worker -> must be None.
        let outer = ctx.specpdl().len();
        let guard = ctx.enter_command_loop_baseline_scope(outer, true)?;
        let minted = guard.is_some();
        drop(guard);
        ctx.unbind_to(outer);
        // Attempt to service BetweenCommands from a non-Main worker -> must be a no-op.
        ctx.parallel_between_commands_boundary()?;
        Ok(Value::bool_val(minted))
    }

    let mut ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    // SAFETY: fresh serial fixture before TLS install or worker threads.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut ctx.tagged_heap,
            true,
        );
    }
    ctx.setup_thread_locals();
    // Register test subr BEFORE starting Green threads (hosted/Green startup order).
    ctx.register_subr(SubrSpec::new(
        "e7p2-worker-boundary-probe",
        NativeFn::ContextVec(worker_boundary_probe),
        SubrArity::new(0, Some(0)),
    ));
    ctx.start_green_threads()
        .expect("start Green threads on Main");
    let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

    // 1. Arm PendingUpgrade on Main, then run the boundary probe on a Baton worker.
    ctx.arm_pending_upgrade_for_test();
    let res1 = eval_printed(
        &mut ctx,
        "(thread-join (make-thread (lambda () (e7p2-worker-boundary-probe)) \"h3-worker-1\"))",
    );
    assert_eq!(
        res1, "nil",
        "non-Main worker must not mint CommandLoopBaseline"
    );
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::PendingUpgrade,
        "HIGH #3: non-Main worker must leave PendingUpgrade untouched (neither Active nor Refused)"
    );
    assert!(!ctx.green_control().era_is_open());

    // 2. Main activates cleanly at top-level BetweenCommands.
    ctx.parallel_between_commands_boundary().unwrap();
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Active
    );
    let (b_before, f_before) = ctx.between_commands_counters_for_test().unwrap();

    // 3. While Active, a non-Main worker calling the boundary must not mutate
    //    Main's fairness counters or panic.
    let res2 = eval_printed(
        &mut ctx,
        "(thread-join (make-thread (lambda () (e7p2-worker-boundary-probe)) \"h3-worker-2\"))",
    );
    assert_eq!(res2, "nil");
    let (b_after, f_after) = ctx.between_commands_counters_for_test().unwrap();
    assert_eq!((b_after, f_after), (b_before, f_before));

    drop(guard);
    ctx.unbind_to(outer);
    ctx.command_loop_mut().recursive_depth = 0;
    ctx.command_loop_mut().running = false;
    ctx.green_teardown();
    assert_eq!(
        ctx.parallel_activation_tag_for_test(),
        ParallelActivationTag::Inactive
    );
    assert!(!ctx.parallel_activation_fast_for_test());
    assert!(
        ctx.legacy_jit_withdrawn_for_test(),
        "T2/P2: post-teardown legacy_jit_withdrawn stays true once an era opened"
    );
}

/// R2 / lead sign-conditional row (a): exhaustive table asserting permanent vs
/// transient classification across every `LiveEraOpenError` variant (including
/// `#[cfg(test)] InjectedFault`).
#[test]
fn e7p2_r2_classify_era_open_error_exhaustive_table() {
    use super::super::live_admission::LiveEraOpenError;

    let cases: &[(LiveEraOpenError, EraOpenErrorClass)] = &[
        (
            LiveEraOpenError::GenerationalRequired,
            EraOpenErrorClass::Permanent(ActivationRefusal::GenerationalDisabled),
        ),
        (
            LiveEraOpenError::HeapUnsupported,
            EraOpenErrorClass::Permanent(ActivationRefusal::HeapUnsupported),
        ),
        (
            LiveEraOpenError::LegacyJitWithdrawalFailed,
            EraOpenErrorClass::Permanent(ActivationRefusal::LegacyJitWithdrawalImpossible),
        ),
        (
            LiveEraOpenError::NotSeatedMain,
            EraOpenErrorClass::Transient,
        ),
        (LiveEraOpenError::NotSolo, EraOpenErrorClass::Transient),
        (LiveEraOpenError::GcBusy, EraOpenErrorClass::Transient),
        (LiveEraOpenError::Closing, EraOpenErrorClass::Transient),
        (LiveEraOpenError::SeatPark, EraOpenErrorClass::Transient),
        (LiveEraOpenError::DrainFailed, EraOpenErrorClass::Transient),
        (
            LiveEraOpenError::TransferFailed,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::KeeperStartFailed,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::OpenEraFailed,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::InitialTurnFailed,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::NativeActivation,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::LegacyJitBusy,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::DeferredToBetweenCommands,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::ActivationRefused,
            EraOpenErrorClass::Transient,
        ),
        (
            LiveEraOpenError::InjectedFault,
            EraOpenErrorClass::Transient,
        ),
    ];

    // Compile-time exhaustiveness witness: adding any variant to
    // `LiveEraOpenError` forces updating this test table too.
    let mut covered = 0usize;
    for &(err, expected) in cases {
        match err {
            LiveEraOpenError::GenerationalRequired
            | LiveEraOpenError::HeapUnsupported
            | LiveEraOpenError::LegacyJitWithdrawalFailed
            | LiveEraOpenError::NotSeatedMain
            | LiveEraOpenError::NotSolo
            | LiveEraOpenError::GcBusy
            | LiveEraOpenError::Closing
            | LiveEraOpenError::SeatPark
            | LiveEraOpenError::DrainFailed
            | LiveEraOpenError::TransferFailed
            | LiveEraOpenError::KeeperStartFailed
            | LiveEraOpenError::OpenEraFailed
            | LiveEraOpenError::InitialTurnFailed
            | LiveEraOpenError::NativeActivation
            | LiveEraOpenError::LegacyJitBusy
            | LiveEraOpenError::DeferredToBetweenCommands
            | LiveEraOpenError::ActivationRefused
            | LiveEraOpenError::InjectedFault => {
                covered += 1;
            }
        }
        assert_eq!(
            classify_era_open_error(err),
            expected,
            "unexpected classification for {err:?}"
        );
    }
    assert_eq!(covered, cases.len());
}

/// R2 / lead sign-conditional row (b): a structural legacy JIT withdrawal
/// failure (`LiveEraOpenError::LegacyJitWithdrawalFailed`) permanently refuses
/// activation (`Refused(LegacyJitWithdrawalImpossible)` + `fast == false`) and
/// subsequent quiescent command boundaries NEVER reach `ensure_live_parallel_era`
/// again (attempt count stays at 1).
#[test]
fn e7p2_r2_structural_legacy_jit_withdrawal_failure_refuses_permanently_without_reprobe() {
    use super::super::live_admission::{
        LiveEraOpenError, LiveEraOpenFault, SpawnParallelError, era_open_attempts_for_test,
        set_era_open_fault_for_test,
    };

    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_gen1_context();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        let attempts_before = era_open_attempts_for_test();

        // First mid-command opt-in falls back to Baton and arms PendingUpgrade.
        let first = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 40 2)) \"r2-struct-1\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(first, "(42 nil)");
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::PendingUpgrade
        );
        assert!(ctx.parallel_activation_fast_for_test());
        assert_eq!(era_open_attempts_for_test(), attempts_before);

        // Inject structural withdrawal failure inside `EraOpened::commit()`.
        set_era_open_fault_for_test(Some(LiveEraOpenFault::FailLegacyJitWithdrawalStructural));
        ctx.parallel_between_commands_boundary()
            .expect("structural withdrawal refusal records permanent Refused");
        set_era_open_fault_for_test(None);

        assert_eq!(
            era_open_attempts_for_test(),
            attempts_before + 1,
            "first quiescent boundary must invoke ensure_live_parallel_era exactly once"
        );
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Refused
        );
        assert_eq!(
            ctx.parallel_activation_refusal_for_test(),
            Some(ActivationRefusal::LegacyJitWithdrawalImpossible)
        );
        assert!(
            !ctx.parallel_activation_fast_for_test(),
            "permanent structural refusal must clear parallel_activation_fast"
        );
        assert!(!ctx.legacy_jit_withdrawn_for_test());
        assert!(!ctx.green_control().era_is_open());

        // Subsequent quiescent boundaries must NOT re-enter ensure_live_parallel_era.
        ctx.parallel_between_commands_boundary()
            .expect("second boundary is fast-path no-op");
        ctx.parallel_between_commands_boundary()
            .expect("third boundary is fast-path no-op");
        assert_eq!(
            era_open_attempts_for_test(),
            attempts_before + 1,
            "subsequent quiescent boundaries must never reach ensure_live_parallel_era after structural refusal"
        );

        // Subsequent opt-in creator calls short-circuit to Baton without re-probing.
        let closure = ctx.eval_str("(lambda () (+ 41 2))").expect("eval closure");
        assert!(matches!(
            ctx.spawn_parallel_opt_in_thread(&[closure]),
            Err(SpawnParallelError::Refused(
                LiveEraOpenError::ActivationRefused
            ))
        ));
        let second = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 41 2)) \"r2-struct-2\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(second, "(43 nil)");
        assert_eq!(era_open_attempts_for_test(), attempts_before + 1);
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Refused
        );
        assert!(!ctx.parallel_activation_fast_for_test());

        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Inactive
        );
        assert!(!ctx.parallel_activation_fast_for_test());
        assert!(!ctx.legacy_jit_withdrawn_for_test());
    });
}

/// Review #3: When a `switch_refusal` condition holds (e.g., `gc_inhibit_depth > 0`),
/// `service_active_between_commands_fairness` skips both `BurstYield` and
/// `FairnessSlice` without raising any Lisp error or mutating yield counters.
#[test]
fn e7p2_switch_refusal_skips_between_commands_fairness_without_error() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_gen1_context();
        let control = ctx.green_control();
        let (outer, guard) = enter_simulated_top_level_command_loop(&mut ctx);

        ctx.arm_pending_upgrade_for_test();
        ctx.parallel_between_commands_boundary().unwrap();
        assert_eq!(
            ctx.parallel_activation_tag_for_test(),
            ParallelActivationTag::Active
        );

        // Spawn a live quiet parallel owner so FairnessSlice would normally fire
        // once the burst quantum expires.
        let setup = eval_printed(
            &mut ctx,
            "(progn
               (setq e7p2-sr-m (make-mutex \"e7p2-sr-m\"))
               (setq e7p2-sr-c (make-condition-variable e7p2-sr-m \"e7p2-sr-c\"))
               (setq e7p2-sr-ready nil)
               (setq e7p2-sr-th
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (mutex-lock e7p2-sr-m)
                        (setq e7p2-sr-ready t)
                        (condition-wait e7p2-sr-c)
                        (mutex-unlock e7p2-sr-m)
                        7)
                      \"e7p2-sr-owner\"))
               (while (null e7p2-sr-ready) (thread-yield))
               (neomacs-thread-parallel-p e7p2-sr-th))",
        );
        assert_eq!(setup, "t");
        let (waiter, _idle_epoch, _pcount) = control
            .era_between_commands_handles()
            .expect("open era handles");

        ctx.set_fairness_overrides_for_test(
            Some(Duration::from_nanos(1)),
            Some(PARALLEL_FAIRNESS_SLICE),
        );
        let (b0, f0) = ctx.between_commands_counters_for_test().unwrap();

        // 1. Inhibit GC (`switch_refusal(ctx).is_some()`) and test BOTH
        //    expired-quantum FairnessSlice and raised-RingWaiter BurstYield paths.
        ctx.gc_inhibit_depth += 1;
        assert!(
            crate::emacs_core::eval::green::switch_refusal(&ctx).is_some(),
            "precondition: gc_inhibit_depth > 0 triggers switch_refusal"
        );
        std::thread::sleep(Duration::from_millis(1));
        ctx.parallel_between_commands_boundary()
            .expect("FairnessSlice path under switch_refusal must return Ok(()) without signaling");
        waiter.set_for_test(true);
        ctx.parallel_between_commands_boundary()
            .expect("BurstYield path under switch_refusal must return Ok(()) without signaling");
        waiter.set_for_test(false);
        ctx.gc_inhibit_depth -= 1;

        let (b1, f1) = ctx.between_commands_counters_for_test().unwrap();
        assert_eq!(
            (b1, f1),
            (b0, f0),
            "review #3: switch_refusal boundary must perform zero BurstYield and zero FairnessSlice yields"
        );

        assert_eq!(
            eval_printed(
                &mut ctx,
                "(progn
                   (mutex-lock e7p2-sr-m)
                   (condition-notify e7p2-sr-c)
                   (mutex-unlock e7p2-sr-m)
                   (thread-join e7p2-sr-th))"
            ),
            "7"
        );
        drop(guard);
        ctx.unbind_to(outer);
        ctx.command_loop_mut().recursive_depth = 0;
        ctx.command_loop_mut().running = false;
        ctx.green_teardown();
    });
}
