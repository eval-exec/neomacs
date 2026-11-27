//! E6-P.4 live-Green admission, era-open/teardown, and opt-in owner regression
//! tests (`E6-P4-ADMISSION-PREREAD.md` §3.2 + Lead Condition 1).
//!
//! Covers:
//! - Dormancy default (`PRODUCTION_ADMISSION_ENABLED == false`)
//! - `make-thread`-only zero-keeper invariant
//! - GEN0 and Sim mode Baton/Sim fallback (`neomacs-thread-parallel-p` = `nil`)
//! - End-to-end `b6`-prelude shape (live opt-in owner parked in
//!   `condition-wait`, `neomacs-thread-parallel-p` = `t` on owner / `nil` on
//!   Main, `(garbage-collect)` during live era, clean `green_teardown`)
//! - Opt-in owner completion, post-join `neomacs-thread-parallel-p` = `t`,
//!   second opt-in owner reusing open era, and default `make-thread` child
//!   staying Baton (`neomacs-thread-parallel-p` = `nil`)
//! - **INV-1 (`E6-P4-I1`):** unwind-safe live-Green era-open `(a)->(b)->(c)`
//!   error & panic rollback (red-first mutation proof target)
//! - **INV-2 (`E6-P4-I2`):** retire-by-wake + exit-gated `Control::close_era`
//!   (red-first mutation proof target)

use super::{LiveEraOpenError, LiveEraOpenFault, set_era_open_fault_for_test};
use crate::emacs_core::eval::green::{EraCloseError, ThreadKey};
use crate::emacs_core::eval::parallel_entry::keeper_driver::{GroupLink, GroupPort, KeeperMsg};
use crate::emacs_core::eval::world::TurnRight;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use crate::emacs_core::threads::with_parallel_admission_enabled_for_test;
use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::time::Duration;

fn green_context(generational: bool) -> Context {
    let mut ctx =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    // SAFETY: fresh serial test fixture before any concurrent activity.
    unsafe {
        neovm_value::test_support::authority::set_generational_enabled_for_test(
            &mut ctx.tagged_heap,
            generational,
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

#[test]
fn e6p4_row1_production_dormant_by_default_falls_back_to_baton() {
    let mut ctx = green_context(true);
    let out = eval_printed(
        &mut ctx,
        "(let ((th (neomacs-make-parallel-thread (lambda () (+ 40 2)) \"dormant\")))
           (list (thread-join th) (neomacs-thread-parallel-p th)))",
    );
    assert_eq!(out, "(42 nil)");
    let control = ctx.green_control();
    assert!(
        !control.era_is_open(),
        "dormant production must not open an era"
    );
    assert!(control.era_state_is_solo_active());
    ctx.green_teardown();
}

#[test]
fn e6p4_row2_make_thread_only_never_opens_keeper_era() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let out = eval_printed(
            &mut ctx,
            "(let ((th (make-thread (lambda () (+ 20 22)) \"ordinary\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(out, "(42 nil)");
        let control = ctx.green_control();
        assert!(
            !control.era_is_open(),
            "make-thread must NEVER open a keeper era even when admission is enabled"
        );
        assert!(control.era_state_is_solo_active());
        ctx.green_teardown();
    });
}

#[test]
fn e6p4_row3_gen0_falls_back_to_baton_with_hint_nil_and_no_era() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(false);
        let out = eval_printed(
            &mut ctx,
            "(progn
               (setq g0-m (make-mutex \"g0-m\"))
               (setq g0-c (make-condition-variable g0-m \"g0-c\"))
               (setq g0-ready nil)
               (setq g0-done nil)
               (setq g0-th (neomacs-make-parallel-thread
                            (lambda ()
                              (mutex-lock g0-m)
                              (setq g0-ready t)
                              (condition-wait g0-c)
                              (setq g0-done 'ok)
                              (mutex-unlock g0-m)
                              99)
                            \"gen0-fallback\"))
               (while (null g0-ready) (thread-yield))
               (let ((live-hint (neomacs-thread-parallel-p g0-th)))
                 (mutex-lock g0-m)
                 (condition-notify g0-c)
                 (mutex-unlock g0-m)
                 (let ((val (thread-join g0-th)))
                   (list val g0-done live-hint (neomacs-thread-parallel-p g0-th)))))",
        );
        assert_eq!(out, "(99 ok nil nil)");
        let control = ctx.green_control();
        assert!(
            !control.era_is_open(),
            "GEN0 fallback must not open a keeper era"
        );
        assert!(control.era_state_is_solo_active());
        ctx.green_teardown();
    });
}

#[test]
fn e6p4_row4_sim_mode_falls_back_to_sim_with_hint_nil() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = Context::new(crate::test_utils::unit_runtime());
        // SAFETY: fresh serial fixture before TLS install or worker threads;
        // no young objects or compiled leaves exist under this mode toggle.
        unsafe {
            neovm_value::test_support::authority::set_generational_enabled_for_test(
                &mut ctx.tagged_heap,
                true,
            );
        }
        ctx.setup_thread_locals();
        let out = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 10 32)) \"sim-fallback\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        assert_eq!(out, "(42 nil)");
    });
}

#[test]
fn e6p4_row5_b6_prelude_live_admitted_parked_owner_gc_and_teardown() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let out = eval_printed(
            &mut ctx,
            "(progn
               (setq e6p4-m (make-mutex \"b6-m\"))
               (setq e6p4-c (make-condition-variable e6p4-m \"b6-c\"))
               (setq e6p4-parked nil)
               (setq e6p4-owner
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (mutex-lock e6p4-m)
                        (setq e6p4-parked t)
                        (condition-wait e6p4-c))
                      \"b6-owner\"))
               (while (null e6p4-parked)
                 (thread-yield))
               (list e6p4-parked
                     (neomacs-thread-parallel-p e6p4-owner)
                     (neomacs-thread-parallel-p main-thread)
                     (length (all-threads))))",
        );
        assert_eq!(out, "(t t nil 2)");
        let control = ctx.green_control();
        assert!(
            control.era_is_open(),
            "opt-in admission must open a live era"
        );
        assert_eq!(
            control.live_parallel_participant_count(),
            1,
            "parked opt-in owner must be tracked as live"
        );

        // Explicit (garbage-collect) while the opt-in owner is parked in the live era.
        let gc_out = eval_printed(&mut ctx, "(progn (garbage-collect) 'gc-ok)");
        assert_eq!(gc_out, "gc-ok");

        // Teardown must wake the parked owner (E6-P4-I2(a)), wait for Exit
        // (E6-P4-I2(b)), close the era, and restore SoloActive on Main.
        ctx.green_teardown();
        assert!(!control.era_is_open());
        assert!(control.era_state_is_solo_active());
        assert_eq!(control.live_parallel_participant_count(), 0);
    });
}

#[test]
fn e6p4_row6_opt_in_completion_post_join_hint_and_default_child_stays_baton() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        ctx.ensure_live_parallel_era()
            .expect("ensure_live_parallel_era must open on fresh GEN1 Green Context");
        // First opt-in owner runs GC inside its SerialTurn and exits normally.
        let out1 = eval_printed(
            &mut ctx,
            "(let ((th1 (neomacs-make-parallel-thread
                         (lambda ()
                           (garbage-collect)
                           (+ 20 22))
                         \"opt-in-1\")))
               (list (thread-join th1)
                     (neomacs-thread-parallel-p th1)))",
        );
        assert_eq!(out1, "(42 t)");
        let control = ctx.green_control();
        assert!(
            control.era_is_open(),
            "open era persists across opt-in owner completions until green_teardown"
        );

        // Second opt-in owner reuses the already-open era and spawns an ordinary
        // `make-thread` child, which must stay Baton (`neomacs-thread-parallel-p` = nil).
        let out2 = eval_printed(
            &mut ctx,
            "(let ((th2 (neomacs-make-parallel-thread
                         (lambda ()
                           (let ((child (make-thread
                                         (lambda ()
                                           (cons 'child
                                                 (neomacs-thread-parallel-p (current-thread))))
                                         \"baton-child\")))
                             (list (thread-join child)
                                   (neomacs-thread-parallel-p child)
                                   (neomacs-thread-parallel-p (current-thread)))))
                         \"opt-in-2\")))
               (list (thread-join th2)
                     (neomacs-thread-parallel-p th2)))",
        );
        assert_eq!(out2, "(((child) nil t) t)");

        ctx.green_teardown();
        assert!(!control.era_is_open());
        assert!(control.era_state_is_solo_active());
        assert_eq!(control.live_parallel_participant_count(), 0);
    });
}

/// **INV-1 (`E6-P4-I1`)** red-first mutation proof targets (3 sub-cases:
/// error after surrender, panic unwind after surrender, and error after
/// `Control::open_era` before `acquire_initial_era_turn`).
/// Deleting `LiveEraOpenGuard::rollback_now()` causes all three sub-cases to
/// fail because `Control` stays stuck outside `SoloActive` and Main loses
/// `TurnRight::Solo`.
#[test]
fn e6p4_row7a_inv1_era_open_error_after_surrender_rolls_back_to_solo() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let control = ctx.green_control();
        set_era_open_fault_for_test(Some(LiveEraOpenFault::FailAfterSurrenderBeforeOpen));
        let out = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 1 2)) \"inv1-a\")))
               (list (thread-join th) (neomacs-thread-parallel-p th)))",
        );
        set_era_open_fault_for_test(None);
        assert_eq!(
            out, "(3 nil)",
            "E6-P4-I1(A): failed era-open must roll back to Solo and fall back to Baton"
        );
        assert!(
            control.era_state_is_solo_active(),
            "E6-P4-I1(A): Control must return to SoloActive after error rollback"
        );
        assert!(
            ctx.green_seat()
                .and_then(|s| s.baton_ref())
                .is_some_and(TurnRight::is_solo),
            "E6-P4-I1(A): Main seat must hold TurnRight::Solo after error rollback"
        );
        assert_eq!(eval_printed(&mut ctx, "(+ 10 20)"), "30");
        ctx.green_teardown();
    });
}

#[test]
fn e6p4_row7b_inv1_era_open_panic_after_surrender_rolls_back_to_solo() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let control = ctx.green_control();
        set_era_open_fault_for_test(Some(LiveEraOpenFault::PanicAfterSurrenderBeforeOpen));
        let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = ctx.ensure_live_parallel_era();
        }));
        set_era_open_fault_for_test(None);
        assert!(
            caught.is_err(),
            "E6-P4-I1(B): injected panic must unwind out of ensure_live_parallel_era"
        );
        assert!(
            control.era_state_is_solo_active(),
            "E6-P4-I1(B): LiveEraOpenGuard::drop must restore SoloActive on panic unwind"
        );
        assert!(
            ctx.green_seat()
                .and_then(|s| s.baton_ref())
                .is_some_and(TurnRight::is_solo),
            "E6-P4-I1(B): Main seat must re-hold TurnRight::Solo after panic unwind"
        );
        assert_eq!(eval_printed(&mut ctx, "(+ 10 20)"), "30");
        ctx.green_teardown();
    });
}

#[test]
fn e6p4_row7c_inv1_era_open_error_after_open_before_acquire_closes_and_rolls_back() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let control = ctx.green_control();
        set_era_open_fault_for_test(Some(LiveEraOpenFault::FailAfterOpenBeforeAcquire));
        let err = ctx.ensure_live_parallel_era();
        set_era_open_fault_for_test(None);
        assert_eq!(
            err,
            Err(LiveEraOpenError::InjectedFault),
            "E6-P4-I1(C): injected post-open fault must return Err"
        );
        assert!(
            control.era_state_is_solo_active(),
            "E6-P4-I1(C): LiveEraOpenGuard::drop must close keeper+era and restore SoloActive"
        );
        assert!(
            ctx.green_seat()
                .and_then(|s| s.baton_ref())
                .is_some_and(TurnRight::is_solo),
            "E6-P4-I1(C): Main seat must re-hold TurnRight::Solo after post-open rollback"
        );
        assert_eq!(eval_printed(&mut ctx, "(+ 5 6)"), "11");
        ctx.green_teardown();
    });
}

/// **INV-2(b) (`E6-P4-I2(b)`)** red-first mutation proof target:
/// Proves `Control::close_era` refuses with `EraCloseError::LiveOwnersRemaining`
/// while any admitted parallel owner slot has `port_exited == false`, and
/// succeeds only after `mark_parallel_owner_exited`.
#[test]
fn e6p4_row8a_inv2_exit_gated_close_era() {
    let mut ctx = green_context(true);
    let control = ctx.green_control();
    let right = ctx
        .green_seat()
        .expect("Main seated")
        .take_baton()
        .expect("Main holds Solo Baton");
    let baton = control
        .surrender_solo_for_era(right)
        .expect("surrender Solo for close_era gate check");
    let world_handle = baton.era_world_handle().expect("era_world_handle");
    let (inbox_tx, _inbox_rx) = mpsc::channel::<KeeperMsg>();
    let group_link = GroupLink::new_for_test();
    let group_port = GroupPort::new(inbox_tx, group_link);
    control
        .open_era(world_handle, group_port)
        .expect("open_era on Control");

    // Register a live `Participant::Parallel` owner.
    let live_key = ThreadKey::from_id(777);
    let (_wake, _hint) = control.register_parallel(live_key);
    assert_eq!(control.live_parallel_participant_count(), 1);

    // `close_era` MUST refuse while a parallel owner has not retired.
    let (baton, err) = control
        .close_era(baton)
        .expect_err("E6-P4-I2(b): close_era must refuse while a parallel owner is live");
    assert_eq!(err, EraCloseError::LiveOwnersRemaining);
    assert!(control.era_is_open());

    // Once the owner retires (`unregister_parallel` / `publish_parallel_owner_exit`), `close_era` succeeds.
    control.unregister_parallel(live_key);
    assert_eq!(control.live_parallel_participant_count(), 0);
    let (solo_right, era_wh, gp) = control
        .close_era(baton)
        .expect("E6-P4-I2(b): close_era must succeed once all parallel owners exited");
    drop(gp);
    drop(era_wh);
    ctx.green_seat()
        .expect("Main seated")
        .install_baton(solo_right);
    assert!(control.era_state_is_solo_active());
    ctx.green_teardown();
}

/// **INV-2(a) (`E6-P4-I2(a)`)** red-first mutation proof target:
/// Parks an opt-in owner forever in
/// `(unwind-protect (... (condition-wait c)) (setq cleaned t))` with no Lisp
/// notifier, then runs `green_teardown()` under a strict 10s bound and
/// verifies the owner woke, ran its unwind-protect cleanup, exited, and
/// restored `SoloActive`.
#[test]
fn e6p4_row8b_inv2_retire_by_wake_parked_owner() {
    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
    let handle = std::thread::Builder::new()
        .name("e6p4-inv2-driver".to_string())
        .spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                with_parallel_admission_enabled_for_test(|| {
                    let mut ctx = green_context(true);
                    let out = eval_printed(
                        &mut ctx,
                        "(progn
                           (setq inv2-m (make-mutex \"inv2-m\"))
                           (setq inv2-c (make-condition-variable inv2-m \"inv2-c\"))
                           (setq inv2-parked nil)
                           (setq inv2-cleaned nil)
                           (setq inv2-owner
                                 (neomacs-make-parallel-thread
                                  (lambda ()
                                    (unwind-protect
                                        (progn
                                          (mutex-lock inv2-m)
                                          (setq inv2-parked t)
                                          (condition-wait inv2-c))
                                      (setq inv2-cleaned t)))
                                  \"inv2-parked-forever\"))
                           (while (null inv2-parked)
                             (thread-yield))
                           (list inv2-parked inv2-cleaned (neomacs-thread-parallel-p inv2-owner)))",
                    );
                    assert_eq!(out, "(t nil t)");
                    let control = ctx.green_control();
                    assert_eq!(control.live_parallel_participant_count(), 1);

                    // Teardown with NO Lisp condition-notify: E6-P4-I2(a) must wake
                    // the parked opt-in owner so it exits via `Flow::shutdown` and
                    // retires cleanly under G5 Steps 1–6.
                    ctx.green_teardown();
                    assert!(!control.era_is_open());
                    assert!(control.era_state_is_solo_active());
                    assert_eq!(control.live_parallel_participant_count(), 0);
                    // Matches Baton's `green/switch.rs:1095-1105` / GNU `exit`:
                    // `Flow::shutdown` exits the parked worker promptly without
                    // spinning or deadlocking teardown, and Main resumes cleanly.
                    assert_eq!(eval_printed(&mut ctx, "(+ 20 22)"), "42");
                });
            }));
            let _ = done_tx.send(result.map_err(|p| {
                if let Some(s) = p.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = p.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "panic".to_string()
                }
            }));
        })
        .expect("spawn inv2 driver thread");

    match done_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => {
            handle.join().expect("inv2 driver thread joined cleanly");
        }
        Ok(Err(msg)) => {
            let _ = handle.join();
            panic!("E6-P4-I2(a) retire-by-wake failed: {msg}");
        }
        Err(_) => {
            panic!(
                "E6-P4-I2(a) violation: parked opt-in owner did not wake and exit during green_teardown within 10s"
            );
        }
    }
}

/// **Slice A (`Control::wait_of` fix, `#15237`)** red-first regression row:
/// `thread-signal` targeting an admitted opt-in parallel owner parked in
/// `(condition-wait c)` MUST observe `Control::wait_of(key) == Some(Wait::Condition(..))`
/// and wake it cleanly (matching `e6p1_an_opt_in_thread_object_takes_signals_mutexes_and_last_error`).
#[test]
fn e6p4_row9_wait_of_wakes_parked_opt_in_owner_on_thread_signal() {
    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
    let handle = std::thread::Builder::new()
        .name("e6p4-row9-driver".to_string())
        .spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                with_parallel_admission_enabled_for_test(|| {
                    let mut ctx = green_context(true);
                    let setup = eval_printed(
                        &mut ctx,
                        "(progn
                           (setq row9-m (make-mutex \"row9-m\"))
                           (setq row9-c (make-condition-variable row9-m \"row9-c\"))
                           (setq row9-ready nil)
                           (setq row9-th
                                 (neomacs-make-parallel-thread
                                  (lambda ()
                                    (mutex-lock row9-m)
                                    (mutex-lock row9-m)
                                    (setq row9-ready t)
                                    (let ((e (condition-case err
                                                 (progn
                                                   (condition-wait row9-c)
                                                   'not-signalled)
                                               (error err))))
                                      (mutex-unlock row9-m)
                                      (mutex-unlock row9-m)
                                      (list (if (eq e 'not-signalled) e (car (cdr e)))
                                            'recursion-restored)))
                                  \"row9-condition-signal\"))
                           (while (null row9-ready)
                             (thread-yield))
                           (mutex-lock row9-m)
                           (neomacs-thread-parallel-p row9-th))",
                    );
                    assert_eq!(setup, "t");
                    let th_val = ctx.eval_str("row9-th").expect("row9-th");
                    let th_id = crate::emacs_core::threads::expect_thread_id(&ctx.threads, &th_val)
                        .expect("row9-th thread id");
                    let control = ctx.green_control();
                    assert!(
                        matches!(
                            control.wait_of(ThreadKey::from_id(th_id)),
                            Some(crate::emacs_core::eval::green::Wait::Condition(_))
                        ),
                        "Control::wait_of must report Wait::Condition for parked opt-in parallel owner (got {:?})",
                        control.wait_of(ThreadKey::from_id(th_id))
                    );

                    let out = eval_printed(
                        &mut ctx,
                        "(progn
                           (thread-signal row9-th 'error '(\"wake-opt-in\"))
                           (mutex-unlock row9-m)
                           (let* ((joined (list (condition-case e (thread-join row9-th)
                                                  (error (list 'join-raised (car (cdr e)))))
                                                (thread-join row9-th)))
                                  (dier (neomacs-make-parallel-thread
                                         (lambda () (signal 'wrong-type-argument '(row9-dies))))))
                             (thread-join dier)
                             (list row9-ready joined (thread-last-error t) (thread-last-error)
                                   (neomacs-thread-parallel-p dier))))",
                    );
                    assert_eq!(
                        out,
                        "(t ((join-raised \"wake-opt-in\") (\"wake-opt-in\" recursion-restored)) (wrong-type-argument row9-dies) nil t)"
                    );
                    ctx.green_teardown();
                });
            }));
            let _ = done_tx.send(result.map_err(|p| {
                if let Some(s) = p.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = p.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "panic".to_string()
                }
            }));
        })
        .expect("spawn row9 driver thread");

    match done_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => {
            handle.join().expect("row9 driver thread joined cleanly");
        }
        Ok(Err(msg)) => {
            let _ = handle.join();
            panic!("Slice A row9 failed: {msg}");
        }
        Err(_) => {
            panic!("Slice A row9 timed out: thread-signal failed to wake parked opt-in owner");
        }
    }
}

/// **Slice A (`#14342` Q4-(b) §4.2 row 1)** red-first regression row:
/// `(message ...)` inside an admitted opt-in parallel owner's `SerialTurn` MUST
/// invoke `set-message-function` (not skip it via a whole-turn `gc_inhibit_depth`).
#[test]
fn e6p4_row10a_q4b_message_runs_set_message_function_inside_era_turn() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let out = eval_printed(
            &mut ctx,
            "(progn
               (setq q4b-msg-log nil)
               (setq q4b-owner
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (let ((noninteractive nil)
                              (set-message-function
                               (lambda (msg)
                                 (setq q4b-msg-log (cons msg q4b-msg-log))
                                 msg)))
                          (message \"q4b-ping\"))
                        q4b-msg-log)
                      \"q4b-message\"))
               (list (neomacs-thread-parallel-p q4b-owner)
                     (thread-join q4b-owner)))",
        );
        assert_eq!(
            out, "(t (\"q4b-ping\"))",
            "Q4-(b): set-message-function must run inside an admitted opt-in owner's SerialTurn"
        );
        ctx.green_teardown();
    });
}

/// **Slice A (`#14342` Q4-(b) §4.2 row 2)** red-first regression row:
/// `(thread-yield)` inside an admitted opt-in parallel owner's `SerialTurn` MUST
/// return `nil` (GNU's answer) rather than signaling `"Cannot switch threads
/// while garbage collection is inhibited"`.
#[test]
fn e6p4_row10b_q4b_thread_yield_inside_era_turn_is_not_refused() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let out = eval_printed(
            &mut ctx,
            "(progn
               (setq q4b-yield-owner
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (condition-case err
                            (list 'ok (thread-yield))
                          (error (list 'err err))))
                      \"q4b-yield\"))
               (list (neomacs-thread-parallel-p q4b-yield-owner)
                     (thread-join q4b-yield-owner)))",
        );
        assert_eq!(
            out, "(t (ok nil))",
            "Q4-(b): (thread-yield) inside an admitted opt-in owner's SerialTurn must not be refused as GC-inhibited"
        );
        ctx.green_teardown();
    });
}

/// **Slice A (`#14342` Q4-(b) + `#15237` `inhibit_base` condition)**:
/// Proves that removing `gc_inhibit_depth += 1` from `SerialHost::enter_turn`
/// preserves every `with_world_collectible` / `inhibit_base` invariant:
/// 1. Threshold GC stays inhibited inside `SerialTurn` (`in_active_serial_turn()`),
///    while `gc_inhibit_depth == 0` so `set-message-function` and `thread-yield` work.
/// 2. Explicit `(garbage-collect)` routes through the keeper inside `with_world_collectible`.
/// 3. A genuine nested `with_gc_inhibited` (`gc_inhibit_depth == 1`) inhibits BOTH
///    explicit `(garbage-collect)` AND `set-message-function` / `thread-yield`,
///    and leaving it restores both.
#[test]
fn e6p4_row10c_q4b_inhibit_base_and_nested_gc_inhibit_inside_era_turn() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let gc_before = ctx.tagged_heap.gc_collections();
        let out = eval_printed(
            &mut ctx,
            "(progn
               (setq q4b-c-owner
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (let* ((before-gc (garbage-collect))
                               (ran-msg nil)
                               (noninteractive nil)
                               (set-message-function
                                (lambda (m) (setq ran-msg m) m)))
                          (message \"q4b-c-ok\")
                          (list (consp before-gc) ran-msg (thread-yield))))
                      \"q4b-c\"))
               (list (neomacs-thread-parallel-p q4b-c-owner)
                     (thread-join q4b-c-owner)))",
        );
        assert_eq!(out, "(t (t \"q4b-c-ok\" nil))");
        assert!(
            ctx.tagged_heap.gc_collections() > gc_before,
            "Q4-(b): explicit (garbage-collect) inside opt-in owner SerialTurn must still collect via keeper"
        );
        ctx.green_teardown();
    });
}

/// **E6-P.4 Row 12 (`#16120` + `#16127` Condition 1):**
/// Proves an admitted opt-in parallel owner running through `neomacs-make-parallel-thread`
/// sees `(eq (current-thread) self)` BEFORE its first park, AFTER `(thread-yield)`,
/// and AFTER `(condition-wait c)` with Main running Lisp on Baton (thread 0) in
/// between, and successfully executes self-owned `(mutex-unlock ...)` on both a
/// second held mutex and the condition variable's mutex after reattach.
#[test]
fn e6p4_row12_opt_in_owner_current_thread_and_mutex_unlock_after_park_reattach() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        let out = eval_printed(
            &mut ctx,
            "(progn
               (setq r12-m1 (make-mutex \"r12-m1\"))
               (setq r12-m2 (make-mutex \"r12-m2\"))
               (setq r12-c (make-condition-variable r12-m1 \"r12-c\"))
               (setq r12-parked nil)
               (setq r12-main-tid (current-thread))
               (setq r12-owner nil)
               (setq r12-owner
                     (neomacs-make-parallel-thread
                      (lambda ()
                        (let* ((ct-entry (current-thread))
                               (_ (mutex-lock r12-m2))
                               (_ (mutex-lock r12-m1))
                               (_ (thread-yield))
                               (ct-after-yield (current-thread))
                               (_ (setq r12-parked t))
                               (_ (condition-wait r12-c))
                               (ct-after-wait (current-thread)))
                          ;; Self-owned unlocks AFTER park/reattach fail if
                          ;; ThreadManager::current_thread was clobbered to Main (0).
                          (mutex-unlock r12-m2)
                          (mutex-unlock r12-m1)
                          (list (eq ct-entry r12-owner)
                                (eq ct-after-yield r12-owner)
                                (eq ct-after-wait r12-owner)
                                (null (eq ct-after-wait r12-main-tid)))))
                      \"r12-identity\"))
               (while (and (null r12-parked) (thread-live-p r12-owner))
                 (thread-yield))
               (if (null r12-parked)
                   (signal 'error
                           (list \"r12-owner exited before parking\"
                                 (thread-last-error))))
               ;; Interleave Main Lisp work as thread 0 while r12-owner is parked.
               (let ((main-still-main (eq (current-thread) r12-main-tid)))
                 (mutex-lock r12-m1)
                 (condition-notify r12-c)
                 (mutex-unlock r12-m1)
                 (list main-still-main
                       (neomacs-thread-parallel-p r12-owner)
                       (thread-join r12-owner)
                       (eq (current-thread) r12-main-tid))))",
        );
        assert_eq!(
            out, "(t t (t t t t) t)",
            "Row 12: admitted opt-in owner must preserve (current-thread) and self-owned mutex-unlock across yield and condition-wait park/reattach"
        );
        ctx.green_teardown();
        assert_eq!(
            eval_printed(&mut ctx, "(eq (current-thread) r12-main-tid)"),
            "t",
            "Row 12: Main must also preserve thread 0 identity after live era teardown"
        );
    });
}

/// **E6-P.4 Row 13a (`#16127` Condition 2(i)) — separate falsifiability of `SERIAL_TURN_COLLECT_SCOPE`:**
/// Proves that a live serial turn with ONLY `SERIAL_TURN_PARK_SCOPE` installed
/// (`with_world_parkable`, NO `with_collect_scope`) keeps explicit `(garbage-collect)`
/// inhibited (`active_serial_collect_inhibit_base() == None`,
/// `is_explicit_gc_inhibited() == true`, zero direct STW collection).
#[test]
fn e6p4_row13a_park_only_scope_inhibits_explicit_gc_separately_falsifiable() {
    use crate::emacs_core::eval::parallel_entry::keeper_driver::{ChannelPort, Driver, GroupPort};
    use crate::emacs_core::eval::parallel_entry::owned_source::{PreparedEraStart, keeper_serve};
    use crate::emacs_core::eval::parallel_entry::serial_park::{
        KeeperPort, LiveSerial, PortRefusal, SerialHome, receive_initial_seat,
    };
    use crate::emacs_core::eval::parallel_entry::serial_turn::OwnerSerialState;
    use crate::emacs_core::eval::parallel_entry::thread_control::{
        ParallelOwnerControl, retire_mutator_rooted, with_world_parkable,
    };
    use crate::emacs_core::eval::parallel_entry::world_seat::ParkedWorldSeat;
    use crate::emacs_core::eval::world::{
        freeze_live_legacy_roots, prepare_reference_free_transfer, recover_transferred_source,
    };
    use neovm_value::tagged::gc::Gen1Admission;

    let mut ctx = green_context(true);
    let control = ctx.green_control();
    let _recovery = ctx.retire_empty_dead_windows_table();
    ctx.tagged_heap.close_alloc_regions();
    ctx.prepare_parked_root_retirement()
        .expect("prepare parked root retirement");
    let parked = unsafe { ParkedWorldSeat::park_seated(&mut ctx) }.expect("park Main");
    let right = ctx
        .green_seat()
        .expect("Main seated")
        .take_baton()
        .expect("Main holds Solo");
    let baton = control.surrender_solo_for_era(right).expect("surrender");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
    let world_handle = baton.era_world_handle().expect("era_world_handle");
    let owner_wh = world_handle.try_clone().expect("clone era_world_handle");
    let heap_ptr = ctx.tagged_heap.heap_root().as_ptr();
    let collector = unsafe { (*heap_ptr).drain_to_quiescent(|_| {}) }.expect("drain");
    let admission = Gen1Admission::check(collector).expect("GEN1");
    let frozen = unsafe { freeze_live_legacy_roots(&baton) };
    let (pending_return, transfer) =
        unsafe { prepare_reference_free_transfer(admission, baton, frozen) }.expect("transfer");
    let saved_obs = crate::emacs_core::eval::green::stash_current_thread_observations();
    let prepared = PreparedEraStart::from_heap_transfer(transfer);

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let owner_key = ThreadKey::from_id(42);
    let registered_owner =
        crate::emacs_core::eval::parallel_entry::thread_control::RegisteredParallelOwner::register(
            std::sync::Arc::clone(&control),
            owner_key,
        );
    let keeper = std::thread::spawn(move || {
        keeper_serve(prepared, |era| {
            std::thread::scope(|scope| {
                let (inbox_tx, inbox_rx) = mpsc::channel::<KeeperMsg>();
                let mut driver = Driver::new(era, inbox_rx);
                let group_link = driver.enroll_group();
                let group_port = GroupPort::new(inbox_tx.clone(), group_link);
                let permit = era.register(1, 64).expect("register owner");
                let key = permit.key();
                let home = SerialHome {
                    heap: era.heap_identity(),
                    owner: 1,
                };
                let (grants_tx, grants_rx) = mpsc::sync_channel(1);
                driver.enroll(1, key, grants_tx);
                let publishing_owners = driver.publishing_owners_counter();
                ready_tx
                    .send((group_port, inbox_tx.clone()))
                    .expect("send ready");

                let owner_handle = scope.spawn(move || {
                    let mut owner_ctl = ParallelOwnerControl::from_registered(registered_owner);
                    let mutator = permit.activate(home.heap, home.owner).expect("activate");
                    let mut port = ChannelPort::new(
                        home.owner,
                        mutator,
                        inbox_tx,
                        grants_rx,
                        publishing_owners,
                    );
                    let grant = loop {
                        match port.request_turn() {
                            Ok(g) => break g,
                            Err(PortRefusal::Retry) => std::thread::yield_now(),
                            Err(e) => panic!("unexpected port refusal: {e:?}"),
                        }
                    };
                    let (seat, local, guard) = receive_initial_seat(home, grant, &mut port, |_| {
                        Ok::<(), std::convert::Infallible>(())
                    })
                    .expect("receive seat");
                    let owner_ctx = owner_wh.spawn_unseated_serial_context(&seat);
                    let mut state = OwnerSerialState::new(home.owner, Box::new(owner_ctx))
                        .ok()
                        .expect("OwnerSerialState::new");
                    let turn = state.begin(local).ok().expect("begin turn");
                    let mut live = LiveSerial::from_parts(turn, seat, guard, home);
                    with_world_parkable(
                        &mut live,
                        &mut owner_ctl,
                        &mut port,
                        |_| Ok::<(), std::convert::Infallible>(()),
                        |ev| {
                            assert_eq!(
                                ev.active_serial_collect_inhibit_base(),
                                None,
                                "park-only scope must NOT install SERIAL_TURN_COLLECT_SCOPE"
                            );
                            assert!(
                                ev.is_explicit_gc_inhibited(),
                                "park-only scope must keep explicit GC inhibited"
                            );
                            let before = ev.tagged_heap.gc_collections();
                            let _ = ev.eval_str("(garbage-collect)");
                            assert_eq!(
                                ev.tagged_heap.gc_collections(),
                                before,
                                "park-only scope must NOT run direct STW collection on (garbage-collect)"
                            );
                        },
                    );
                    live.end(&mut port).ok().expect("end live turn");
                    let mutator = port.exit();
                    retire_mutator_rooted(mutator).expect("retire mutator");
                    drop(state);
                    drop(owner_wh);
                    crate::emacs_core::eval::green::clear_current_thread_observations();
                    owner_ctl.unregister();
                });

                driver.run();
                owner_handle.join().expect("owner thread joined");
            });
        })
        .expect("keeper_serve")
        .0
    });

    let (group_port, inbox_tx) = ready_rx.recv().expect("keeper ready");
    control
        .open_era(world_handle, group_port)
        .expect("open_era");
    assert!(control.wait_all_parallel_participants_exited(Duration::from_secs(5)));
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().reset_observations_to_placeholder());
    crate::emacs_core::eval::green::clear_current_thread_observations();
    if let Some(gp) = control.take_era_port_for_exit() {
        gp.exit();
    }
    drop(inbox_tx);
    let returned_end = keeper.join().expect("keeper joined");
    crate::emacs_core::eval::green::restore_current_thread_observations(saved_obs);
    let restored = pending_return
        .restore(returned_end.into_heap_transfer())
        .expect("restore");
    let (baton, _roots, pub_) = unsafe { recover_transferred_source(restored) }.expect("recover");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
    let (solo_right, era_wh, _) = control.close_era(baton).expect("close_era");
    drop(era_wh);
    parked.resume_with_turn_right(&mut ctx, solo_right);
    let _ = crate::emacs_core::eval::green::retire_transfer_publication_on_main(&mut ctx, pub_);
    ctx.green_teardown();
}

/// **E6-P.4 Row 13b (`#16127` Condition 2(ii)) — separate falsifiability of `SERIAL_TURN_PARK_SCOPE`:**
/// Proves that a live serial turn with ONLY `SERIAL_TURN_COLLECT_SCOPE` installed
/// (`with_world_collectible`, NO `with_park_scope_on_frame`) routes `(garbage-collect)`
/// through the keeper while `(condition-wait c)` signals
/// `"called in an unbridged parallel serial turn"`.
#[test]
fn e6p4_row13b_collect_only_scope_refuses_condition_wait_separately_falsifiable() {
    use crate::emacs_core::eval::parallel_entry::keeper_driver::{ChannelPort, Driver, GroupPort};
    use crate::emacs_core::eval::parallel_entry::owned_source::{PreparedEraStart, keeper_serve};
    use crate::emacs_core::eval::parallel_entry::serial_park::{
        KeeperPort, LiveSerial, PortRefusal, SerialHome, receive_initial_seat,
    };
    use crate::emacs_core::eval::parallel_entry::serial_turn::OwnerSerialState;
    use crate::emacs_core::eval::parallel_entry::thread_control::retire_mutator_rooted;
    use crate::emacs_core::eval::parallel_entry::world_seat::ParkedWorldSeat;
    use crate::emacs_core::eval::world::{
        freeze_live_legacy_roots, prepare_reference_free_transfer, recover_transferred_source,
    };
    use neovm_value::tagged::gc::Gen1Admission;

    let mut ctx = green_context(true);
    let control = ctx.green_control();
    let _recovery = ctx.retire_empty_dead_windows_table();
    ctx.tagged_heap.close_alloc_regions();
    ctx.prepare_parked_root_retirement()
        .expect("prepare parked root retirement");
    let parked = unsafe { ParkedWorldSeat::park_seated(&mut ctx) }.expect("park Main");
    let right = ctx
        .green_seat()
        .expect("Main seated")
        .take_baton()
        .expect("Main holds Solo");
    let baton = control.surrender_solo_for_era(right).expect("surrender");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
    let world_handle = baton.era_world_handle().expect("era_world_handle");
    let owner_wh = world_handle.try_clone().expect("clone era_world_handle");
    let heap_ptr = ctx.tagged_heap.heap_root().as_ptr();
    let collector = unsafe { (*heap_ptr).drain_to_quiescent(|_| {}) }.expect("drain");
    let admission = Gen1Admission::check(collector).expect("GEN1");
    let frozen = unsafe { freeze_live_legacy_roots(&baton) };
    let (pending_return, transfer) =
        unsafe { prepare_reference_free_transfer(admission, baton, frozen) }.expect("transfer");
    let saved_obs = crate::emacs_core::eval::green::stash_current_thread_observations();
    let prepared = PreparedEraStart::from_heap_transfer(transfer);

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (owner_done_tx, owner_done_rx) = mpsc::sync_channel(1);
    let keeper = std::thread::spawn(move || {
        keeper_serve(prepared, |era| {
            std::thread::scope(|scope| {
                let (inbox_tx, inbox_rx) = mpsc::channel::<KeeperMsg>();
                let mut driver = Driver::new(era, inbox_rx);
                let group_link = driver.enroll_group();
                let group_port = GroupPort::new(inbox_tx.clone(), group_link);
                let permit = era.register(1, 64).expect("register owner");
                let key = permit.key();
                let home = SerialHome {
                    heap: era.heap_identity(),
                    owner: 1,
                };
                let (grants_tx, grants_rx) = mpsc::sync_channel(1);
                driver.enroll(1, key, grants_tx);
                let publishing_owners = driver.publishing_owners_counter();
                ready_tx
                    .send((group_port, inbox_tx.clone()))
                    .expect("send ready");

                let owner_handle = scope.spawn(move || {
                    let mutator = permit.activate(home.heap, home.owner).expect("activate");
                    let mut port = ChannelPort::new(
                        home.owner,
                        mutator,
                        inbox_tx,
                        grants_rx,
                        publishing_owners,
                    );
                    let grant = loop {
                        match port.request_turn() {
                            Ok(g) => break g,
                            Err(PortRefusal::Retry) => std::thread::yield_now(),
                            Err(e) => panic!("unexpected port refusal: {e:?}"),
                        }
                    };
                    let (seat, local, guard) = receive_initial_seat(home, grant, &mut port, |_| {
                        Ok::<(), std::convert::Infallible>(())
                    })
                    .expect("receive seat");
                    let owner_ctx = owner_wh.spawn_unseated_serial_context(&seat);
                    let mut state = OwnerSerialState::new(home.owner, Box::new(owner_ctx))
                        .ok()
                        .expect("OwnerSerialState::new");
                    let turn = state.begin(local).ok().expect("begin turn");
                    let mut live = LiveSerial::from_parts(turn, seat, guard, home);
                    live.with_world_collectible(
                        &mut port,
                        |_| Ok::<(), std::convert::Infallible>(()),
                        |ev| {
                            assert_eq!(
                                ev.active_serial_collect_inhibit_base(),
                                Some(0),
                                "collect-only scope must install SERIAL_TURN_COLLECT_SCOPE"
                            );
                            let before = ev.tagged_heap.gc_collections();
                            let gc_res = ev.eval_str("(consp (garbage-collect))").expect("gc ok");
                            assert_eq!(print_value(&gc_res), "t");
                            assert!(
                                ev.tagged_heap.gc_collections() > before,
                                "collect-only scope must still route (garbage-collect) through keeper"
                            );
                            let cw_res = ev
                                .eval_str(
                                    "(let* ((m (make-mutex \"r13b-m\"))
                                            (c (make-condition-variable m \"r13b-c\")))
                                       (mutex-lock m)
                                       (condition-case err
                                           (progn (condition-wait c) 'unexpected)
                                         (error (car (cdr err)))))",
                                )
                                .expect("condition-case catches unbridged refusal");
                            let msg = print_value(&cw_res);
                            assert!(
                                msg.contains("condition-wait called in an unbridged parallel serial turn"),
                                "collect-only scope must fail closed on (condition-wait) without SERIAL_TURN_PARK_SCOPE, got: {msg}"
                            );
                        },
                    );
                    live.end(&mut port).ok().expect("end live turn");
                    let mutator = port.exit();
                    retire_mutator_rooted(mutator).expect("retire mutator");
                    drop(state);
                    drop(owner_wh);
                    crate::emacs_core::eval::green::clear_current_thread_observations();
                    let _ = owner_done_tx.send(());
                });

                driver.run();
                owner_handle.join().expect("owner thread joined");
            });
        })
        .expect("keeper_serve")
        .0
    });

    let (group_port, inbox_tx) = ready_rx.recv().expect("keeper ready");
    control
        .open_era(world_handle, group_port)
        .expect("open_era");
    owner_done_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("owner finished collect-only assertions");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().reset_observations_to_placeholder());
    crate::emacs_core::eval::green::clear_current_thread_observations();
    if let Some(gp) = control.take_era_port_for_exit() {
        gp.exit();
    }
    drop(inbox_tx);
    let returned_end = keeper.join().expect("keeper joined");
    crate::emacs_core::eval::green::restore_current_thread_observations(saved_obs);
    let restored = pending_return
        .restore(returned_end.into_heap_transfer())
        .expect("restore");
    let (baton, _roots, pub_) = unsafe { recover_transferred_source(restored) }.expect("recover");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
    let (solo_right, era_wh, _) = control.close_era(baton).expect("close_era");
    drop(era_wh);
    parked.resume_with_turn_right(&mut ctx, solo_right);
    let _ = crate::emacs_core::eval::green::retire_transfer_publication_on_main(&mut ctx, pub_);
    ctx.green_teardown();
}

/// **DEFECT #15497 / #15498 Pin Row 1 (`e6p4_row11a`)**:
/// When `neomacs-make-parallel-thread` refuses era admission (here via a real
/// unsupported heap payload `alloc_sqlite`, exercising the exact `supported(heap)`
/// refusal class hit on pdump permanent weak hash tables), the caller's
/// `TaggedHeap` TLS MUST stay installed and fallback to `make-thread` MUST
/// return a live Baton thread with `neomacs-thread-parallel-p` = `nil`,
/// `(thread-join th)` = `42`, and a subsequent `(thread-join (make-thread (lambda () 42)))` = `42`.
#[test]
fn e6p4_row11a_refused_keeper_start_keeps_heap_tls_and_falls_back_cleanly() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        // Install a real unsupported payload (`VecLikeType::Sqlite`) rooted in
        // a Lisp symbol so `supported(heap)` genuinely refuses era admission.
        let sqlite = ctx.tagged_heap.alloc_sqlite(false, 7);
        ctx.set_variable("row11a-sqlite-anchor", sqlite);

        let out = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () 42) \"row11a-probe\")))
               (list (neomacs-thread-parallel-p th)
                     (thread-join th)
                     (thread-join (make-thread (lambda () 42)))))",
        );
        assert!(
            neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap),
            "#15497: caller TaggedHeap TLS must remain installed after refused opt-in admission"
        );
        assert_eq!(
            out, "(nil 42 42)",
            "#15497 pin: refused admission must fall back cleanly and join 42"
        );
        ctx.green_teardown();
    });
}

/// **DEFECT #15498 Row 2 (`e6p4_row11b`)**:
/// After a refused keeper start / unsupported heap preflight, `Control` MUST
/// return to `SoloActive` and MUST NOT remain stuck in `SoloSurrendered`.
#[test]
fn e6p4_row11b_refused_keeper_start_does_not_leave_control_solo_surrendered() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        // Seat Main first so we can inspect `Control` before and after refusal.
        assert_eq!(
            eval_printed(&mut ctx, "(thread-join (make-thread (lambda () 1)))"),
            "1"
        );
        let control = ctx.green_control();
        assert!(control.era_state_is_solo_active());

        let sqlite = ctx.tagged_heap.alloc_sqlite(false, 9);
        ctx.set_variable("row11b-sqlite-anchor", sqlite);

        let out = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () 42) \"row11b-probe\")))
               (list (neomacs-thread-parallel-p th) (thread-join th)))",
        );
        assert_eq!(out, "(nil 42)");
        assert!(
            neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap),
            "#15497: caller TaggedHeap TLS must stay installed"
        );
        assert!(
            control.era_state_is_solo_active(),
            "#15498: Control must return to SoloActive (not remain SoloSurrendered) after keeper-start refusal"
        );
        assert!(!control.era_is_open());
        assert_eq!(
            ctx.ensure_live_parallel_era(),
            Err(LiveEraOpenError::HeapUnsupported),
            "#15497: unsupported heap payload must refuse with named HeapUnsupported before transfer"
        );
        assert!(control.era_state_is_solo_active());
        assert!(neovm_value::tagged::gc::tagged_heap_is_current(
            &ctx.tagged_heap
        ));
        ctx.green_teardown();
    });
}

/// **DEFECT #15497 / #15498 + Nit 5 (`e6p4_row11c`)**:
/// An OS keeper-thread spawn failure (`LiveEraOpenFault::FailKeeperSpawn`)
/// happens while `Baton` is still non-optionally owned by `LiveEraOpenGuard`
/// (`SoloSurrendered`), so rollback cleanly restores Main's `TaggedHeap` TLS
/// and `Control::SoloActive`, the refused call falls back to `make-thread` and
/// joins `42`, AND a subsequent clean `(neomacs-make-parallel-thread ...)`
/// succeeds with `neomacs-thread-parallel-p` = `t`.
#[test]
fn e6p4_row11c_keeper_spawn_failure_restores_tls_and_solo_active_and_allows_retry() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        assert_eq!(
            eval_printed(&mut ctx, "(thread-join (make-thread (lambda () 1)))"),
            "1"
        );
        let control = ctx.green_control();
        assert!(control.era_state_is_solo_active());

        set_era_open_fault_for_test(Some(LiveEraOpenFault::FailKeeperSpawn));
        let refused_out = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () 42) \"row11c-refused\")))
               (list (neomacs-thread-parallel-p th)
                     (thread-join th)
                     (thread-join (make-thread (lambda () 42)))))",
        );
        assert_eq!(refused_out, "(nil 42 42)");
        assert!(
            neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap),
            "#15497/Nit 5: caller TaggedHeap TLS must remain installed after FailKeeperSpawn"
        );
        assert!(
            control.era_state_is_solo_active(),
            "#15498/Nit 5: Control must return to SoloActive after FailKeeperSpawn"
        );
        assert!(!control.era_is_open());

        // Subsequent clean opt-in admission on the same World must succeed.
        let retry_out = eval_printed(
            &mut ctx,
            "(let ((th (neomacs-make-parallel-thread (lambda () (+ 20 22)) \"row11c-retry\")))
               (list (neomacs-thread-parallel-p th) (thread-join th)))",
        );
        assert_eq!(retry_out, "(t 42)");
        assert!(control.era_is_open());
        ctx.green_teardown();
    });
}

/// **DEFECT #15498 Blocker-3 Unwind Row (`e6p4_row11d`)**:
/// A panic unwind after `spawn_idle_keeper()` + `pull_observations_to_tls()`
/// (while `LiveEraOpenGuard<IdleKeeperSpawned>` owns both `Baton` and the idle
/// keeper thread, with ZERO `Committed` sentinel) MUST cleanly stop and join
/// the idle keeper thread, restore observations, return `Control` to
/// `SoloActive`, reinstall Main's `TaggedHeap` TLS, and allow subsequent
/// `make-thread` and `neomacs-make-parallel-thread` calls to succeed.
#[test]
fn e6p4_row11d_panic_after_idle_keeper_spawn_restores_tls_and_solo_active() {
    with_parallel_admission_enabled_for_test(|| {
        let mut ctx = green_context(true);
        assert_eq!(
            eval_printed(&mut ctx, "(thread-join (make-thread (lambda () 1)))"),
            "1"
        );
        let control = ctx.green_control();
        assert!(control.era_state_is_solo_active());

        set_era_open_fault_for_test(Some(LiveEraOpenFault::PanicAfterIdleKeeperSpawn));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = ctx.ensure_live_parallel_era();
        }));
        assert!(
            caught.is_err(),
            "PanicAfterIdleKeeperSpawn must unwind through LiveEraOpenGuard<IdleKeeperSpawned>"
        );
        assert!(
            neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap),
            "Main TaggedHeap TLS must be reinstalled after IdleKeeperSpawned panic unwind"
        );
        assert!(
            control.era_state_is_solo_active(),
            "Control must return to SoloActive after IdleKeeperSpawned panic unwind"
        );
        assert!(!control.era_is_open());

        let after_out = eval_printed(
            &mut ctx,
            "(list (thread-join (make-thread (lambda () 42)))
                   (let ((th (neomacs-make-parallel-thread (lambda () 42) \"row11d-retry\")))
                     (list (neomacs-thread-parallel-p th) (thread-join th))))",
        );
        assert_eq!(after_out, "(42 (t 42))");
        assert!(control.era_is_open());
        ctx.green_teardown();
    });
}

/// **E6-P.4-FU1 Row 14 (MEDIUM #1):**
/// Proves `ChannelPort::publish_idle()` cannot bypass the `publishing_owners`
/// Dekker guard: when an owner that is already `native` calls `publish_idle()`
/// while the Baton group holds an active `WorldTurn` stop, `publish_idle()`
/// runs through `with_published_receipt` (`PublishingGuard` + `KeeperMsg::Wake`),
/// raises the group's `ring_waiter` so the group releases its stop instead of
/// deadlocking in `leave_native()`, succeeds with `Ok(())`, and restores
/// `publishing_owners` to `0`.
#[test]
fn e6p4_row14_publish_idle_uses_dekker_guard_while_group_holds_world() {
    use crate::emacs_core::eval::parallel_entry::keeper_driver::{ChannelPort, Driver, GroupPort};
    use crate::emacs_core::eval::parallel_entry::owned_source::{PreparedEraStart, keeper_serve};
    use crate::emacs_core::eval::parallel_entry::serial_park::{PortRefusal, SerialHome};
    use crate::emacs_core::eval::parallel_entry::thread_control::retire_mutator_rooted;
    use crate::emacs_core::eval::parallel_entry::world_seat::ParkedWorldSeat;
    use crate::emacs_core::eval::world::{
        freeze_live_legacy_roots, prepare_reference_free_transfer, recover_transferred_source,
    };
    use neovm_value::tagged::gc::Gen1Admission;

    let mut ctx = green_context(true);
    let control = ctx.green_control();
    let _recovery = ctx.retire_empty_dead_windows_table();
    ctx.tagged_heap.close_alloc_regions();
    ctx.prepare_parked_root_retirement()
        .expect("prepare parked root retirement");
    let parked = unsafe { ParkedWorldSeat::park_seated(&mut ctx) }.expect("park Main");
    let right = ctx
        .green_seat()
        .expect("Main seated")
        .take_baton()
        .expect("Main holds Solo");
    let baton = control.surrender_solo_for_era(right).expect("surrender");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
    let heap_ptr = ctx.tagged_heap.heap_root().as_ptr();
    let collector = unsafe { (*heap_ptr).drain_to_quiescent(|_| {}) }.expect("drain");
    let admission = Gen1Admission::check(collector).expect("GEN1");
    let frozen = unsafe { freeze_live_legacy_roots(&baton) };
    let (pending_return, transfer) =
        unsafe { prepare_reference_free_transfer(admission, baton, frozen) }.expect("transfer");
    let saved_obs = crate::emacs_core::eval::green::stash_current_thread_observations();
    let prepared = PreparedEraStart::from_heap_transfer(transfer);

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (first_idle_tx, first_idle_rx) = mpsc::sync_channel(1);
    let (world_held_tx, world_held_rx) = mpsc::sync_channel(1);

    let keeper = std::thread::spawn(move || {
        keeper_serve(prepared, |era| {
            std::thread::scope(|scope| {
                let (inbox_tx, inbox_rx) = mpsc::channel::<KeeperMsg>();
                let mut driver = Driver::new(era, inbox_rx);
                let group_link = driver.enroll_group();
                let group_port = GroupPort::new(inbox_tx.clone(), group_link);
                let permit = era.register(1, 64).expect("register owner");
                let key = permit.key();
                let home = SerialHome {
                    heap: era.heap_identity(),
                    owner: 1,
                };
                let (grants_tx, grants_rx) = mpsc::sync_channel(1);
                driver.enroll(1, key, grants_tx);
                let publishing_owners = driver.publishing_owners_counter();
                let pub_check = std::sync::Arc::clone(&publishing_owners);
                ready_tx
                    .send((group_port, inbox_tx.clone(), pub_check))
                    .expect("send ready");

                let owner_handle = scope.spawn(move || {
                    let mutator = permit.activate(home.heap, home.owner).expect("activate");
                    let mut port = ChannelPort::new(
                        home.owner,
                        mutator,
                        inbox_tx,
                        grants_rx,
                        publishing_owners,
                    );
                    // First `publish_idle()` enters native mode (`self.native = true`).
                    loop {
                        match port.publish_idle() {
                            Ok(()) => break,
                            Err(PortRefusal::Retry) => std::thread::yield_now(),
                            Err(other) => panic!("first publish_idle failed: {other:?}"),
                        }
                    }
                    first_idle_tx.send(()).expect("signal first publish_idle");
                    // Wait until Baton group holds an active `WorldTurn` stop.
                    world_held_rx
                        .recv()
                        .expect("wait for Baton group WorldTurn");
                    // Second `publish_idle()` must execute `leave_native()` under
                    // `PublishingGuard` + `KeeperMsg::Wake` and wake the group.
                    loop {
                        match port.publish_idle() {
                            Ok(()) => break,
                            Err(PortRefusal::Retry) => std::thread::yield_now(),
                            Err(other) => panic!("second publish_idle failed: {other:?}"),
                        }
                    }
                    let mutator = port.exit();
                    retire_mutator_rooted(mutator).expect("retire mutator");
                });

                let _ = driver.run();
                owner_handle.join().expect("owner joined");
            });
        })
        .expect("keeper_serve clean")
        .0
    });

    let (mut group_port, inbox_tx, pub_check) = ready_rx.recv().expect("keeper ready");
    first_idle_rx
        .recv()
        .expect("owner finished first publish_idle");
    let (turn, _epoch, lend) = group_port
        .request_world()
        .expect("group acquires WorldTurn");
    assert!(
        !group_port.ring_waiter(),
        "Row 14: WorldTurn must start with ring_waiter lowered before second publish_idle()"
    );
    world_held_tx.send(()).expect("signal WorldTurn held");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !group_port.ring_waiter() {
        assert!(
            std::time::Instant::now() < deadline,
            "Row 14: publish_idle() did not raise group ring_waiter via Dekker guard + Wake"
        );
        std::thread::yield_now();
    }
    // Sample `publishing_owners` while `WorldTurn` is still held: the stop is
    // open, so `leave_native()` cannot have returned and `PublishingGuard` is
    // deterministically held (`== 1`).
    let held_while_world_open = pub_check.load(std::sync::atomic::Ordering::SeqCst);
    group_port.give_back(
        lend,
        crate::emacs_core::eval::parallel_entry::world_turn::WorldReturned::from_end(turn),
    );
    group_port.exit();
    drop(inbox_tx);

    let returned_end = keeper.join().expect("keeper joined");
    assert_eq!(
        held_while_world_open, 1,
        "Row 14: publish_idle() must hold publishing_owners == 1 while WorldTurn stop is open"
    );
    assert_eq!(
        pub_check.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "Row 14: publishing_owners must return to 0 after publish_idle()"
    );
    crate::emacs_core::eval::green::restore_current_thread_observations(saved_obs);
    let restored = pending_return
        .restore(returned_end.into_heap_transfer())
        .expect("restore");
    let (baton, _roots, pub_) = unsafe { recover_transferred_source(restored) }.expect("recover");
    ctx.with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
    let right = control
        .restore_surrendered_solo(baton)
        .expect("restore surrendered solo");
    parked.resume_with_turn_right(&mut ctx, right);
    let _ = crate::emacs_core::eval::green::retire_transfer_publication_on_main(&mut ctx, pub_);
    ctx.green_teardown();
}
