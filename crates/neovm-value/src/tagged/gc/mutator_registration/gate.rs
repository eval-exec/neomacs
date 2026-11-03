//! Cold predicate-gate ownership, including event-driven terminal poison.
use super::*;
use std::ops::{Deref, DerefMut};

static NEXT_GATE_THREAD: AtomicU64 = AtomicU64::new(1);
thread_local! {
    // No destructor or recycled OS identifier: zero always means unowned.
    static GATE_THREAD: u64 = issue_generation(&NEXT_GATE_THREAD)
        .expect("registry gate thread identity exhausted").get();
}
fn current_thread() -> u64 {
    GATE_THREAD.with(|thread| *thread)
}

/// The owner tag describes only a held real mutex, never substitutes for it.
/// Clearing precedes unlock; reacquisition restores it before inspecting state.
///
/// Every release path also publishes the phase word computed from the final
/// gated state (`Release`, still under the mutex), so a mutation of the
/// rendezvous phase or the catalog-transition flag can never become
/// invisible to the lock-free admission polls: there is exactly one
/// publication point per release, not one per mutation site.
pub(super) struct RegistryGuard<'a> {
    registry: &'a Registry,
    guard: Option<MutexGuard<'a, RegistryState>>,
}
impl<'a> RegistryGuard<'a> {
    fn new(registry: &'a Registry, guard: MutexGuard<'a, RegistryState>) -> Self {
        registry
            .gate
            .gate_owner
            .store(current_thread(), Ordering::Release);
        Self {
            registry,
            guard: Some(guard),
        }
    }
    fn take_for_wait(mut self) -> MutexGuard<'a, RegistryState> {
        if let Some(state) = self.guard.as_ref() {
            self.registry.phase.publish(state);
        }
        self.registry.gate.gate_owner.store(0, Ordering::Release);
        self.guard.take().expect("live registry guard")
    }
}
impl Deref for RegistryGuard<'_> {
    type Target = RegistryState;
    fn deref(&self) -> &Self::Target {
        self.guard.as_deref().expect("live registry guard")
    }
}
impl DerefMut for RegistryGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_deref_mut().expect("live registry guard")
    }
}
impl Drop for RegistryGuard<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.guard.take() {
            self.registry.phase.publish(&state);
            self.registry.gate.gate_owner.store(0, Ordering::Release);
            drop(state);
        }
    }
}

/// Counts threads inside a predicate-wait episode. A run-slice drop reads it
/// to decide whether the wake condvar can hold a waiter at all: the ticket is
/// taken before an episode's first predicate check and held across the whole
/// loop, so a waiter can only park while its ticket keeps the count nonzero.
pub(super) struct ParkTicket<'a> {
    registry: &'a Registry,
}

impl Registry {
    pub(super) fn park_ticket(&self) -> ParkTicket<'_> {
        self.park.parked.fetch_add(1, Ordering::SeqCst);
        ParkTicket { registry: self }
    }
}
impl Drop for ParkTicket<'_> {
    fn drop(&mut self) {
        self.registry.park.parked.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Registry {
    pub(super) fn gate_is_held_by_current_thread(&self) -> bool {
        self.gate.gate_owner.load(Ordering::Acquire) == current_thread()
    }

    /// Internal stopped image prepare/commit only. Never wait while retaining a gate,
    /// entry/root-table/allocator guard, or a callback into owner code.
    pub(super) fn lock_blocking(&self) -> Result<RegistryGuard<'_>, RegistrationError> {
        if self.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        if self.gate.gate_owner.load(Ordering::Acquire) == current_thread() {
            return Err(RegistrationError::Busy);
        }
        match self.gate.state.lock() {
            Ok(guard) => {
                let guard = RegistryGuard::new(self, guard);
                if self.park.poisoned.load(Ordering::Acquire) {
                    Err(RegistrationError::Poisoned)
                } else {
                    Ok(guard)
                }
            }
            Err(error) => {
                let _guard = RegistryGuard::new(self, error.into_inner());
                self.poison();
                Err(RegistrationError::Poisoned)
            }
        }
    }

    pub(super) fn lock(&self) -> Result<RegistryGuard<'_>, RegistrationError> {
        if self.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        match self.gate.state.try_lock() {
            Ok(guard) => {
                let guard = RegistryGuard::new(self, guard);
                if self.park.poisoned.load(Ordering::Acquire) {
                    Err(RegistrationError::Poisoned)
                } else {
                    Ok(guard)
                }
            }
            Err(std::sync::TryLockError::WouldBlock) => Err(RegistrationError::Busy),
            Err(std::sync::TryLockError::Poisoned(error)) => {
                // Install ownership BEFORE poison, avoiding recursive locking
                // while a recovered PoisonError still owns the real mutex.
                let _guard = RegistryGuard::new(self, error.into_inner());
                self.poison();
                Err(RegistrationError::Poisoned)
            }
        }
    }

    pub(super) fn poison(&self) {
        if self.gate.gate_owner.load(Ordering::Acquire) == current_thread() {
            self.publish_poison();
        } else {
            // No publication, collector callback or join holds the control
            // gate. Entry locks use try_lock, so they cannot reverse this edge.
            let guard = self
                .gate
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let _guard = RegistryGuard::new(self, guard);
            self.publish_poison();
        }
    }
    fn publish_poison(&self) {
        self.park.poisoned.store(true, Ordering::Release);
        self.park.wake.notify_all();
    }

    pub(super) fn wait<'a>(
        &'a self,
        guard: RegistryGuard<'a>,
    ) -> Result<RegistryGuard<'a>, RegistrationError> {
        debug_assert!(std::ptr::eq(self, guard.registry));
        // Includes same-thread poison after the caller's predicate check. An
        // external poisoner must obtain the mutex, so it cannot notify in the
        // following check->atomic-release/wait gap.
        if self.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        match self.park.wake.wait(guard.take_for_wait()) {
            Ok(guard) => {
                let guard = RegistryGuard::new(self, guard);
                if self.park.poisoned.load(Ordering::Acquire) {
                    Err(RegistrationError::Poisoned)
                } else {
                    Ok(guard)
                }
            }
            Err(error) => {
                let _guard = RegistryGuard::new(self, error.into_inner());
                self.poison();
                Err(RegistrationError::Poisoned)
            }
        }
    }
}
