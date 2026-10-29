//! #395: framed input must select its editing target before the key executes.

use super::*;
use crate::buffer::BufferId;
use crate::emacs_core::Context;
use crate::window::FrameId;

struct FocusFixture {
    eval: Context,
    first: FrameId,
    second: FrameId,
    primary: BufferId,
    secondary: BufferId,
}

impl FocusFixture {
    fn new() -> Self {
        let mut eval = crate::test_utils::runtime_startup_context();
        eval.eval_str(
            "(progn
               (switch-to-buffer (get-buffer-create \"*issue395-primary*\"))
               (erase-buffer)
               (defvar issue395-input-sequence nil))",
        )
        .unwrap();
        let first = eval.frames.selected_frame().unwrap().id;
        let primary = eval.buffers.current_buffer_id().unwrap();
        let secondary = eval.buffers.create_buffer("*issue395-secondary*");
        let second = eval
            .frames
            .create_frame("issue395-second", 800, 600, secondary);
        for id in [first, second] {
            eval.frames
                .get_mut(id)
                .unwrap()
                .set_window_system(Some(Value::symbol("neomacs")));
        }
        assert_eq!(eval.frames.selected_frame().unwrap().id, first);
        Self {
            eval,
            first,
            second,
            primary,
            secondary,
        }
    }

    fn keyboard_input(&self, frame: FrameId, character: char) -> InputEvent {
        render_key_transport_to_input_event(FrontendKey::Character(character), 0, true, frame.0)
            .unwrap()
    }

    fn deliver(&mut self, events: Vec<InputEvent>) {
        let sender = crate::host_input::HostInputPublisher::install(&mut self.eval);
        for event in events {
            sender.send(event).unwrap();
        }
        // The finite stream makes an accidentally discarded key reach EOF
        // rather than leave the regression waiting for another input forever.
        drop(sender);
    }

    fn execute_next_frame_key(&mut self, target: FrameId, buffer: BufferId, character: char) {
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
        assert_eq!(self.eval.frames.selected_frame().unwrap().id, target);
        assert_eq!(self.eval.buffers.current_buffer_id(), Some(buffer));
        let selected = self.eval.eval_str("(selected-window)").unwrap();
        assert_eq!(
            selected.as_window_id(),
            Some(self.eval.frames.get(target).unwrap().selected_window.0)
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
        assert_eq!(self.eval.frames.selected_frame().unwrap().id, target);
        assert_eq!(self.eval.buffers.current_buffer_id(), Some(buffer));
    }

    fn verify_both_destinations(&mut self) {
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
