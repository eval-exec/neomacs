//! Eval-side hook publication entry and the typed boot proof (stage-4 E3a).
//!
//! Core's one process-level `install_eval_hooks()` calls
//! [`install_eval_hooks`] here with the single canonical [`EvalHookSet`].
//! This entry publishes every cold PC2 slot the evaluator needs, in one
//! FIXED order, and mints [`EvalHooksInstalled`] only after the last
//! publication succeeds. Every eval `Context` constructor (`new_inner`,
//! `from_dump`, and the bootstrap constructors) takes the proof by value, so
//! the GC-root-enumerating slots provably exist before the first possible
//! allocation (peer E3a, refining E3).
//!
//! The proof is a `Copy` zero-sized type with a private constructor.
//! Installation is process-permanent (each table's `OnceLock`), so an
//! existing `Context` implies the proof; eval-internal worker/owner
//! constructors obtain it from the parent through
//! [`Context::hooks_installed`] and never re-install.
//!
//! A conflicting second set is the existing typed refusal
//! (`HookInstallError::DifferentTable`), never ignored. The same set
//! re-publishes as `AlreadyInstalled` and still yields the proof.

use super::*;
use neovm_value::HookInstallError;
use neovm_value::HookInstallStatus;
use neovm_value::error::Flow;
use neovm_value::value::Value;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Installer for the process-global sqlite object-finalizer table.
///
/// The slot returns the typed publication status so [`install_eval_hooks`]
/// propagates a conflict (`HookInstallError::DifferentTable`) instead of
/// swallowing it in release builds (peer follow-up F1).
pub type SqliteFinalizerInstaller = fn() -> Result<HookInstallStatus, HookInstallError>;

/// The one core-owned set of eval cold tables.
///
/// Field order is the publication order and is part of the contract: the
/// lifecycle descriptor (which carries the GC-root enumerators and the
/// pdump-reconstruct resets) publishes first.
#[derive(Clone, Copy)]
pub struct EvalHookSet {
    lifecycle: &'static lifecycle::LifecycleHooks,
    process: &'static process_hooks::ProcessHooks,
    input: &'static input_hooks::InputHooks,
    call_interactively: &'static call_interactively::CallInteractivelyHooks,
    case_tables: &'static case_tables::CaseTableHooks,
    buffer_kill: &'static buffer_kill::BufferKillHooks,
    kmacro: &'static kmacro_hooks::KmacroHooks,
    display_query: &'static display_query_hooks::DisplayQueryHooks,
    perf_enabled: bool,
    perf: &'static perf::PerfHooks,
    /// The C14 subr declaration file's registrar. Unlike the descriptor
    /// slots this takes the Context being constructed, so it is published at
    /// boot but invoked from each constructor.
    register_core_subrs: fn(&mut Context),
    /// The sqlite object-finalizer table (a no-op without the feature).
    ///
    /// Returns the typed publication status so a conflicting table is
    /// propagated by [`install_eval_hooks`] (peer follow-up F1).
    sqlite: SqliteFinalizerInstaller,
    wait: &'static WaitServiceHooks,
    timer: &'static TimerServiceHooks,
    frontend: &'static FrontendServiceHooks,
    display: &'static DisplayServiceHooks,
    file_notify: &'static FileNotifyHostQueries,
}

/// Wait/special-input service upcalls, called per command boundary from the
/// evaluator's input loop (never per bytecode step).
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct WaitServiceHooks {
    #[doc(hidden)]
    pub service_input_pending_with_timers: fn(&mut Context) -> Result<(), Flow>,
    #[doc(hidden)]
    pub service_read_char_preliminary_redisplay:
        fn(&mut Context, keyboard::IdleTransitionPolicy) -> Result<(), Flow>,
    #[doc(hidden)]
    pub service_timers_without_redisplay: fn(&mut Context) -> Result<(), Flow>,
    #[doc(hidden)]
    pub wait_for_command_input_with_idle_policy:
        fn(
            &mut Context,
            Option<Instant>,
            keyboard::IdleTransitionPolicy,
        ) -> Result<input_hooks::CommandInputWaitOutcome, Flow>,
    #[doc(hidden)]
    pub wait_for_resize_ack_until: fn(&mut Context, Instant) -> Result<bool, Flow>,
}

/// Timer-deadline upcalls, read once per input-wait decision.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct TimerServiceHooks {
    #[doc(hidden)]
    pub next_ordinary_gnu_timer_timeout_before: fn(&Context) -> Option<Duration>,
    #[doc(hidden)]
    pub next_idle_gnu_timer_timeout: fn(&Context) -> Option<Duration>,
}

/// Frontend host-event upcalls, per frontend event.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct FrontendServiceHooks {
    #[doc(hidden)]
    pub apply_xwidget_frontend_event:
        fn(&mut Context, &keyboard::FrontendWebViewEvent) -> Result<bool, Flow>,
}

/// Display scheduling queries, per redisplay / command boundary.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct DisplayServiceHooks {
    #[doc(hidden)]
    pub compositor_scrolling_enabled:
        fn(&Context, neovm_edit::emacs_core::display_metrics::WindowId) -> bool,
    #[doc(hidden)]
    pub permits_compositor_pixel_scroll:
        fn(&Context, neovm_edit::emacs_core::display_metrics::WindowId) -> bool,
    #[doc(hidden)]
    pub redisplay_buffer_layout_inputs: fn(
        &Context,
        neovm_edit::buffer::BufferId,
    )
        -> Option<data::layout_input::BufferLayoutInputState>,
}

/// System file-layer queries the file-notify backends need (one call per
/// watch add; a missing table is refused at the call site).
#[derive(Clone, Copy)]
pub struct FileNotifyHostQueries {
    pub expand_file_name: fn(&mut Context, Vec<Value>) -> neovm_value::error::EvalResult,
    pub directory_file_name: fn(&mut Context, Vec<Value>) -> neovm_value::error::EvalResult,
    pub file_exists_p: fn(&mut Context, Vec<Value>) -> neovm_value::error::EvalResult,
}

impl EvalHookSet {
    /// Build the one canonical set core owns.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one install site
    pub const fn new(
        lifecycle: &'static lifecycle::LifecycleHooks,
        process: &'static process_hooks::ProcessHooks,
        input: &'static input_hooks::InputHooks,
        call_interactively: &'static call_interactively::CallInteractivelyHooks,
        case_tables: &'static case_tables::CaseTableHooks,
        buffer_kill: &'static buffer_kill::BufferKillHooks,
        kmacro: &'static kmacro_hooks::KmacroHooks,
        display_query: &'static display_query_hooks::DisplayQueryHooks,
        perf_enabled: bool,
        perf: &'static perf::PerfHooks,
        register_core_subrs: fn(&mut Context),
        sqlite: SqliteFinalizerInstaller,
        wait: &'static WaitServiceHooks,
        timer: &'static TimerServiceHooks,
        frontend: &'static FrontendServiceHooks,
        display: &'static DisplayServiceHooks,
        file_notify: &'static FileNotifyHostQueries,
    ) -> Self {
        Self {
            lifecycle,
            process,
            input,
            call_interactively,
            case_tables,
            buffer_kill,
            kmacro,
            display_query,
            perf_enabled,
            perf,
            register_core_subrs,
            sqlite,
            wait,
            timer,
            frontend,
            display,
            file_notify,
        }
    }
}

/// Proof that every eval cold table was published in this process.
///
/// `Copy` zero-sized type with a private constructor; installation is
/// process-permanent, and every constructor requires the value.
#[derive(Clone, Copy)]
pub struct EvalHooksInstalled(());

impl EvalHooksInstalled {
    /// Mint the proof. Private (S-1): the only callers are the publication
    /// entry (after the last slot) and `Context::hooks_installed` (an
    /// existing Context implies permanent installation). No crate outside
    /// this module may forge it.
    fn mint() -> Self {
        Self(())
    }
}

/// Set once the publication entry has returned the proof at least once.
/// Not TLS and not stored in `Context`; the proof carries no state.
static HOOKS_INSTALLED: OnceLock<()> = OnceLock::new();

/// Publish every eval cold table in the fixed order and mint the boot proof.
///
/// Core's `install_eval_hooks` is the exact caller. The proof is minted only
/// after the last publication succeeds; a conflicting set returns the typed
/// refusal before any proof exists. Re-running with the same set publishes
/// each slot as `AlreadyInstalled` and still returns the proof.
/// Both publication statuses are success at this entry: `Installed` is the
/// first boot, `AlreadyInstalled` is the documented idempotent re-run.  Bind
/// and match explicitly instead of discarding the must-use status.
fn accept_hook_status(status: HookInstallStatus) {
    match status {
        HookInstallStatus::Installed | HookInstallStatus::AlreadyInstalled => {}
    }
}

#[doc(hidden)]
pub fn install_eval_hooks(
    set: &'static EvalHookSet,
) -> Result<EvalHooksInstalled, HookInstallError> {
    // 1. Lifecycle: the GC-root-enumerating dynamic-module slot, the
    //    pdump-reconstruct resets and the boot entries publish first.
    accept_hook_status(lifecycle::install_lifecycle_hooks(set.lifecycle)?);
    // 2. Process and input wait/OS-signal slots.
    accept_hook_status(process_hooks::install_process_hooks(set.process)?);
    accept_hook_status(input_hooks::install_input_hooks(set.input)?);
    // 3. Command-layer cold tables.
    accept_hook_status(call_interactively::install_call_interactively_hooks(
        set.call_interactively,
    )?);
    accept_hook_status(case_tables::install_case_table_hooks(set.case_tables)?);
    accept_hook_status(buffer_kill::install_buffer_kill_hooks(set.buffer_kill)?);
    accept_hook_status(kmacro_hooks::install_kmacro_hooks(set.kmacro)?);
    // 4. Display policy queries.
    accept_hook_status(display_query_hooks::install_display_query_hooks(
        set.display_query,
    )?);
    // 5. Hot-path timing pair (also carries its enable answer).
    accept_hook_status(perf::install_perf_hooks(set.perf_enabled, set.perf)?);
    // 6. Process-global feature tables, then the runtime upcall tables (S3):
    //    published last; none enumerates GC roots or participates in pdump
    //    reconstruction. The sqlite slot answers with the typed publication
    //    status, so a conflicting finalizer table reaches the caller instead
    //    of failing a release-mode assertion (peer follow-up F1).
    accept_hook_status((set.sqlite)()?);
    publish(&WAIT_SERVICE, set.wait)?;
    publish(&TIMER_SERVICE, set.timer)?;
    publish(&FRONTEND_SERVICE, set.frontend)?;
    publish(&DISPLAY_SERVICE, set.display)?;
    publish(&FILE_NOTIFY_QUERIES, set.file_notify)?;
    // The proof exists only now: every GC-root-enumerating and
    // pdump-reconstruct slot above is published.
    INSTALLED_SET.get_or_init(|| set);
    HOOKS_INSTALLED.get_or_init(|| ());
    Ok(EvalHooksInstalled::mint())
}

static INSTALLED_SET: OnceLock<&'static EvalHookSet> = OnceLock::new();
static WAIT_SERVICE: OnceLock<&'static WaitServiceHooks> = OnceLock::new();
static TIMER_SERVICE: OnceLock<&'static TimerServiceHooks> = OnceLock::new();
static FRONTEND_SERVICE: OnceLock<&'static FrontendServiceHooks> = OnceLock::new();
static DISPLAY_SERVICE: OnceLock<&'static DisplayServiceHooks> = OnceLock::new();
static FILE_NOTIFY_QUERIES: OnceLock<&'static FileNotifyHostQueries> = OnceLock::new();

fn publish<T: 'static>(
    slot: &'static OnceLock<&'static T>,
    table: &'static T,
) -> Result<(), HookInstallError> {
    // The identity rule is the one `neovm_value` exports so that this
    // publisher and `value::hook_publication::publish` cannot drift apart:
    // the first offered descriptor wins, re-publishing the same descriptor is
    // idempotent, and a different descriptor is the typed refusal.  See the
    // helper's docstring for why `OnceLock::set`'s error cannot answer the
    // conflict (its payload is the rejected argument, not the resident value).
    neovm_value::select_published_identity(slot, table).map(|_| ())
}

fn installed() -> &'static EvalHookSet {
    INSTALLED_SET
        .get()
        .copied()
        .expect("the evaluator hook set is published before any Context can exist")
}

/// Register the core-resident C14 subr catalog on a Context under
/// construction. The registrar is published by core's install entry.
pub(crate) fn register_core_subrs(ctx: &mut Context) {
    (installed().register_core_subrs)(ctx)
}

/// The file-notify host queries; a missing table is a typed refusal at the
/// call site (the only runtime accessor here, because watch-add has a Lisp
/// error path).  Only the macOS/Windows backends (and the Linux cfg(test)
/// mirrors of them) call this accessor.
#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(test, target_os = "linux")
))]
pub(crate) fn file_notify_host_queries() -> Option<&'static FileNotifyHostQueries> {
    FILE_NOTIFY_QUERIES.get().copied()
}

/// Wait/special-input service upcalls.
pub(crate) mod wait {
    use super::*;

    pub(crate) fn service_input_pending_with_timers(ctx: &mut Context) -> Result<(), Flow> {
        (installed().wait.service_input_pending_with_timers)(ctx)
    }

    pub(crate) fn service_read_char_preliminary_redisplay(
        ctx: &mut Context,
        idle_policy: keyboard::IdleTransitionPolicy,
    ) -> Result<(), Flow> {
        (installed().wait.service_read_char_preliminary_redisplay)(ctx, idle_policy)
    }

    pub(crate) fn service_timers_without_redisplay(ctx: &mut Context) -> Result<(), Flow> {
        (installed().wait.service_timers_without_redisplay)(ctx)
    }

    pub(crate) fn wait_for_command_input_with_idle_policy(
        ctx: &mut Context,
        deadline: Option<Instant>,
        idle_policy: keyboard::IdleTransitionPolicy,
    ) -> Result<input_hooks::CommandInputWaitOutcome, Flow> {
        (installed().wait.wait_for_command_input_with_idle_policy)(ctx, deadline, idle_policy)
    }

    pub(crate) fn wait_for_resize_ack_until(
        ctx: &mut Context,
        deadline: Instant,
    ) -> Result<bool, Flow> {
        (installed().wait.wait_for_resize_ack_until)(ctx, deadline)
    }
}

/// Timer-deadline upcalls.
pub(crate) mod timer {
    use super::*;

    pub(crate) fn next_ordinary_gnu_timer_timeout_before(ctx: &Context) -> Option<Duration> {
        (installed().timer.next_ordinary_gnu_timer_timeout_before)(ctx)
    }

    pub(crate) fn next_idle_gnu_timer_timeout(ctx: &Context) -> Option<Duration> {
        (installed().timer.next_idle_gnu_timer_timeout)(ctx)
    }
}

/// Frontend host-event upcalls.
pub(crate) mod frontend {
    use super::*;

    pub(crate) fn apply_xwidget_frontend_event(
        ctx: &mut Context,
        event: &keyboard::FrontendWebViewEvent,
    ) -> Result<bool, Flow> {
        (installed().frontend.apply_xwidget_frontend_event)(ctx, event)
    }
}

/// Display scheduling queries.
pub(crate) mod display {
    use super::*;

    pub(crate) fn compositor_scrolling_enabled(
        ctx: &Context,
        window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> bool {
        (installed().display.compositor_scrolling_enabled)(ctx, window)
    }

    pub(crate) fn permits_compositor_pixel_scroll(
        ctx: &Context,
        window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> bool {
        (installed().display.permits_compositor_pixel_scroll)(ctx, window)
    }

    pub(crate) fn redisplay_buffer_layout_inputs(
        ctx: &Context,
        buffer: neovm_edit::buffer::BufferId,
    ) -> Option<data::layout_input::BufferLayoutInputState> {
        (installed().display.redisplay_buffer_layout_inputs)(ctx, buffer)
    }
}

impl Context {
    /// The boot proof for an eval-internal worker/owner constructor.
    ///
    /// An existing `Context` can only have been built with the proof, and
    /// installation is process-permanent, so the value is always available.
    /// These constructors never re-install.
    pub fn hooks_installed(&self) -> EvalHooksInstalled {
        assert!(
            HOOKS_INSTALLED.get().is_some(),
            "an existing Context implies a published eval hook set"
        );
        EvalHooksInstalled::mint()
    }
}
