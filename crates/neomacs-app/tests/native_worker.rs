#![cfg(not(target_family = "wasm"))]

mod support;

use std::sync::mpsc;
use std::time::Duration;

use neomacs_app::host::HostProfile;
use neomacs_app::presentation::PresentationMetrics;
use neomacs_app::runtime_image::AuthenticatedPortableRuntimeImage;
use neomacs_app::session::{EditorSession, NativeEditorWorker, NativeEditorWorkerEvent};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::pdump::encode_portable_snapshot;
use support::runtime_resources::{content_id, mounted_runtime_resources};

#[test]
fn restored_session_runs_after_pdump_hook_before_top_level() {
    let mut image = Context::new();
    image
        .eval_str(
            "(setq top-level '(kill-emacs 7)\n\
                   after-pdump-load-hook\n\
                   (list (lambda () (setq top-level '(kill-emacs 23)))))",
        )
        .expect("prepare portable runtime image");
    let bytes = encode_portable_snapshot(&image).expect("encode portable runtime image");
    let resources = mounted_runtime_resources(&[
        ("lisp/loadup.el", b"(provide 'loadup)"),
        ("etc/NEWS", b"news"),
    ]);
    let image_id = content_id(&bytes);
    let evaluator = AuthenticatedPortableRuntimeImage::from_assets(&bytes, image_id.as_bytes())
        .expect("authenticate portable image")
        .load_for_with_mounted_runtime_resources(HostProfile::WASM, resources)
        .expect("restore portable runtime image");

    let (session, _frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});
    let exit = session.run();

    assert_eq!(
        exit.shutdown_request().map(|request| request.exit_code),
        Some(23),
        "the restored image hook must run before the outer command loop",
    );
}

#[test]
fn native_worker_constructs_and_runs_the_evaluator_off_the_frontend_thread() {
    let frontend_thread = std::thread::current().id();
    let (event_tx, event_rx) = mpsc::channel();

    let worker = NativeEditorWorker::spawn(
        "test-editor",
        move || {
            assert_ne!(std::thread::current().id(), frontend_thread);
            let mut evaluator = Context::new();
            evaluator.set_variable("noninteractive", Value::T);
            let top_level = evaluator
                .eval_str("'(kill-emacs 0)")
                .map_err(|error| format!("failed to prepare test top-level: {error:?}"))?;
            evaluator.set_variable("top-level", top_level);
            Ok(evaluator)
        },
        PresentationMetrics::CellGrid,
        move |event| event_tx.send(event).expect("event receiver remains alive"),
    )
    .expect("spawn evaluator worker");

    assert!(matches!(
        event_rx.recv_timeout(Duration::from_secs(5)),
        Ok(NativeEditorWorkerEvent::Started(_))
    ));
    let exit = loop {
        match event_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker should report its terminal state")
        {
            NativeEditorWorkerEvent::Exited(exit) => break exit,
            NativeEditorWorkerEvent::FramesReady => {}
            NativeEditorWorkerEvent::Started(_) => panic!("worker started more than once"),
            NativeEditorWorkerEvent::StartupFailed(error) => {
                panic!("worker startup unexpectedly failed: {error}")
            }
        }
    };

    assert!(exit.is_success());
    assert_eq!(
        exit.shutdown_request().map(|request| request.exit_code),
        Some(0)
    );
    worker.join().expect("worker should not panic");
}

#[test]
fn native_worker_reports_factory_failure_without_starting_a_session() {
    let (event_tx, event_rx) = mpsc::channel();
    let worker = NativeEditorWorker::spawn(
        "failing-editor",
        || Err("runtime image is unavailable".to_owned()),
        PresentationMetrics::CellGrid,
        move |event| event_tx.send(event).expect("event receiver remains alive"),
    )
    .expect("spawn evaluator worker");

    assert!(matches!(
        event_rx.recv_timeout(Duration::from_secs(5)),
        Ok(NativeEditorWorkerEvent::StartupFailed(error))
            if error == "runtime image is unavailable"
    ));
    worker.join().expect("worker should not panic");
    assert!(event_rx.try_recv().is_err());
}
