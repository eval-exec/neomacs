//! Cold owner admission after another owner's resume debt, using the stop gate.
use super::super::gate::RegistryGuard;
use super::super::{EntryPhase, Mutator, RegistrationError, RunAccess, entry_lock_error};
use super::{Mode, Phase};

impl<'era> Mutator<'era> {
    /// Admit a run slice, waiting only for other owners to finish restoring.
    ///
    /// Restoring this owner's image does not necessarily open global admission:
    /// the last peer's `finish_debt` opens Idle and notifies the predicate CV.
    /// This wait holds no entry, root, allocator or run loan. It atomically
    /// releases the registry gate while parked and rechecks after every wake.
    ///
    /// A requested/in-progress stop returns `Busy` immediately: this owner may
    /// owe an ACK and must perform that duty rather than wait for itself. An
    /// owner still owing its own restore is likewise refused. Poison and stale
    /// registration remain terminal errors. The predicate gate is released
    /// before the returned slice can execute any caller code.
    ///
    /// The hot path stays `run_access`. This entry is the cold owner wait.
    #[cold]
    #[inline(never)]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn run_access_after_resume_drain(
        &mut self,
    ) -> Result<RunAccess<'_, 'era>, RegistrationError> {
        self.check_thread()?;
        let registry = self.registry;
        let gate = registry.lock_blocking()?;
        self.run_access_after_resume_drain_with_gate(gate)
    }

    // Keeping the acquired gate as a capability also lets the native tests
    // force the predicate-check -> atomic-release/wait edge without hooks.
    fn run_access_after_resume_drain_with_gate(
        &mut self,
        mut gate: RegistryGuard<'era>,
    ) -> Result<RunAccess<'_, 'era>, RegistrationError> {
        let registry = self.registry;
        let _park = registry.park_ticket();
        loop {
            self.check_thread()?;
            registry.validate_ref(&gate, self.entry.key)?;
            match gate.rendezvous.phase {
                Phase::Idle => {
                    // The one canonical admission path executes under the
                    // predicate gate; no stop can intervene before admission.
                    let admitted = self.run_access();
                    drop(gate);
                    return admitted;
                }
                Phase::DrainingResumes => {
                    // Waiting can discharge only a PEER's debt. Retained local
                    // loans or this owner's own publication require its work.
                    let state = self.entry.state.try_lock().map_err(entry_lock_error)?;
                    match state.phase {
                        EntryPhase::Running => {}
                        EntryPhase::Frozen
                        | EntryPhase::Starting
                        | EntryPhase::Published
                        | EntryPhase::Retiring => return Err(RegistrationError::Busy),
                        EntryPhase::Retired => return Err(RegistrationError::StaleRegistration),
                    }
                    match state.rendezvous.mode {
                        Mode::Running => {}
                        Mode::Publishing
                        | Mode::Stopped
                        | Mode::NativeStopped
                        | Mode::Resuming
                        | Mode::StartingStopped => return Err(RegistrationError::Busy),
                        Mode::Retired => return Err(RegistrationError::StaleRegistration),
                    }
                    if self.entry.admitted() || state.run_loan || state.worker_loan {
                        return Err(RegistrationError::OutstandingLoan);
                    }
                    if gate.rendezvous.debt.contains(&self.entry.key) {
                        return Err(RegistrationError::Busy);
                    }
                    drop(state);
                    gate = registry.wait(gate)?;
                }
                Phase::Stopping(_) | Phase::Tracing(_) => return Err(RegistrationError::Busy),
                Phase::Poisoned => return Err(RegistrationError::Poisoned),
            }
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "run_admission_tests.rs"]
mod tests;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use tests::{
    first_restored_owner_waits_for_the_final_peer_debt_body,
    poison_wakes_a_restored_owner_waiting_for_peer_debt_body,
    stop_and_unrestored_owner_refuse_instead_of_waiting_for_themselves_body,
};
