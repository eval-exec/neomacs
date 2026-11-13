//! The one core process-level eval boot entry (stage-4 E3a).
//!
//! Core owns exactly one boot entry, [`install_eval_hooks`]. It delegates to
//! the eval-side publication entry with the single canonical [`EvalHookSet`],
//! which publishes every eval PC2 slot in a fixed order and mints the
//! [`EvalHooksInstalled`] proof only after the last slot succeeds. Eval
//! `Context` constructors take that proof by value, so the GC-root-enumerating
//! and pdump-reconstruct slots provably exist before the first possible
//! allocation (peer E3a, refining E3).
//!
//! The command/display modules keep the canonical descriptor statics and the
//! bodies; this module only assembles the set and is the exact caller recorded
//! by the eval publication entry's PC2 row.
//!
//! Frequency: construction / dump restore, once per process. Not a hot path.

use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::hooks_proof::{
    DisplayServiceHooks, EvalHookSet, EvalHooksInstalled, FileNotifyHostQueries,
    FrontendServiceHooks, TimerServiceHooks, WaitServiceHooks,
};
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

static CANONICAL_EVAL_HOOK_SET: OnceLock<EvalHookSet> = OnceLock::new();

/// Publish every eval cold table and return the boot proof.
///
/// Idempotent: a second call re-publishes the same set as `AlreadyInstalled`
/// and returns the proof again. A conflicting set is the eval entry's typed
/// refusal, which this process-level entry treats as a fatal invariant
/// violation (two canonical sets can never exist).
pub fn install_eval_hooks() -> EvalHooksInstalled {
    let set = CANONICAL_EVAL_HOOK_SET.get_or_init(|| {
        EvalHookSet::new(
            crate::emacs_core::lifecycle_hooks::canonical_lifecycle_hooks(),
            crate::emacs_core::process_hooks::canonical_process_hooks(),
            crate::emacs_core::input_hooks::canonical_input_hooks(),
            crate::emacs_core::interactive::canonical_call_interactively_hooks(),
            crate::emacs_core::casetab_entries::canonical_case_table_hooks(),
            crate::emacs_core::buffer_ops::canonical_buffer_kill_hooks(),
            crate::emacs_core::kmacro::canonical_kmacro_hooks(),
            crate::emacs_core::xdisp::canonical_display_query_hooks(),
            crate::emacs_core::perf_trace::canonical_perf_enabled(),
            crate::emacs_core::perf_trace::canonical_perf_hooks(),
            crate::emacs_core::builtins::init_builtins,
            install_sqlite_finalizer_hook_for_eval_hooks,
            &CANONICAL_WAIT_SERVICE,
            &CANONICAL_TIMER_SERVICE,
            &CANONICAL_FRONTEND_SERVICE,
            &CANONICAL_DISPLAY_SERVICE,
            &CANONICAL_FILE_NOTIFY_QUERIES,
        )
    });
    match crate::emacs_core::eval::hooks_proof::install_eval_hooks(set) {
        Ok(proof) => proof,
        Err(_) => panic!("a different eval hook set is already installed"),
    }
}

#[cfg(feature = "sqlite")]
fn install_sqlite_finalizer_hook_for_eval_hooks() -> Result<HookInstallStatus, HookInstallError> {
    crate::emacs_core::sqlite::install_sqlite_finalizer_hook()
}

#[cfg(not(feature = "sqlite"))]
fn install_sqlite_finalizer_hook_for_eval_hooks() -> Result<HookInstallStatus, HookInstallError> {
    // No finalizer table is compiled in; report it as published so the one
    // boot entry stays uniform across feature configurations.
    Ok(HookInstallStatus::Installed)
}

// The wait/timer/frontend/display slots call the upper-layer service
// implementations in place. They run per command boundary / per frontend
// event, never on the evaluator hot path.
use crate::emacs_core::neo::scroll::ScrollContextExt;
use crate::emacs_core::timer::TimerContextExt;
use crate::emacs_core::wait::WaitContextExt;
use crate::emacs_core::xwidget::XwidgetContextExt;
use crate::window::display::WindowDisplayContextExt;

fn wait_service_input_pending_with_timers(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<(), neovm_value::error::Flow> {
    ctx.service_input_pending_with_timers()
}

fn wait_service_read_char_preliminary_redisplay(
    ctx: &mut crate::emacs_core::eval::Context,
    idle_policy: crate::emacs_core::eval::keyboard::IdleTransitionPolicy,
) -> Result<(), neovm_value::error::Flow> {
    ctx.service_read_char_preliminary_redisplay(idle_policy)
}

fn wait_service_timers_without_redisplay(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<(), neovm_value::error::Flow> {
    ctx.service_timers_without_redisplay()
}

fn wait_for_command_input_with_idle_policy(
    ctx: &mut crate::emacs_core::eval::Context,
    deadline: Option<std::time::Instant>,
    idle_policy: crate::emacs_core::eval::keyboard::IdleTransitionPolicy,
) -> Result<crate::emacs_core::eval::input_hooks::CommandInputWaitOutcome, neovm_value::error::Flow>
{
    ctx.wait_for_command_input_with_idle_policy(deadline, idle_policy)
}

fn wait_for_resize_ack_until(
    ctx: &mut crate::emacs_core::eval::Context,
    deadline: std::time::Instant,
) -> Result<bool, neovm_value::error::Flow> {
    ctx.wait_for_resize_ack_until(deadline)
}

fn timer_next_ordinary_gnu_timer_timeout_before(
    ctx: &crate::emacs_core::eval::Context,
) -> Option<std::time::Duration> {
    ctx.next_ordinary_gnu_timer_timeout_before(None)
}

fn timer_next_idle_gnu_timer_timeout(
    ctx: &crate::emacs_core::eval::Context,
) -> Option<std::time::Duration> {
    ctx.next_idle_gnu_timer_timeout()
}

fn frontend_apply_xwidget_frontend_event(
    ctx: &mut crate::emacs_core::eval::Context,
    event: &crate::emacs_core::eval::keyboard::FrontendWebViewEvent,
) -> Result<bool, neovm_value::error::Flow> {
    ctx.apply_xwidget_frontend_event(event)
}

fn display_compositor_scrolling_enabled(
    ctx: &crate::emacs_core::eval::Context,
    window: neovm_edit::emacs_core::display_metrics::WindowId,
) -> bool {
    ctx.compositor_scrolling_enabled(window)
}

fn display_permits_compositor_pixel_scroll(
    ctx: &crate::emacs_core::eval::Context,
    window: neovm_edit::emacs_core::display_metrics::WindowId,
) -> bool {
    ctx.permits_compositor_pixel_scroll(window)
}

fn display_redisplay_buffer_layout_inputs(
    ctx: &crate::emacs_core::eval::Context,
    buffer: neovm_edit::buffer::BufferId,
) -> Option<crate::emacs_core::eval::data::layout_input::BufferLayoutInputState> {
    ctx.redisplay_buffer_layout_inputs(buffer)
}

static CANONICAL_WAIT_SERVICE: WaitServiceHooks = WaitServiceHooks {
    service_input_pending_with_timers: wait_service_input_pending_with_timers,
    service_read_char_preliminary_redisplay: wait_service_read_char_preliminary_redisplay,
    service_timers_without_redisplay: wait_service_timers_without_redisplay,
    wait_for_command_input_with_idle_policy: wait_for_command_input_with_idle_policy,
    wait_for_resize_ack_until: wait_for_resize_ack_until,
};

static CANONICAL_TIMER_SERVICE: TimerServiceHooks = TimerServiceHooks {
    next_ordinary_gnu_timer_timeout_before: timer_next_ordinary_gnu_timer_timeout_before,
    next_idle_gnu_timer_timeout: timer_next_idle_gnu_timer_timeout,
};

static CANONICAL_FRONTEND_SERVICE: FrontendServiceHooks = FrontendServiceHooks {
    apply_xwidget_frontend_event: frontend_apply_xwidget_frontend_event,
};

static CANONICAL_DISPLAY_SERVICE: DisplayServiceHooks = DisplayServiceHooks {
    compositor_scrolling_enabled: display_compositor_scrolling_enabled,
    permits_compositor_pixel_scroll: display_permits_compositor_pixel_scroll,
    redisplay_buffer_layout_inputs: display_redisplay_buffer_layout_inputs,
};

static CANONICAL_FILE_NOTIFY_QUERIES: FileNotifyHostQueries = FileNotifyHostQueries {
    expand_file_name: crate::emacs_core::fileio::builtin_expand_file_name,
    directory_file_name: crate::emacs_core::fileio::builtin_directory_file_name,
    file_exists_p: crate::emacs_core::fileio::builtin_file_exists_p,
};

/// Core facade constructor: boot the eval hooks, then build a full Context.
///
/// This is the only way for a core-side caller (neomacs, the worker, tests)
/// to construct the evaluator without naming the individual tables.
pub fn new_context() -> Context {
    let proof = install_eval_hooks();
    Context::new(proof)
}

#[cfg(test)]
mod concurrent_boot_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// Concurrent boot of several Contexts must not conflict with itself.
    ///
    /// The eval-side `publish` (neovm-eval
    /// `emacs_core/runtime/eval/hooks_proof.rs`) checks `slot.get()` and then
    /// calls `slot.set(table)`; the two are not atomic together, so two boots
    /// can both observe the empty slot.  The loser of `set` receives the
    /// winner's identical descriptor, but it was mapped unconditionally to
    /// `DifferentTable`, so this constructor panicked "a different eval hook
    /// set is already installed" whenever two threads booted at once.
    ///
    /// The spin barrier releases all workers together (std's futex barrier
    /// would stagger the wakeups past the publication window), so they enter
    /// `install_eval_hooks` simultaneously.  The first boot is the only one
    /// that can race: publication is process-permanent.
    #[test]
    fn concurrent_first_boot_of_contexts_does_not_conflict_with_itself() {
        const THREADS: usize = 16;
        let arrived = Arc::new(AtomicUsize::new(0));
        let go = Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    let arrived = Arc::clone(&arrived);
                    let go = Arc::clone(&go);
                    scope.spawn(move || {
                        arrived.fetch_add(1, Ordering::SeqCst);
                        while !go.load(Ordering::SeqCst) {
                            std::hint::spin_loop();
                        }
                        // A panic in install_eval_hooks fails this join, which
                        // is exactly the regression under test.
                        drop(new_context());
                        // The documented idempotent re-run path as well.
                        install_eval_hooks();
                    })
                })
                .collect();
            while arrived.load(Ordering::SeqCst) != THREADS {
                std::hint::spin_loop();
            }
            go.store(true, Ordering::SeqCst);
            for handle in handles {
                handle.join().unwrap();
            }
        });
    }

    /// Build the canonical eval hook set with WAIT as the wait-service table.
    fn set_with_wait(wait: &'static WaitServiceHooks) -> EvalHookSet {
        set_with_wait_and_perf(
            wait,
            crate::emacs_core::perf_trace::canonical_perf_enabled(),
        )
    }

    /// The same set with an explicit PERF_ENABLED answer.
    fn set_with_wait_and_perf(wait: &'static WaitServiceHooks, perf_enabled: bool) -> EvalHookSet {
        EvalHookSet::new(
            crate::emacs_core::lifecycle_hooks::canonical_lifecycle_hooks(),
            crate::emacs_core::process_hooks::canonical_process_hooks(),
            crate::emacs_core::input_hooks::canonical_input_hooks(),
            crate::emacs_core::interactive::canonical_call_interactively_hooks(),
            crate::emacs_core::casetab_entries::canonical_case_table_hooks(),
            crate::emacs_core::buffer_ops::canonical_buffer_kill_hooks(),
            crate::emacs_core::kmacro::canonical_kmacro_hooks(),
            crate::emacs_core::xdisp::canonical_display_query_hooks(),
            perf_enabled,
            crate::emacs_core::perf_trace::canonical_perf_hooks(),
            crate::emacs_core::builtins::init_builtins,
            install_sqlite_finalizer_hook_for_eval_hooks,
            wait,
            &CANONICAL_TIMER_SERVICE,
            &CANONICAL_FRONTEND_SERVICE,
            &CANONICAL_DISPLAY_SERVICE,
            &CANONICAL_FILE_NOTIFY_QUERIES,
        )
    }

    /// A race between two DIFFERENT descriptor tables must refuse the loser,
    /// never silently mint the proof for the winner's table.
    ///
    /// `OnceLock::set`'s `Err` payload is the rejected ARGUMENT, not the
    /// resident value, so a fix that compares that payload with the caller's
    /// own table always answers "same" and masks a genuine conflict.  This
    /// test offers two distinct wait tables (same callbacks, different
    /// address: publication identity is the address) from a spin barrier.
    /// Exactly one table may win; every caller offering the winner's table is
    /// accepted, and every caller offering the loser's table receives the
    /// typed refusal.  RED on the masked-fix commit.
    #[test]
    fn racing_two_different_tables_never_mints_a_masked_proof() {
        use crate::emacs_core::eval::hooks_proof::install_eval_hooks as install_with_set;

        const THREADS: usize = 16;
        let canonical_wait: &'static WaitServiceHooks = &CANONICAL_WAIT_SERVICE;
        let alternate_wait: &'static WaitServiceHooks = Box::leak(Box::new(CANONICAL_WAIT_SERVICE));
        assert!(!std::ptr::eq(canonical_wait, alternate_wait));
        let set_a: &'static EvalHookSet = Box::leak(Box::new(set_with_wait(canonical_wait)));
        let set_b: &'static EvalHookSet = Box::leak(Box::new(set_with_wait(alternate_wait)));

        let arrived = Arc::new(AtomicUsize::new(0));
        let go = Arc::new(AtomicBool::new(false));
        let results: Vec<(bool, Result<(), HookInstallError>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|index| {
                    let offers_b = index % 2 == 1;
                    let arrived = Arc::clone(&arrived);
                    let go = Arc::clone(&go);
                    scope.spawn(move || {
                        arrived.fetch_add(1, Ordering::SeqCst);
                        while !go.load(Ordering::SeqCst) {
                            std::hint::spin_loop();
                        }
                        let set = if offers_b { set_b } else { set_a };
                        (offers_b, install_with_set(set).map(|_proof| ()))
                    })
                })
                .collect();
            while arrived.load(Ordering::SeqCst) != THREADS {
                std::hint::spin_loop();
            }
            go.store(true, Ordering::SeqCst);
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });

        let a_results: Vec<_> = results
            .iter()
            .filter(|(b, _)| !*b)
            .map(|(_, r)| *r)
            .collect();
        let b_results: Vec<_> = results
            .iter()
            .filter(|(b, _)| *b)
            .map(|(_, r)| *r)
            .collect();
        let a_any_ok = a_results.contains(&Ok(()));
        let b_any_ok = b_results.contains(&Ok(()));
        assert_ne!(
            a_any_ok, b_any_ok,
            "exactly one offered table must win the slot: {results:?}"
        );
        let (winner, loser, loser_name) = if a_any_ok {
            (&a_results, &b_results, "alternate")
        } else {
            (&b_results, &a_results, "canonical")
        };
        assert!(
            winner.iter().all(|result| result.is_ok()),
            "a caller re-publishing the winning table was refused: {results:?}"
        );
        assert!(
            loser
                .iter()
                .all(|result| *result == Err(HookInstallError::DifferentTable)),
            "a caller offering the losing {loser_name} table was not refused \
             (masked conflict): {results:?}"
        );
    }

    /// PERF_ENABLED pins both outcomes: the same answer re-publishes
    /// idempotently, a different answer is the typed refusal (hook installers
    /// unification condition 3).
    ///
    /// PERF_ENABLED is a process-lifetime OnceLock beside the PerfHooks
    /// table, so the first boot fixes the answer and a later boot that
    /// disagrees must not mint a proof.  Nextest runs each test in its own
    /// process, so the cell starts empty here.
    #[test]
    fn perf_enabled_same_answer_is_idempotent_and_different_answer_is_refused() {
        use crate::emacs_core::eval::hooks_proof::install_eval_hooks as install_with_set;

        let first = crate::emacs_core::perf_trace::canonical_perf_enabled();
        let same: &'static EvalHookSet = Box::leak(Box::new(set_with_wait_and_perf(
            &CANONICAL_WAIT_SERVICE,
            first,
        )));
        let different: &'static EvalHookSet = Box::leak(Box::new(set_with_wait_and_perf(
            &CANONICAL_WAIT_SERVICE,
            !first,
        )));
        assert!(
            install_with_set(same).is_ok(),
            "the first publication of the canonical set must install"
        );
        assert!(
            install_with_set(same).is_ok(),
            "re-publishing the same PERF_ENABLED answer must stay idempotent"
        );
        assert_eq!(
            install_with_set(different).map(|_proof| ()),
            Err(HookInstallError::DifferentTable),
            "a different PERF_ENABLED answer must be refused, not overwritten"
        );
    }
}
