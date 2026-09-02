use neomacs_app::presentation::PresentationMetrics;
use neomacs_app::session::{EditorSession, FrontendFrameReceive};
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
