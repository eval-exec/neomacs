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
    /// Why a detach or reattach was refused.
    type Refusal;
    /// Take the host off this OS thread, or refuse and leave it as it was.
    fn detach(&mut self, transition: HostTransition) -> Result<Self::Detached, Self::Refusal>;
    /// Put the host back on this OS thread, or refuse and hand the proof
    /// back unchanged.
    fn reattach(
        &mut self,
        detached: Self::Detached,
        transition: HostTransition,
    ) -> Result<(), (Self::Detached, Self::Refusal)>;
    /// Called once a [`SerialTurn`] becomes live (after `reattach` succeeds).
    /// Paired 1:1 with [`SerialHost::leave_turn`] by `SerialTurn`'s RAII
    /// lifecycle (normal `end`/`park` or panic unwind `Drop`).
    ///
    /// Default no-op body is for stand-in test hosts only (the contract guard
    /// `l64_context_is_the_only_production_serial_host` enforces that `Context`
    /// is the sole production `SerialHost`, and `Context` overrides both hooks
    /// to bracket per-turn threshold-GC inhibition).
    fn enter_turn(&mut self, _transition: HostTransition) {}
    /// Called when a [`SerialTurn`] ends, parks, or unwind-drops.
    ///
    /// Default no-op body is for stand-in test hosts only; production hosts
    /// must override this hook paired 1:1 with [`SerialHost::enter_turn`].
    fn leave_turn(&mut self, _transition: HostTransition) {}
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
    /// Take a freshly built host off this OS thread. A refusal hands it
    /// back.
    fn new(mut context: Box<C>) -> Result<Self, (Box<C>, C::Refusal)> {
        match context.detach(HostTransition(())) {
            Ok(detached) => Ok(Self {
                context,
                detached: Some(detached),
                confined: PhantomData,
            }),
            Err(refusal) => Err((context, refusal)),
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

    /// Take the host off this OS thread when its turn ends or parks. A
    /// refusal leaves the turn live.
    fn detach(&mut self) -> Result<(), C::Refusal> {
        debug_assert!(self.detached.is_none(), "{LIVE_HOST}");
        self.detached = Some(self.context.detach(HostTransition(()))?);
        Ok(())
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
///
/// `SerialTurn` also serves as the RAII guard for [`SerialHost::enter_turn`] /
/// [`SerialHost::leave_turn`]: `begin` and `resume` call `enter_turn` after
/// `attach` succeeds, and `end`, `park`, or a panic unwind `Drop` calls
/// `leave_turn` exactly once so per-turn host state (such as L6.7c (a)'s
/// `gc_inhibit_depth`) cannot leak on unwind.
#[must_use = "a serial turn must end or park"]
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct SerialTurn<'turn, C: SerialHost, L: TicketOwner> {
    state: Option<&'turn mut OwnerSerialState<C, L>>,
    local: Option<L>,
    armed: Option<Armed>,
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
    /// OS thread; it leaves the thread here. A host that refuses to detach is
    /// handed back.
    pub(crate) fn new(owner: L::Owner, context: Box<C>) -> Result<Self, (Box<C>, C::Refusal)> {
        Ok(Self {
            dormant: DormantContext::new(context)?,
            owner,
        })
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
        self.dormant.context.enter_turn(HostTransition(()));
        Ok(SerialTurn {
            state: Some(self),
            local: Some(local),
            armed: Some(Armed(TURN_DROPPED)),
            confined: PhantomData,
        })
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn is_detached_for_test(&self) -> bool {
        self.dormant.detached.is_some()
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
        let state = self
            .state
            .as_deref_mut()
            .expect("live turn holds its state");
        body(&mut state.dormant.context)
    }

    /// End the turn: the host leaves this OS thread, and the local owner goes
    /// back to L6.4's finish and return. A host that refuses to detach keeps
    /// the turn live, and the turn is handed back.
    pub(crate) fn end(self) -> Result<L, (Self, C::Refusal)> {
        let mut this = self;
        if let Err(refusal) = this
            .state
            .as_deref_mut()
            .expect("live turn holds its state")
            .dormant
            .detach()
        {
            return Err((this, refusal));
        }
        let state = this.state.take().expect("live turn holds its state");
        state.dormant.context.leave_turn(HostTransition(()));
        let local = this.local.take().expect("live turn holds its local owner");
        this.armed.take().expect("live turn is armed").disarm();
        Ok(local)
    }

    /// Park on WAIT: the host leaves this OS thread, the local owner goes
    /// back with the ticket, and the parked turn keeps only the owner's
    /// state. A host that refuses to detach keeps the turn live, and the turn
    /// and WAIT are handed back.
    #[allow(clippy::type_complexity)]
    pub(crate) fn park<W>(
        self,
        wait: W,
    ) -> Result<(ParkedTurn<'turn, C, L, W>, L), (Self, W, C::Refusal)> {
        let mut this = self;
        if let Err(refusal) = this
            .state
            .as_deref_mut()
            .expect("live turn holds its state")
            .dormant
            .detach()
        {
            return Err((this, wait, refusal));
        }
        let state = this.state.take().expect("live turn holds its state");
        state.dormant.context.leave_turn(HostTransition(()));
        let local = this.local.take().expect("live turn holds its local owner");
        this.armed.take().expect("live turn is armed").disarm();
        let parked = ParkedTurn {
            state,
            wait,
            armed: Armed(PARKED_DROPPED),
            confined: PhantomData,
        };
        Ok((parked, local))
    }

    /// Lend the owner's Context to BODY alongside disjoint mutable access to
    /// the turn's ticket slot and [`TurnHostOps`], so an in-stack collection
    /// (L6.7c (c)) can detach the host directly into `DormantContext::detached`,
    /// return the ticket to collect, receive the re-granted ticket, and
    /// reattach without aliasing `&mut C`.
    #[allow(dead_code)]
    pub(crate) fn with_world_rebind<R>(
        &mut self,
        body: impl for<'g> FnOnce(&'g mut C, &'g mut Option<L>, TurnHostOps<'g, C>) -> R,
    ) -> R {
        let state = self
            .state
            .as_deref_mut()
            .expect("live turn holds its state");
        let DormantContext {
            context, detached, ..
        } = &mut state.dormant;
        let ops = TurnHostOps {
            detached,
            confined: PhantomData,
        };
        body(&mut **context, &mut self.local, ops)
    }
}

/// Host transition capability scoped to [`SerialTurn::with_world_rebind`],
/// holding `&mut Option<C::Detached>` from the owner's [`DormantContext`] so
/// any mid-turn detach/reattach keeps `DormantContext::detached` consistent
/// even on panic unwind (L6.7c (c), Catch 1).
#[allow(dead_code)]
pub(crate) struct TurnHostOps<'a, C: SerialHost> {
    detached: &'a mut Option<C::Detached>,
    confined: Confined,
}

#[allow(dead_code)]
impl<C: SerialHost> TurnHostOps<'_, C> {
    /// Mint a [`HostTransition`] while asserting this turn's host is currently
    /// attached (`DormantContext::detached == None`), then store the resulting
    /// detached proof directly inside `DormantContext::detached`.
    pub(crate) fn detach_with<E>(
        &mut self,
        f: impl FnOnce(HostTransition) -> Result<C::Detached, E>,
    ) -> Result<(), E> {
        assert!(self.detached.is_none(), "{LIVE_HOST}");
        let proof = f(HostTransition(()))?;
        *self.detached = Some(proof);
        Ok(())
    }

    /// Take the detached proof out of `DormantContext::detached`, pass it with
    /// a [`HostTransition`] to `f`, and restore it on refusal.
    pub(crate) fn reattach_with<E>(
        &mut self,
        f: impl FnOnce(C::Detached, HostTransition) -> Result<(), (C::Detached, E)>,
    ) -> Result<(), E> {
        let proof = self.detached.take().expect(LIVE_HOST);
        match f(proof, HostTransition(())) {
            Ok(()) => Ok(()),
            Err((proof, err)) => {
                *self.detached = Some(proof);
                Err(err)
            }
        }
    }
}

impl<C: SerialHost, L: TicketOwner> Drop for SerialTurn<'_, C, L> {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            state.dormant.context.leave_turn(HostTransition(()));
        }
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
        state.dormant.context.enter_turn(HostTransition(()));
        Ok(SerialTurn {
            state: Some(state),
            local: Some(local),
            armed: Some(Armed(TURN_DROPPED)),
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
    fn detach(&mut self, _: HostTransition) -> Result<(), ()> {
        Ok(())
    }
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
    use super::{
        HostTransition, OwnerSerialState, ParkedTurn, SerialHost, SerialTurn, TurnRefusal,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    /// A stand-in Context that counts its constructions in the test's own
    /// counter and logs its host transitions. While `refuse` is set it
    /// refuses to reattach, and while `stuck` is set it refuses to detach.
    struct Counted {
        writes: Vec<&'static str>,
        refuse: Rc<Cell<bool>>,
        stuck: Rc<Cell<bool>>,
        turn_depth: Rc<Cell<usize>>,
    }

    impl Counted {
        fn new(built: &Rc<Cell<usize>>) -> Self {
            built.set(built.get() + 1);
            Self {
                writes: Vec::new(),
                refuse: Rc::new(Cell::new(false)),
                stuck: Rc::new(Cell::new(false)),
                turn_depth: Rc::new(Cell::new(0)),
            }
        }
    }

    impl SerialHost for Counted {
        type Detached = &'static str;
        type Refusal = &'static str;
        fn detach(&mut self, _: HostTransition) -> Result<&'static str, &'static str> {
            if self.stuck.get() {
                return Err("stuck");
            }
            self.writes.push("detach");
            Ok("proof")
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
        fn enter_turn(&mut self, _: HostTransition) {
            self.turn_depth.set(self.turn_depth.get() + 1);
        }
        fn leave_turn(&mut self, _: HostTransition) {
            self.turn_depth.set(self.turn_depth.get() - 1);
        }
    }

    type Ticket = (u64, &'static str);
    type State = OwnerSerialState<Counted, Ticket>;

    fn dormant(host: Counted) -> State {
        OwnerSerialState::new(7, Box::new(host))
            .ok()
            .expect("a fresh host detaches")
    }

    fn owner_seven_counted(built: &Rc<Cell<usize>>) -> State {
        dormant(Counted::new(built))
    }

    fn owner_seven() -> State {
        owner_seven_counted(&Rc::new(Cell::new(0)))
    }

    fn ended(turn: SerialTurn<'_, Counted, Ticket>) -> Ticket {
        turn.end().ok().expect("the host detaches")
    }

    fn parked<'t, W>(
        turn: SerialTurn<'t, Counted, Ticket>,
        wait: W,
    ) -> (ParkedTurn<'t, Counted, Ticket, W>, Ticket) {
        turn.park(wait).ok().expect("the host detaches")
    }

    #[test]
    fn e51_two_turns_of_one_owner_reuse_the_dormant_context() {
        let built = Rc::new(Cell::new(0));
        let mut state = owner_seven_counted(&built);
        let mut first = state.begin((7, "ticket-1")).ok().expect("own ticket");
        first.with_world(|context| context.writes.push("first"));
        assert_eq!(ended(first), (7, "ticket-1"));
        let mut second = state.begin((7, "ticket-2")).ok().expect("own ticket");
        let seen = second.with_world(|context| {
            context.writes.push("second");
            context.writes.clone()
        });
        assert_eq!(ended(second), (7, "ticket-2"));
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
        let (parked, returned) = parked(turn, "mutex");
        assert_eq!(returned, (7, "ticket-1"), "park gives the ticket back");
        assert_eq!(*parked.wait(), "mutex");
        let mut resumed = parked.resume((7, "ticket-2")).ok().expect("own ticket");
        let seen = resumed.with_world(|context| {
            context.writes.push("after resume");
            context.writes.clone()
        });
        assert_eq!(ended(resumed), (7, "ticket-2"));
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
        let (parked, _) = parked(turn, "join");
        let (parked, refused, why) = parked.resume((9, "foreign")).err().expect("foreign ticket");
        assert_eq!((refused, why), ((9, "foreign"), TurnRefusal::WrongOwner));
        assert_eq!(*parked.wait(), "join", "the refused resume keeps the wait");
        let turn = parked.resume((7, "ticket-2")).ok().expect("own ticket");
        assert_eq!(ended(turn), (7, "ticket-2"));
    }

    #[test]
    fn e51_a_host_that_refuses_to_reattach_stays_dormant_and_hands_the_ticket_back() {
        let host = Counted::new(&Rc::new(Cell::new(0)));
        let refuse = Rc::clone(&host.refuse);
        let mut state = dormant(host);
        refuse.set(true);
        let refused = state.begin((7, "ticket-1")).err().expect("refused host");
        assert_eq!(refused, ((7, "ticket-1"), TurnRefusal::Host("busy")));
        refuse.set(false);
        let mut turn = state.begin((7, "ticket-1")).ok().expect("host reattaches");
        turn.with_world(|context| context.writes.push("ran"));
        let (parked, _) = parked(turn, "mutex");
        refuse.set(true);
        let (parked, refused, why) = parked.resume((7, "ticket-2")).err().expect("refused host");
        assert_eq!((refused, why), ((7, "ticket-2"), TurnRefusal::Host("busy")));
        refuse.set(false);
        let mut turn = parked
            .resume((7, "ticket-2"))
            .ok()
            .expect("host reattaches");
        let seen = turn.with_world(|context| context.writes.clone());
        assert_eq!(ended(turn), (7, "ticket-2"));
        assert_eq!(
            seen,
            ["detach", "reattach", "ran", "detach", "reattach"],
            "a refused reattach leaves the proof in place for the next one"
        );
    }

    #[test]
    fn e51_a_host_that_refuses_to_detach_keeps_its_turn_live() {
        let host = Counted::new(&Rc::new(Cell::new(0)));
        let stuck = Rc::clone(&host.stuck);
        stuck.set(true);
        let host = match OwnerSerialState::<Counted, Ticket>::new(7, Box::new(host)) {
            Ok(_) => panic!("a stuck host cannot be kept dormant"),
            Err((host, why)) => {
                assert_eq!(why, "stuck");
                host
            }
        };
        stuck.set(false);
        let mut state = dormant(*host);
        let turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        stuck.set(true);
        let Err((mut turn, why)) = turn.end() else {
            panic!("a stuck host keeps the turn");
        };
        assert_eq!(why, "stuck");
        turn.with_world(|context| context.writes.push("still live"));
        let Err((turn, wait, why)) = turn.park("mutex") else {
            panic!("a stuck host keeps the turn");
        };
        assert_eq!((wait, why), ("mutex", "stuck"));
        stuck.set(false);
        let mut turn = turn;
        let seen = turn.with_world(|context| context.writes.clone());
        assert_eq!(ended(turn), (7, "ticket-1"));
        assert_eq!(
            seen,
            ["detach", "reattach", "still live"],
            "a refused detach leaves the host attached for the live turn"
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
        let (parked, _) = parked(turn, "mutex");
        drop(parked);
    }

    #[test]
    fn l67c_a_serial_turn_raii_leave_runs_on_end_park_and_unwind_drop() {
        let host = Counted::new(&Rc::new(Cell::new(0)));
        let depth = Rc::clone(&host.turn_depth);
        let stuck = Rc::clone(&host.stuck);
        let refuse = Rc::clone(&host.refuse);
        let mut state = dormant(host);
        assert_eq!(depth.get(), 0, "dormant host starts at depth 0");

        // Refused begin must not enter_turn.
        refuse.set(true);
        assert!(state.begin((7, "refused")).is_err());
        assert_eq!(depth.get(), 0, "refused begin leaves depth 0");
        refuse.set(false);

        // Normal begin -> refused end (stays 1) -> end (returns to 0).
        let turn = state.begin((7, "t1")).ok().expect("own ticket");
        assert_eq!(depth.get(), 1);
        stuck.set(true);
        let Err((turn, _)) = turn.end() else {
            panic!("stuck detach keeps turn live");
        };
        assert_eq!(depth.get(), 1, "refused end keeps depth 1");
        stuck.set(false);
        assert_eq!(ended(turn), (7, "t1"));
        assert_eq!(depth.get(), 0, "end restores depth 0");

        // Park -> resume -> panic unwind dropping SerialTurn mid-turn must
        // still invoke leave_turn via SerialTurn's Drop impl (no depth leak).
        let turn = state.begin((7, "t2")).ok().expect("own ticket");
        assert_eq!(depth.get(), 1);
        let (parked_turn, _) = parked(turn, "wait");
        assert_eq!(depth.get(), 0, "park restores depth 0");
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut resumed = parked_turn.resume((7, "t3")).ok().expect("own ticket");
            assert_eq!(depth.get(), 1, "resume restores depth 1");
            resumed.with_world(|_| panic!("unwind past SerialTurn"));
        }));
        assert!(unwound.is_err(), "panic unwound past SerialTurn");
        assert_eq!(
            depth.get(),
            0,
            "SerialTurn::drop must run leave_turn on unwind so depth cannot leak"
        );
    }
}
