// Include the actual Control::finish method, including its consuming witness
// parameter. These native scheduler neighbours are compile-only and cannot
// supply a fabricated completion token or a successful runtime authority.
#![allow(dead_code)]

use crate::tagged::gc::HeapIdentity;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::{Condvar, MutexGuard};
use std::time::Instant;

pub(crate) mod emacs_core {
    pub(crate) use crate::actual_payload::emacs_core::value;
    pub(crate) mod threads {
        pub(crate) use crate::actual_completion::completion::RetiredCompletion;
    }
}

pub(crate) struct TurnRight {
    heap: HeapIdentity,
    _confined: PhantomData<Rc<()>>,
}

impl TurnRight {
    fn held_here(&self) -> bool {
        unreachable!("compile-only held-TurnRight authority check")
    }

    fn parallel_heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    fn is_era(&self) -> bool {
        unreachable!("compile-only era-mode TurnRight check")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ThreadKey {
    Main,
    Worker(NonZeroU64),
}

impl ThreadKey {
    fn from_id(id: u64) -> Self {
        match NonZeroU64::new(id) {
            Some(id) => Self::Worker(id),
            None => Self::Main,
        }
    }
}

enum Wait {
    Exited,
    Runnable,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct DeadlineKey {
    wait_end: Instant,
    key: ThreadKey,
}

struct WaitMap {
    by_key: BTreeMap<ThreadKey, Wait>,
    select_index: BTreeMap<DeadlineKey, ()>,
}

impl WaitMap {
    fn iter(&self) -> std::collections::btree_map::Iter<'_, ThreadKey, Wait> {
        self.by_key.iter()
    }

    fn select_len(&self) -> usize {
        self.select_index.len()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnerLocation {
    NativeReleased,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupTurn {
    Idle,
    Requested,
    Holding,
    Collecting,
}

#[derive(Clone)]
struct GroupRequestSender(PhantomData<Rc<()>>);

impl GroupRequestSender {
    fn request_tail(&self) -> Result<(), ()> {
        unreachable!("compile-only era tail request")
    }
}

struct GroupPort(PhantomData<Rc<()>>);

struct EraLink {
    requester: Option<GroupRequestSender>,
    port: Option<GroupPort>,
    turn: GroupTurn,
}

enum EraState {
    SoloActive,
    EraOpen(EraLink),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeWork {
    Ready,
    Idle,
}

struct NativeTurn(PhantomData<Rc<()>>);

enum NextOwner {
    Resume(ThreadKey),
    Native(NativeTurn),
}

enum PickNext {
    Owner(NextOwner),
    NoRunnable(Result<(), SchedulerIndexInconsistency>),
}

struct SchedulerIndexInconsistency(PhantomData<Rc<()>>);

struct Wake;

struct SchedulerState {
    closing: bool,
    waits: WaitMap,
    era: EraState,
    senders: BTreeMap<ThreadKey, ()>,
    physical: BTreeMap<ThreadKey, OwnerLocation>,
}

impl SchedulerState {
    fn baton_participants_empty(&self) -> bool {
        self.senders.is_empty()
    }

    fn set_era_turn(&mut self, turn: GroupTurn) {
        let EraState::EraOpen(era) = &mut self.era else {
            std::process::abort();
        };
        era.turn = turn;
    }
}

pub(crate) struct Control {
    metadata_changed: Condvar,
    _confined: PhantomData<Rc<()>>,
}

impl Control {
    fn remove_key(_: &mut SchedulerState, _: ThreadKey) {
        unreachable!("compile-only indexed scheduler retirement")
    }

    fn publish_owner_exit(&self, _: ThreadKey) {
        unreachable!("compile-only native exit publication")
    }

    fn pick_next(&self) -> PickNext {
        unreachable!("compile-only next owner selection")
    }

    fn state(&self) -> MutexGuard<'_, SchedulerState> {
        unreachable!("compile-only scheduler state authority")
    }

    fn wake_locked(_: &mut SchedulerState, _: ThreadKey) {
        unreachable!("compile-only wake under scheduler lock")
    }

    fn pick(_: &mut SchedulerState) -> Option<(ThreadKey, Option<Instant>)> {
        unreachable!("compile-only native ready queue selection")
    }

    fn pick_service(_: &mut SchedulerState) -> Option<NativeTurn> {
        unreachable!("compile-only native service turn selection")
    }

    fn wake_for(_: NextOwner, _: TurnRight) -> (ThreadKey, Wake) {
        unreachable!("compile-only TurnRight handoff")
    }

    fn send_wake(&self, _: ThreadKey, _: Wake) {
        unreachable!("compile-only native owner wake")
    }

    fn has_eligible_owner_now(&self, _: &mut SchedulerState) -> bool {
        unreachable!("compile-only eligible owner check")
    }

    fn era_ring_waiter(&self, _: &SchedulerState) -> bool {
        unreachable!("compile-only era ring waiter check")
    }

    fn give_back_era_turn(_: TurnRight) {
        unreachable!("compile-only era turn return")
    }

    fn await_era_world_with_port(&self, _: GroupPort) -> Result<TurnRight, ()> {
        unreachable!("compile-only era world wait")
    }

    fn take_era_port_if_requested(_: &mut SchedulerState) -> Option<GroupPort> {
        unreachable!("compile-only era port take")
    }

    fn coalesce_request_locked(_: &mut SchedulerState) -> bool {
        unreachable!("compile-only era request coalesce")
    }

    fn main_excluded(_: &SchedulerState) -> bool {
        unreachable!("compile-only main exclusion check")
    }

    fn query_era_idle_for_deadlock(&self) -> Option<(u64, usize)> {
        unreachable!("compile-only D8/QD2 deadlock check")
    }

    fn confirm_era_deadlock_after(_: &SchedulerState, _: u64, _: usize) -> bool {
        unreachable!("compile-only D8 deadlock after-check")
    }

    fn eligible_runnable_count(_: &SchedulerState) -> usize {
        unreachable!("compile-only runnable count")
    }

    fn queued_native_work(_: &SchedulerState) -> NativeWork {
        unreachable!("compile-only queued native work check")
    }
}

#[path = "../../../neovm-eval/src/emacs_core/runtime/eval/green/scheduler/completion_exit.rs"]
mod completion_exit;
