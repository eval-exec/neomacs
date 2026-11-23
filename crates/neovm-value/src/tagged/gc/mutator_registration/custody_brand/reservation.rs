//! Dormant unentered export.
//!
//! Pending keeper commits 30–37, which may reshape this type without a
//! compatibility burden. It is not a heap reconstruction and it does not write
//! a phase word. The only mint is [`StopScope::export_unentered`], which
//! consumes that scope and today's [`AllStopped`] proof.

use super::super::RendezvousError;
use super::super::rendezvous::{AllStopped, AllStoppedFailure};
use super::stop_scope::{StopScope, StoppedInterval};
use std::marker::PhantomData;
use std::rc::Rc;

/// Inert reservation for one lexical stop. No collector, loan, or resume API.
#[must_use]
#[doc(hidden)]
pub struct UnenteredReservation<'s, 'era> {
    interval: StoppedInterval<'s, 'era>,
    _thread: PhantomData<Rc<()>>,
}

impl<'s, 'era> UnenteredReservation<'s, 'era> {
    pub(super) fn from_interval(interval: StoppedInterval<'s, 'era>) -> Self {
        Self {
            interval,
            _thread: PhantomData,
        }
    }

    /// Heap identity recorded on the stopped proof. Not a loan of that heap.
    #[doc(hidden)]
    pub fn heap_identity(&self) -> crate::tagged::gc::HeapIdentity {
        self.interval.heap_identity()
    }

    /// Re-check the publication. Not a heap loan and not a phase write.
    #[doc(hidden)]
    pub fn validate_owner(
        &self,
        owner: &super::PublishedOwner,
    ) -> Result<(), super::super::RendezvousError> {
        self.interval.validate_owner(owner)
    }

    /// Finish the already-traced epoch. This reservation does not collect,
    /// loan the heap, or hand out the stopped proof.
    #[doc(hidden)]
    pub fn complete_traced(self) -> Result<(), AllStoppedFailure<'era>> {
        self.interval.complete_traced()
    }
}

impl<'s> StopScope<'s> {
    /// Mint the dormant reservation from this scope and the existing stop.
    /// Failure returns both inputs. Nothing here writes the phase word.
    #[doc(hidden)]
    pub fn export_unentered<'era>(
        self,
        stopped: AllStopped<'era>,
    ) -> Result<UnenteredReservation<'s, 'era>, (StopScope<'s>, AllStopped<'era>, RendezvousError)>
    {
        match self.bind(stopped) {
            Ok(interval) => Ok(UnenteredReservation::from_interval(interval)),
            Err(held) => Err(held),
        }
    }
}

static_assertions::assert_not_impl_any!(UnenteredReservation<'static, 'static>: Send, Sync, Clone, Copy);
