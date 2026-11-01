//! The Green scheduler's owned OS wait handle. It carries no process map,
//! Lisp value, or World borrow: only poll readiness and its control owner.

use super::{Arc, Instant, Ordering, ProcessManager};
use crate::host_effect::contracts::ReadDeadline;
use crate::host_effect::wake::native_wait::{NativeWaitPreparationError, PreparedNativeWait};
use crate::host_effect::wake::{
    ControlDestination, ControlNudge, ControlNudgeOutcome, ControlWake, ControlWakeOutcome,
    HostClosure, NativePollOwner, NotificationLatch, WaitRegistration,
};

#[derive(Debug)]
pub(crate) enum MainNativePreparation {
    Prepared(PreparedNativeWait),
    Ready,
    Scheduler,
}

/// An observer of the backend lifetime, not its session-retirement authority.
/// Dropping a temporary capture must not retire the backend's shared source.
pub(crate) struct GreenSelectHandle {
    wake: Arc<ControlWake>,
    notification_pending: Arc<NotificationLatch>,
    #[cfg(unix)]
    signal_source: Option<(Arc<polling::Poller>, std::os::fd::RawFd)>,
}

static_assertions::assert_impl_all!(GreenSelectHandle: Send, Sync);

impl GreenSelectHandle {
    /// Cold native-service capture of this backend's existing readiness owner.
    pub(crate) fn control_source(&self) -> Arc<ControlWake> {
        Arc::clone(&self.wake)
    }

    /// The durable scheduler transition precedes this hint. Native input keeps
    /// its original bit and notify path; control has independent destinations.
    pub(crate) fn notify(&self) {
        self.nudge(
            ControlDestination::IdleScheduler,
            ControlNudge::HostServiceReady,
        );
    }

    pub(crate) fn nudge(&self, destination: ControlDestination, reason: ControlNudge) {
        match self.wake.handle().nudge(destination, reason) {
            ControlNudgeOutcome::Notified
            | ControlNudgeOutcome::AlreadyPending
            | ControlNudgeOutcome::RetiredSession => {}
            ControlNudgeOutcome::WakeFailedAfterPublication(error) => {
                // The lower owner closes the registered write side, making
                // the kernel read side ready. Accepted work is never replayed.
                tracing::warn!(%error, "control wake failed after publication");
            }
        }
    }

    fn prepare_signal_source(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        if let Some((poller, fd)) = &self.signal_source {
            // SAFETY: the process signal installation owns this source longer
            // than any wait registration; only the native lease blocks on it.
            let source = unsafe { std::os::fd::BorrowedFd::borrow_raw(*fd) };
            if poller
                .modify(source, polling::Event::readable(usize::MAX - 1))
                .is_err()
            {
                return Err(std::io::Error::other("cannot rearm native signal source"));
            }
        }
        Ok(())
    }

    fn prepare_main_wait(&self, deadline: Instant) -> MainNativePreparation {
        // Claim/arm first; recheck the ordinary durable input bit afterwards.
        let wait = match PreparedNativeWait::new(
            &self.wake,
            NativePollOwner::NativeMain,
            ReadDeadline::At(deadline),
        ) {
            Ok(wait) => wait,
            Err(
                NativeWaitPreparationError::Closed(_) | NativeWaitPreparationError::PollBusy(_),
            ) => {
                return MainNativePreparation::Scheduler;
            }
        };
        if crate::emacs_core::os_signal::pending()
            || self.notification_pending.swap(false, Ordering::AcqRel)
            || wait.poll().is_some()
            || self.prepare_signal_source().is_err()
        {
            MainNativePreparation::Ready
        } else {
            MainNativePreparation::Prepared(wait)
        }
    }

    pub(crate) fn arm(&self) -> Result<WaitRegistration, HostClosure> {
        self.wake.arm(ControlDestination::IdleScheduler)
    }

    pub(crate) fn wait_until(&self, deadline: Instant) -> bool {
        match self.arm() {
            Ok(registration) => self.wait_registered(&registration, deadline),
            Err(_) => true,
        }
    }

    /// Arm before the caller's next authoritative-state check. One native
    /// lease prevents main's future native pump and idle scheduler competing.
    pub(crate) fn wait_registered(
        &self,
        registration: &WaitRegistration,
        deadline: Instant,
    ) -> bool {
        if crate::emacs_core::os_signal::pending()
            || self.notification_pending.swap(false, Ordering::AcqRel)
        {
            return true;
        }
        if self.prepare_signal_source().is_err() {
            return true;
        }
        let deadline = if deadline <= Instant::now() {
            ReadDeadline::Immediate
        } else {
            ReadDeadline::At(deadline)
        };
        let outcome = match self.wake.try_native_poll(NativePollOwner::IdleScheduler) {
            Ok(mut lease) => {
                let mut events = polling::Events::new();
                match lease.wait(registration, &mut events, deadline) {
                    Ok(outcome) => outcome,
                    // Reacquire and let the owner process policy select its
                    // degraded wait. The lower source remembers backend loss.
                    Err(_) => return true,
                }
            }
            Err(_) => registration.wait(deadline),
        };
        match outcome {
            ControlWakeOutcome::DeadlineElapsed => false,
            ControlWakeOutcome::ControlReady(_)
            | ControlWakeOutcome::CancellationRequested
            | ControlWakeOutcome::NativeReady
            | ControlWakeOutcome::HostClosed(_) => true,
        }
    }
}

impl ProcessManager {
    pub(crate) fn prepare_green_main_wait(&self, deadline: Instant) -> MainNativePreparation {
        match self.green_select_handle() {
            Some(select) => select.prepare_main_wait(deadline),
            None => MainNativePreparation::Scheduler,
        }
    }

    pub(crate) fn attach_host_control_session(
        &self,
    ) -> crate::host_effect::wake::HostSessionControl {
        self.wait_backend
            .notification_pending
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .control()
            .attach_host_session()
    }

    pub(crate) fn revoke_host_control_session(&self) {
        if let Some(wake) = self.wait_backend.notification_pending.control() {
            wake.revoke_host_session();
        }
    }

    /// Capture under the Baton. The shared source is privately attached once
    /// to this backend. Missing native readiness is a cold admission refusal;
    /// condition-only control owners never qualify this process selector.
    pub(crate) fn observe_green_admission_refusal(
        &self,
        observe: impl FnOnce(crate::host_effect::wake::NativeReadinessCause),
    ) {
        self.wait_backend
            .notification_pending
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .observe_admission_refusal(observe);
    }

    pub(crate) fn green_native_readiness(
        &self,
    ) -> Result<(), crate::host_effect::wake::NativeReadinessCause> {
        self.wait_backend
            .notification_pending
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .require_native()
            .map(|_| ())
    }

    #[cfg(test)]
    pub(crate) fn refuse_green_native_readiness_for_test(
        &mut self,
        cause: crate::host_effect::wake::NativeReadinessCause,
    ) {
        if matches!(
            cause,
            crate::host_effect::wake::NativeReadinessCause::AbsentPoller
        ) {
            self.wait_backend.poller = None;
            #[cfg(unix)]
            {
                self.wait_backend.signal_fd = None;
            }
        }
        self.wait_backend
            .notification_pending
            .refuse_native_for_test(cause);
    }

    pub(crate) fn green_select_handle(&self) -> Option<GreenSelectHandle> {
        let latch = &self.wait_backend.notification_pending;
        let wake = latch
            .attach_native(self.wait_backend.poller.as_ref().map(Arc::clone))
            .require_native()
            .ok()?;
        Some(GreenSelectHandle {
            wake: Arc::clone(wake),
            notification_pending: Arc::clone(latch),
            #[cfg(unix)]
            signal_source: self.wait_backend.poller.as_ref().and_then(|poller| {
                self.wait_backend
                    .signal_fd
                    .map(|fd| (Arc::clone(poller), fd))
            }),
        })
    }
}
