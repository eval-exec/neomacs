//! E5.2: a serial turn parks over the L6 ring and resumes on a later grant
//! (E5-DESIGN §2; E5.2-PARK-RESUME r0).
//!
//! A serial turn runs inside a stop: the keeper lends the World to one owner
//! while every other owner is stopped. A turn that reaches a GNU release
//! point must not hold that stop while it waits (DESIGN §4), so it parks:
//! the host leaves the OS thread (E5.1 `park`, the L6.4 detach), the ticket
//! goes back to the keeper, and the owner waits with its image still
//! published. On wake it asks for a new turn at the ring tail and, on the
//! grant, receives the new ticket and resumes the SAME turn on its own stack.
//!
//! The owner side never holds a ticket across the wait: [`LiveSerial::park`]
//! returns the ticket to the keeper itself, and a [`ParkedSerial`] holds only
//! the parked turn and the owner's fixed facts. The return and the later
//! request travel through one [`KeeperPort`], in order, because the ring
//! holds one entry per owner.
//!
//! A publication receipt is single-use (`ready_request` consumes it) and only
//! a native publication mints one, so every request publishes afresh: the
//! generation is per grant, like the stop epoch. The owner is NativeStopped
//! for the whole wait; it leaves native only inside `request_turn`, after
//! the wake, and publishes again before the request is committed.
//!
//! The keeper side is L6.7's service; the rows drive it with a test keeper.

use super::keeper_custody::ReturnedSerialTicket;
use super::serial_ticket::{
    LocalSerialOwner, ReceiveReason, SerialDestination, StoppedSerialTicket, TicketIdentity,
};
use super::serial_turn::{
    OwnerSerialState, ParkedTurn, SerialHost, SerialTurn, TicketOwner, TurnRefusal,
};
use neovm_value::tagged::gc::HeapIdentity;
use std::num::NonZeroU64;

/// The owner key of a received ticket is the owner id alone (#12338); the
/// generation and the stop epoch are checked by `receive_serial`.
impl TicketOwner for LocalSerialOwner {
    type Owner = u64;
    fn owner(&self) -> u64 {
        self.identity().owner
    }
}

/// What a serial owner is across all its turns: its owner id and the era
/// heap it is registered on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SerialHome {
    pub owner: u64,
    pub heap: HeapIdentity,
}

/// One grant: the lent ticket, the publication its request was committed
/// under, and the epoch of the stop that cut it. The generation is the
/// owner's own fact (the port reads it from the receipt it published); the
/// epoch is the keeper's.
#[derive(Debug)]
pub(crate) struct Grant {
    pub ticket: StoppedSerialTicket,
    pub generation: NonZeroU64,
    pub epoch: NonZeroU64,
}

impl Grant {
    /// The identity a seat for this grant expects.
    fn expected(&self, home: SerialHome) -> TicketIdentity {
        TicketIdentity {
            owner: home.owner,
            generation: self.generation,
            heap: home.heap,
            epoch: self.epoch,
        }
    }
}

/// Why the keeper did not grant a requested turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortRefusal {
    /// The era was poisoned; no further grant will come.
    Poisoned,
    /// A transient owner-side refusal (K8 r2): this owner could not publish
    /// within the `Busy` contention bound, or its request was sent back to
    /// publish more often than the republish limit. Nothing was lent and
    /// nothing is poisoned: request again. A slow keeper stop never answers
    /// this; the requester just waits for its grant.
    Retry,
    /// A permanent non-`Poisoned` owner-side publish or protocol error
    /// (`WrongThread`, `Identity`, `Loan`, `Publication`, `Generation`,
    /// `Capacity`). Retrying would spin forever; fail closed and release any
    /// held ring HEAD (L6.7c (c) F2).
    Failed,
}

/// The owner's hold on one lent ticket (L6.7 K7). It is armed while the
/// ticket is out and disarmed right after the ticket goes back. A guard that
/// drops armed means the ticket was lost: the port's guard reports that to
/// the keeper, which poisons, because only a Rust unwind or a leaked turn
/// can drop it armed. A Lisp signal ends the turn normally.
pub(crate) trait LendGuard {
    /// The ticket went back to the keeper.
    fn disarm(self);
}

/// The unit-test ports lend with no keeper to tell.
impl LendGuard for () {
    fn disarm(self) {}
}

/// The owner's side of the serial ring.
///
/// One port per owner, and its calls are ordered: a ticket given back is
/// processed by the keeper before a later request from the same owner, so
/// the request never meets the owner's previous ring entry.
pub(crate) trait KeeperPort {
    /// The guard over one lent ticket.
    type LendGuard: LendGuard;
    /// Where a collection's report goes (L6.7c (c)).
    type CollectReply;
    /// Give a finished turn's ticket back to the keeper.
    fn give_back(&mut self, returned: ReturnedSerialTicket);
    /// Give a parked turn's ticket back asking for a collection (L6.7c (c)).
    /// The keeper holds the ring's HEAD for this owner's next request and
    /// collects first in the stop that grants it; REPLY gets the report.
    fn give_back_to_collect(&mut self, returned: ReturnedSerialTicket, reply: Self::CollectReply);
    /// Publish this owner's image, commit a request at the ring tail under
    /// that publication, and wait, NativeStopped, for the grant. The guard
    /// is armed: the ticket is out from here until it goes back.
    fn request_turn(&mut self) -> Result<(Grant, Self::LendGuard), PortRefusal>;
    /// A granted ticket this owner could not receive goes back unopened.
    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason);
    /// Abandon an active `Driver::head_hold` if this owner unwinds or fails
    /// before its follow-up `Request` arrives (L6.7c (c) D1).
    fn abandon_head_hold(&mut self) {}
    /// Retain the era's facade-mark exclusion epoch for publishing parked
    /// serial-turn roots into the heap's `TransportRootTable` (L6.7c (c)).
    fn collect_root_epoch(&self) -> Option<neovm_value::tagged::gc::FacadeEpochRetention> {
        None
    }
}

/// A turn in progress: the E5.1 turn, the seat that received its ticket,
/// and the armed guard over that ticket.
#[must_use = "a serial turn must end or park"]
pub(crate) struct LiveSerial<'t, C: SerialHost, G: LendGuard = ()> {
    turn: SerialTurn<'t, C, LocalSerialOwner>,
    active: Option<(SerialDestination, G)>,
    home: SerialHome,
}

/// What a turn parked for a collection waits on (L6.7c (c)): the grant the
/// keeper lends it at the ring's HEAD, after the collection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Collecting;

/// A parked turn. It holds no ticket: only the parked E5.1 turn and the
/// owner's fixed facts.
#[must_use = "a parked serial turn must resume"]
pub(crate) struct ParkedSerial<'t, C: SerialHost, W> {
    parked: ParkedTurn<'t, C, LocalSerialOwner, W>,
    home: SerialHome,
}

/// Why a turn did not begin or resume. The ticket already went back.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TurnStartRefusal<R, A> {
    Port(PortRefusal),
    Receive(ReceiveReason),
    /// The caller's activation refused the received owner.
    Activate(A),
    Turn(TurnRefusal<R>),
}

/// Receive a granted ticket into a fresh seat armed for `home` at the grant's
/// generation and epoch, then run the caller's activation on the received
/// owner. A receive
/// refusal sends the ticket back unopened; an activation refusal gives the
/// received owner back through its seat. Activation owns its own undo:
/// nothing is half-activated when it refuses.
fn receive<P: KeeperPort, R, A>(
    home: SerialHome,
    grant: Grant,
    guard: P::LendGuard,
    port: &mut P,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<(SerialDestination, LocalSerialOwner, P::LendGuard), TurnStartRefusal<R, A>> {
    let mut seat = SerialDestination::new(grant.expected(home));
    let mut local = match seat.receive_serial(grant.ticket) {
        Ok(received) => received.guard(),
        Err(refusal) => {
            let reason = refusal.reason;
            port.refuse(refusal.ticket, reason);
            guard.disarm();
            return Err(TurnStartRefusal::Receive(reason));
        }
    };
    // An unwind out of `activate` drops `guard` armed: the keeper learns the
    // ticket is lost instead of waiting for it.
    match activate(&mut local) {
        Ok(()) => Ok((seat, local, guard)),
        Err(refusal) => {
            give_back(seat, local, port);
            guard.disarm();
            Err(TurnStartRefusal::Activate(refusal))
        }
    }
}

/// Finish a local owner on the seat that received it, for its ticket's
/// return. The pairing is structural: a `LiveSerial` holds the seat beside
/// the turn whose owner that seat received, and E5.1 hands back the same
/// owner. A mismatch would leave the keeper's stop open forever, so it fails
/// loudly.
fn returned(mut seat: SerialDestination, local: LocalSerialOwner) -> ReturnedSerialTicket {
    match local.finish(&mut seat) {
        Ok(returnable) => returnable.return_ticket(),
        Err(local) => panic!(
            "a serial seat refused the owner it received: {:?}",
            local.identity()
        ),
    }
}

/// Finish a local owner and give its ticket back.
fn give_back(seat: SerialDestination, local: LocalSerialOwner, port: &mut impl KeeperPort) {
    port.give_back(returned(seat, local));
}

/// Give a collecting turn's ticket back asking for a keeper collection and
/// disarm its lend guard (shared between `LiveSerial::park_to_collect` and
/// `LiveSerial::with_world_collectible`, L6.7c (c)).
#[allow(dead_code)]
fn park_ticket_to_collect<P: KeeperPort>(
    seat: SerialDestination,
    local: LocalSerialOwner,
    guard: P::LendGuard,
    reply: P::CollectReply,
    port: &mut P,
) {
    port.give_back_to_collect(returned(seat, local), reply);
    guard.disarm();
}

/// RAII guard armed while `Driver::head_hold` stands for this owner (after
/// `park_ticket_to_collect` disarms `LendGuard` and before the follow-up
/// `Request` is granted). If this owner unwinds or fails during that window,
/// `Drop` calls `KeeperPort::abandon_head_hold` (`KeeperMsg::Exit`), so the
/// driver clears `head_hold` and still runs the requested collection (L6.7c (c)
/// D1, Loom L16).
#[allow(dead_code)]
struct HeadHoldExitGuard<'p, P: KeeperPort> {
    port: &'p mut P,
    armed: bool,
}

#[allow(dead_code)]
impl<'p, P: KeeperPort> HeadHoldExitGuard<'p, P> {
    fn new(port: &'p mut P) -> Self {
        Self { port, armed: true }
    }

    fn port_mut(&mut self) -> &mut P {
        self.port
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl<P: KeeperPort> Drop for HeadHoldExitGuard<'_, P> {
    fn drop(&mut self) {
        if self.armed {
            self.armed = false;
            self.port.abandon_head_hold();
        }
    }
}

/// Re-request and validate a turn while `Driver::head_hold` stands for this
/// owner (L6.7c (c)): wraps ONLY `port.request_turn()` in `HeadHoldExitGuard`
/// (F5: once `request_turn()` returns `Ok`, `Driver::commit` has already
/// cleared `head_hold` before granting), retries transient `PortRefusal::Retry`
/// with `yield_now()` (D1 fix (a)), fails closed on permanent
/// `PortRefusal::Failed` leaving `HeadHoldExitGuard` armed to release HEAD
/// (F2), disarms on `PortRefusal::Poisoned`, and then validates the grant via
/// `receive(...)`.
#[allow(dead_code)]
fn resume_ticket_for_collect<P: KeeperPort, A>(
    home: SerialHome,
    port: &mut P,
    activate: &mut impl FnMut(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<
    (SerialDestination, LocalSerialOwner, P::LendGuard),
    TurnStartRefusal<std::convert::Infallible, A>,
> {
    let (grant, guard) = {
        let mut head_guard = HeadHoldExitGuard::new(port);
        loop {
            match head_guard.port_mut().request_turn() {
                Ok(granted) => {
                    head_guard.disarm();
                    break granted;
                }
                Err(PortRefusal::Retry) => std::thread::yield_now(),
                Err(PortRefusal::Poisoned) => {
                    head_guard.disarm();
                    return Err(TurnStartRefusal::Port(PortRefusal::Poisoned));
                }
                Err(PortRefusal::Failed) => {
                    // `head_guard` drops armed -> `abandon_head_hold()` sends
                    // `KeeperMsg::Exit` so `Driver::head_hold` is released (F2).
                    return Err(TurnStartRefusal::Port(PortRefusal::Failed));
                }
            }
        }
    };
    receive(home, grant, guard, port, |local| activate(local))
}

/// Step (a) of SERIAL-TURN-CONTEXT r1 point 2: verify that the received
/// ticket's local serial heap loan matches the dormant host's heap identity
/// before `OwnerSerialState::begin` / `ParkedTurn::resume` runs steps (b)-(d)
/// (reattach: heap TLS install, frame/slab/buffer, and carried/warm World
/// restore). Refuses a mismatch with [`ReceiveReason::WrongHeap`] without
/// touching TLS.
#[cold]
#[inline(never)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E5.3, E5.4 and E6-P pass this closure to begin_turn and ParkedSerial::resume; the rows call it now"
    )
)]
pub(crate) fn verify_serial_host_heap(
    host_heap: HeapIdentity,
) -> impl FnOnce(&mut LocalSerialOwner) -> Result<(), ReceiveReason> {
    move |local| local.verify_host_heap(host_heap)
}

/// Begin a turn on a grant. `home` names the owner and the era heap; the
/// grant supplies the publication generation and the stop epoch.
/// ACTIVATE is the heap loan check on the received owner
/// ([`verify_serial_host_heap`] in production; the standalone rows pass a
/// no-op).
#[cold]
#[inline(never)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "E5.3, E5.4 and E6-P call it; the rows do now")
)]
pub(crate) fn begin_turn<'t, C: SerialHost, P: KeeperPort, A>(
    state: &'t mut OwnerSerialState<C, LocalSerialOwner>,
    home: SerialHome,
    (grant, guard): (Grant, P::LendGuard),
    port: &mut P,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<LiveSerial<'t, C, P::LendGuard>, TurnStartRefusal<C::Refusal, A>> {
    let (seat, local, guard) = receive(home, grant, guard, port, activate)?;
    match state.begin(local) {
        Ok(turn) => Ok(LiveSerial {
            turn,
            active: Some((seat, guard)),
            home,
        }),
        Err((local, refusal)) => {
            give_back(seat, local, port);
            guard.disarm();
            Err(TurnStartRefusal::Turn(refusal))
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "E5.3, E5.4 and E6-P call it; the rows do now")
)]
impl<'t, C: SerialHost, G: LendGuard> LiveSerial<'t, C, G> {
    /// Lend the owner's Context to BODY (E5.1 `with_world`).
    pub(crate) fn with_world<R>(&mut self, body: impl for<'g> FnOnce(&'g mut C) -> R) -> R {
        self.turn.with_world(body)
    }

    /// End the turn: the host leaves the thread and the ticket goes back.
    /// A host that refuses to detach keeps the turn live.
    #[cold]
    #[inline(never)]
    pub(crate) fn end<P: KeeperPort<LendGuard = G>>(
        self,
        port: &mut P,
    ) -> Result<(), (Self, C::Refusal)> {
        let LiveSerial { turn, active, home } = self;
        let (seat, guard) = active.expect("live serial turn holds active seat and guard");
        match turn.end() {
            Ok(local) => {
                give_back(seat, local, port);
                guard.disarm();
                Ok(())
            }
            Err((turn, refusal)) => Err((
                LiveSerial {
                    turn,
                    active: Some((seat, guard)),
                    home,
                },
                refusal,
            )),
        }
    }

    /// Park on WAIT: the host leaves the thread and the ticket goes back
    /// before this returns, so nothing ticket-shaped survives the wait. A
    /// host that refuses to detach keeps the turn live and hands WAIT back.
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn park<W, P: KeeperPort<LendGuard = G>>(
        self,
        wait: W,
        port: &mut P,
    ) -> Result<ParkedSerial<'t, C, W>, (Self, W, C::Refusal)> {
        let LiveSerial { turn, active, home } = self;
        let (seat, guard) = active.expect("live serial turn holds active seat and guard");
        match turn.park(wait) {
            Ok((parked, local)) => {
                give_back(seat, local, port);
                guard.disarm();
                Ok(ParkedSerial { parked, home })
            }
            Err((turn, wait, refusal)) => Err((
                LiveSerial {
                    turn,
                    active: Some((seat, guard)),
                    home,
                },
                wait,
                refusal,
            )),
        }
    }

    /// Park for a collection (L6.7c (c), the b5 shape for an owner): the
    /// host leaves the thread and the ticket goes back asking the keeper to
    /// collect, before this returns. The keeper holds the ring's HEAD for
    /// this owner, so the [`ParkedSerial::resume`] that follows asks again
    /// under a new publication, is granted in the stop that collects first,
    /// and no other client's turn comes in between. REPLY gets that
    /// collection's report. No stop runs until that resume asks, so a resume
    /// refused `Retry` (this owner could not publish) keeps the HEAD held:
    /// resume again. A host that refuses to detach keeps the turn live and
    /// hands REPLY back.
    ///
    /// The returned `ParkedSerial` is deliberately unguarded (it cannot hold
    /// the `&mut P` this call borrows): the lend guard was disarmed above, so
    /// between this return and the `resume` `Request` the driver holds the
    /// ring HEAD and no armed guard exists. A caller that can unwind or give
    /// up in that window MUST drop an `Exit`-sending guard (e.g.
    /// `KeeperPort::abandon_head_hold`) before the `resume`, or the era
    /// deadlocks — the driver only sees `Disconnected` when every inbox
    /// sender is gone (L6.7c (c) D1 / F1).
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn park_to_collect<P: KeeperPort<LendGuard = G>>(
        self,
        reply: P::CollectReply,
        port: &mut P,
    ) -> Result<ParkedSerial<'t, C, Collecting>, (Self, P::CollectReply, C::Refusal)> {
        let LiveSerial { turn, active, home } = self;
        let (seat, guard) = active.expect("live serial turn holds active seat and guard");
        match turn.park(Collecting) {
            Ok((parked, local)) => {
                park_ticket_to_collect(seat, local, guard, reply, port);
                Ok(ParkedSerial { parked, home })
            }
            Err((turn, Collecting, refusal)) => Err((
                LiveSerial {
                    turn,
                    active: Some((seat, guard)),
                    home,
                },
                reply,
                refusal,
            )),
        }
    }
}

/// Failure modes of an in-stack `SerialTurn` keeper collection (L6.7c (c)).
#[allow(dead_code)]
#[derive(Debug)]
enum SerialCollectError {
    MissingEpoch,
    Detach(super::serial_detach::HostRefusal),
    Poisoned,
    Failed,
    Receive(ReceiveReason),
    Activate,
    ReportRefused(super::keeper_driver::CollectRefusal),
    ReportChannelClosed,
    Reattach(super::serial_detach::HostRefusal),
}

type SerialCollectBridgeFn = dyn FnMut(
        &mut super::super::Context,
    ) -> Result<neovm_value::tagged::gc::CollectionReport, SerialCollectError>
    + 'static;

/// Thread-local RAII hook for in-stack explicit GC inside
/// [`LiveSerial::with_world_collectible`]. Stores zero heap `Value`s and zero
/// `TaggedHeap` pointers (Q2), so `park_carrier_tls()` never interacts with it.
struct ActiveSerialCollectScope {
    ctx_addr: usize,
    heap: HeapIdentity,
    inhibit_base: usize,
    bridge: *mut SerialCollectBridgeFn,
}

std::thread_local! {
    static SERIAL_TURN_COLLECT_SCOPE: std::cell::Cell<Option<ActiveSerialCollectScope>> =
        const { std::cell::Cell::new(None) };
}

#[allow(dead_code)]
struct SerialCollectScopeGuard {
    previous: Option<ActiveSerialCollectScope>,
}

#[allow(dead_code)]
impl SerialCollectScopeGuard {
    fn install(scope: ActiveSerialCollectScope) -> Self {
        let previous = SERIAL_TURN_COLLECT_SCOPE.with(|slot| slot.replace(Some(scope)));
        Self { previous }
    }
}

impl Drop for SerialCollectScopeGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SERIAL_TURN_COLLECT_SCOPE.with(|slot| slot.set(previous));
    }
}

impl<'t, G: LendGuard> LiveSerial<'t, super::super::Context, G> {
    /// Lend the owner's `Context` to BODY while routing explicit Lisp
    /// `(garbage-collect)` / `(garbage-collect-maybe FACTOR)` through the
    /// keeper's HEAD-hold collect protocol (L6.7c (c), Option B).
    #[cold]
    #[inline(never)]
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "E6-P parallel owner loop is the production caller; live rows call it now"
        )
    )]
    pub(crate) fn with_world_collectible<P, A, R>(
        &mut self,
        port: &mut P,
        mut activate: impl FnMut(&mut LocalSerialOwner) -> Result<(), A>,
        body: impl for<'g> FnOnce(&'g mut super::super::Context) -> R,
    ) -> R
    where
        P: KeeperPort<LendGuard = G, CollectReply = super::keeper_driver::CollectReply>,
    {
        let home = self.home;
        let active_slot = &mut self.active;
        self.turn.with_world_rebind(|ctx, local_slot, mut ops| {
            let ctx_addr = std::ptr::from_ref(ctx) as usize;
            let heap = ctx.tagged_heap.heap_identity();
            let inhibit_base = ctx.gc_inhibit_depth;
            let mut bridge_fn = |ev: &mut super::super::Context| -> Result<
                neovm_value::tagged::gc::CollectionReport,
                SerialCollectError,
            > {
                let epoch = port
                    .collect_root_epoch()
                    .ok_or(SerialCollectError::MissingEpoch)?;
                ops.detach_with(|t| ev.detach_serial_owner_for_collect(epoch, t))
                    .map_err(SerialCollectError::Detach)?;
                let local = local_slot
                    .take()
                    .expect("live serial turn holds its local owner before collect park");
                let (seat, guard) = active_slot
                    .take()
                    .expect("live serial turn holds its seat and guard before collect park");
                let (report_tx, report_rx) = super::sync::mpsc::sync_channel(1);
                park_ticket_to_collect(seat, local, guard, Some(report_tx), port);
                let (seat2, local2, guard2) =
                    match resume_ticket_for_collect(home, port, &mut activate) {
                        Ok(resumed) => resumed,
                        Err(TurnStartRefusal::Port(PortRefusal::Poisoned)) => {
                            return Err(SerialCollectError::Poisoned);
                        }
                        Err(TurnStartRefusal::Port(PortRefusal::Failed)) => {
                            return Err(SerialCollectError::Failed);
                        }
                        Err(TurnStartRefusal::Port(PortRefusal::Retry)) => {
                            unreachable!("resume_ticket_for_collect retries PortRefusal::Retry")
                        }
                        Err(TurnStartRefusal::Receive(reason)) => {
                            return Err(SerialCollectError::Receive(reason));
                        }
                        Err(TurnStartRefusal::Activate(_)) => {
                            return Err(SerialCollectError::Activate);
                        }
                        Err(TurnStartRefusal::Turn(_)) => {
                            unreachable!("resume_ticket_for_collect does not call Turn::resume")
                        }
                    };
                *local_slot = Some(local2);
                *active_slot = Some((seat2, guard2));
                let report = match report_rx.recv() {
                    Ok(Ok(report)) => report,
                    Ok(Err(refusal)) => return Err(SerialCollectError::ReportRefused(refusal)),
                    Err(_) => return Err(SerialCollectError::ReportChannelClosed),
                };
                ops.reattach_with(|d, t| ev.reattach(d, t))
                    .map_err(SerialCollectError::Reattach)?;
                Ok(report)
            };
            let bridge_ref: &mut (
                     dyn FnMut(
                &mut super::super::Context,
            ) -> Result<
                neovm_value::tagged::gc::CollectionReport,
                SerialCollectError,
            > + '_
                 ) = &mut bridge_fn;
            // SAFETY: `SerialCollectScopeGuard` removes this pointer from
            // `SERIAL_TURN_COLLECT_SCOPE` before `bridge_fn` leaves the stack
            // frame (both on normal return and panic unwind), and
            // `gc_collect_serial_turn` takes the scope out of TLS for the
            // duration of the bridge call, ensuring at most one `&mut`
            // invocation is active at a time.
            let bridge_ptr: *mut SerialCollectBridgeFn = unsafe { std::mem::transmute(bridge_ref) };
            let _scope_guard = SerialCollectScopeGuard::install(ActiveSerialCollectScope {
                ctx_addr,
                heap,
                inhibit_base,
                bridge: bridge_ptr,
            });
            body(ctx)
        })
    }
}

impl super::super::Context {
    /// Returns the baseline `gc_inhibit_depth` recorded at entry to an active
    /// [`LiveSerial::with_world_collectible`] scope on this OS thread when both
    /// `self`'s address and `HeapIdentity` match (zero bytes on `Context`, Q2).
    #[inline]
    pub(crate) fn active_serial_collect_inhibit_base(&self) -> Option<usize> {
        let my_addr = std::ptr::from_ref(self) as usize;
        let my_heap = self.tagged_heap.heap_identity();
        SERIAL_TURN_COLLECT_SCOPE.with(|slot| {
            let scope = slot.take()?;
            let matched =
                (scope.ctx_addr == my_addr && scope.heap == my_heap).then_some(scope.inhibit_base);
            slot.set(Some(scope));
            matched
        })
    }

    /// Whether explicit Lisp `(garbage-collect)` / `(garbage-collect-maybe)` is
    /// currently inhibited on this `Context` (L6.7c (c) §3.2 + F3).
    #[inline]
    pub(crate) fn is_explicit_gc_inhibited(&self) -> bool {
        if let Some(inhibit_base) = self.active_serial_collect_inhibit_base() {
            return self.gc_inhibit_depth > inhibit_base;
        }
        if self.in_active_serial_turn() {
            // Unbridged SerialTurn (`LiveSerial::with_world` / raw
            // `SerialTurn::with_world`): stay unconditionally inhibited
            // regardless of `NEOVM_HASH_TEST_PARITY` (L6.7c (a) + F3).
            return true;
        }
        self.gc_inhibit_depth > 0
            && (neovm_value::hashtab::hash_test_parity_enabled()
                || self.green_seat_holds_era_turn())
    }

    /// Route an explicit Lisp `(garbage-collect)` or firing
    /// `(garbage-collect-maybe FACTOR)` inside a `SerialTurn` through the
    /// keeper's HEAD-hold collection protocol (L6.7c (c) §3.3).
    #[cold]
    #[inline(never)]
    pub(crate) fn gc_collect_serial_turn(&mut self) -> Result<(), super::super::Flow> {
        debug_assert!(
            !self.gc_driver_active,
            "L6.7c (c) Q2: gc_driver_active must remain false across serial-turn collection"
        );
        // 1. Pre-stop World prep while heap TLS is installed (GNU alloc.c:5793-5794).
        neovm_edit::emacs_core::undo::compact_buffers_for_gc(self);
        let start = std::time::Instant::now();
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();

        // 2. Take the active scope out of TLS for the bridge call (leaving TLS
        //    empty while carrier TLS is parked and preventing re-entrant `&mut`
        //    aliasing). Restore it ONLY on `Ok(report)` so an unwinding or
        //    failing bridge never re-exposes a broken/detached scope to
        //    intermediate destructors inside `body(ctx)` (F6.1).
        let my_addr = std::ptr::from_ref(self) as usize;
        let my_heap = self.tagged_heap.heap_identity();
        let taken = SERIAL_TURN_COLLECT_SCOPE.with(|slot| slot.take());
        let Some(scope) = taken else {
            panic!("gc_collect_serial_turn called without an active SerialTurn collect scope");
        };
        if scope.ctx_addr != my_addr || scope.heap != my_heap {
            SERIAL_TURN_COLLECT_SCOPE.with(|slot| slot.set(Some(scope)));
            panic!("gc_collect_serial_turn scope mismatch (foreign Context or heap)");
        }
        let bridge_ptr = scope.bridge;
        // SAFETY: `bridge_ptr` points to the live stack closure installed by
        // our enclosing `LiveSerial::with_world_collectible`, and taking the
        // scope out of TLS guarantees unique `&mut` access for this call.
        let bridge = unsafe { &mut *bridge_ptr };
        let report = match bridge(self) {
            Ok(report) => {
                SERIAL_TURN_COLLECT_SCOPE.with(|slot| slot.set(Some(scope)));
                report
            }
            Err(error) => {
                // Fail closed without returning to Lisp or running direct
                // `TaggedHeap` collection: pre-detach errors (`MissingEpoch` /
                // `Detach`) leave `ChannelLend` armed to unwind K7 `Abandon`;
                // post-detach errors leave `DormantContext::detached == Some(...)`
                // (Catch 1) with carrier TLS parked (pentry NIT-1).
                panic!("serial-turn keeper collection failed closed: {error:?}");
            }
        };

        // 3. Shared post-collection bookkeeping on the resumed owner thread
        //    sourced from the keeper's completed cycle (no second local scan).
        self.finish_keeper_collection(&report, start)
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "E5.3, E5.4 and E6-P call it; the rows do now")
)]
impl<'t, C: SerialHost, W> ParkedSerial<'t, C, W> {
    /// What this turn waits on.
    pub(crate) fn wait(&self) -> &W {
        self.parked.wait()
    }

    /// Ask for a new turn at the ring tail and resume this one on the grant,
    /// activating the received owner as [`begin_turn`] does. Any refusal
    /// keeps the turn parked; a received ticket that cannot be used goes back
    /// to the keeper first.
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn resume<P: KeeperPort, A>(
        self,
        port: &mut P,
        activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
    ) -> Result<LiveSerial<'t, C, P::LendGuard>, (Self, TurnStartRefusal<C::Refusal, A>)> {
        let ParkedSerial { parked, home } = self;
        let (grant, guard) = match port.request_turn() {
            Ok(granted) => granted,
            Err(refusal) => {
                return Err((
                    ParkedSerial { parked, home },
                    TurnStartRefusal::Port(refusal),
                ));
            }
        };
        let (seat, local, guard) = match receive(home, grant, guard, port, activate) {
            Ok(received) => received,
            Err(refusal) => return Err((ParkedSerial { parked, home }, refusal)),
        };
        match parked.resume(local) {
            Ok(turn) => Ok(LiveSerial {
                turn,
                active: Some((seat, guard)),
                home,
            }),
            Err((parked, local, refusal)) => {
                give_back(seat, local, port);
                guard.disarm();
                Err((
                    ParkedSerial { parked, home },
                    TurnStartRefusal::Turn(refusal),
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/serial_park_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/serial_park_live_test.rs"]
mod live_tests;
