use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use neomacs_app::frontend_event::{FrontendEvent, FrontendFrameId};
use neomacs_app::presentation::PresentationMetrics;
use neomacs_app::session::{EditorSession, ImeReply};
use neovm_core::emacs_core::eval::Context;

#[test]
fn acknowledged_replacement_delivers_a_text_conversion_event() {
    use neovm_host_abi::ime::{ImeReplacement, ImeReplacementOutcome};
    let mut evaluator = neovm_core::emacs_core::load::create_runtime_startup_evaluator_cached()
        .expect("load GNU text-conversion support");
    evaluator
        .eval_str(
            r##"(progn
      (erase-buffer) (insert "prefix😀tail")
      (put-text-property 1 (point-max) 'face 'bold)
      (setq noninteractive t top-level
        '(progn (setq observed-ime-event (read-event)) (kill-emacs 0))))"##,
        )
        .unwrap();
    let old = evaluator.ime_surrounding_text().unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .replace_and_observe(ImeReplacement {
            snapshot: old.id(),
            start: 6,
            end: 10,
            text: "X".into(),
            cursor: 7,
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
    let ImeReply::Ready(Ok(ack)) = reply.try_receive() else {
        panic!("missing replacement acknowledgement")
    };
    assert_eq!(ack.outcome, ImeReplacementOutcome::Applied);
    let current = ack.snapshot.unwrap();
    assert_ne!(current.id(), old.id());
    assert_eq!(current.text(), "prefixXtail");
    assert_eq!(current.cursor(), 7);
    assert!(
        evaluator
            .eval_str("(get-text-property 7 'face)")
            .unwrap()
            .is_symbol_named("bold")
    );
    assert!(
        evaluator
            .eval_str("observed-ime-event")
            .unwrap()
            .is_symbol_named("text-conversion")
    );
    assert!(
        evaluator
            .eval_str(
                r##"(let ((inserted (car text-conversion-edits))
                                      (deleted (cadr text-conversion-edits)))
      (and (markerp (nth 1 inserted)) (markerp (nth 2 inserted))
           (marker-insertion-type (nth 2 inserted))
           (markerp (nth 1 deleted)) (eq (nth 1 deleted) (nth 2 deleted))
           (eq (nth 3 deleted) t)))"##
            )
            .unwrap()
            .is_truthy()
    );
    evaluator
        .eval_str(r##"(save-excursion (goto-char 1) (insert "!"))"##)
        .unwrap();
    assert!(
        evaluator
            .eval_str(
                r##"(equal
      (mapcar (lambda (edit) (list (marker-position (nth 1 edit))
                                  (marker-position (nth 2 edit))))
              text-conversion-edits)
      '((8 9) (8 8)))"##
            )
            .unwrap()
            .is_truthy()
    );
}

#[test]
fn applied_selection_acknowledges_the_resulting_cursor() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut evaluator = neovm_core::emacs_core::load::create_runtime_startup_evaluator_cached()
        .expect("load GNU point/mark functions");
    evaluator
        .eval_str(
            r##"(progn
      (erase-buffer) (insert "a😀b")
      (setq noninteractive t top-level '(progn (read-event) (kill-emacs 0))))"##,
        )
        .unwrap();
    let old = evaluator.ime_surrounding_text().unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .select_and_observe(ImeSelection {
            snapshot: old.id(),
            cursor: 5,
            anchor: 5,
        })
        .unwrap();
    frontend
        .input()
        .submit(&FrontendEvent::TextCommitted {
            text: "z".into(),
            target: FrontendFrameId::PRIMARY,
        })
        .unwrap();
    assert!(session.run().is_success());
    let ImeReply::Ready(Ok(ack)) = reply.try_receive() else {
        panic!("missing selection acknowledgement")
    };
    assert_eq!(ack.outcome, ImeSelectionOutcome::Applied);
    let current = ack.snapshot.unwrap();
    assert_ne!(current.id(), old.id());
    assert_eq!(current.text(), "a😀b");
    assert_eq!(current.cursor(), 5);
    assert_eq!(current.anchor(), 5);
}

#[test]
fn replacement_hook_cannot_redirect_an_observed_range() {
    use neovm_host_abi::ime::{ImeReplacement, ImeReplacementOutcome};
    let mut evaluator = neovm_core::emacs_core::load::create_runtime_startup_evaluator_cached()
        .expect("load GNU text-conversion support");
    evaluator
        .eval_str(
            r##"(progn
      (erase-buffer) (insert "original")
      (setq before-change-functions
        (list (lambda (_start _end) (set-buffer (get-buffer-create "hook-target")))))
      (setq noninteractive t top-level '(progn (read-event) (kill-emacs 0))))"##,
        )
        .unwrap();
    let old = evaluator.ime_surrounding_text().unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .replace_and_observe(ImeReplacement {
            snapshot: old.id(),
            start: 0,
            end: 8,
            text: "wrong".into(),
            cursor: 5,
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
    let ImeReply::Ready(Ok(ack)) = reply.try_receive() else {
        panic!("missing stale replacement acknowledgement")
    };
    assert_eq!(ack.outcome, ImeReplacementOutcome::StaleSnapshot);
    assert_eq!(
        evaluator.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("")
    );
    assert_eq!(
        evaluator
            .eval_str(r##"(with-current-buffer "*scratch*" (buffer-string))"##)
            .unwrap()
            .as_utf8_str(),
        Some("original")
    );
}

#[test]
fn replacement_rejects_invalid_observed_coordinates_without_editing() {
    use ImeReplacementOutcome::{InvalidCursor, InvalidRange};
    use neovm_host_abi::ime::{ImeReplacement, ImeReplacementOutcome};
    for (start, end, text, cursor, expected) in [
        (0, 2, "x", 1, InvalidRange),
        (5, 1, "x", 1, InvalidRange),
        (1, 5, "😀", 2, InvalidCursor),
        (1, 5, "X", 99, InvalidCursor),
        (1, 5, "X", usize::MAX, InvalidCursor),
    ] {
        let mut evaluator = Context::new();
        evaluator
            .eval_str(
                r##"(progn (insert "a😀b")
          (setq noninteractive t top-level '(progn (read-event) (kill-emacs 0))))"##,
            )
            .unwrap();
        let old = evaluator.ime_surrounding_text().unwrap();
        let (session, frontend) =
            EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
        let reply = frontend
            .input()
            .ime_client(|| {})
            .replace_and_observe(ImeReplacement {
                snapshot: old.id(),
                start,
                end,
                text: text.into(),
                cursor,
            })
            .unwrap();
        frontend
            .input()
            .submit(&FrontendEvent::TextCommitted {
                text: "z".into(),
                target: FrontendFrameId::PRIMARY,
            })
            .unwrap();
        assert!(session.run().is_success());
        let ImeReply::Ready(Ok(ack)) = reply.try_receive() else {
            panic!("missing rejected replacement acknowledgement")
        };
        assert_eq!(ack.outcome, expected);
        let current = ack.snapshot.unwrap();
        assert_eq!(current.text(), "a😀b");
        assert_eq!(current.cursor(), 6);
        assert_ne!(current.id(), old.id());
    }
}

#[test]
fn stale_selection_acknowledges_the_current_snapshot_in_input_order() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut evaluator = Context::new();
    evaluator.eval_str(r##"(insert "old")"##).unwrap();
    let old = evaluator.ime_surrounding_text().unwrap();
    evaluator
        .eval_str(
            r##"(progn (insert "new")
      (setq noninteractive t top-level '(progn (read-event) (kill-emacs 0))))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .select_and_observe(ImeSelection {
            snapshot: old.id(),
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
    assert!(session.run().is_success());
    let ImeReply::Ready(Ok(ack)) = reply.try_receive() else {
        panic!("missing acknowledgement")
    };
    assert_eq!(ack.outcome, ImeSelectionOutcome::StaleSnapshot);
    let current = ack.snapshot.unwrap();
    assert_ne!(current.id(), old.id());
    assert_eq!(current.text(), "oldnew");
    assert_eq!(current.cursor(), 6);
}

#[test]
fn selection_acknowledgement_does_not_export_private_input() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut evaluator = Context::new();
    evaluator.eval_str(r##"(insert "secret")"##).unwrap();
    let old = evaluator.ime_surrounding_text().unwrap();
    evaluator
        .eval_str(
            r##"(setq noninteractive t top-level
      '(progn (let ((overriding-text-conversion-style 'password)) (read-event))
              (kill-emacs 0)))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .select_and_observe(ImeSelection {
            snapshot: old.id(),
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
    assert!(session.run().is_success());
    let ImeReply::Ready(Ok(ack)) = reply.try_receive() else {
        panic!("missing acknowledgement")
    };
    assert_eq!(ack.outcome, ImeSelectionOutcome::StaleSnapshot);
    assert!(ack.snapshot.is_none());
}

#[test]
fn snapshot_request_honors_dynamic_password_bindings_during_input() {
    let mut evaluator = Context::new();
    evaluator
        .eval_str(
            r##"(progn
        (insert "secret")
        (setq noninteractive t top-level
          '(progn
            (let ((read-hide-char ?*)
                  (overriding-text-conversion-style 'password))
              (read-event))
            (kill-emacs 0))))"##,
        )
        .unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
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
    assert!(session.run().is_success());
    assert!(matches!(reply.try_receive(), ImeReply::Ready(None)));
}

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
    check_selection_failure(false);
}

#[test]
fn selection_acknowledgement_preserves_lisp_quit() {
    check_selection_failure(true);
}

fn check_selection_failure(observe: bool) {
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
    let client = frontend.input().ime_client(|| {});
    let selection = ImeSelection {
        snapshot: snapshot.id(),
        cursor: 0,
        anchor: 0,
    };
    let failed: Box<dyn FnOnce() -> bool> = if observe {
        let reply = client.select_and_observe(selection).unwrap();
        Box::new(move || matches!(reply.try_receive(), ImeReply::Ready(Err(_))))
    } else {
        let reply = client.select(selection).unwrap();
        Box::new(move || matches!(reply.try_receive(), ImeReply::Ready(Err(_))))
    };
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
    assert!(failed());
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

#[test]
fn replacement_is_pending_user_input() {
    use neovm_host_abi::ime::ImeReplacement;
    let mut evaluator = Context::new();
    evaluator
        .eval_str(
            r##"(setq noninteractive t top-level
      '(progn (setq replacement-pending (input-pending-p)) (kill-emacs 0)))"##,
        )
        .unwrap();
    let snapshot = evaluator.ime_surrounding_text().unwrap();
    let (session, frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let reply = frontend
        .input()
        .ime_client(|| {})
        .replace_and_observe(ImeReplacement {
            snapshot: snapshot.id(),
            start: 0,
            end: 0,
            text: "x".into(),
            cursor: 1,
        })
        .unwrap();
    let (exit, mut evaluator) = session.run_until_stopped(|_| {}).into_parts();
    assert!(exit.is_success());
    assert!(
        evaluator
            .eval_str("replacement-pending")
            .unwrap()
            .is_truthy()
    );
    drop(evaluator);
    assert!(matches!(reply.try_receive(), ImeReply::Disconnected));
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn native_worker_answers_without_moving_context_to_the_frontend() {
    check_native_worker_snapshot(false);
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn native_worker_answers_snapshot_while_already_waiting() {
    check_native_worker_snapshot(true);
}

#[cfg(not(target_family = "wasm"))]
fn check_native_worker_snapshot(query_after_idle: bool) {
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
    if query_after_idle {
        // Exercise arrival after the worker has had time to enter its wait,
        // not just the easier case where input is queued before read-event.
        std::thread::sleep(Duration::from_millis(50));
    }
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
