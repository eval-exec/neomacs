use neomacs_layout_engine::neovm_bridge::collect_layout_params;
use neovm_core::emacs_core::Context;

#[test]
fn collect_layout_params_marks_minibuffer_only_root() {
    let mut evaluator = Context::new();
    let buf_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*minibuffer-only*");
    let frame_id =
        evaluator
            .frame_manager_mut()
            .create_frame("minibuffer-only-frame", 800, 600, buf_id);

    {
        let frame = evaluator
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame");
        let root_window_id = frame.root_window.id();
        frame.minibuffer_leaf = None;
        frame.minibuffer_window = Some(root_window_id);
    }

    let (_, windows) = collect_layout_params(&evaluator, frame_id, None).expect("layout params");
    assert_eq!(windows.len(), 1);
    assert!(windows[0].is_minibuffer());
}
