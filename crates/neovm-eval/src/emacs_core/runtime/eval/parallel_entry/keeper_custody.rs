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
use neovm_value::tagged::gc::{AllStopped, RendezvousError};
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

/// Dormant return envelope. No constructor and no notice conversion.
#[allow(dead_code)]
#[must_use]
pub struct ReturnedSerialTicket {
    _not_sync: PhantomData<Cell<()>>,
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
    /// Consumes a returned envelope. A failure notice does not satisfy this input.
    pub fn accept_return(
        self,
        returned: ReturnedSerialTicket,
    ) -> Result<RestoredKeeper<'s, 'era>, KeeperCustodyFailure<'s, 'era>> {
        let ReturnedSerialTicket { _not_sync: _ } = returned;
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
}

static_assertions::assert_not_impl_any!(KeeperReadyRequest<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(StoppedKeeper<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ExportReservation<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SuspendedKeeper<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RestoredKeeper<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ReturnedSerialTicket: Sync, Clone, Copy);
static_assertions::assert_impl_all!(ReturnedSerialTicket: Send);
