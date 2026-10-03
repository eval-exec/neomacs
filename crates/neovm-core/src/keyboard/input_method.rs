//! Ownership of the outer key reader while Lisp runs nested input.

use super::{KBoard, KeyEchoState, ReadKeySequenceState};
use crate::emacs_core::{Context, error::EvalResult, value::Value};

/// Keep the suspended accumulator separate from Lisp-visible key publication.
/// Observing callbacks (including read-key's ambiguity timer) must still see
/// the outer prefix until a nested read publishes its own keys. GNU's
/// in-progress key-sequence accumulator lives on each read's C stack.
#[must_use = "restore the suspended reader after the callback returns"]
struct SuspendedKeyReader {
    sequence: ReadKeySequenceState,
    command_keys: Vec<Value>,
    raw_command_keys: Vec<Value>,
    echo: KeyEchoState,
}

impl SuspendedKeyReader {
    fn take(keyboard: &mut KBoard) -> Self {
        Self {
            sequence: std::mem::take(&mut keyboard.current_key_sequence),
            command_keys: keyboard.command_keys.clone(),
            raw_command_keys: keyboard.raw_command_keys.clone(),
            echo: std::mem::take(&mut keyboard.key_echo_state),
        }
    }

    fn root_in(&self, eval: &mut Context) {
        for event in self
            .sequence
            .raw_events()
            .iter()
            .chain(self.sequence.translated_events())
            .chain(&self.command_keys)
            .chain(&self.raw_command_keys)
        {
            eval.push_vm_frame_root(*event);
        }
        if let KeyEchoState::Immediate {
            prompt: Some(prompt),
        } = &self.echo
        {
            prompt
                .intervals()
                .for_each_root(|value| eval.push_vm_frame_root(value));
        }
    }

    fn restore(self, keyboard: &mut KBoard) {
        keyboard.current_key_sequence = self.sequence;
        keyboard.command_keys = self.command_keys;
        keyboard.raw_command_keys = self.raw_command_keys;
        keyboard.key_echo_state = self.echo;
    }
}

impl Context {
    /// A callback can read input recursively while an outer key read is waiting.
    /// Isolate its accumulator and echo, but retain published keys for callbacks
    /// that only observe them. Root and restore all three on normal returns,
    /// signals and throws. Input queues and receipts
    /// remain owned by the reads which actually consume their events.
    pub(crate) fn with_saved_key_reader(
        &mut self,
        callback: impl FnOnce(&mut Context) -> EvalResult,
    ) -> EvalResult {
        let roots = self.save_vm_roots();
        let reader = SuspendedKeyReader::take(&mut self.command_loop.keyboard.kboard);
        reader.root_in(self);
        let result = callback(self);
        reader.restore(&mut self.command_loop.keyboard.kboard);
        self.restore_vm_roots(roots);
        result
    }

    pub(super) fn apply_input_method_with_saved_reader(
        &mut self,
        function: Value,
        event: Value,
    ) -> EvalResult {
        self.with_saved_key_reader(|eval| {
            // GNU clears publication specifically for input-method callbacks,
            // not for timers which may only inspect the pending outer prefix.
            eval.command_loop.keyboard.kboard.command_keys.clear();
            eval.command_loop.keyboard.kboard.raw_command_keys.clear();
            eval.command_loop.keyboard.kboard.in_input_method_function = true;
            let result = eval.apply(function, vec![event]);
            eval.command_loop.keyboard.kboard.in_input_method_function = false;
            result
        })
    }
}
