use super::*;

#[test]
fn setter_under_temporary_limit_only_reapplies_its_target() {
    let mut state = FrameOpacityState::default();
    state.accept(1, [0.3; 2], 0.0);
    state.accept(2, [0.5; 2], 0.0);
    state.focus(1, true);
    state.set_lower_limit(0.8);
    state.accept(2, [0.5; 2], 0.8);
    state.set_lower_limit(0.0);
    assert_eq!(state.applied(1), Some(0.3));
    assert_eq!(state.applied(2), Some(0.8));
}

#[test]
fn unchanged_highlight_and_stale_focus_do_not_reapply_policy() {
    let mut state = FrameOpacityState::default();
    for frame in [1, 2, 3] {
        state.accept(frame, [0.7, 0.3], 0.0);
    }
    state.focus(1, true);
    state.set_redirects(vec![(1, Some(2))]);
    state.set_lower_limit(0.9);
    state.set_redirects(vec![(1, Some(2)), (3, Some(1))]);
    state.focus(1, true);
    state.focus(3, false);
    assert_eq!(state.applied(1), Some(0.3));
    assert_eq!(state.applied(2), Some(0.7));
    assert_eq!(state.applied(3), Some(0.3));
    state.set_redirects(vec![(1, Some(3))]);
    assert_eq!(state.applied(1), Some(0.3));
    assert_eq!(state.applied(2), Some(0.9));
    assert_eq!(state.applied(3), Some(0.9));
}

#[test]
fn shared_child_reader_projects_redirects_into_applied_native_opacity() {
    use neovm_core::emacs_core::{Context, DisplayHost, GuiFrameHostRequest, Value};
    use neovm_core::window::FrameId;
    use std::sync::{Arc, Mutex};

    type AppliedOpacitySnapshot = (Option<f32>, Option<f32>);
    struct Host {
        state: Arc<Mutex<FrameOpacityState>>,
        snapshots: Arc<Mutex<Vec<AppliedOpacitySnapshot>>>,
        owner: Arc<Mutex<Option<(FrameId, FrameId)>>>,
    }
    impl DisplayHost for Host {
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
    for abort in [false, true] {
        let mut eval = Context::new();
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
        let (tx, rx) = crossbeam_channel::unbounded();
        tx.send(neovm_core::keyboard::InputEvent::key_press(
            neovm_core::keyboard::KeyEvent::char('\r'),
        ))
        .unwrap();
        eval.input_rx = Some(rx);
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

#[test]
fn accepted_numeric_then_nil_survives_absent_and_replaced_scenes() {
    let mut state = FrameOpacityState::default();
    state.focus(1, true);
    for frame in [1, 2] {
        state.accept(frame, [0.5; 2], 0.2);
        state.accept(frame, [-1.0; 2], 0.2);
        assert_eq!(state.applied(frame), Some(0.5));
    }
    state.focus(1, false);
    state.focus(2, true);
    assert_eq!(state.applied(1), Some(0.5));
    assert_eq!(state.applied(2), Some(0.5));
}

#[test]
fn lower_limit_changes_apply_at_focus_without_replaying_nil() {
    let mut state = FrameOpacityState::default();
    state.accept(1, [0.8, 0.3], 0.2);
    state.focus(1, true);
    state.set_lower_limit(0.6);
    assert_eq!(state.applied(1), Some(0.8));
    state.focus(1, false);
    assert_eq!(state.applied(1), Some(0.6));
    state.accept(1, [-1.0; 2], 0.6);
    state.set_lower_limit(0.9);
    state.focus(1, true);
    assert_eq!(state.applied(1), Some(0.6));
}

#[test]
fn nil_component_retains_last_scalar_not_previous_policy_component() {
    let mut state = FrameOpacityState::default();
    state.focus(1, true);
    state.accept(1, [0.8, 0.4], 0.2);
    state.focus(1, false);
    state.accept(1, [-1.0, 0.6], 0.2);
    state.focus(1, true);
    assert_eq!(state.applied(1), Some(0.6));
    state.accept(1, [-1.0; 2], 0.2);
    state.focus(1, false);
    assert_eq!(state.applied(1), Some(0.6));
}

#[test]
fn redirect_clear_dead_target_and_late_leave_use_effective_highlight() {
    let mut state = FrameOpacityState::default();
    for frame in [1, 2, 3] {
        state.accept(frame, [0.8, 0.4], 0.2);
    }
    state.focus(1, true);
    assert_eq!((state.applied(1), state.applied(2)), (Some(0.8), Some(0.4)));
    state.set_redirects(vec![(1, Some(2))]);
    assert_eq!((state.applied(1), state.applied(2)), (Some(0.4), Some(0.8)));
    state.set_redirects(vec![(1, None)]);
    assert_eq!((state.applied(1), state.applied(2)), (Some(0.8), Some(0.4)));
    state.set_redirects(vec![(1, Some(2)), (2, Some(3))]);
    assert_eq!(state.applied(2), Some(0.8)); // one hop, not recursive
    state.retire(2);
    assert_eq!(state.applied(1), Some(0.8));
    state.focus(3, true);
    state.focus(1, false);
    assert_eq!(state.applied(3), Some(0.8));
}
