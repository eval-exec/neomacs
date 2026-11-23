//! E5.1: the serial turn of a parallel owner (E5-DESIGN §2, E5.1-TYPES).
//!
//! A parallel owner reaches the World only inside a serial turn, which is
//! the L6 ticket plus the Baton. This file states the turn's consumption and
//! borrow contracts and nothing else. Like `capability.rs` it is
//! self-contained, so the compile-fail fixtures can path-include it.
//!
//! The production instantiation names the owner's dormant `Context` as `C`
//! and the activated local owner of a received ticket as `L`. L6.4's
//! activation (heap loan, heap TLS, frame/slab/buffer reattach, carrier
//! restore) produces that `L` before [`OwnerSerialState::begin`], so holding
//! one is the proof that activation ran. Its suspension follows
//! [`SerialTurn::end`] and [`SerialTurn::park`]. Both are outside this file.
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
//!   owner's OS thread, and only the L6 ticket crosses threads.

use std::marker::PhantomData;
use std::rc::Rc;

type Confined = PhantomData<Rc<()>>;

/// The local owner of a received ticket. Its owner identity binds it to one
/// parallel owner's serial state.
pub(crate) trait TicketOwner {
    type Owner: Copy + Eq;
    fn owner(&self) -> Self::Owner;
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
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct DormantContext<C> {
    context: Box<C>,
    confined: Confined,
}

/// One parallel owner's serial-side state, kept on its OS thread across
/// turns (E5.1-TYPES §1). It holds the one dormant Context, built once per
/// owner; every turn reuses it.
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct OwnerSerialState<C, L: TicketOwner> {
    dormant: DormantContext<C>,
    owner: L::Owner,
}

/// A live serial turn. `L` owns the received ticket until the turn ends.
#[must_use = "a serial turn must end or park"]
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct SerialTurn<'turn, C, L: TicketOwner> {
    state: &'turn mut OwnerSerialState<C, L>,
    local: L,
    armed: Armed,
    confined: Confined,
}

/// A turn that gave its ticket back and waits on `W` (E5 §2). Resuming it
/// takes a newly received ticket of the same owner.
#[must_use = "a parked turn must resume"]
#[allow(dead_code)] // Dormant E5.1; E5.2 parks and resumes.
pub(crate) struct ParkedTurn<'turn, C, L: TicketOwner, W> {
    state: &'turn mut OwnerSerialState<C, L>,
    wait: W,
    armed: Armed,
    confined: Confined,
}

const TURN_DROPPED: &str = "a serial turn was dropped without end or park";
const PARKED_DROPPED: &str = "a parked serial turn was dropped without resume";

#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
impl<C, L: TicketOwner> OwnerSerialState<C, L> {
    /// Keep CONTEXT dormant for OWNER's turns.
    pub(crate) fn new(owner: L::Owner, context: Box<C>) -> Self {
        Self {
            dormant: DormantContext {
                context,
                confined: PhantomData,
            },
            owner,
        }
    }

    /// Begin a turn for LOCAL, the local owner of the received ticket.
    /// Another owner's ticket is refused and handed back.
    pub(crate) fn begin(&mut self, local: L) -> Result<SerialTurn<'_, C, L>, L> {
        if local.owner() != self.owner {
            return Err(local);
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
impl<'turn, C, L: TicketOwner> SerialTurn<'turn, C, L> {
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

    /// End the turn. The local owner goes back to L6.4's finish and return.
    pub(crate) fn end(self) -> L {
        let SerialTurn { local, armed, .. } = self;
        armed.disarm();
        local
    }

    /// Park on WAIT. The local owner goes back with the ticket, and the
    /// parked turn keeps only the owner's state.
    pub(crate) fn park<W>(self, wait: W) -> (ParkedTurn<'turn, C, L, W>, L) {
        let SerialTurn {
            state,
            local,
            armed,
            ..
        } = self;
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
impl<'turn, C, L: TicketOwner, W> ParkedTurn<'turn, C, L, W> {
    /// What this turn waits on.
    pub(crate) fn wait(&self) -> &W {
        &self.wait
    }

    /// Resume with LOCAL, the local owner of a newly received ticket. Another
    /// owner's ticket is refused and handed back with the parked turn.
    pub(crate) fn resume(self, local: L) -> Result<SerialTurn<'turn, C, L>, (Self, L)> {
        if local.owner() != self.state.owner {
            return Err((self, local));
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

/// A `Send + Sync` ticket for the assertions below, so that only the turn
/// types themselves can make them fail.
struct AssertedTicket;

impl TicketOwner for AssertedTicket {
    type Owner = ();
    fn owner(&self) {}
}

static_assertions::assert_not_impl_any!(DormantContext<()>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(OwnerSerialState<(), AssertedTicket>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(SerialTurn<'static, (), AssertedTicket>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(ParkedTurn<'static, (), AssertedTicket, ()>: Send, Sync, Clone);

#[cfg(test)]
mod tests {
    use super::OwnerSerialState;
    use std::cell::Cell;

    thread_local! {
        static BUILT: Cell<usize> = const { Cell::new(0) };
    }

    /// A stand-in Context that counts its constructions.
    struct Counted {
        writes: Vec<&'static str>,
    }

    impl Counted {
        fn new() -> Self {
            BUILT.with(|built| built.set(built.get() + 1));
            Self { writes: Vec::new() }
        }
    }

    type State = OwnerSerialState<Counted, (u64, &'static str)>;

    fn owner_seven() -> State {
        OwnerSerialState::new(7, Box::new(Counted::new()))
    }

    #[test]
    fn e51_two_turns_of_one_owner_reuse_the_dormant_context() {
        let before = BUILT.with(Cell::get);
        let mut state = owner_seven();
        let mut first = state.begin((7, "ticket-1")).ok().expect("own ticket");
        first.with_world(|context| context.writes.push("first"));
        assert_eq!(first.end(), (7, "ticket-1"));
        let mut second = state.begin((7, "ticket-2")).ok().expect("own ticket");
        let seen = second.with_world(|context| {
            context.writes.push("second");
            context.writes.clone()
        });
        assert_eq!(second.end(), (7, "ticket-2"));
        assert_eq!(seen, ["first", "second"]);
        assert_eq!(BUILT.with(Cell::get) - before, 1, "one construction");
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
        assert_eq!(seen, ["before park", "after resume"]);
    }

    #[test]
    fn e51_begin_and_resume_refuse_another_owners_ticket_and_hand_it_back() {
        let mut state = owner_seven();
        let refused = state.begin((8, "foreign")).err().expect("foreign ticket");
        assert_eq!(refused, (8, "foreign"));
        let turn = state.begin((7, "ticket-1")).ok().expect("own ticket");
        let (parked, _) = turn.park("join");
        let (parked, refused) = parked.resume((9, "foreign")).err().expect("foreign ticket");
        assert_eq!(refused, (9, "foreign"));
        assert_eq!(*parked.wait(), "join", "the refused resume keeps the wait");
        let turn = parked.resume((7, "ticket-2")).ok().expect("own ticket");
        assert_eq!(turn.end(), (7, "ticket-2"));
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
