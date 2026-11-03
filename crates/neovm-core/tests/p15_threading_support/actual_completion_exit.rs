// Include the actual Control::finish method, including its consuming witness
// parameter. These native scheduler neighbours are compile-only and cannot
// supply a fabricated completion token or a successful runtime authority.
#![allow(dead_code)]

use crate::tagged::gc::HeapIdentity;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::MutexGuard;

pub(crate) mod emacs_core {
    pub(crate) use crate::actual_payload::emacs_core::value;
    pub(crate) mod threads {
        pub(crate) use crate::actual_completion::completion::RetiredCompletion;
    }
}

pub(crate) struct Baton {
    heap: HeapIdentity,
    _confined: PhantomData<Rc<()>>,
}

impl Baton {
    fn held_here(&self) -> bool {
        unreachable!("compile-only held-Baton authority check")
    }

    fn parallel_heap_identity(&self) -> HeapIdentity {
        self.heap
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

enum NextOwner {
    Resume(ThreadKey),
}

enum PickNext {
    Owner(NextOwner),
    NoRunnable(Result<(), SchedulerIndexInconsistency>),
}

struct SchedulerIndexInconsistency(PhantomData<Rc<()>>);

struct Wake;

struct SchedulerState {
    closing: bool,
    waits: BTreeMap<ThreadKey, Wait>,
}

pub(crate) struct Control(PhantomData<Rc<()>>);

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

    fn pick(_: &mut SchedulerState) -> Option<(ThreadKey, ())> {
        unreachable!("compile-only native ready queue selection")
    }

    fn wake_for(_: NextOwner, _: Baton) -> (ThreadKey, Wake) {
        unreachable!("compile-only Baton handoff")
    }

    fn send_wake(&self, _: ThreadKey, _: Wake) {
        unreachable!("compile-only native owner wake")
    }
}

#[path = "../../src/emacs_core/runtime/eval/green/scheduler/completion_exit.rs"]
mod completion_exit;
