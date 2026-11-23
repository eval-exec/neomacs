use super::*;
use crate::output::builder::DisplayOutputBuilder;
use crate::window_layout::WindowDividerLayout;
use crate::window_output::DisplayTextRowBegin;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::window::DisplayFrameManagerExt;

#[derive(Clone, Copy)]
enum ChromeSource {
    MatchingMetrics,
    SwitchesWindowBuffer,
}

fn measure_then_capture_next_leaf(source: ChromeSource) {
    let mut evaluator = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer_id = evaluator
        .buffer_manager_mut()
        .create_buffer("*empty-body-chrome*");
    evaluator.buffer_manager_mut().set_current(buffer_id);
    let frame_id =
        evaluator
            .frame_manager_mut()
            .create_frame("empty body chrome", 640, 16, buffer_id);
    assert!(evaluator.frame_manager_mut().select_frame(frame_id));
    evaluator
        .eval_str("(setq header-line-format nil tab-line-format nil)")
        .unwrap();
    evaluator
        .eval_str(match source {
            ChromeSource::MatchingMetrics => "(setq mode-line-format '(\"M\"))",
            ChromeSource::SwitchesWindowBuffer => {
                "(setq mode-line-format '((:eval (progn
                (set-window-buffer (selected-window) (get-buffer-create \"*chrome-replacement*\"))
                \"M\"))))"
            }
        })
        .unwrap();
    let mut params = live_window_params(&evaluator, frame_id, buffer_id);
    let metrics = DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0);
    let table = neovm_core::face::FaceTable::new();
    let resolver = FaceResolver::new(&table, 0, 0xffffff, 14.0, None);
    let mut mode_line_face = resolver.resolve_named_face("mode-line");
    mode_line_face.font_line_height = 16.0;
    mode_line_face.font_ascent = 12.0;
    mode_line_face.box_type = 0;
    let mut font_metrics = None;
    let chrome_height = crate::display_status_line::window_chrome_row_height_for_face(
        &mut font_metrics,
        &mode_line_face,
        metrics,
    );
    assert!(chrome_height > 0.0);
    params.bounds = Rect::new(0.0, 0.0, 640.0, chrome_height);
    params.text_bounds = Rect::new(0.0, 0.0, 640.0, 0.0);
    params.char_width = 8.0;
    params.char_height = 16.0;
    params.font_ascent = 12.0;
    params.mode_line_height = chrome_height;
    params.header_line_height = 0.0;
    params.tab_line_height = 0.0;
    let layout_box = WindowLayoutBox::resolve(
        &params,
        WindowChromeMetrics::from_params(&params),
        WindowDividerLayout::default(),
    );
    assert_eq!(
        layout_box.body().height,
        0.0,
        "chrome consumes the entire body allocation"
    );
    let window_id = WindowId(params.window_id as u64);
    let setup = BufferSourceOutputSetup {
        begin_request: TextWindowBeginRequest::new(
            frame_id,
            window_id,
            0,
            0.0,
            0.0,
            window_id.0,
            1,
            80,
            params.bounds,
            params.text_bounds,
            layout_box.body(),
            true,
            DisplayTextRowBegin {
                display_row_index: 0,
                row: 0,
                col: 0,
                y: 0.0,
                x: 0.0,
                start_charpos: crate::types::LayoutCharPos0::new(0),
            },
        ),
        row_visibility_limit: DisplayRowVisibilityLimit {
            allow_partial: false,
            max_rows: 0,
            bottom_y: 0.0,
        },
        row_limit: DisplayRowLimit { max_rows: 0 },
        body_install_context: BufferSourceBodyInstallContext::new(window_id.0, 0, 80),
        retry_bounds: BufferSourceRetryBounds::new(0, 0),
        position_publication: WindowPositionPublication::SynchronousQueryPosition,
        remeasure_chrome: true,
    };
    let request = WindowChromeRowsRenderRequest {
        params: &params,
        layout_box,
        tab_line_face: None,
        header_line_face: None,
        mode_line_face: Some(&mode_line_face),
        tab_line_height: 0.0,
        header_line_height: 0.0,
        mode_line_height: chrome_height,
        mode_line_display_row: 0,
        reserve_right_border_col: false,
        metrics,
        buffer_name: "*empty-body-chrome*",
    };
    let face_attempt = crate::frame_face_arena::FrameFaceArena::default().begin_attempt();
    let mut builder = DisplayOutputBuilder::new();
    builder.set_face_attempt(face_attempt.clone());
    let mut snapshot = None;
    let outcome = setup.measure_chrome_for_empty_body(
        BufferSourceRenderAttemptContext::new(
            TextWindowOutputTarget::from_builder(&mut builder),
            &mut evaluator,
            &mut font_metrics,
            &resolver,
            face_attempt,
            &mut snapshot,
        ),
        request,
        &params,
    );
    match source {
        ChromeSource::MatchingMetrics => {
            let BufferSourceRenderAttemptOutcome::EmptyBodyChromeMeasured { measured, .. } =
                outcome
            else {
                panic!("matching chrome must complete its measurement, got {outcome:?}");
            };
            assert_eq!(measured.mode_line_height, chrome_height);
            assert_eq!(measured.header_line_height, 0.0);
            assert_eq!(measured.tab_line_height, 0.0);
        }
        ChromeSource::SwitchesWindowBuffer => {
            assert!(
                matches!(
                    outcome,
                    BufferSourceRenderAttemptOutcome::LogicalInputsChanged
                ),
                "the real chrome source changed its window buffer: {outcome:?}"
            );
            assert_ne!(
                evaluator
                    .frame_manager()
                    .get(frame_id)
                    .unwrap()
                    .find_window(window_id)
                    .unwrap()
                    .buffer_id(),
                Some(buffer_id)
            );
        }
    }
    assert!(
        snapshot.is_none(),
        "a measurement never publishes a presentation snapshot"
    );
    assert_eq!(
        builder.completed_window_count(),
        0,
        "measurement must not publish a chrome-only matrix"
    );
    // This is the actual next-leaf invariant introduced by #830. Before the
    // producer fix, capture panics because chrome measurement left its grid open.
    let _next =
        builder.capture_leaf_attempt_checkpoint(DisplayWindowId::new(params.window_id as i64));
    assert!(builder.unpublished_window_grid_owner().is_none());
}

fn live_window_params(evaluator: &Context, frame_id: FrameId, buffer_id: BufferId) -> WindowParams {
    let frame = evaluator.frame_manager().get(frame_id).unwrap();
    let window = frame.find_window(frame.selected_window).unwrap();
    let buffer = evaluator.buffer_manager().get(buffer_id).unwrap();
    crate::neovm_bridge::window_params_from_neovm(
        window,
        buffer,
        frame,
        evaluator.variables(),
        evaluator.face_table(),
        Some(12.0),
        true,
        false,
        Value::T,
        Value::NIL,
        None,
    )
    .expect("a real live leaf supplies chrome freshness and parameters")
}

#[test]
fn empty_body_matching_chrome_measurement_closes_output_before_the_next_leaf_checkpoint() {
    measure_then_capture_next_leaf(ChromeSource::MatchingMetrics);
}

#[test]
fn empty_body_invalidated_chrome_measurement_closes_output_before_the_next_leaf_checkpoint() {
    measure_then_capture_next_leaf(ChromeSource::SwitchesWindowBuffer);
}
