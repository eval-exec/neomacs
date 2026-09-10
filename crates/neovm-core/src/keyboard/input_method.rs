//! Ownership of the outer key reader while Lisp runs an input method.

use super::{KBoard, KeyEchoState, ReadKeySequenceState};
use crate::emacs_core::{Context, error::EvalResult, value::Value};

/// Quail performs nested key reads. Their accumulator and command-key
/// publication belong to those reads, not to the suspended outer reader.
/// GNU keyboard.c saves this-command-keys and echo state around the call;
/// its in-progress key-sequence accumulator lives on the C stack.
#[must_use = "restore the suspended reader after the input method returns"]
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
            command_keys: std::mem::take(&mut keyboard.command_keys),
            raw_command_keys: std::mem::take(&mut keyboard.raw_command_keys),
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
    pub(super) fn apply_input_method_with_saved_reader(
        &mut self,
        function: Value,
        event: Value,
    ) -> EvalResult {
        let roots = self.save_vm_roots();
        let reader = SuspendedKeyReader::take(&mut self.command_loop.keyboard.kboard);
        reader.root_in(self);
        self.command_loop.keyboard.kboard.in_input_method_function = true;
        let result = self.apply(function, vec![event]);
        // Restore on signals and throws too; no early `?` may leave a nested
        // read's key sequence installed in its caller.
        self.command_loop.keyboard.kboard.in_input_method_function = false;
        reader.restore(&mut self.command_loop.keyboard.kboard);
        self.restore_vm_roots(roots);
        result
    }
}
