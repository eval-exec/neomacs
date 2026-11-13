//! #395: framed input must select its editing target before the key executes.

use crate::emacs_core::Context;
use crate::emacs_core::eval::*;
use crate::host_input::HostInputPublisher;
use crate::keyboard::{FrontendKey, InputEvent, render_key_transport_to_input_event};
use crate::window::DisplayFrameManagerExt;
use crate::window::FrameId;
use neovm_edit::buffer::BufferId;
use neovm_value::value::Value;

struct FocusFixture {
    eval: Context,
    first: FrameId,
    second: FrameId,
    primary: BufferId,
    secondary: BufferId,
}

impl FocusFixture {
    #[doc(hidden)]
    pub fn new() -> Self {
        let mut eval = crate::test_utils::runtime_startup_context();
        eval.eval_str(
            "(progn
               (switch-to-buffer (get-buffer-create \"*issue395-primary*\"))
               (erase-buffer)
               (defvar issue395-input-sequence nil))",
        )
        .unwrap();
        let first = eval.frame_manager().selected_frame().unwrap().id;
        let primary = eval.buffer_manager().current_buffer_id().unwrap();
        let secondary = eval
            .buffer_manager_mut()
            .create_buffer("*issue395-secondary*");
        let second = eval
            .frame_manager_mut()
            .create_frame("issue395-second", 800, 600, secondary);
        for id in [first, second] {
            eval.frame_manager_mut()
                .get_mut(id)
                .unwrap()
                .set_window_system(Some(Value::symbol("neomacs")));
        }
        assert_eq!(eval.frame_manager().selected_frame().unwrap().id, first);
        Self {
            eval,
            first,
            second,
            primary,
            secondary,
        }
    }

    #[doc(hidden)]
    pub fn keyboard_input(&self, frame: FrameId, character: char) -> InputEvent {
        render_key_transport_to_input_event(FrontendKey::Character(character), 0, true, frame.0)
            .unwrap()
    }

    #[doc(hidden)]
    pub fn deliver(&mut self, events: Vec<InputEvent>) {
        let sender = HostInputPublisher::install(&mut self.eval);
        for event in events {
            sender.send(event).unwrap();
        }
        // The finite stream makes an accidentally discarded key reach EOF
        // rather than leave the regression waiting for another input forever.
        drop(sender);
    }

    #[doc(hidden)]
    pub fn execute_next_frame_key(&mut self, target: FrameId, buffer: BufferId, character: char) {
        let sequence = self
            .eval
            .eval_str(
                "(setq issue395-input-sequence
                       (read-key-sequence-vector nil nil nil t))",
            )
            .unwrap();
        {
            let events = sequence.as_vector_data().expect("symbolic input sequence");
            assert_eq!(events.len(), 1, "frame switch must precede its key");
            let event = events[0];
            assert!(event.is_cons(), "expected a switch-frame event");
            assert_eq!(event.cons_car(), Value::symbol("switch-frame"));
            assert_eq!(event.cons_cdr().cons_car(), Value::make_frame(target.0));
        }
        self.eval
            .eval_str("(execute-kbd-macro issue395-input-sequence)")
            .unwrap();

        // Fail on the historical no-op handler before attempting the key:
        // native focus alone must not count as logical editing selection.
        assert_eq!(
            self.eval.frame_manager().selected_frame().unwrap().id,
            target
        );
        assert_eq!(self.eval.buffer_manager().current_buffer_id(), Some(buffer));
        let selected = self.eval.eval_str("(selected-window)").unwrap();
        assert_eq!(
            selected.as_window_id(),
            Some(
                self.eval
                    .frame_manager()
                    .get(target)
                    .unwrap()
                    .selected_window
                    .0
            )
        );

        let key = self
            .eval
            .eval_str("(setq issue395-input-sequence (read-key-sequence-vector nil))")
            .unwrap();
        assert_eq!(
            key.as_vector_data().unwrap().as_slice(),
            &[Value::fixnum(character as i64)],
            "the original key must be delivered exactly once after selection"
        );
        self.eval
            .eval_str("(execute-kbd-macro issue395-input-sequence)")
            .unwrap();
        assert_eq!(
            self.eval.frame_manager().selected_frame().unwrap().id,
            target
        );
        assert_eq!(self.eval.buffer_manager().current_buffer_id(), Some(buffer));
    }

    #[doc(hidden)]
    pub fn verify_both_destinations(&mut self) {
        self.execute_next_frame_key(self.second, self.secondary, 'n');
        self.execute_next_frame_key(self.first, self.primary, 'p');
        let text = self
            .eval
            .eval_str(
                "(list (with-current-buffer \"*issue395-primary*\" (buffer-string))
                       (with-current-buffer \"*issue395-secondary*\" (buffer-string)))",
            )
            .unwrap();
        assert_eq!(crate::emacs_core::print_value(&text), "(\"p\" \"n\")");
    }
}

#[test]
fn native_focus_input_selects_the_frame_before_its_editing_command() {
    let mut fixture = FocusFixture::new();
    fixture.deliver(vec![
        InputEvent::Focus {
            focused: true,
            emacs_frame_id: fixture.second.0,
        },
        // A late focus-out for the old frame must not undo the new focus.
        InputEvent::Focus {
            focused: false,
            emacs_frame_id: fixture.first.0,
        },
        fixture.keyboard_input(fixture.second, 'n'),
        InputEvent::Focus {
            focused: true,
            emacs_frame_id: fixture.first.0,
        },
        InputEvent::Focus {
            focused: false,
            emacs_frame_id: fixture.second.0,
        },
        fixture.keyboard_input(fixture.first, 'p'),
    ]);
    fixture.verify_both_destinations();
}

#[test]
fn frame_tagged_keys_select_their_source_without_a_focus_notification() {
    let mut fixture = FocusFixture::new();
    fixture.deliver(vec![
        fixture.keyboard_input(fixture.second, 'n'),
        fixture.keyboard_input(fixture.first, 'p'),
    ]);
    fixture.verify_both_destinations();
}
