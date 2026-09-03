use std::cell::Cell;
use std::rc::Rc;

use neomacs_app::presentation::{EditorPresentationRuntime, PresentationMetrics};
use neomacs_app::session::{EditorSession, FrontendFrameReceive, SessionRedisplayAction};
use neovm_core::emacs_core::eval::Context;

#[test]
fn attaching_an_empty_evaluator_installs_transport_without_inventing_a_frame() {
    let (mut session, mut frontend) =
        EditorSession::attach(Context::new(), PresentationMetrics::CellGrid, || {});

    let result = session.publish_now();

    assert_eq!(result.published(), 0);
    assert!(matches!(
        frontend.frames().try_latest(),
        FrontendFrameReceive::Empty
    ));
}

#[test]
fn host_transport_can_claim_redisplay_before_frontend_publication() {
    let route_calls = Rc::new(Cell::new(0));
    let observed_route_calls = Rc::clone(&route_calls);
    let mut session = EditorSession::attach_host_transport(
        Context::new(),
        EditorPresentationRuntime::new(PresentationMetrics::CellGrid),
        move |_| {
            observed_route_calls.set(observed_route_calls.get() + 1);
            SessionRedisplayAction::Handled
        },
        |_| panic!("handled redisplay must not reach the presentation sink"),
        || panic!("handled redisplay must not notify the frontend"),
    );

    let result = session.publish_now();

    assert_eq!(route_calls.get(), 1);
    assert_eq!(result.published(), 0);
    assert_eq!(result.rejected(), 0);
}

#[test]
fn stopped_session_returns_its_evaluator_to_the_native_owner() {
    let mut evaluator = Context::new();
    evaluator.set_variable("noninteractive", neovm_core::emacs_core::Value::T);
    let top_level = evaluator.eval_str("'(kill-emacs 13)").unwrap();
    evaluator.set_variable("top-level", top_level);
    let (session, _frontend) =
        EditorSession::attach(evaluator, PresentationMetrics::CellGrid, || {});

    let stopped = session.run_until_stopped(|_| {});
    let (exit, evaluator) = stopped.into_parts();

    assert_eq!(
        exit.shutdown_request().map(|request| request.exit_code),
        Some(13)
    );
    assert_eq!(
        evaluator
            .shutdown_request()
            .map(|request| request.exit_code),
        Some(13)
    );
}
