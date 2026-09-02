use neomacs_app::presentation::{EditorPresentationRuntime, PresentationMetrics};
use neovm_core::emacs_core::eval::Context;

#[test]
fn empty_evaluator_publishes_no_frontend_frames() {
    let runtime = EditorPresentationRuntime::new(PresentationMetrics::CellGrid);
    let mut evaluator = Context::new();

    let result = runtime.publish_visible_frames(&mut evaluator, |_| true);

    assert_eq!(result.published(), 0);
    assert_eq!(result.rejected(), 0);
}

#[test]
fn one_runtime_installs_the_evaluator_snapshot_hook() {
    let runtime = EditorPresentationRuntime::new(PresentationMetrics::CellGrid);
    let mut evaluator = Context::new();

    runtime.install_evaluator_query_hooks(&mut evaluator);

    assert!(evaluator.frame_snapshot_fn.is_some());
}
