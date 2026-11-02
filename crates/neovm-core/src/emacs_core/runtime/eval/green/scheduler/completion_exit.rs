//! Exit publication after the native completion registration was retired.

use super::{Baton, Control, NextOwner, PickNext, ThreadKey, Wait};

impl Control {
    /// ME ended: hand the Baton to the next thread, wake its joiners, and
    /// return without waiting. With no thread able to run, the runtime is
    /// closing: every waiting thread is woken (it signals) and gets the
    /// Baton in turn.
    pub(crate) fn finish(
        &self,
        completion: crate::emacs_core::threads::RetiredCompletion,
        baton: Baton,
    ) {
        let thread = completion.into_thread_id();
        if !baton.held_here() || thread.heap_identity() != baton.parallel_heap_identity() {
            std::process::abort();
        }
        let me = ThreadKey::from_id(thread.native_key());
        self.publish_owner_exit(me);
        // A finished thread must stop charging the scheduler (P11 soak
        // finding #2): the joiners and the native main already observed the
        // exit publication above, and the record goes before the Baton can
        // move, so the next owner never selects or scans a dead entry.
        {
            let mut state = self.state();
            Self::remove_key(&mut state, me);
        }
        let next = match self.pick_next() {
            PickNext::Owner(next) => Some(next),
            PickNext::NoRunnable(_) => {
                let mut state = self.state();
                state.closing = true;
                let blocked: Vec<ThreadKey> = state
                    .waits
                    .iter()
                    .filter(|(_, wait)| !matches!(wait, Wait::Exited))
                    .map(|(key, _)| *key)
                    .collect();
                for key in blocked {
                    Self::wake_locked(&mut state, key);
                }
                Self::pick(&mut state).map(|(next, _)| NextOwner::Resume(next))
            }
        };
        match next {
            Some(next) => {
                let (target, wake) = Self::wake_for(next, baton);
                self.send_wake(target, wake);
            }
            // Nothing else lives: the Baton and facade epoch end together.
            None => drop(baton),
        }
    }
}
