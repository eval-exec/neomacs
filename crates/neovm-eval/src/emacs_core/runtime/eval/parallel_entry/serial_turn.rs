//! E5.1: the serial turn of a parallel owner (E5-DESIGN §2, E5.1-TYPES).
//!
//! A parallel owner reaches the World only inside a serial turn, which is
//! the L6 ticket plus the Baton. This file states the turn's consumption and
//! borrow contracts and nothing else. Like `capability.rs` it is
//! self-contained, so the compile-fail fixtures can path-include it.
//!
//! The production instantiation names the owner's dormant `Context` as `C`
//! and the activated local owner of a received ticket as `L`. L6.4's heap
//! loan produces that `L` before [`OwnerSerialState::begin`]. The rest of
//! activation needs the Context, which only this state holds, so the state
//! runs it: `C` is a [`SerialHost`], taken off its OS thread when the state
//! is built and when a turn ends or parks, and put back when a turn begins
//! or resumes.
//!
//! Contracts:
//! - a state serves one owner: `begin` and `resume` refuse, and hand back,
//!   another owner's ticket, and `L` is one type for the state's whole life;
//! - the owner's Context is reachable only through [`SerialTurn::with_world`],
//!   and its higher-ranked `'g` keeps every view inside the closure;
//! - `end` and `park` consume the turn, so nothing derived from it survives
//!   them, and a turn cannot end or park from inside `with_world`;
//! - a turn dropped without `end` or `park`, or a parked turn dropped without
//!   `resume`, is a bug that debug builds report (its TLS would stay
//!   installed, or its owner would never run again);
//! - none of these types is `Send` or `Sync`: serial state stays on the
//!   owner's OS thread, and only the L6 ticket crosses threads;
//! - only this file detaches or reattaches a host (a [`HostTransition`] has
//!   a private field), and a host's detached proof is kept beside that host,
//!   so no proof can be reattached into another host.

use std::marker::PhantomData;
use std::rc::Rc;

type Confined = PhantomData<Rc<()>>;

/// The local owner of a received ticket. Its owner identity binds it to one
/// parallel owner's serial state.
pub(crate) trait TicketOwner {
    type Owner: Copy + Eq;
    fn owner(&self) -> Self::Owner;
}

/// The dormant Context's side of a turn boundary (L6.4).
///
/// The methods take a [`HostTransition`], which only this file can make, so
/// [`OwnerSerialState`] is their only caller.
pub(crate) trait SerialHost {
    /// The proof that the host is off its OS thread.
    type Detached;
    /// Why a reattach was refused.
    type Refusal;
    /// Take the host off this OS thread.
    fn detach(&mut self, transition: HostTransition) -> Self::Detached;
    /// Put the host back on this OS thread, or refuse and hand the proof
    /// back unchanged.
    fn reattach(
        &mut self,
        detached: Self::Detached,
        transition: HostTransition,
    ) -> Result<(), (Self::Detached, Self::Refusal)>;
}

/// Permission to detach or reattach a [`SerialHost`]. The private field
/// keeps its construction in this file.
pub(crate) struct HostTransition(());

/// Why a turn did not begin or resume. The local owner is handed back with
/// it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TurnRefusal<R> {
    /// The ticket belongs to another owner.
    WrongOwner,
    /// The host refused to reattach. It stays dormant.
    Host(R),
}

/// Reports, in debug builds, a turn or parked turn that was dropped instead
/// of ended, parked or resumed. Each transition disarms it.
struct Armed(&'static str);

impl Armed {
    fn disarm(self) {
        std::mem::forget(self);
    }
}

impl Drop for Armed {
    fn drop(&mut self) {
        debug_assert!(std::thread::panicking(), "{}", self.0);
    }
}

/// A suspended Context. No method yields a reference to it: only a turn's
/// [`SerialTurn::with_world`] lends it.
///
/// `detached` is the host's proof that it is off its OS thread. It is `Some`
/// exactly while no turn is live. It is dropped only together with its host.
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct DormantContext<C: SerialHost> {
    context: Box<C>,
    detached: Option<C::Detached>,
    confined: Confined,
}

const LIVE_HOST: &str = "a dormant host is detached between turns";

#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
impl<C: SerialHost> DormantContext<C> {
    /// Take a freshly built host off this OS thread.
    fn new(mut context: Box<C>) -> Self {
        let detached = context.detach(HostTransition(()));
        Self {
            context,
            detached: Some(detached),
            confined: PhantomData,
        }
    }

    /// Put the host back on this OS thread for a turn. A refusal keeps it
    /// dormant.
    fn attach(&mut self) -> Result<(), C::Refusal> {
        let detached = self.detached.take().expect(LIVE_HOST);
        self.context
            .reattach(detached, HostTransition(()))
            .map_err(|(detached, refusal)| {
                self.detached = Some(detached);
                refusal
            })
    }

    /// Take the host off this OS thread when its turn ends or parks.
    fn detach(&mut self) {
        debug_assert!(self.detached.is_none(), "{LIVE_HOST}");
        self.detached = Some(self.context.detach(HostTransition(())));
    }
}

/// One parallel owner's serial-side state, kept on its OS thread across
/// turns (E5.1-TYPES §1). It holds the one dormant Context, built once per
/// owner; every turn reuses it.
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct OwnerSerialState<C: SerialHost, L: TicketOwner> {
    dormant: DormantContext<C>,
    owner: L::Owner,
}

/// A live serial turn. `L` owns the received ticket until the turn ends.
#[must_use = "a serial turn must end or park"]
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct SerialTurn<'turn, C: SerialHost, L: TicketOwner> {
    state: &'turn mut OwnerSerialState<C, L>,
    local: L,
    armed: Armed,
    confined: Confined,
}

/// A turn that gave its ticket back and waits on `W` (E5 §2). Resuming it
/// takes a newly received ticket of the same owner.
#[must_use = "a parked turn must resume"]
#[allow(dead_code)] // Dormant E5.1; E5.2 parks and resumes.
pub(crate) struct ParkedTurn<'turn, C: SerialHost, L: TicketOwner, W> {
    state: &'turn mut OwnerSerialState<C, L>,
    wait: W,
    armed: Armed,
    confined: Confined,
}

const TURN_DROPPED: &str = "a serial turn was dropped without end or park";
const PARKED_DROPPED: &str = "a parked serial turn was dropped without resume";

#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
impl<C: SerialHost, L: TicketOwner> OwnerSerialState<C, L> {
    /// Keep CONTEXT dormant for OWNER's turns. CONTEXT was just built on this
    /// OS thread; it leaves the thread here.
    pub(crate) fn new(owner: L::Owner, context: Box<C>) -> Self {
        Self {
            dormant: DormantContext::new(context),
            owner,
        }
    }

    /// Begin a turn for LOCAL, the local owner of the received ticket: the
    /// host comes back onto this OS thread. Another owner's ticket, or a
    /// host that refuses to reattach, is refused and LOCAL handed back.
    pub(crate) fn begin(
        &mut self,
        local: L,
    ) -> Result<SerialTurn<'_, C, L>, (L, TurnRefusal<C::Refusal>)> {
        if local.owner() != self.owner {
            return Err((local, TurnRefusal::WrongOwner));
        }
        if let Err(refusal) = self.dormant.attach() {
            return Err((local, TurnRefusal::Host(refusal)));
        }
        Ok(SerialTurn {
            state: self,
            local,
            armed: Armed(TURN_DROPPED),
            confined: PhantomData,
        })
    }
}

#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
impl<'turn, C: SerialHost, L: TicketOwner> SerialTurn<'turn, C, L> {
    /// Lend the owner's Context to BODY. No view outlives the call.
    ///
    /// BODY gets the whole `&mut C`, so it could also replace the Context
    /// (for example with `std::mem::swap`). The legacy primitives' ABI takes
    /// `&mut Context`, so this cannot be narrowed here. When L6.4's authority
    /// proof lands, BODY also receives the `'g`-branded `BatonWitness`
    /// (`capability.rs`).
    pub(crate) fn with_world<R>(&mut self, body: impl for<'g> FnOnce(&'g mut C) -> R) -> R {
        body(&mut self.state.dormant.context)
    }

    /// End the turn: the host leaves this OS thread, and the local owner goes
    /// back to L6.4's finish and return.
    pub(crate) fn end(self) -> L {
        let SerialTurn {
            state,
            local,
            armed,
            ..
        } = self;
        state.dormant.detach();
        armed.disarm();
        local
    }

    /// Park on WAIT: the host leaves this OS thread, the local owner goes
    /// back with the ticket, and the parked turn keeps only the owner's
    /// state.
    pub(crate) fn park<W>(self, wait: W) -> (ParkedTurn<'turn, C, L, W>, L) {
        let SerialTurn {
            state,
            local,
            armed,
            ..
        } = self;
        state.dormant.detach();
        armed.disarm();
        let parked = ParkedTurn {
            state,
            wait,
            armed: Armed(PARKED_DROPPED),
            confined: PhantomData,
        };
        (parked, local)
    }
}

#[allow(dead_code)] // Dormant E5.1; E5.2 parks and resumes.
impl<'turn, C: SerialHost, L: TicketOwner, W> ParkedTurn<'turn, C, L, W> {
    /// What this turn waits on.
    pub(crate) fn wait(&self) -> &W {
        &self.wait
    }

    /// Resume with LOCAL, the local owner of a newly received ticket: the
    /// host comes back onto this OS thread. Another owner's ticket, or a host
    /// that refuses to reattach, is refused and handed back with the parked
    /// turn.
    #[allow(clippy::type_complexity)]
    pub(crate) fn resume(
        self,
        local: L,
    ) -> Result<SerialTurn<'turn, C, L>, (Self, L, TurnRefusal<C::Refusal>)> {
        if local.owner() != self.state.owner {
            return Err((self, local, TurnRefusal::WrongOwner));
        }
        if let Err(refusal) = self.state.dormant.attach() {
            return Err((self, local, TurnRefusal::Host(refusal)));
        }
        let ParkedTurn { state, armed, .. } = self;
        armed.disarm();
        Ok(SerialTurn {
            state,
            local,
            armed: Armed(TURN_DROPPED),
            confined: PhantomData,
        })
    }
}

/// The fixtures' and rows' ticket: any `Copy + Eq` owner.
#[cfg(test)]
impl TicketOwner for (u64, &'static str) {
    type Owner = u64;
    fn owner(&self) -> u64 {
        self.0
    }
}

/// A `Send + Sync` ticket and host for the assertions below, so that only
/// the turn types themselves can make them fail.
struct AssertedTicket;

impl TicketOwner for AssertedTicket {
    type Owner = ();
    fn owner(&self) {}
}

struct AssertedHost;

impl SerialHost for AssertedHost {
    type Detached = ();
    type Refusal = ();
    fn detach(&mut self, _: HostTransition) {}
    fn reattach(&mut self, _: (), _: HostTransition) -> Result<(), ((), ())> {
        Ok(())
    }
}

static_assertions::assert_not_impl_any!(DormantContext<AssertedHost>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(OwnerSerialState<AssertedHost, AssertedTicket>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(SerialTurn<'static, AssertedHost, AssertedTicket>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(ParkedTurn<'static, AssertedHost, AssertedTicket, ()>: Send, Sync, Clone);

#[cfg(test)]
mod tests {
    use super::{HostTransition, OwnerSerialState, SerialHost, TurnRefusal};
    use std::cell::Cell;
    use std::rc::Rc;

    /// A stand-in Context that counts its constructions in the test's own
    /// counter and logs its host transitions. While `refuse` is set it
    /// refuses to reattach.
    struct Counted {
        writes: Vec<&'static str>,
        refuse: Rc<Cell<bool>>,
    }

    impl Counted {
        fn new(built: &Rc<Cell<usize>>) -> Self {
            built.set(built.get() + 1);
            Self {
                writes: Vec::new(),
                refuse: Rc::new(Cell::new(false)),
            }
        }
    }

    impl SerialHost for Counted {
        type Detached = &'static str;
        type Refusal = &'static str;
        fn detach(&mut self, _: HostTransition) -> &'static str {
            self.writes.push("detach");
            "proof"
        }
        fn reattach(
            &mut self,
            detached: &'static str,
            _: HostTransition,
        ) -> Result<(), (&'static str, &'static str)> {
            assert_eq!(detached, "proof");
            if self.refuse.get() {
                return Err((detached, "busy"));
            }
            self.writes.push("reattach");
            Ok(())
        }
    }

    type State = OwnerSerialState<Counted, (u64, &'static str)>;

    fn owner_seven_counted(built: &Rc<Cell<usize>>) -> State {
        OwnerSerialState::new(7, Box::new(Counted::new(built)))
    }

    fn owner_seven() -> State {
        owner_seven_counted(&Rc::new(Cell::new(0)))
    }

    #[test]
    fn e51_two_turns_of_one_owner_reuse_the_dormant_context() {
        let built = Rc::new(Cell::new(0));
        let mut state = owner_seven_counted(&built);
        let mut first = state.begin((7, "ticket-1")).ok().expect("own ticket");
        first.with_world(|context| context.writes.push("first"));
        assert_eq!(first.end(), (7, "ticket-1"));
        let mut second = state.begin((7, "ticket-2")).ok().expect("own ticket");
        let seen = second.with_world(|context| {
            context.writes.push("second");
            context.writes.clone()
        });
        assert_eq!(second.end(), (7, "ticket-2"));
        assert_eq!(
            seen,
            [
                "detach", "reattach", "first", "detach", "reattach", "second"
            ],
            "built dormant; each turn reattaches its host and each end detaches it"
        );
        assert_eq!(built.get(), 1, "one construction");
    }

    #[test]
    fn e51_a_parked_turn_returns_its_ticket_and_resumes_on_the_same_context() {
        let mut state = owner_seven();
        let mut turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        turn.with_world(|context| context.writes.push("before park"));
        let (parked, returned) = turn.park("mutex");
        assert_eq!(returned, (7, "ticket-1"), "park gives the ticket back");
        assert_eq!(*parked.wait(), "mutex");
        let mut resumed = parked.resume((7, "ticket-2")).ok().expect("own ticket");
        let seen = resumed.with_world(|context| {
            context.writes.push("after resume");
            context.writes.clone()
        });
        assert_eq!(resumed.end(), (7, "ticket-2"));
        assert_eq!(
            seen,
            [
                "detach",
                "reattach",
                "before park",
                "detach",
                "reattach",
                "after resume"
            ],
            "park detaches the host and resume reattaches it"
        );
    }

    #[test]
    fn e51_begin_and_resume_refuse_another_owners_ticket_and_hand_it_back() {
        let mut state = owner_seven();
        let refused = state.begin((8, "foreign")).err().expect("foreign ticket");
        assert_eq!(refused, ((8, "foreign"), TurnRefusal::WrongOwner));
        let turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        let (parked, _) = turn.park("join");
        let (parked, refused, why) = parked.resume((9, "foreign")).err().expect("foreign ticket");
        assert_eq!((refused, why), ((9, "foreign"), TurnRefusal::WrongOwner));
        assert_eq!(*parked.wait(), "join", "the refused resume keeps the wait");
        let turn = parked.resume((7, "ticket-2")).ok().expect("own ticket");
        assert_eq!(turn.end(), (7, "ticket-2"));
    }

    #[test]
    fn e51_a_host_that_refuses_to_reattach_stays_dormant_and_hands_the_ticket_back() {
        let built = Rc::new(Cell::new(0));
        let host = Counted::new(&built);
        let refuse = Rc::clone(&host.refuse);
        let mut state: State = OwnerSerialState::new(7, Box::new(host));
        refuse.set(true);
        let refused = state.begin((7, "ticket-1")).err().expect("refused host");
        assert_eq!(refused, ((7, "ticket-1"), TurnRefusal::Host("busy")));
        refuse.set(false);
        let mut turn = state.begin((7, "ticket-1")).ok().expect("host reattaches");
        turn.with_world(|context| context.writes.push("ran"));
        let (parked, _) = turn.park("mutex");
        refuse.set(true);
        let (parked, refused, why) = parked.resume((7, "ticket-2")).err().expect("refused host");
        assert_eq!((refused, why), ((7, "ticket-2"), TurnRefusal::Host("busy")));
        refuse.set(false);
        let mut turn = parked
            .resume((7, "ticket-2"))
            .ok()
            .expect("host reattaches");
        let seen = turn.with_world(|context| context.writes.clone());
        assert_eq!(turn.end(), (7, "ticket-2"));
        assert_eq!(
            seen,
            ["detach", "reattach", "ran", "detach", "reattach"],
            "a refused reattach leaves the proof in place for the next one"
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "a serial turn was dropped without end or park")]
    fn e51_a_turn_dropped_without_end_or_park_is_reported() {
        let mut state = owner_seven();
        let turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        drop(turn);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "a parked serial turn was dropped without resume")]
    fn e51_a_parked_turn_dropped_without_resume_is_reported() {
        let mut state = owner_seven();
        let turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        let (parked, _) = turn.park("mutex");
        drop(parked);
    }
}
