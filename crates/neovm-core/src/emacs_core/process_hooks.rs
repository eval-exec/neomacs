//! Canonical process-IO hook descriptor (stage-4 W4/E10).
//!
//! ProcessManager and its IO methods stay in the process module; this module
//! only assembles the one descriptor eval publishes at `Context::new_inner` /
//! `Context::from_dump`.  `WaitNotifier` is eval-owned since A3 and has a slot;
//! the remaining host-effect types are deferred to W5/E13 per peer 13:3xZ.

#![allow(private_interfaces)] // host_effect/switch vocabulary becomes eval-owned at W5/E13
use crate::emacs_core::eval::process_hooks::{
    MainWaitHooks, MainWaitPreparation, NativeWaitGuard, NativeWaitToken, ProcessHooks, SelectArm,
    SelectArmToken, SelectHandleToken, SelectHooks,
};
use crate::emacs_core::process::GreenSelectHandle;
use crate::emacs_core::process::SystemProcessExt;
use crate::heap_types::ProcessId;
use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWake};
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

fn unlink_owned_unix_listener_for_exit(ctx: &mut crate::emacs_core::eval::Context, id: ProcessId) {
    #[cfg(unix)]
    {
        ctx.processes_mut().unlink_owned_unix_listener_for_exit(id);
    }
    #[cfg(not(unix))]
    {
        let _ = (ctx, id);
    }
}

fn delete_process(ctx: &mut crate::emacs_core::eval::Context, id: ProcessId) -> bool {
    ctx.processes_mut().delete_process(id)
}

fn open_channel_for_module(
    ctx: &mut crate::emacs_core::eval::Context,
    process: crate::emacs_core::value::Value,
) -> Result<std::ffi::c_int, crate::emacs_core::error::Flow> {
    ctx.processes().open_channel_for_module(process)
}

fn print_process_handle(value: &crate::emacs_core::value::Value) -> Option<String> {
    crate::emacs_core::process::print_process_handle(value)
}

fn wait_notifier(
    ctx: &crate::emacs_core::eval::Context,
) -> Option<crate::emacs_core::eval::data::process_state::WaitNotifier> {
    ctx.processes().wait_notifier()
}

fn terminate_and_defer_reaping(
    child: &mut crate::emacs_core::eval::data::process_state::ChildOwnership,
    kill_signal: i32,
) {
    use crate::emacs_core::process::reap::ChildOwnershipIo;
    child.terminate_and_defer_reaping(kill_signal);
}

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

fn next_token() -> Option<NonZeroU64> {
    NonZeroU64::new(NEXT_TOKEN.fetch_add(1, Ordering::Relaxed))
}

static SELECT_HANDLES: OnceLock<Mutex<HashMap<u64, Arc<GreenSelectHandle>>>> = OnceLock::new();
static SELECT_REGISTRATIONS: OnceLock<
    Mutex<HashMap<u64, crate::host_effect::wake::WaitRegistration>>,
> = OnceLock::new();

fn select_handles() -> &'static Mutex<HashMap<u64, Arc<GreenSelectHandle>>> {
    SELECT_HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn select_registrations() -> &'static Mutex<HashMap<u64, crate::host_effect::wake::WaitRegistration>>
{
    SELECT_REGISTRATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_select_handle(handle: GreenSelectHandle) -> Option<SelectHandleToken> {
    let raw = next_token()?;
    select_handles()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(raw.get(), Arc::new(handle));
    Some(SelectHandleToken::from_raw(raw))
}

fn with_select<R>(token: SelectHandleToken, f: impl FnOnce(&GreenSelectHandle) -> R) -> Option<R> {
    let handles = select_handles()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    handles.get(&token.raw().get()).map(|handle| f(handle))
}

/// Clone the handle out of the map. A blocking wait must not hold this mutex:
/// `notify` takes the same lock, and a main thread whose own select deadline
/// has elapsed has to nudge the scheduler while that wait is still parked
/// (`src/thread.c:613-643` releases the global lock for the caller's own
/// `thread_select` timeout, not for a peer's).
fn select_handle(token: SelectHandleToken) -> Option<Arc<GreenSelectHandle>> {
    select_handles()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&token.raw().get())
        .cloned()
}

fn eval_select_handle(ctx: &mut crate::emacs_core::eval::Context) -> Option<SelectHandleToken> {
    register_select_handle(ctx.processes().green_select_handle()?)
}

/// Test seam: register a manager's select handle without a Context.
#[cfg(test)]
pub(crate) fn select_handle_for_manager(
    manager: &crate::emacs_core::process::ProcessManager,
) -> Option<SelectHandleToken> {
    register_select_handle(manager.green_select_handle()?)
}

fn select_notify(token: SelectHandleToken) {
    let _ = with_select(token, GreenSelectHandle::notify);
}

fn select_nudge(token: SelectHandleToken, destination: ControlDestination, reason: ControlNudge) {
    let _ = with_select(token, |handle| handle.nudge(destination, reason));
}

fn select_control_source(token: SelectHandleToken) -> Option<Arc<ControlWake>> {
    with_select(token, GreenSelectHandle::control_source)
}

fn select_wait_until(token: SelectHandleToken, deadline: Instant) -> bool {
    // A missing handle must not block: report activity so the scheduler
    // rechecks its own state instead of sleeping on an unknown source.
    // The map lock is dropped before the wait so a concurrent `notify` can
    // take it (see [`select_handle`]).
    select_handle(token)
        .map(|handle| handle.wait_until(deadline))
        .unwrap_or(true)
}

fn select_arm(token: SelectHandleToken) -> SelectArm {
    match with_select(token, GreenSelectHandle::arm) {
        Some(Ok(registration)) => {
            let Some(raw) = next_token() else {
                return SelectArm::Failed;
            };
            select_registrations()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(raw.get(), registration);
            SelectArm::Armed(SelectArmToken::from_raw(raw))
        }
        Some(Err(_)) => SelectArm::Failed,
        None => SelectArm::Unavailable,
    }
}

fn select_wait_registered(
    token: SelectHandleToken,
    registration: SelectArmToken,
    deadline: Instant,
) -> bool {
    let registration = select_registrations()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&registration.raw().get());
    let Some(registration) = registration else {
        return true;
    };
    let Some(handle) = select_handle(token) else {
        return true;
    };
    handle.wait_registered(&registration, deadline)
}

fn cancel_select_arm(registration: SelectArmToken) {
    let _ = select_registrations()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&registration.raw().get());
}

static NATIVE_WAITS: OnceLock<
    Mutex<HashMap<u64, crate::host_effect::wake::native_wait::PreparedNativeWait>>,
> = OnceLock::new();

fn native_waits()
-> &'static Mutex<HashMap<u64, crate::host_effect::wake::native_wait::PreparedNativeWait>> {
    NATIVE_WAITS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prepare_main_wait(
    ctx: &mut crate::emacs_core::eval::Context,
    deadline: Instant,
) -> MainWaitPreparation {
    match ctx.processes().prepare_green_main_wait(deadline) {
        crate::emacs_core::process::MainNativePreparation::Ready => MainWaitPreparation::Ready,
        crate::emacs_core::process::MainNativePreparation::Scheduler => {
            MainWaitPreparation::Scheduler
        }
        crate::emacs_core::process::MainNativePreparation::Prepared(wait) => {
            let Some(raw) = next_token() else {
                return MainWaitPreparation::Scheduler;
            };
            native_waits()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(raw.get(), wait);
            MainWaitPreparation::Native(NativeWaitGuard::from_raw(NativeWaitToken::from_raw(raw)))
        }
    }
}

fn wait_main_native(
    token: NativeWaitToken,
    handoff: &mut crate::emacs_core::eval::MainNativeHandoff<'_>,
) -> std::io::Result<crate::host_effect::wake::ControlWakeOutcome> {
    let wait = native_waits()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&token.raw().get());
    let Some(mut wait) = wait else {
        return Err(std::io::Error::other("native wait token is not live"));
    };
    wait.wait_with(handoff)
}

fn cancel_native_wait(token: NativeWaitToken) {
    let _ = native_waits()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&token.raw().get());
}

static CANONICAL_PROCESS_HOOKS: ProcessHooks = ProcessHooks::new(
    unlink_owned_unix_listener_for_exit,
    delete_process,
    open_channel_for_module,
    print_process_handle,
    wait_notifier,
    terminate_and_defer_reaping,
    SelectHooks::new(
        eval_select_handle,
        select_notify,
        select_nudge,
        select_control_source,
        select_wait_until,
        select_arm,
        select_wait_registered,
        cancel_select_arm,
    ),
    MainWaitHooks::new(prepare_main_wait, wait_main_native, cancel_native_wait),
);

/// The one canonical process descriptor (published only by the one core
/// boot entry, peer follow-up F2).
pub(crate) fn canonical_process_hooks() -> &'static ProcessHooks {
    &CANONICAL_PROCESS_HOOKS
}
