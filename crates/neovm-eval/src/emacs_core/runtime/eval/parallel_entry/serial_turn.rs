//! E5.1: the serial turn of a parallel owner (E5-DESIGN §2, E5.1-TYPES).
//!
//! A parallel owner reaches the World only inside a serial turn, which is
//! the L6 ticket plus the Baton. This file states the turn's consumption and
//! borrow contracts and nothing else. Like `capability.rs` it is
//! self-contained, so the compile-fail fixtures can path-include it.
//!
//! The production instantiation names the owner's dormant `Context` as `C`
//! and L6.4's `LocalSerialOwner` as `L`. L6.4's activation (heap loan, heap
//! TLS, frame/slab/buffer reattach, carrier restore) runs before
//! [`OwnerSerialState::begin`], and its suspension after [`SerialTurn::end`]
//! and [`SerialTurn::park`]. Both are outside this file.
//!
//! Contracts:
//! - the owner's Context is reachable only through [`SerialTurn::with_world`],
//!   and its higher-ranked `'g` keeps every view inside the closure;
//! - `end` and `park` consume the turn, so nothing derived from it survives
//!   them, and a turn cannot end or park from inside `with_world`;
//! - none of these types is `Send` or `Sync`: serial state stays on the
//!   owner's OS thread, and only the L6 ticket crosses threads.

use std::marker::PhantomData;
use std::rc::Rc;

type Confined = PhantomData<Rc<()>>;

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
pub(crate) struct OwnerSerialState<C> {
    dormant: DormantContext<C>,
}

/// A live serial turn. `L` owns the received ticket until the turn ends.
#[must_use = "a serial turn must end or park"]
#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
pub(crate) struct SerialTurn<'turn, C, L> {
    state: &'turn mut OwnerSerialState<C>,
    local: L,
    confined: Confined,
}

/// A turn that gave its ticket back and waits on `W` (E5 §2). Resuming it
/// takes a newly received ticket.
#[must_use = "a parked turn must resume"]
#[allow(dead_code)] // Dormant E5.1; E5.2 parks and resumes.
pub(crate) struct ParkedTurn<'turn, C, W> {
    state: &'turn mut OwnerSerialState<C>,
    wait: W,
    confined: Confined,
}

#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
impl<C> OwnerSerialState<C> {
    /// Keep CONTEXT dormant for this owner's turns.
    pub(crate) fn new(context: Box<C>) -> Self {
        Self {
            dormant: DormantContext {
                context,
                confined: PhantomData,
            },
        }
    }

    /// Begin a turn for LOCAL, the local owner of the received ticket.
    pub(crate) fn begin<L>(&mut self, local: L) -> SerialTurn<'_, C, L> {
        SerialTurn {
            state: self,
            local,
            confined: PhantomData,
        }
    }
}

#[allow(dead_code)] // Dormant E5.1; E6-P.2 builds the owner's Context.
impl<'turn, C, L> SerialTurn<'turn, C, L> {
    /// Lend the owner's Context to BODY. No view outlives the call.
    pub(crate) fn with_world<R>(&mut self, body: impl for<'g> FnOnce(&'g mut C) -> R) -> R {
        body(&mut self.state.dormant.context)
    }

    /// End the turn. The local owner goes back to L6.4's finish and return.
    pub(crate) fn end(self) -> L {
        self.local
    }

    /// Park on WAIT. The local owner goes back with the ticket, and the
    /// parked turn keeps only the owner's state.
    pub(crate) fn park<W>(self, wait: W) -> (ParkedTurn<'turn, C, W>, L) {
        let parked = ParkedTurn {
            state: self.state,
            wait,
            confined: PhantomData,
        };
        (parked, self.local)
    }
}

#[allow(dead_code)] // Dormant E5.1; E5.2 parks and resumes.
impl<'turn, C, W> ParkedTurn<'turn, C, W> {
    /// What this turn waits on.
    pub(crate) fn wait(&self) -> &W {
        &self.wait
    }

    /// Resume with LOCAL, the local owner of a newly received ticket.
    pub(crate) fn resume<L>(self, local: L) -> SerialTurn<'turn, C, L> {
        SerialTurn {
            state: self.state,
            local,
            confined: PhantomData,
        }
    }
}

static_assertions::assert_not_impl_any!(DormantContext<()>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(OwnerSerialState<()>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(SerialTurn<'static, (), ()>: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(ParkedTurn<'static, (), ()>: Send, Sync, Clone);

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

    #[test]
    fn e51_two_turns_of_one_owner_reuse_the_dormant_context() {
        let before = BUILT.with(Cell::get);
        let mut state = OwnerSerialState::new(Box::new(Counted::new()));
        let mut first = state.begin("ticket-1");
        first.with_world(|context| context.writes.push("first"));
        assert_eq!(first.end(), "ticket-1");
        let mut second = state.begin("ticket-2");
        let seen = second.with_world(|context| {
            context.writes.push("second");
            context.writes.clone()
        });
        assert_eq!(second.end(), "ticket-2");
        assert_eq!(seen, ["first", "second"]);
        assert_eq!(BUILT.with(Cell::get) - before, 1, "one construction");
    }

    #[test]
    fn e51_a_parked_turn_returns_its_ticket_and_resumes_on_the_same_context() {
        let mut state = OwnerSerialState::new(Box::new(Counted::new()));
        let mut turn = state.begin("ticket-1");
        turn.with_world(|context| context.writes.push("before park"));
        let (parked, returned) = turn.park("mutex");
        assert_eq!(returned, "ticket-1", "park gives the ticket back");
        assert_eq!(*parked.wait(), "mutex");
        let mut resumed = parked.resume("ticket-2");
        let seen = resumed.with_world(|context| {
            context.writes.push("after resume");
            context.writes.clone()
        });
        assert_eq!(resumed.end(), "ticket-2");
        assert_eq!(seen, ["before park", "after resume"]);
    }
}
