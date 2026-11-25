//! Moved from the display runtime (`thread_comm::frame_opacity::tests`,
//! MILESTONES M4): the runtime's frame-opacity state over a live evaluator,
//! through the runtime's public API only (DECISIONS D44).

use neomacs_display_runtime::thread_comm::FrameOpacityState;

#[test]
fn shared_child_reader_projects_redirects_into_applied_native_opacity() {
    use neovm_core::emacs_core::{GuiFrameHostRequest, Value};
    use neovm_core::window::{DisplayFrameManagerExt, FrameId};
    use std::sync::{Arc, Mutex};

    type AppliedOpacitySnapshot = (Option<f32>, Option<f32>);
    struct Host {
        state: Arc<Mutex<FrameOpacityState>>,
        snapshots: Arc<Mutex<Vec<AppliedOpacitySnapshot>>>,
        owner: Arc<Mutex<Option<(FrameId, FrameId)>>>,
    }
    impl neovm_core::emacs_core::DisplayConnection for Host {
        fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn set_gui_frame_focus_redirects(
            &mut self,
            redirects: Vec<(FrameId, Option<FrameId>)>,
        ) -> Result<(), String> {
            let mut state = self.state.lock().unwrap();
            state.set_redirects(
                redirects
                    .into_iter()
                    .map(|(id, target)| (id.0, target.map(|id| id.0)))
                    .collect(),
            );
            if let Some((owner, child)) = *self.owner.lock().unwrap() {
                self.snapshots
                    .lock()
                    .unwrap()
                    .push((state.applied(owner.0), state.applied(child.0)));
            }
            Ok(())
        }
    }

    impl neovm_core::emacs_core::DisplayServices for Host {}
    for abort in [false, true] {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        let buffer = eval.buffer_manager_mut().create_buffer("*scratch*");
        eval.buffer_manager_mut().set_current(buffer);
        let native = eval.frame_manager_mut().create_frame("A", 800, 600, buffer);
        eval.frame_manager_mut()
            .get_mut(native)
            .unwrap()
            .set_window_system(Some(Value::symbol("neo")));
        let state = Arc::new(Mutex::new(FrameOpacityState::default()));
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let owner_ids = Arc::new(Mutex::new(None));
        eval.set_display_host(Box::new(Host {
            state: state.clone(),
            snapshots: snapshots.clone(),
            owner: owner_ids.clone(),
        }));
        for result in eval.eval_str_each("(setq owner (x-create-frame nil)) (setq child (x-create-frame (list (cons 'parent-frame owner) (cons 'minibuffer (minibuffer-window owner))))) (select-frame child)") {
            assert!(result.is_ok(), "{result:?}");
        }
        let owner = FrameId(eval.eval_str("owner").unwrap().as_frame_id().unwrap());
        let child = FrameId(eval.eval_str("child").unwrap().as_frame_id().unwrap());
        {
            let mut state = state.lock().unwrap();
            state.accept(native.0, [0.7, 0.2], 0.0);
            state.accept(owner.0, [0.9, 0.3], 0.0);
            state.accept(child.0, [0.8, 0.4], 0.0);
            state.focus(native.0, true);
        }
        *owner_ids.lock().unwrap() = Some((owner, child));
        eval.set_variable("native", Value::make_frame(native.0));
        eval.eval_str("(redirect-frame-focus native child)")
            .unwrap();
        let tx = neovm_core::host_input::HostInputPublisher::install(&mut eval);
        tx.send(neovm_core::keyboard::InputEvent::key_press(
            neovm_core::keyboard::KeyEvent::char('\r'),
        ))
        .unwrap();

        let result = eval.eval_str(&format!(
            r#"(progn
            (fset 'command-execute (lambda (command &rest ignored) (call-interactively command)))
            (setq read-map (make-sparse-keymap))
            (define-key read-map "\r" (lambda () (interactive) (throw 'exit nil)))
            (setq minibuffer-setup-hook {})
            (read-from-minibuffer "Prompt: " nil read-map))"#,
            if abort {
                "(list (lambda () (error \"setup abort\")))"
            } else {
                "nil"
            }
        ));
        assert_eq!(result.is_err(), abort, "{result:?}");
        let snapshots = snapshots.lock().unwrap();
        assert!(
            snapshots.contains(&(Some(0.9), Some(0.4))),
            "entry: {snapshots:?}"
        );
        assert_eq!(snapshots.last(), Some(&(Some(0.3), Some(0.8))), "unwind");
    }
}
