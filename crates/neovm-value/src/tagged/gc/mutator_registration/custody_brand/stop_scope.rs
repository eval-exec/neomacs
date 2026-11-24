//! Generative stop brand. `with_stop_scope` is the only constructor.
//! Binding consumes today's `AllStopped` proof; it does not replace `stop`.

use super::super::RendezvousError;
use super::super::rendezvous::{AllStopped, AllStoppedFailure};
use super::PublishedOwner;
use std::marker::PhantomData;
use std::rc::Rc;

/// Invariant in `'s`. A brand from one callback is not a brand from another.
type Invariant<'s> = PhantomData<fn(&'s mut ()) -> &'s mut ()>;

/// Keeper-local stop brand. The thread check is `AllStopped`'s existing
/// keeper predicate, applied again when the scope is bound.
#[must_use]
#[doc(hidden)]
pub struct StopScope<'s> {
    _brand: Invariant<'s>,
    _thread: PhantomData<Rc<()>>,
}

/// The stopped proof for one lexical stop. Drop retains the epoch by the
/// same poison `AllStopped` already uses. There is no heap loan on this type.
#[must_use]
#[doc(hidden)]
pub struct StoppedInterval<'s, 'era> {
    stopped: AllStopped<'era>,
    _brand: Invariant<'s>,
}

/// `'s` is generative. Returning the scope from `body` is a lifetime error.
#[doc(hidden)]
pub fn with_stop_scope<R>(body: impl for<'s> FnOnce(StopScope<'s>) -> R) -> R {
    body(StopScope {
        _brand: PhantomData,
        _thread: PhantomData,
    })
}

impl<'s> StopScope<'s> {
    /// Bind this brand to the guard `stop` already minted on the keeper thread.
    /// Failure returns the scope and the guard; it does not finish the epoch.
    #[doc(hidden)]
    pub fn bind<'era>(
        self,
        stopped: AllStopped<'era>,
    ) -> Result<StoppedInterval<'s, 'era>, (StopScope<'s>, AllStopped<'era>, RendezvousError)> {
        match stopped.with_stopped_gate(|_registry, _gate| Ok(())) {
            Ok(()) => Ok(StoppedInterval {
                stopped,
                _brand: PhantomData,
            }),
            Err(reason) => Err((self, stopped, reason)),
        }
    }
}

impl<'s, 'era> StoppedInterval<'s, 'era> {
    /// Re-check the canonical publication against this stop. A passing check
    /// is not a heap loan and not a second stop.
    #[doc(hidden)]
    pub fn validate_owner(&self, owner: &PublishedOwner) -> Result<(), RendezvousError> {
        self.stopped
            .with_stopped_gate(|registry, gate| owner.validate_current(registry, gate))
    }

    pub(super) fn heap_identity(&self) -> crate::tagged::gc::HeapIdentity {
        self.stopped.heap_identity()
    }

    /// Stop epoch recorded on the proof. Not a loan and not a new stop.
    pub(super) fn epoch(&self) -> std::num::NonZeroU64 {
        self.stopped.epoch()
    }

    /// The serial ring's head under this stop: an owner with its queued
    /// generation, or the Baton group.
    #[doc(hidden)]
    pub fn serial_head(
        &self,
    ) -> Result<Option<super::super::rendezvous::RingClient>, RendezvousError> {
        self.stopped.serial_head()
    }

    /// Serial requests queued or granted under this stop.
    #[doc(hidden)]
    pub fn serial_ring_len(&self) -> Result<usize, RendezvousError> {
        self.stopped.serial_ring_len()
    }

    /// L6.7 K3: grant the serial ring's head inside this stop.
    #[doc(hidden)]
    pub fn grant_serial_head(
        &mut self,
        ready: impl FnOnce(super::super::MutatorKey, std::num::NonZeroU64) -> bool,
    ) -> Result<Option<super::super::rendezvous::SerialGrant>, RendezvousError> {
        self.stopped.grant_serial_head(ready)
    }

    /// L6.7b b3: grant a Baton-group head inside this stop.
    #[doc(hidden)]
    pub fn grant_baton_group(
        &mut self,
    ) -> Result<Option<super::super::rendezvous::SerialGrant>, RendezvousError> {
        self.stopped.grant_baton_group()
    }

    /// Retire a grant issued under this stop, exactly once.
    #[doc(hidden)]
    pub fn retire_serial_grant(
        &mut self,
        grant: super::super::rendezvous::SerialGrant,
    ) -> Result<(), RendezvousError> {
        self.stopped.retire_serial_grant(grant)
    }

    /// Move this proof into the dormant reservation. Not a second mint.
    #[doc(hidden)]
    pub fn into_unentered(self) -> super::reservation::UnenteredReservation<'s, 'era> {
        super::reservation::UnenteredReservation::from_interval(self)
    }

    /// Finish the already-traced epoch. This does not hand out `AllStopped`,
    /// so there is no collector, loan, or resume API on the interval.
    #[doc(hidden)]
    pub fn complete_traced(self) -> Result<(), AllStoppedFailure<'era>> {
        self.stopped.complete()
    }
}

static_assertions::assert_not_impl_any!(StopScope<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(StoppedInterval<'static, 'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests {
    use super::with_stop_scope;

    #[test]
    fn with_stop_scope_returns_the_body() {
        assert_eq!(
            with_stop_scope(|scope| {
                let _ = scope;
                7
            }),
            7
        );
    }
}
