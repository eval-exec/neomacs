#![forbid(unsafe_code)]

//! L6.7a K9: the keeper driver's protocol core under Loom.
//!
//! build.rs copies the product `parallel_entry/driver_core.rs` and
//! `rendezvous/control.rs` read-only. The core runs unchanged over the model
//! era in `model.rs`, which drives the product ring under a Loom Mutex,
//! with `sync.rs` supplying the core's channels, backoff and the era's
//! one-waiter `Signal`. Each positive row has a one-edge negative built from
//! a mutated copy, with the exact assertion it trips as the witness.

/// The product key's shape, as `pentry-l6-loom` supplies it to control.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MutatorKey {
    pub heap: u64,
    pub world: u64,
    pub owner: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendezvousError {
    Poisoned,
    Identity,
    Loan,
    Publication,
    Generation,
    Busy,
    Readers,
    Capacity,
    Root,
}

mod sync;

/// The product core and ring.
#[allow(dead_code)]
mod real {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core.rs"));
    }
    include!("model.rs");
}

/// N-L1: the ring commits at the head.
#[allow(dead_code, unused)]
mod lifo {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control_lifo.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core.rs"));
    }
    include!("model.rs");
}

/// N-L13: the collecting group is re-queued at the tail.
#[allow(dead_code, unused)]
mod requeue_tail {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control_requeue_tail.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core.rs"));
    }
    include!("model.rs");
}

/// N-L8: the poison fan-out tells nobody.
#[allow(dead_code, unused)]
mod silent_poison {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core_silent_poison.rs"));
    }
    include!("model.rs");
}

/// N-K8: a pending stop poisons, as K8 r1 had it.
#[allow(dead_code, unused, unreachable_code)]
mod pending_poisons {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core_pending_poisons.rs"));
    }
    include!("model.rs");
}

/// N-L14: an owner's request during the World leaves the waiter word down.
#[allow(dead_code, unused)]
mod silent_waiter {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core_silent_waiter.rs"));
    }
    include!("model.rs");
}

/// N-L15: a stop runs while the HEAD is held for a collecting owner.
#[allow(dead_code, unused)]
mod unheld_head {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core_unheld_head.rs"));
    }
    include!("model.rs");
}

/// N-L7: the grant goes to another enrolled owner.
#[allow(dead_code, unused)]
mod misrouted_grant {
    use crate::sync;
    mod control {
        include!(concat!(env!("OUT_DIR"), "/control.rs"));
    }
    mod driver_core {
        include!(concat!(env!("OUT_DIR"), "/driver_core_misrouted_grant.rs"));
    }
    include!("model.rs");
}

/// L1 + L3/L4/L5 + L7.
#[test]
fn two_owners_one_turn_each_in_commit_order() {
    real::two_owners_one_turn_each_in_commit_order();
}

#[test]
#[should_panic(expected = "a grant woke an owner it was not cut for")]
fn negative_a_misrouted_grant_wakes_the_wrong_owner() {
    misrouted_grant::two_owners_one_turn_each_in_commit_order();
}

#[test]
#[should_panic(expected = "two serial turns at once")]
fn negative_an_early_give_back_overlaps_two_turns() {
    real::an_early_give_back_overlaps_two_turns();
}

#[test]
#[should_panic(expected = "grants follow commit order")]
fn negative_a_lifo_ring_breaks_commit_order() {
    lifo::two_owners_one_turn_each_in_commit_order();
}

/// L8.
#[test]
fn an_abandoned_lend_poisons_and_wakes_the_rest() {
    real::an_abandoned_lend_poisons_and_wakes_the_rest();
}

#[test]
#[should_panic(expected = "the queued owner reads Poisoned")]
fn negative_a_silent_poison_leaves_the_queue_uninformed() {
    silent_poison::an_abandoned_lend_poisons_and_wakes_the_rest();
}

/// L9.
#[test]
fn a_dead_keeper_disconnects_every_owner() {
    real::a_dead_keeper_disconnects_every_owner(false);
}

#[test]
#[should_panic(expected = "the owner learns the keeper is gone")]
fn negative_a_leaked_grant_sender_leaves_the_owner_waiting() {
    real::a_dead_keeper_disconnects_every_owner(true);
}

/// K8 r2.
#[test]
fn a_slow_owner_delays_the_turn_and_poisons_nothing() {
    real::a_slow_owner_delays_the_turn_and_poisons_nothing();
}

#[test]
#[should_panic(expected = "a slow owner poisons nothing")]
fn negative_a_pending_stop_that_poisons_is_caught() {
    pending_poisons::a_slow_owner_delays_the_turn_and_poisons_nothing();
}

/// L10 (L6.7b b3).
#[test]
fn a_group_and_an_owner_alternate_in_commit_order() {
    real::a_group_and_an_owner_alternate_in_commit_order();
}

#[test]
#[should_panic(expected = "grants follow commit order")]
fn negative_a_lifo_ring_breaks_commit_order_across_kinds() {
    lifo::a_group_and_an_owner_alternate_in_commit_order();
}

#[test]
#[should_panic(expected = "two serial turns at once")]
fn negative_an_early_world_return_overlaps_the_owner() {
    real::an_early_world_return_overlaps_the_owner();
}

/// L11.
#[test]
fn an_abandoned_world_turn_poisons_the_owners() {
    real::an_abandoned_world_turn_poisons_the_owners();
}

#[test]
#[should_panic(expected = "the queued owner reads Poisoned")]
fn negative_a_silent_poison_after_a_group_abandon() {
    silent_poison::an_abandoned_world_turn_poisons_the_owners();
}

/// L12.
#[test]
fn an_abandoned_lend_poisons_the_queued_group() {
    real::an_abandoned_lend_poisons_the_queued_group();
}

#[test]
#[should_panic(expected = "the queued group reads Poisoned")]
fn negative_a_silent_poison_leaves_the_group_uninformed() {
    silent_poison::an_abandoned_lend_poisons_the_queued_group();
}

/// L13 (L6.7b b5).
#[test]
fn a_collecting_group_resumes_before_any_other_turn() {
    real::a_collecting_group_resumes_before_any_other_turn();
}

#[test]
#[should_panic(expected = "the collecting group resumes next")]
fn negative_a_tail_requeue_lets_the_owner_in_first() {
    requeue_tail::a_collecting_group_resumes_before_any_other_turn();
}

/// L14 (L6.7b b4.1).
#[test]
fn an_owner_request_during_the_world_raises_the_waiter_word() {
    real::an_owner_request_during_the_world_raises_the_waiter_word();
}

#[test]
#[should_panic(expected = "the waiter word was never raised")]
fn negative_a_silent_waiter_word_keeps_the_owner_waiting() {
    silent_waiter::an_owner_request_during_the_world_raises_the_waiter_word();
}

/// L15 (L6.7c (c)).
#[test]
fn a_collecting_owner_resumes_before_any_other_turn() {
    real::a_collecting_owner_resumes_before_any_other_turn();
}

#[test]
#[should_panic(expected = "the collecting owner resumes next")]
fn negative_an_unheld_head_lets_the_queued_owner_in_first() {
    unheld_head::a_collecting_owner_resumes_before_any_other_turn();
}

#[test]
#[should_panic(expected = "the collecting owner resumes next")]
fn negative_a_tail_requeue_lets_the_queued_owner_in_first() {
    requeue_tail::a_collecting_owner_resumes_before_any_other_turn();
}

/// L16 (L6.7c (c)).
#[test]
fn a_collecting_owner_that_exits_gives_up_the_head() {
    real::a_collecting_owner_that_exits_gives_up_the_head();
}
