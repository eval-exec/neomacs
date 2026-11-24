#![allow(dead_code)]
#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/parallel_entry/keeper_custody.rs"]
mod keeper_custody;

use keeper_custody::{KeeperReadyRequest, StoppedKeeper};

fn lend<'inner, 'outer: 'inner, 'era>(
    keeper: StoppedKeeper<'inner, 'era>,
    request: KeeperReadyRequest<'outer>,
) {
    let _ = keeper.lend_serial(request);
}

fn main() {}

// Compile-only neighbour for the included keeper_custody.rs: the return
// envelope names the serial ticket identity and the ended owner. Kept below
// main so the golden's line numbers stay put.
mod serial_ticket {
    #[derive(Clone, Copy)]
    pub(crate) struct TicketIdentity;
    pub(crate) struct ReturnableSerialOwner;
    impl ReturnableSerialOwner {
        pub(crate) fn identity(&self) -> TicketIdentity {
            TicketIdentity
        }
    }
}

// Compile-only neighbour: the World turn's identity, which the custody cuts
// for the Baton group (L6.7b b3).
mod world_turn {
    pub(crate) struct WorldTurnIdentity {
        pub heap: neovm_value::tagged::gc::HeapIdentity,
        pub epoch: std::num::NonZeroU64,
    }
}
