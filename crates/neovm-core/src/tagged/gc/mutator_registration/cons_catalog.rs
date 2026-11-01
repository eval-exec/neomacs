//! Cold transfer of real cons storage; no whole-heap reference during runs.
use super::*;

struct CatalogTransition<'a> {
    registry: &'a Registry,
    complete: bool,
}
impl CatalogTransition<'_> {
    fn finish(mut self) -> Result<(), RegistrationError> {
        self.registry.lock_blocking()?.catalog_transition = false;
        self.complete = true;
        Ok(())
    }
}
impl Drop for CatalogTransition<'_> {
    fn drop(&mut self) {
        if !self.complete {
            // All catalog/allocator/entry guards and collector borrows have
            // unwound before this outer attempt guard publishes poison.
            self.registry.poison();
        }
    }
}

impl<K: EraKeeper> EraCore<'_, K> {
    /// Detach only before any owner has activated. No execution or Context
    /// reference is granted: existing permits acquire the catalog after this
    /// cold transaction commits.
    #[cold]
    #[inline(never)]
    pub fn start_cons_storage(&self) -> Result<(), RegistrationError> {
        self.transfer_cons_storage(None, true)
    }

    /// Every owner has explicitly closed its region and acknowledged STOP.
    /// Restore the actual catalog before the keeper can borrow the heap for GC.
    #[cold]
    #[inline(never)]
    pub fn return_cons_storage(&self, stopped: &AllStopped<'_>) -> Result<(), RegistrationError> {
        self.transfer_cons_storage(Some(stopped), false)
    }

    /// Following collection, detach the updated real catalog before resume.
    #[cold]
    #[inline(never)]
    pub fn resume_cons_storage(&self, stopped: &AllStopped<'_>) -> Result<(), RegistrationError> {
        self.transfer_cons_storage(Some(stopped), true)
    }

    fn transfer_cons_storage(
        &self,
        stopped: Option<&AllStopped<'_>>,
        install: bool,
    ) -> Result<(), RegistrationError> {
        let registry = self.registry();
        if registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if std::thread::current().id() != registry.keeper_thread {
            return Err(RegistrationError::WrongThread);
        }
        if self.legacy_roots.is_none() {
            return Err(RegistrationError::Publication);
        }
        if let Some(stopped) = stopped {
            if !stopped.matches_registry(registry) {
                return Err(RegistrationError::WrongHeap);
            }
            stopped
                .check_world_roots(registry.heap)
                .map_err(|error| match error {
                    RendezvousError::Busy => RegistrationError::Busy,
                    RendezvousError::Poisoned => RegistrationError::Poisoned,
                    RendezvousError::Identity => RegistrationError::WrongHeap,
                    _ => RegistrationError::Publication,
                })?;
        }
        let mut collector = self
            .collector
            .try_borrow_mut()
            .map_err(|_| RegistrationError::Busy)?;
        let mut suspended = self
            .suspended_cons
            .try_borrow_mut()
            .map_err(|_| RegistrationError::Busy)?;
        if install {
            if suspended.is_some() || collector.is_none() {
                return Err(RegistrationError::Busy);
            }
        } else if suspended.is_none() || collector.is_some() {
            return Err(RegistrationError::Publication);
        }
        // The entry set is append-only, so the post-release scan may iterate
        // this snapshot; an entry registered after it can only reach the
        // phase word's blocked bit, never an admission. Fallible reserve
        // keeps the capacity discipline of the registration paths.
        let mut entries = Vec::new();
        {
            let mut gate = registry.lock_blocking()?;
            if gate.catalog_transition {
                return Err(RegistrationError::Busy);
            }
            if stopped.is_none() && gate.rendezvous.phase != Phase::Idle {
                return Err(RegistrationError::Busy);
            }
            entries
                .try_reserve(gate.entries.len())
                .map_err(|_| RegistrationError::Capacity)?;
            entries.extend(gate.entries.iter().cloned());
            for entry in &entries {
                let state = entry.state.try_lock().map_err(entry_lock_error)?;
                if state.run_loan || state.worker_loan || state.readers != 0 {
                    return Err(RegistrationError::OutstandingLoan);
                }
                if stopped.is_none()
                    && !matches!(state.phase, EntryPhase::Frozen | EntryPhase::Starting)
                {
                    return Err(RegistrationError::Busy);
                }
            }
            // Prevent a permit activated immediately after this check from
            // acquiring RunAccess until the complete transaction is visible.
            // The guard drop below publishes the blocked phase word, which
            // is what actually excludes lock-free admissions from now on.
            gate.catalog_transition = true;
        }
        let attempt = CatalogTransition {
            registry,
            complete: false,
        };
        // Dekker-scan the admission words against the published blocked
        // word: any slice admitted before the block backs the transition
        // out (the caller retries); none admitted after it can exist. This
        // is the load-bearing exclusion replacing the old gate try_lock in
        // `run_access`.
        if !admissions_quiescent(&entries) {
            attempt.finish()?;
            return Err(RegistrationError::OutstandingLoan);
        }
        if install {
            let mut active = collector.take().ok_or(RegistrationError::Publication)?;
            // SAFETY: transition excludes every new run, all prior run/worker
            // loans were checked, and activation supplied no heap authority.
            let result = unsafe {
                registry
                    .cons_catalog
                    .install(active.registration_heap_mut())
            };
            if let Err(error) = result {
                *collector = Some(active);
                return Err(error);
            }
            *suspended = Some(active.suspend_cons_storage());
        } else {
            if registry.cons_catalog.active_regions()? != 0 {
                return Err(RegistrationError::OpenRegion);
            }
            let inert = suspended.take().ok_or(RegistrationError::Publication)?;
            if inert.heap_identity() != registry.heap {
                *suspended = Some(inert);
                return Err(RegistrationError::WrongHeap);
            }
            // SAFETY: matching AllStopped, no run/worker/region loan, and the
            // transition prevents new loans. The catalog contains owned raw
            // storage, never references derived from the old heap loan.
            let mut active = unsafe { inert.resume() };
            // SAFETY: all owner regions are closed and no catalog mutation can
            // race this phase. Reunion precedes any heap walk or collection.
            let result = unsafe {
                registry
                    .cons_catalog
                    .restore(active.registration_heap_mut())
            };
            if let Err(error) = result {
                *suspended = Some(active.suspend_cons_storage());
                return Err(error);
            }
            *collector = Some(active);
        }
        drop(suspended);
        drop(collector);
        attempt.finish()
    }

    #[cfg(test)]
    pub(super) fn cons_catalog_snapshot_for_test(
        &self,
    ) -> Result<super::super::cons_storage::CatalogSnapshot, RegistrationError> {
        self.registry().cons_catalog.snapshot_for_test()
    }
}
