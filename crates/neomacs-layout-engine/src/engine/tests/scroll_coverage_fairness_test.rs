use super::*;
use neomacs_display_protocol::types::DisplayWindowId;
use neovm_core::window::{FrameId, WindowId};

const LINE: &str = "ordinary offscreen text\n";

struct TwoWindows {
    // Struct fields drop in declaration order: retire worker/root ownership
    // before dropping the Context that owns the fixture's Lisp heap.
    engine: LayoutEngine,
    eval: Context,
    frame: FrameId,
    selected: WindowId,
    other: WindowId,
}

fn display(window: WindowId) -> DisplayWindowId {
    DisplayWindowId::new(window.0 as i64)
}

fn two_windows() -> TwoWindows {
    let (mut eval, frame, buffer, selected) = incr_editing_frame(&LINE.repeat(300), 800, 600);
    let other = eval
        .frame_manager_mut()
        .split_window(
            frame,
            selected,
            neovm_core::window::SplitDirection::Horizontal,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    // Initialize the real per-frame scheduler and first source fragment before
    // submitting any worker job. One fragment cannot trigger the four-row
    // preview, so no completion can be consumed by this priming call.
    assert!(engine.maintain_scroll_coverage(&eval).is_some());
    assert_eq!(engine.scroll_coverage.fairness_owner_for_test(), None);
    assert!(
        !engine
            .prepared_viewports
            .has_computed(frame, display(selected))
    );
    assert_eq!(
        engine.scroll_coverage.active_owner_for_test(),
        Some(display(selected))
    );
    TwoWindows {
        eval,
        frame,
        selected,
        other,
        engine,
    }
}

fn submit_selected_page(fixture: &mut TwoWindows) {
    fixture
        .engine
        .request_scroll_coverage(
            &fixture.eval,
            fixture.frame,
            fixture.selected,
            CharPos0::new(120 * LINE.len()),
        )
        .unwrap();
    // This native helper acquires the complete bounded page and submits an
    // actual RowWorker job. It never drains that job's result.
    assert!(
        fixture
            .engine
            .scroll_coverage
            .active_capture_progress_for_test()
            .is_none()
    );
    assert_eq!(
        fixture.engine.scroll_coverage.active_owner_for_test(),
        Some(display(fixture.selected))
    );
}

fn frame_drain_until_computed(fixture: &mut TwoWindows) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !fixture
        .engine
        .prepared_viewports
        .has_computed(fixture.frame, display(fixture.selected))
    {
        assert!(
            std::time::Instant::now() < deadline,
            "actual worker result did not reach the frame drain"
        );
        // ONLY the real early frame drain can consume this completion. In
        // particular, no maintenance/direct drain poll races this caller.
        fixture
            .engine
            .layout_frame_rust(&mut fixture.eval, fixture.frame);
        std::thread::yield_now();
    }
}

#[test]
fn early_frame_drain_hands_completed_owner_turn_to_other_window() {
    let mut fixture = two_windows();
    submit_selected_page(&mut fixture);
    frame_drain_until_computed(&mut fixture);
    // Before the production repair this assertion deterministically reads
    // None: the only completed-result consumer was layout_frame_rust, and
    // only the maintenance consumer used to advance this cursor.
    assert_eq!(
        fixture.engine.scroll_coverage.fairness_owner_for_test(),
        Some(display(fixture.selected)),
        "early frame completion must retire the admitted owner's turn"
    );
    assert_eq!(fixture.engine.scroll_coverage.active_owner_for_test(), None);

    // An empty early drain must preserve the completed owner's turn.
    fixture
        .engine
        .layout_frame_rust(&mut fixture.eval, fixture.frame);
    assert_eq!(
        fixture.engine.scroll_coverage.fairness_owner_for_test(),
        Some(display(fixture.selected))
    );
    assert!(
        fixture
            .engine
            .maintain_scroll_coverage(&fixture.eval)
            .is_some()
    );
    assert_eq!(
        fixture.engine.scroll_coverage.active_owner_for_test(),
        Some(display(fixture.other)),
        "the next idle page must give the other owner an opportunity"
    );
}

#[test]
fn maintenance_drain_hands_completed_owner_turn_to_other_window() {
    let mut fixture = two_windows();
    submit_selected_page(&mut fixture);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !fixture
        .engine
        .prepared_viewports
        .has_computed(fixture.frame, display(fixture.selected))
    {
        assert!(std::time::Instant::now() < deadline);
        // The complementary real consumer: no frame layout after submission.
        assert!(
            fixture
                .engine
                .maintain_scroll_coverage(&fixture.eval)
                .is_some()
        );
        std::thread::yield_now();
    }
    assert_eq!(
        fixture.engine.scroll_coverage.fairness_owner_for_test(),
        Some(display(fixture.selected))
    );
    assert!(
        fixture
            .engine
            .maintain_scroll_coverage(&fixture.eval)
            .is_some()
    );
    assert_eq!(
        fixture.engine.scroll_coverage.active_owner_for_test(),
        Some(display(fixture.other))
    );
}

#[test]
fn early_frame_preview_drain_preserves_active_page_until_completion() {
    let mut fixture = two_windows();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // Acquire precisely the first four-program preview. The priming call
    // acquired one line; submission happens at four and we stop immediately,
    // so no subsequent idle call can consume the submitted completion.
    while fixture
        .engine
        .scroll_coverage
        .active_capture_progress_for_test()
        .is_none_or(|(_, _, programs)| programs < 4)
    {
        assert!(std::time::Instant::now() < deadline);
        assert!(
            fixture
                .engine
                .maintain_scroll_coverage(&fixture.eval)
                .is_some()
        );
    }
    frame_drain_until_computed(&mut fixture);
    assert_eq!(
        fixture.engine.scroll_coverage.fairness_owner_for_test(),
        Some(display(fixture.selected)),
        "the early preview drain must share the owner handoff"
    );
    let before = fixture
        .engine
        .scroll_coverage
        .active_capture_progress_for_test()
        .expect("a four-row preview must retain its unfinished page");
    assert_eq!(
        fixture.engine.scroll_coverage.active_owner_for_test(),
        Some(display(fixture.selected))
    );
    assert!(
        fixture
            .engine
            .maintain_scroll_coverage(&fixture.eval)
            .is_some()
    );
    assert_eq!(
        fixture.engine.scroll_coverage.active_owner_for_test(),
        Some(display(fixture.selected)),
        "round-robin handoff must not preempt an unfinished bounded page"
    );
    let after = fixture
        .engine
        .scroll_coverage
        .active_capture_progress_for_test()
        .expect("the active page must continue acquisition");
    assert!(after.1 > before.1);
}
