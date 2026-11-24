//! Dormant keeper typestates over the value stop brand.
//!
//! No production caller enters these types. There is no Baton field: the loan
//! that would mint one is still the held keeper work. Native observation
//! tickets stay with that same held destination loan. Pending keeper commits
//! 30–37, which may reshape the stored reservation without a compatibility
//! burden.
#![forbid(unsafe_code)]

use neovm_value::tagged::gc::heap_transfer::{
    PublishedOwner, StopScope, StoppedInterval, UnenteredReservation,
};
use neovm_value::tagged::gc::{
    AllStopped, AllStoppedFailure, RendezvousError, RingClient, SerialGrant,
};
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

type KeeperStop<'s> = PhantomData<fn(&'s mut ()) -> &'s mut ()>;
type Local = PhantomData<Rc<()>>;

/// Invariant request brand. The marker line is the A1 one-line control.
#[allow(dead_code)]
#[must_use]
pub struct KeeperReadyRequest<'s> {
    owner: PublishedOwner,
    stop: PhantomData<fn(&'s mut ()) -> &'s mut ()>,
    local: Local,
}

#[allow(dead_code)]
#[must_use]
pub struct StoppedKeeper<'s, 'era> {
    interval: StoppedInterval<'s, 'era>,
    stop: KeeperStop<'s>,
    local: Local,
}

/// Inert export. No heap, collector, or World method.
#[allow(dead_code)]
#[must_use]
pub struct ExportReservation<'s, 'era> {
    reservation: UnenteredReservation<'s, 'era>,
    stop: KeeperStop<'s>,
    local: Local,
}

/// Ticket is out. No lend method: that absence is the S1 boundary.
#[allow(dead_code)]
#[must_use]
pub struct SuspendedKeeper<'s, 'era> {
    export: ExportReservation<'s, 'era>,
    request: KeeperReadyRequest<'s>,
    stop: KeeperStop<'s>,
    local: Local,
}

#[allow(dead_code)]
#[must_use]
pub struct RestoredKeeper<'s, 'era> {
    export: ExportReservation<'s, 'era>,
    request: KeeperReadyRequest<'s>,
    stop: KeeperStop<'s>,
    local: Local,
}

/// Distinct from [`ReturnedSerialTicket`]. Accepting it is the S11 mismatch.
#[allow(dead_code)]
#[derive(Debug)]
pub enum CustodyFailureNotice {
    Publication(RendezvousError),
    Export,
    Return,
}

/// Dormant return envelope. No notice conversion.
#[allow(dead_code)]
#[must_use]
pub struct ReturnedSerialTicket {
    identity: super::serial_ticket::TicketIdentity,
    _not_sync: PhantomData<Cell<()>>,
}

impl ReturnedSerialTicket {
    /// Production return. Consumes the owner `end_serial` yielded, so the
    /// envelope is that lend's received ticket and not a copied blank.
    #[allow(dead_code)] // the L6.4 row is the caller until activation lands.
    pub(crate) fn from_end(owner: super::serial_ticket::ReturnableSerialOwner) -> Self {
        let identity = owner.identity();
        drop(owner);
        Self {
            identity,
            _not_sync: PhantomData,
        }
    }

    pub(crate) fn identity(&self) -> super::serial_ticket::TicketIdentity {
        self.identity
    }
}

#[cfg(test)]
impl ReturnedSerialTicket {
    /// Test-only return. Production return is [`Self::from_end`].
    pub(crate) fn for_test(identity: super::serial_ticket::TicketIdentity) -> Self {
        Self {
            identity,
            _not_sync: PhantomData,
        }
    }
}

#[allow(dead_code)]
enum RetainedCustody<'s, 'era> {
    Interval(StoppedInterval<'s, 'era>),
    Reservation(UnenteredReservation<'s, 'era>),
}

/// Owns the unresolved stop. Drop still follows the value reservation.
#[allow(dead_code)]
#[must_use]
pub struct KeeperCustodyFailure<'s, 'era> {
    notice: CustodyFailureNotice,
    retained: RetainedCustody<'s, 'era>,
    request: KeeperReadyRequest<'s>,
}

#[allow(dead_code)]
impl<'s, 'era> StoppedKeeper<'s, 'era> {
    /// Bind this lexical scope to the guard `stop` already minted.
    pub fn from_scope(
        scope: StopScope<'s>,
        stopped: AllStopped<'era>,
    ) -> Result<Self, (StopScope<'s>, AllStopped<'era>, RendezvousError)> {
        scope.bind(stopped).map(|interval| Self {
            interval,
            stop: PhantomData,
            local: PhantomData,
        })
    }

    pub fn ready_request(
        &self,
        owner: PublishedOwner,
    ) -> Result<KeeperReadyRequest<'s>, RendezvousError> {
        self.interval.validate_owner(&owner)?;
        Ok(KeeperReadyRequest {
            owner,
            stop: PhantomData,
            local: PhantomData,
        })
    }

    /// The ring head under this stop: an owner with its queued generation,
    /// or the Baton group.
    pub(crate) fn serial_head(&self) -> Result<Option<RingClient>, RendezvousError> {
        self.interval.serial_head()
    }

    /// Serial requests queued or granted under this stop (the K4 batch bound).
    pub(crate) fn serial_ring_len(&self) -> Result<usize, RendezvousError> {
        self.interval.serial_ring_len()
    }

    /// L6.7 K3: grant the ring head to `request`'s owner, only when the head
    /// is that owner at the publication the request carries.
    pub(crate) fn grant_for(
        &mut self,
        request: &KeeperReadyRequest<'s>,
    ) -> Result<Option<SerialGrant>, RendezvousError> {
        let (owner, generation, _) = request.owner.serial_facts();
        self.interval
            .grant_serial_head(|key, queued| key.owner() == owner && queued == generation)
    }

    /// Finish the traced stop with nothing lent.
    pub(crate) fn complete_traced(self) -> Result<(), AllStoppedFailure<'era>> {
        self.interval.complete_traced()
    }

    /// Consume this keeper. The suspended state has no lend method.
    pub fn lend_serial(
        self,
        request: KeeperReadyRequest<'s>,
    ) -> Result<SuspendedKeeper<'s, 'era>, KeeperCustodyFailure<'s, 'era>> {
        if let Err(reason) = self.interval.validate_owner(&request.owner) {
            return Err(KeeperCustodyFailure {
                notice: CustodyFailureNotice::Publication(reason),
                retained: RetainedCustody::Interval(self.interval),
                request,
            });
        }
        Ok(SuspendedKeeper {
            export: ExportReservation {
                reservation: self.interval.into_unentered(),
                stop: PhantomData,
                local: PhantomData,
            },
            request,
            stop: PhantomData,
            local: PhantomData,
        })
    }
}

#[allow(dead_code)]
impl<'s, 'era> SuspendedKeeper<'s, 'era> {
    /// Owner tag, publication generation, and the stopped proof's heap.
    ///
    /// Copyable facts for one ticket. Not a second lend.
    pub(crate) fn serial_facts(
        &self,
    ) -> (
        u64,
        std::num::NonZeroU64,
        neovm_value::tagged::gc::HeapIdentity,
        std::num::NonZeroU64,
    ) {
        let (owner, generation, owner_heap) = self.request.owner.serial_facts();
        let heap = self.export.reservation.heap_identity();
        let epoch = self.export.reservation.epoch();
        debug_assert_eq!(owner_heap, heap);
        (owner, generation, heap, epoch)
    }

    /// Consumes a returned envelope. A failure notice does not satisfy this input.
    pub fn accept_return(
        self,
        returned: ReturnedSerialTicket,
    ) -> Result<RestoredKeeper<'s, 'era>, KeeperCustodyFailure<'s, 'era>> {
        let ReturnedSerialTicket {
            identity: _,
            _not_sync: _,
        } = returned;
        Ok(RestoredKeeper {
            export: self.export,
            request: self.request,
            stop: PhantomData,
            local: PhantomData,
        })
    }
}

#[allow(dead_code)]
impl<'s, 'era> RestoredKeeper<'s, 'era> {
    /// Re-check a publication against this stop. Not a heap loan.
    pub(crate) fn ready_request(
        &self,
        owner: PublishedOwner,
    ) -> Result<KeeperReadyRequest<'s>, RendezvousError> {
        self.export.reservation.validate_owner(&owner)?;
        Ok(KeeperReadyRequest {
            owner,
            stop: PhantomData,
            local: PhantomData,
        })
    }

    /// The ring head under this stop: an owner with its queued generation,
    /// or the Baton group.
    pub(crate) fn serial_head(&self) -> Result<Option<RingClient>, RendezvousError> {
        self.export.reservation.serial_head()
    }

    /// L6.7 K3: grant the ring head to `request`'s owner, as on
    /// [`StoppedKeeper::grant_for`]. The previous ticket is back.
    pub(crate) fn grant_for(
        &mut self,
        request: &KeeperReadyRequest<'s>,
    ) -> Result<Option<SerialGrant>, RendezvousError> {
        let (owner, generation, _) = request.owner.serial_facts();
        self.export
            .reservation
            .grant_serial_head(|key, queued| key.owner() == owner && queued == generation)
    }

    /// Retire the grant whose ticket came back, exactly once.
    pub(crate) fn retire(&mut self, grant: SerialGrant) -> Result<(), RendezvousError> {
        self.export.reservation.retire_serial_grant(grant)
    }

    pub fn lend_next(
        self,
        request: KeeperReadyRequest<'s>,
    ) -> Result<SuspendedKeeper<'s, 'era>, KeeperCustodyFailure<'s, 'era>> {
        if let Err(reason) = self.export.reservation.validate_owner(&request.owner) {
            return Err(KeeperCustodyFailure {
                notice: CustodyFailureNotice::Publication(reason),
                retained: RetainedCustody::Reservation(self.export.reservation),
                request,
            });
        }
        Ok(SuspendedKeeper {
            export: self.export,
            request,
            stop: PhantomData,
            local: PhantomData,
        })
    }

    /// Finish the traced epoch after the ticket has come back. The export
    /// reservation stays inert: it has no collect or heap method.
    pub(crate) fn complete_traced(self) -> Result<(), AllStoppedFailure<'era>> {
        self.export.reservation.complete_traced()
    }
}

#[allow(dead_code)]
impl<'s, 'era> KeeperCustodyFailure<'s, 'era> {
    /// Finish the traced epoch this failure still holds. Not a heap loan.
    pub(crate) fn complete_traced(self) -> Result<(), AllStoppedFailure<'era>> {
        match self.retained {
            RetainedCustody::Interval(interval) => interval.complete_traced(),
            RetainedCustody::Reservation(reservation) => reservation.complete_traced(),
        }
    }
}

static_assertions::assert_not_impl_any!(KeeperReadyRequest<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(StoppedKeeper<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ExportReservation<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SuspendedKeeper<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RestoredKeeper<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ReturnedSerialTicket: Sync, Clone, Copy);
static_assertions::assert_impl_all!(ReturnedSerialTicket: Send);
