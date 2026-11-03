// Compile-only neighbours for the INCLUDED production condition_wait.rs.
// No wait-state enum, validating constructor, recursion rule or transition is
// defined here. Native calls cannot return a fabricated success or authority.
#![allow(dead_code)]
use std::sync::Arc;

pub(crate) mod heap_types {
    pub(crate) use crate::threading;
}

pub(crate) mod emacs_core {
    pub(crate) mod error {
        pub use neovm_core::emacs_core::error::Flow;
        pub(crate) type EvalResult = Result<super::value::Value, Flow>;
        pub(crate) fn signal(_: &str, _: Vec<super::value::Value>) -> Flow {
            unreachable!("real Lisp error construction")
        }
    }
    pub(crate) mod value {
        pub use neovm_core::emacs_core::value::Value;
        pub(crate) use crate::actual_payload::emacs_core::value::buffer_vocab;
    }
    pub(crate) mod threads {
        use super::error::Flow;
        use super::value::Value;
        use crate::threading::RecursionDepth;

        // Opaque neighbours retain the real heap word's auto traits. No shim
        // constructor or association query can pretend to validate handles.
        #[derive(Clone, Copy)]
        pub(crate) struct CondVarHandle(Value);
        #[derive(Clone, Copy)]
        pub(crate) struct MutexHandle(Value);
        impl CondVarHandle {
            pub(crate) fn value(self) -> Value {
                self.0
            }
            pub(crate) fn native_key(self) -> u64 {
                unreachable!("native identity projection")
            }
        }
        impl MutexHandle {
            pub(crate) fn native_key(self) -> u64 {
                unreachable!("native identity projection")
            }
        }
        pub(crate) enum MutexAcquisition {
            Acquired,
            Blocked,
            DepthExhausted,
        }
        pub(crate) struct ThreadManager(Value);
        impl ThreadManager {
            pub(crate) fn condition_variable_mutex(&self, _: CondVarHandle) -> MutexHandle {
                unreachable!("real World validates the association")
            }
            pub(crate) fn mutex_unlock_for_wait(
                &mut self,
                _: MutexHandle,
            ) -> Option<RecursionDepth> {
                unreachable!("real World saves held recursive depth")
            }
            pub(crate) fn is_condition_variable(&self, _: CondVarHandle) -> bool {
                unreachable!("real World validates the condition")
            }
            pub(crate) fn set_thread_blocker(&mut self, _: u64, _: Value) -> bool {
                unreachable!("real World publishes the blocker")
            }
            pub(crate) fn clear_thread_blocker(&mut self, _: u64) -> bool {
                unreachable!("real World clears the blocker")
            }
            pub(crate) fn pending_thread_signal(&self, _: u64) -> Option<Flow> {
                unreachable!("real World owns pending errors")
            }
            pub(crate) fn mutex_relock_after_wait(
                &mut self,
                _: MutexHandle,
                _: RecursionDepth,
            ) -> MutexAcquisition {
                unreachable!("real World reacquires held recursive depth")
            }
        }
    }
    pub(crate) mod eval {
        use super::error::Flow;
        use super::threads::ThreadManager;
        use crate::actual_condition_neighbors::{GreenSeat, ThreadKey, Wait};
        pub(crate) struct Context {
            pub(crate) threads: ThreadManager,
        }
        impl Context {
            pub(crate) fn green_key(&mut self) -> Option<ThreadKey> {
                unreachable!("real owner identity")
            }
            pub(crate) fn green_seat(&mut self) -> Option<&mut GreenSeat> {
                unreachable!("real owner's stowed seat")
            }
            pub(crate) fn green_park(&mut self, _: Wait) -> Result<(), Flow> {
                unreachable!("real Baton handoff")
            }
            pub(crate) fn green_raise_pending_signal(&mut self, _: ThreadKey) -> Result<(), Flow> {
                unreachable!("real deferred error delivery")
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ThreadKey(u64);
impl ThreadKey {
    pub(crate) fn id(self) -> u64 {
        self.0
    }
}
pub(crate) struct ConditionId(u64);
impl ConditionId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }
}
pub(crate) struct MutexId(u64);
impl MutexId {
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }
}
pub(crate) enum Wait {
    Mutex(MutexId),
    Condition(ConditionId),
}
pub(crate) struct Control;
impl Control {
    pub(crate) fn wake_waiting_for(&self, _: Wait) -> usize {
        unreachable!("real scheduler owns indexed waiters")
    }
}
pub(crate) struct GreenSeat {
    pub(crate) condition_wait: crate::condition_wait::ConditionWait,
    pub(crate) control: Arc<Control>,
}
pub(crate) mod api {
    pub(crate) fn mutex_depth_exhausted() -> super::emacs_core::error::Flow {
        unreachable!("real overflow error construction")
    }
}
