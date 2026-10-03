//! Bound timer-generated repeats at the evaluator read boundary, not the
//! render-to-input bridge. GNU's GTK repeat timer shares its command thread;
//! our native event loop must not accrue seconds of commands while Lisp runs.
use neomacs_display_protocol::input_progress::InputReceipt;
use std::collections::HashMap;
use winit::keyboard::PhysicalKey;
use winit::window::WindowId;

#[derive(Default)]
pub(super) struct KeyRepeats {
    pending: HashMap<(WindowId, PhysicalKey), InputReceipt>,
}

impl KeyRepeats {
    pub fn admit(&self, window: WindowId, key: PhysicalKey, repeat: bool) -> bool {
        // Never coalesce physical presses (including rapid release/repress).
        !repeat
            || self
                .pending
                .get(&(window, key))
                .is_some_and(InputReceipt::consumed_or_cancelled)
    }

    pub fn queued(&mut self, window: WindowId, key: PhysicalKey, receipt: Option<InputReceipt>) {
        self.release(window, key);
        if let Some(receipt) = receipt {
            self.pending.insert((window, key), receipt);
        }
    }

    pub fn release(&mut self, window: WindowId, key: PhysicalKey) {
        self.pending.remove(&(window, key));
    }

    pub fn retire_window(&mut self, window: WindowId) {
        self.pending.retain(|(owner, _), _| *owner != window);
    }
}

#[cfg(test)]
#[path = "key_repeat/tests.rs"]
mod tests;
