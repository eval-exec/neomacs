//! The one production select and native-wait token registry (peer 5695).
//! Blocking waits clone/remove their value before releasing the map lock.

use super::{
    MainWaitPreparation, NativeWaitGuard, NativeWaitToken, SelectArm, SelectArmToken,
    SelectHandleToken,
};
use crate::emacs_core::eval::green_select::{GreenSelectHandle, MainNativePreparation};
use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWake};
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

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

/// Register a manager's actual backend with the production select registry.
/// The Context entry and scheduler fixtures both use this one path.
pub(crate) fn register_manager(
    manager: &crate::emacs_core::eval::data::process_state::ProcessManager,
) -> Option<SelectHandleToken> {
    register_select_handle(manager.green_select_handle()?)
}

pub(super) fn select_notify(token: SelectHandleToken) {
    let _ = with_select(token, GreenSelectHandle::notify);
}

pub(super) fn select_nudge(
    token: SelectHandleToken,
    destination: ControlDestination,
    reason: ControlNudge,
) {
    let _ = with_select(token, |handle| handle.nudge(destination, reason));
}

pub(super) fn select_control_source(token: SelectHandleToken) -> Option<Arc<ControlWake>> {
    with_select(token, GreenSelectHandle::control_source)
}

pub(super) fn select_wait_until(token: SelectHandleToken, deadline: Instant) -> bool {
    // A missing handle must not block: report activity so the scheduler
    // rechecks its own state instead of sleeping on an unknown source.
    // The map lock is dropped before the wait so a concurrent `notify` can
    // take it (see [`select_handle`]).
    select_handle(token)
        .map(|handle| handle.wait_until(deadline))
        .unwrap_or(true)
}

pub(super) fn select_arm(token: SelectHandleToken) -> SelectArm {
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

pub(super) fn select_wait_registered(
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

pub(super) fn cancel_select_arm(registration: SelectArmToken) {
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

pub(super) fn prepare_main_wait(
    ctx: &mut crate::emacs_core::eval::Context,
    deadline: Instant,
) -> MainWaitPreparation {
    match ctx.processes().prepare_green_main_wait(deadline) {
        MainNativePreparation::Ready => MainWaitPreparation::Ready,
        MainNativePreparation::Scheduler => MainWaitPreparation::Scheduler,
        MainNativePreparation::Prepared(wait) => {
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

pub(super) fn wait_main_native(
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

pub(super) fn cancel_native_wait(token: NativeWaitToken) {
    let _ = native_waits()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&token.raw().get());
}
