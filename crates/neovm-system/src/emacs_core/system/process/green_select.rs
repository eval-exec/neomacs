//! The Green scheduler's owned OS wait handle. It carries no process map,
//! Lisp value, or World borrow: only poll readiness and its control owner.

use super::{Arc, Instant, Ordering};
use neovm_eval::host_effect::contracts::ReadDeadline;
use neovm_eval::host_effect::wake::native_wait::{NativeWaitPreparationError, PreparedNativeWait};
use neovm_eval::host_effect::wake::{
    ControlDestination, ControlNudge, ControlNudgeOutcome, ControlWake, ControlWakeOutcome,
    HostClosure, NativePollOwner, NotificationLatch, WaitRegistration,
};

#[derive(Debug)]
#[doc(hidden)]
pub enum MainNativePreparation {
    Prepared(PreparedNativeWait),
    Ready,
    Scheduler,
}

/// An observer of the backend lifetime, not its session-retirement authority.
/// Dropping a temporary capture must not retire the backend's shared source.
#[doc(hidden)]
pub struct GreenSelectHandle {
    pub(crate) wake: Arc<ControlWake>,
    pub(crate) notification_pending: Arc<NotificationLatch>,
    #[cfg(unix)]
    pub(crate) signal_source: Option<(Arc<polling::Poller>, std::os::fd::RawFd)>,
}

static_assertions::assert_impl_all!(GreenSelectHandle: Send, Sync);

impl GreenSelectHandle {
    /// Cold native-service capture of this backend's existing readiness owner.
    pub fn control_source(&self) -> Arc<ControlWake> {
        Arc::clone(&self.wake)
    }

    /// The durable scheduler transition precedes this hint. Native input keeps
    /// its original bit and notify path; control has independent destinations.
    pub fn notify(&self) {
        self.nudge(
            ControlDestination::IdleScheduler,
            ControlNudge::HostServiceReady,
        );
    }

    pub fn nudge(&self, destination: ControlDestination, reason: ControlNudge) {
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

    pub(crate) fn prepare_signal_source(&self) -> std::io::Result<()> {
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

    pub(crate) fn prepare_main_wait(&self, deadline: Instant) -> MainNativePreparation {
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

    pub fn arm(&self) -> Result<WaitRegistration, HostClosure> {
        self.wake.arm(ControlDestination::IdleScheduler)
    }

    pub fn wait_until(&self, deadline: Instant) -> bool {
        match self.arm() {
            Ok(registration) => self.wait_registered(&registration, deadline),
            Err(_) => true,
        }
    }

    /// Arm before the caller's next authoritative-state check. One native
    /// lease prevents main's future native pump and idle scheduler competing.
    pub fn wait_registered(&self, registration: &WaitRegistration, deadline: Instant) -> bool {
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
            // A scheduling handoff (`SchedulerReady`) only means some owner
            // can be picked. Returning false keeps `pick_next` from running
            // `wake_selects`, so a peer's `thread_select` continues until its
            // own deadline (`src/thread.c:974-982`). Native input still
            // reports true and wakes selectors.
            ControlWakeOutcome::DeadlineElapsed
            | ControlWakeOutcome::ControlReady(ControlNudge::SchedulerReady) => false,
            ControlWakeOutcome::ControlReady(_)
            | ControlWakeOutcome::CancellationRequested
            | ControlWakeOutcome::NativeReady
            | ControlWakeOutcome::HostClosed(_) => true,
        }
    }
}
