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
            install_sqlite_finalizer_hook,
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
fn install_sqlite_finalizer_hook() {
    crate::emacs_core::sqlite::install_sqlite_finalizer_hook();
}

#[cfg(not(feature = "sqlite"))]
fn install_sqlite_finalizer_hook() {}

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
