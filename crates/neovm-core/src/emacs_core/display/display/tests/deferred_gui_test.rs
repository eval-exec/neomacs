use super::*;
use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct DeferredHost {
    terminal: u64,
    identity: GraphicalDisplayIdentity,
    fail_frame: bool,
    fail_completion: bool,
    native_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
}

impl DisplayHost for DeferredHost {
    fn gui_terminal(&self) -> Option<(u64, GraphicalDisplayIdentity)> {
        Some((self.terminal, self.identity.clone()))
    }
    fn gui_frame_metrics(&self) -> Option<(f32, f32, f32, f64)> {
        Some((8.0, 16.0, 14.0, 1.0))
    }
    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        // Native command admission can precede a later realization failure.
        self.native_frames.borrow_mut().push(request.frame_id);
        if self.fail_frame {
            Err("native frame refused".into())
        } else {
            Ok(())
        }
    }
    fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
    fn destroy_gui_frame(&mut self, frame: crate::window::FrameId) -> Result<(), String> {
        self.native_frames.borrow_mut().retain(|id| *id != frame);
        Ok(())
    }
    fn poll_gui_frame_ready(&mut self, _: crate::window::FrameId) -> Option<Result<(), String>> {
        Some(if self.fail_completion {
            Err("surface creation failed".into())
        } else {
            Ok(())
        })
    }
}

fn deferred_host(fail_frame: bool) -> DeferredHost {
    let identity =
        GraphicalDisplayIdentity::named(GraphicalBackend::Wayland, "wayland-owned").unwrap();
    let terminal = crate::emacs_core::terminal::pure::register_graphical_terminal(identity.clone());
    DeferredHost {
        terminal,
        identity,
        fail_frame,
        fail_completion: false,
        native_frames: Rc::new(RefCell::new(Vec::new())),
    }
}

#[test]
fn deferred_gui_failed_open_preserves_context_and_can_retry() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    eval.eval_str("(setq preserved-state (list 7 11))").unwrap();
    let requests = Rc::new(Cell::new(0));
    let calls = requests.clone();
    eval.set_gui_display_initializer(Box::new(move |eval, display| {
        assert_eq!(display, Some("wayland-owned"));
        let n = calls.get();
        calls.set(n + 1);
        if n == 0 {
            return Err(crate::emacs_core::error::EvalError::signal(
                intern("error"),
                vec![Value::string("no display yet")],
                None,
            ));
        }
        if eval.display_host.is_none() {
            eval.set_display_host(Box::new(deferred_host(false)));
        }
        Ok(())
    }));
    assert!(
        eval.eval_str("(x-open-connection \"wayland-owned\")")
            .is_err()
    );
    assert!(
        eval.eval_str("(equal preserved-state '(7 11))")
            .unwrap()
            .is_truthy()
    );
    eval.eval_str("(x-open-connection \"wayland-owned\")")
        .unwrap();
    assert_eq!(requests.get(), 2);
    assert!(
        eval.eval_str("(equal preserved-state '(7 11))")
            .unwrap()
            .is_truthy()
    );
    assert!(eval.shutdown_request().is_none());
}

#[test]
fn deferred_gui_open_is_not_frame_creation_and_validates_designators() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    eval.eval_str("(selected-frame)").unwrap();
    let before = eval.frames.frame_list();
    let requests = Rc::new(Cell::new(0));
    let calls = requests.clone();
    eval.set_gui_display_initializer(Box::new(move |eval, _| {
        calls.set(calls.get() + 1);
        eval.set_display_host(Box::new(deferred_host(false)));
        Ok(())
    }));
    assert!(eval.eval_str("(x-open-connection 42)").is_err());
    assert_eq!(requests.get(), 0);
    eval.eval_str("(x-open-connection \"wayland-owned\")")
        .unwrap();
    assert_eq!(eval.frames.frame_list(), before);
    assert!(
        eval.eval_str("(equal (x-display-list) '(\"wayland-owned\"))")
            .unwrap()
            .is_truthy()
    );
}

#[test]
fn deferred_gui_delete_recreate_retains_separate_initial_terminal() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let initial = eval.eval_str("(selected-frame)").unwrap();
    let initial_terminal = builtin_frame_terminal(&mut eval, vec![initial]).unwrap();
    let host = deferred_host(false);
    let gui_terminal = host.terminal;
    eval.set_display_host(Box::new(host));
    let frame = eval
        .eval_str("(x-create-frame '((width . 40) (height . 20)))")
        .unwrap();
    let fid = crate::window::FrameId(frame.as_frame_id().unwrap());
    assert_eq!(eval.frames.get(fid).unwrap().terminal_id, gui_terminal);
    assert_eq!(eval.frames.get(fid).unwrap().char_width, 8.0);
    assert!(eval.frames.get(fid).unwrap().width >= 320);
    assert_ne!(
        builtin_frame_terminal(&mut eval, vec![frame]).unwrap(),
        initial_terminal
    );
    eval.set_variable("owned-frame", frame);
    eval.eval_str("(setq owned-terminal (frame-terminal owned-frame))")
        .unwrap();
    eval.eval_str("(delete-frame owned-frame t)").unwrap();
    eval.set_variable("initial-frame", initial);
    eval.eval_str("(select-frame initial-frame)").unwrap();
    assert_eq!(
        eval.eval_str("(terminal-live-p owned-terminal)").unwrap(),
        Value::symbol("neo"),
        "the native connection must retain its type after its last frame closes"
    );
    assert!(eval.shutdown_request().is_none());
    assert!(
        eval.frames
            .get(crate::window::FrameId(initial.as_frame_id().unwrap()))
            .is_some()
    );
    assert_eq!(
        builtin_terminal_name(&mut eval, vec![initial_terminal])
            .unwrap()
            .as_lisp_string()
            .unwrap()
            .as_utf8_str(),
        Some("initial_terminal")
    );
    let next = eval.eval_str("(x-create-frame nil)").unwrap();
    assert_ne!(next, frame);
    eval.set_variable("recreated-frame", next);
    assert!(
        eval.eval_str("(and (eq (frame-terminal recreated-frame) owned-terminal) (terminal-live-p (frame-terminal recreated-frame)))")
            .unwrap()
            .is_truthy()
    );
    assert_eq!(
        eval.frames
            .get(crate::window::FrameId(next.as_frame_id().unwrap()))
            .unwrap()
            .terminal_id,
        gui_terminal
    );
    assert!(
        eval.eval_str("(equal (x-display-list) '(\"wayland-owned\"))")
            .unwrap()
            .is_truthy()
    );
}

#[test]
fn deferred_gui_explicit_terminal_deletion_is_rejected_before_hooks() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    eval.eval_str("(selected-frame)").unwrap();
    let host = deferred_host(false);
    let native_frames = host.native_frames.clone();
    eval.set_display_host(Box::new(host));
    let frame = eval.eval_str("(x-create-frame nil)").unwrap();
    eval.set_variable("owned-frame", frame);
    eval.eval_str(
        "(setq owned-terminal (frame-terminal owned-frame) deletion-hooks nil)
         (select-frame owned-frame)
         (setq delete-terminal-functions
               (list (lambda (term) (setq deletion-hooks t) (delete-terminal term t)))
               delete-frame-functions
               (list (lambda (_frame) (setq deletion-hooks t))))",
    )
    .unwrap();
    // Another active terminal allows the unforced deletion past its usual
    // sole-terminal check; the retained graphical owner must still reject it.
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        99,
        "other-active-tty",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(
            None,
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(8),
        ),
    );
    let before = eval.frames.frame_list();
    let selected = eval.frames.selected_frame().unwrap().id;
    let native_before = native_frames.borrow().clone();
    for expression in [
        "(delete-terminal owned-terminal t)",
        "(delete-terminal owned-frame t)",
        "(delete-terminal nil t)",
        "(delete-terminal owned-terminal)",
    ] {
        let error = eval.eval_str(expression).unwrap_err();
        match error {
            crate::emacs_core::error::EvalError::Signal { data, .. } => assert_eq!(
                data,
                vec![Value::string(
                    "Deleting a retained graphical display terminal is not supported",
                )],
                "{expression}"
            ),
            other => panic!("expected deletion rejection, got {other:?}"),
        }
        eval.flush_pending_safe_funcalls();
        assert_eq!(eval.frames.frame_list(), before);
        assert_eq!(eval.frames.selected_frame().unwrap().id, selected);
        assert_eq!(*native_frames.borrow(), native_before);
        assert!(eval.eval_str("deletion-hooks").unwrap().is_nil());
        assert!(
            eval.eval_str("(terminal-live-p owned-terminal)")
                .unwrap()
                .is_truthy()
        );
    }
    eval.eval_str("(delete-frame owned-frame t)").unwrap();
    eval.eval_str("(setq deletion-hooks nil)").unwrap();
    assert!(eval.eval_str("(delete-terminal owned-terminal t)").is_err());
    eval.flush_pending_safe_funcalls();
    assert!(eval.eval_str("deletion-hooks").unwrap().is_nil());
    let recreated = eval.eval_str("(x-create-frame nil)").unwrap();
    eval.set_variable("recreated", recreated);
    assert!(
        eval.eval_str(
            "(and (eq (frame-terminal recreated) owned-terminal) (terminal-live-p owned-terminal))"
        )
        .unwrap()
        .is_truthy()
    );
    assert!(eval.shutdown_request().is_none());
}

#[test]
fn deferred_gui_retained_terminal_does_not_block_noelisp_or_other_terminal_cleanup() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let initial = eval.eval_str("(selected-frame)").unwrap();
    let initial_terminal = builtin_frame_terminal(&mut eval, vec![initial]).unwrap();
    let host = deferred_host(false);
    let terminal = host.terminal;
    let native_frames = host.native_frames.clone();
    eval.set_display_host(Box::new(host));
    let frame = eval.eval_str("(x-create-frame nil)").unwrap();
    // Display-host presence must not prohibit deletion of a different owner.
    crate::emacs_core::terminal::pure::builtin_delete_terminal(
        &mut eval,
        vec![initial_terminal, Value::T],
    )
    .unwrap();
    assert!(
        eval.frames
            .get(crate::window::FrameId(frame.as_frame_id().unwrap()))
            .is_some()
    );
    eval.eval_str("(setq cleanup-hook nil delete-terminal-functions (list (lambda (_terminal) (setq cleanup-hook t))))").unwrap();
    crate::emacs_core::terminal::pure::delete_terminal_noelisp_owned(&mut eval, terminal).unwrap();
    assert!(eval.frames.frame_list().is_empty());
    assert!(native_frames.borrow().is_empty());
    assert!(eval.eval_str("cleanup-hook").unwrap().is_nil());
    eval.flush_pending_safe_funcalls();
    assert!(eval.eval_str("cleanup-hook").unwrap().is_truthy());
}

#[test]
fn deferred_gui_failed_surface_completion_rolls_back_frame() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    eval.eval_str("(selected-frame)").unwrap();
    let before = eval.frames.frame_list();
    let mut host = deferred_host(false);
    host.fail_completion = true;
    eval.set_display_host(Box::new(host));
    assert!(eval.eval_str("(x-create-frame nil)").is_err());
    assert_eq!(eval.frames.frame_list(), before);
    assert!(eval.shutdown_request().is_none());
}

#[test]
fn deferred_gui_failed_native_frame_does_not_publish_a_lisp_frame() {
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    eval.eval_str("(selected-frame)").unwrap();
    let before = eval.frames.frame_list();
    let host = deferred_host(true);
    let native_frames = host.native_frames.clone();
    eval.set_display_host(Box::new(host));
    assert!(eval.eval_str("(x-create-frame nil)").is_err());
    assert_eq!(eval.frames.frame_list(), before);
    assert!(
        native_frames.borrow().is_empty(),
        "partial native admission must be rolled back"
    );
    assert!(eval.shutdown_request().is_none());
}
