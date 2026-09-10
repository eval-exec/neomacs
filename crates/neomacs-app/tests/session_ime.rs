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
    evaluator
        .eval_str(
            r##"(progn
      (insert "abc")
      (fset 'goto-char (lambda (_) (signal 'quit nil)))
      (setq noninteractive t
        top-level '(progn
          (condition-case nil (read-event) (quit (setq observed-quit t)))
          (kill-emacs 0))))"##,
        )
        .unwrap();
    let snapshot = evaluator.ime_surrounding_text().unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .select(ImeSelection {
            snapshot: snapshot.id(),
            cursor: 0,
            anchor: 0,
        })
        .unwrap();
    // If the transport swallows quit, read-event consumes this instead.
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    let (exit, mut evaluator) = session.run_until_stopped(|_| {}).into_parts();
    assert!(exit.is_success());
    assert!(matches!(reply.try_receive(), ImeReply::Ready(Err(_))));
    assert!(evaluator.eval_str("observed-quit").unwrap().is_truthy());
}

#[test]
fn abandoning_a_reply_does_not_block_input_processing() {
    let mut evaluator = Context::new();
    evaluator
        .eval_str(
            r##"(setq noninteractive t
        top-level '(progn (read-event) (kill-emacs 0)))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let ime = frontend
        .input()
        .ime_client(|| panic!("abandoned reply must not wake"));
    drop(ime.surrounding_text().unwrap());
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    assert!(session.run().is_success());
}

#[test]
fn query_is_not_pending_user_input_and_shutdown_disconnects_its_reply() {
    let mut evaluator = Context::new();
    evaluator
        .eval_str(
            r##"(setq noninteractive t
        top-level '(progn
          (setq query-counted-as-input (input-pending-p))
          (kill-emacs 0)))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let ime = frontend
        .input()
        .ime_client(|| panic!("unserviced query must not wake"));
    let reply = ime.surrounding_text().unwrap();
    let (exit, mut evaluator) = session.run_until_stopped(|_| {}).into_parts();
    assert!(exit.is_success());
    assert!(
        evaluator
            .eval_str("query-counted-as-input")
            .unwrap()
            .is_nil()
    );
    drop(evaluator);
    assert!(matches!(reply.try_receive(), ImeReply::Disconnected));
    assert!(ime.surrounding_text().is_err());
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn native_worker_answers_without_moving_context_to_the_frontend() {
    use neomacs_app::session::{NativeEditorWorker, NativeEditorWorkerEvent};
    use std::{sync::mpsc, time::Duration};
    let frontend_thread = std::thread::current().id();
    let (events_tx, events_rx) = mpsc::channel();
    let worker = NativeEditorWorker::spawn(
        "ime-transport-test",
        move || {
            assert_ne!(std::thread::current().id(), frontend_thread);
            let mut evaluator = Context::new();
            evaluator
                .eval_str(
                    r##"(progn (insert "VM-owned")
                (setq noninteractive t
                    top-level '(progn (read-event) (kill-emacs 0))))"##,
                )
                .map_err(|e| format!("{e:?}"))?;
            Ok(evaluator)
        },
        PresentationMetrics::CellGrid,
        move |event| {
            events_tx.send(event).unwrap();
        },
    )
    .unwrap();
    let NativeEditorWorkerEvent::Started(frontend) =
        events_rx.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("worker must start before accepting requests");
    };
    let (wake_tx, wake_rx) = mpsc::channel();
    let ime = frontend.input().ime_client(move || {
        wake_tx.send(std::thread::current().id()).unwrap();
    });
    let reply = ime.surrounding_text().unwrap();
    assert_ne!(
        wake_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        frontend_thread
    );
    let ImeReply::Ready(Some(snapshot)) = reply.try_receive() else {
        panic!("reply must be enqueued before the frontend wake");
    };
    assert_eq!(snapshot.text(), "VM-owned");
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    loop {
        match events_rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            NativeEditorWorkerEvent::FramesReady => {}
            NativeEditorWorkerEvent::Exited(exit) => {
                assert!(exit.is_success());
                break;
            }
            _ => panic!("unexpected worker lifecycle event"),
        }
    }
    worker.join().unwrap();
}

#[test]
fn snapshot_follows_command_execution_not_just_key_dequeue() {
    let mut evaluator = neovm_core::emacs_core::load::create_runtime_startup_evaluator_cached()
        .expect("load real GNU command-execute and command-loop hooks");
    evaluator
        .eval_str(
            r##"(progn
      (setq noninteractive t top-level nil)
      (erase-buffer)
      (use-global-map (make-sparse-keymap))
      (define-key (current-global-map) "a"
        (lambda () (interactive) (insert "command executed")))
      (define-key (current-global-map) "z"
        (lambda () (interactive) (kill-emacs 0))))"##,
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
        .surrounding_text()
        .unwrap();
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    let exit = session.run();
    assert!(exit.is_success(), "{exit:?}");
    let ImeReply::Ready(Some(snapshot)) = reply.try_receive() else {
        panic!("command loop must answer the snapshot request");
    };
    assert_eq!(snapshot.text(), "command executed");
}
