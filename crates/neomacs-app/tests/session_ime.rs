use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use neomacs_app::frontend_event::{FrontendEvent, FrontendFrameId};
use neomacs_app::presentation::PresentationMetrics;
use neomacs_app::session::{EditorSession, ImeReply};
use neovm_core::emacs_core::eval::Context;

#[test]
fn snapshot_request_observes_the_preceding_keyboard_edit_and_wakes_frontend() {
    let mut evaluator = Context::new();
    evaluator
        .eval_str(
            r##"(setq noninteractive t
        top-level '(progn
          (insert (char-to-string (read-event)))
          (read-event)
          (kill-emacs 0)))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let wakes = Arc::new(AtomicUsize::new(0));
    let observed = wakes.clone();
    let ime = frontend.input().ime_client(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    });
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "a".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    let reply = ime.surrounding_text().unwrap();
    assert!(matches!(reply.try_receive(), ImeReply::Pending));
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();

    assert!(session.run().is_success());
    let ImeReply::Ready(Some(snapshot)) = reply.try_receive() else {
        panic!("VM must answer the ordered snapshot request");
    };
    assert_eq!(snapshot.text(), "a");
    assert_eq!(snapshot.cursor(), 1);
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
}

#[test]
fn queued_selection_cannot_target_a_buffer_switched_by_preceding_input() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut evaluator = Context::new();
    evaluator.eval_str(r##"(insert "original")"##).unwrap();
    let snapshot = evaluator.ime_surrounding_text().unwrap();
    evaluator
        .eval_str(
            r##"(setq noninteractive t
        top-level '(progn
          (read-event)
          (set-buffer (get-buffer-create "other"))
          (insert "untouched")
          (read-event)
          (kill-emacs 0)))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "a".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    let reply = frontend
        .input()
        .ime_client(|| {})
        .select(ImeSelection {
            snapshot: snapshot.id(),
            cursor: 0,
            anchor: 0,
        })
        .unwrap();
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    let (exit, mut evaluator) = session.run_until_stopped(|_| {}).into_parts();
    assert!(exit.is_success());
    assert!(matches!(
        reply.try_receive(),
        ImeReply::Ready(Ok(ImeSelectionOutcome::StaleSnapshot))
    ));
    assert_eq!(evaluator.eval_str("(point)").unwrap().as_fixnum(), Some(10));
}

#[test]
fn selection_failure_replies_without_swallowing_lisp_quit() {
    use neovm_host_abi::ime::ImeSelection;
    let mut evaluator = Context::new();
    evaluator.eval_str(r##"(progn
      (insert "abc")
      (fset 'goto-char (lambda (_) (signal 'quit nil)))
      (setq noninteractive t
        top-level '(progn
          (condition-case nil (read-event) (quit (setq observed-quit t)))
          (kill-emacs 0))))"##).unwrap();
    let snapshot = evaluator.ime_surrounding_text().unwrap();
    let (session, frontend) = EditorSession::attach(
        evaluator, PresentationMetrics::CellGrid, || {},
    );
    let reply = frontend.input().ime_client(|| {}).select(ImeSelection {
        snapshot: snapshot.id(), cursor: 0, anchor: 0,
    }).unwrap();
    // If the transport swallows quit, read-event consumes this instead.
    frontend.input().submit(&FrontendEvent::TextCommitted {
        text: "z".into(), target: FrontendFrameId::PRIMARY,
    }).unwrap();
    let (exit, mut evaluator) = session.run_until_stopped(|_| {}).into_parts();
    assert!(exit.is_success());
    assert!(matches!(reply.try_receive(), ImeReply::Ready(Err(_))));
    assert!(evaluator.eval_str("observed-quit").unwrap().is_truthy());
}
