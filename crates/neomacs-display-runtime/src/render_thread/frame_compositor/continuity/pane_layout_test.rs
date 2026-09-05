use super::*;
use neomacs_display_protocol::motion_spec::{MotionDuration, TweenSpec};
use neomacs_display_protocol::presentation_origin::BufferModiff;
use neomacs_display_protocol::scroll_animation::TransitionEasing;
use neomacs_display_protocol::types::DisplayWindowId;
use std::time::Duration;

fn origin() -> EventTime {
    neomacs_display_protocol::frame_time::observe_platform_now()
}

fn frame_at(origin: EventTime, millis: u64) -> FrameSample {
    FrameSample::new(origin.plus(Duration::from_millis(millis)), Duration::ZERO)
}

/// A 100ms linear tween, so a sample's progress is the elapsed fraction and
/// every placement below can be read off directly.
fn linear_100ms() -> MotionSpec {
    MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::new(Duration::from_millis(100)).expect("a positive duration"),
        easing: TransitionEasing::Linear,
    })
}

fn window(id: i64, bounds: Rect) -> WindowInfo {
    WindowInfo {
        window_id: DisplayWindowId::new(id),
        buffer_id: id as u64 + 100,
        window_start: 1,
        window_end: 100,
        buffer_size: 1000,
        buffer_modiff: BufferModiff::new(1),
        bounds,
        geometry: neomacs_display_protocol::PresentedWindowGeometry::default(),
        line_number_field: None,
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: false,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::from("scratch"),
        buffer_file_name: String::new(),
        modified: false,
    }
}

fn live(id: i64) -> LiveDisplayWindowId {
    LiveDisplayWindowId::try_from(DisplayWindowId::new(id)).expect("a live window id")
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

/// The placement of `window` in `sample`.
fn placed(sample: &LayoutSample, id: i64) -> PanePlacement {
    *sample
        .panes
        .iter()
        .find(|placement| placement.window == live(id))
        .expect("the window was placed")
}

// =======================================================================
// What counts as a morph
// =======================================================================

#[test]
fn an_unchanged_layout_is_not_a_morph() {
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    assert!(PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin()).is_none());
}

#[test]
fn a_sub_pixel_difference_is_not_a_move() {
    // Layout arrives as f32 from integer cell arithmetic; animating a rounding
    // difference would start a motion on a frame that is not moving.
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.2, 0.0, 800.0, 599.8))];
    assert!(PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin()).is_none());
}

#[test]
fn an_instant_policy_produces_no_morph_to_allocate() {
    // A reduced-motion setting must be free, not merely fast.
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    assert!(PaneLayoutMorph::try_new(&before, &after, MotionSpec::Instant, origin()).is_none());
}

#[test]
fn the_minibuffer_never_participates() {
    // The echo area resizes on nearly every command; sliding it would put a
    // moving pane under the cursor while the user types.
    let mut before_mini = window(9, rect(0.0, 580.0, 800.0, 20.0));
    before_mini.is_minibuffer = true;
    let mut after_mini = window(9, rect(0.0, 540.0, 800.0, 60.0));
    after_mini.is_minibuffer = true;
    assert!(
        PaneLayoutMorph::try_new(&[before_mini], &[after_mini], linear_100ms(), origin()).is_none()
    );
}

#[test]
fn a_split_is_one_persisted_pane_and_one_entered_pane() {
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin()).expect("a morph");
    let changes: Vec<_> = morph.changes().collect();
    assert_eq!(changes.len(), 2);
    assert!(matches!(changes[0], PaneChange::Persisted { .. }));
    assert!(matches!(changes[1], PaneChange::Entered { .. }));
}

#[test]
fn deleting_a_window_leaves_one_persisted_pane_and_one_exited_pane() {
    let before = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let after = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin()).expect("a morph");
    let changes: Vec<_> = morph.changes().collect();
    assert_eq!(changes.len(), 2);
    assert!(matches!(changes[0], PaneChange::Persisted { .. }));
    assert!(matches!(
        changes[1],
        PaneChange::Exited { window, .. } if window == live(2)
    ));
}

#[test]
fn changes_are_ordered_by_window_so_a_morph_is_reproducible() {
    // The diff walks hash maps. A projection whose pane order varied between
    // two runs of the same layout change would resolve an overlapping hit
    // differently each time.
    let before = [window(7, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [
        window(7, rect(0.0, 0.0, 400.0, 600.0)),
        window(3, rect(400.0, 0.0, 200.0, 600.0)),
        window(5, rect(600.0, 0.0, 200.0, 600.0)),
    ];
    let morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin()).expect("a morph");
    let ids: Vec<_> = morph
        .changes()
        .map(|change| change.window().get())
        .collect();
    assert_eq!(ids, vec![3, 5, 7]);
}

#[test]
fn a_window_without_a_live_identity_is_skipped_rather_than_matched() {
    // A placeholder id is not an identity, so pairing two of them would pair
    // whatever happened to be published in the same slot.
    let mut placeholder = window(1, rect(0.0, 0.0, 800.0, 600.0));
    placeholder.window_id = DisplayWindowId::new(0);
    let mut moved = window(1, rect(0.0, 0.0, 400.0, 600.0));
    moved.window_id = DisplayWindowId::new(0);
    assert!(PaneLayoutMorph::try_new(&[placeholder], &[moved], linear_100ms(), origin()).is_none());
}

// =======================================================================
// Sampling
// =======================================================================

#[test]
fn every_pane_is_placed_from_one_shared_sample() {
    // Two sides of a split must agree about where their shared edge is at
    // every instant. A gap or overlap on a moving seam is far more visible
    // than the motion itself.
    let before = [
        window(1, rect(0.0, 0.0, 800.0, 600.0)),
        window(2, rect(800.0, 0.0, 0.0, 600.0)),
    ];
    let after = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    for ms in [0, 25, 50, 75, 100] {
        let sample = morph.sample(frame_at(origin, ms));
        let left = placed(&sample, 1);
        let right = placed(&sample, 2);
        assert!(
            (left.bounds.x + left.bounds.width - right.bounds.x).abs() < 1e-3,
            "the shared edge separated at {ms}ms"
        );
    }
}

#[test]
fn a_persisted_pane_starts_at_its_old_rect_and_arrives_at_its_new_one() {
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");

    assert!((placed(&morph.sample(frame_at(origin, 0)), 1).bounds.width - 800.0).abs() < 1e-3);
    assert!((placed(&morph.sample(frame_at(origin, 50)), 1).bounds.width - 600.0).abs() < 1e-3);

    let arrived = morph.sample(frame_at(origin, 100));
    assert!((placed(&arrived, 1).bounds.width - 400.0).abs() < 1e-3);
    assert!(arrived.motion.finished);
}

#[test]
fn a_moving_pane_keeps_showing_its_destination_content() {
    // Interpolating the content origin as well would scroll the text inside
    // the pane while the pane travelled, which is not what a split does.
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    for ms in [0, 50, 100] {
        assert_eq!(
            placed(&morph.sample(frame_at(origin, ms)), 1).content_origin,
            (400.0, 0.0),
            "the destination's content origin, at every instant"
        );
    }
}

#[test]
fn an_entering_pane_stays_at_its_destination_for_the_whole_motion() {
    // It has nowhere to come from. Step 8 gives it a snapshot to fade.
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    for ms in [0, 50, 100] {
        assert_eq!(
            placed(&morph.sample(frame_at(origin, ms)), 2).bounds,
            rect(400.0, 0.0, 400.0, 600.0)
        );
    }
}

// =======================================================================
// The projection and the pixels come from one place
// =======================================================================

#[test]
fn the_projection_maps_a_surface_point_back_to_the_content_under_it() {
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    // Halfway: the pane spans x ∈ [200, 800) while showing content from 400.
    let sample = morph.sample(frame_at(origin, 50));
    let presentation = neomacs_display_protocol::PresentationId::new(3);
    let projection = sample.projection(presentation);

    let surface = neomacs_display_protocol::GeometryPoint::<
        neomacs_display_protocol::RootSurfaceSpace,
        neomacs_display_protocol::LogicalPixels,
    >::from_px(260.0, 10.0)
    .expect("a finite point");
    let mapped = projection.map(surface).expect("a mapped point");
    assert!(
        (mapped.x() - 460.0).abs() < 1e-3,
        "60px into a pane whose content starts at 400"
    );
}

#[test]
fn at_rest_the_projection_is_the_identity_the_settled_frame_would_use() {
    // The last frame of a morph and the first settled frame after it must
    // answer a click identically, or a hit would shift by a pixel exactly as
    // the motion ended.
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let presentation = neomacs_display_protocol::PresentationId::new(3);
    let projection = morph.sample(frame_at(origin, 100)).projection(presentation);

    let surface = neomacs_display_protocol::GeometryPoint::<
        neomacs_display_protocol::RootSurfaceSpace,
        neomacs_display_protocol::LogicalPixels,
    >::from_px(500.0, 10.0)
    .expect("a finite point");
    let mapped = projection.map(surface).expect("a mapped point");
    assert!((mapped.x() - 500.0).abs() < 1e-3);
    assert!((mapped.y() - 10.0).abs() < 1e-3);
}

// =======================================================================
// The compositor path: install, sample, settle
// =======================================================================

fn empty_render() -> crate::render_thread::frame_windows::GuiFrameRenderState {
    crate::render_thread::frame_windows::GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    )
}

/// Install a presentation whose panes are `windows`, measuring the morph
/// against whatever is already retained.
fn install(
    render: &mut crate::render_thread::frame_windows::GuiFrameRenderState,
    windows: &[WindowInfo],
    at: EventTime,
) {
    let mut frame = crate::core::frame_glyphs::FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = neomacs_display_protocol::PresentationId::new(1);
    frame.window_infos = windows.to_vec();
    render.measure_pane_layout(Some(&frame), at);
    render.compositor.current_frame = Some(frame);
    // Composed, so the next install measures against this one. See the note on
    // `FrameCompositor::baseline`.
    render.begin_presentable_render();
}

#[test]
fn a_split_installs_a_morph_that_settles_and_is_dropped() {
    let mut render = empty_render();
    render.compositor.pane_motion = linear_100ms();
    let origin = origin();

    install(
        &mut render,
        &[window(1, rect(0.0, 0.0, 800.0, 600.0))],
        origin,
    );
    assert!(
        render.compositor.pane_morph.is_none(),
        "the first install has no previous layout to have moved from"
    );

    install(
        &mut render,
        &[
            window(1, rect(0.0, 0.0, 400.0, 600.0)),
            window(2, rect(400.0, 0.0, 400.0, 600.0)),
        ],
        origin,
    );
    assert!(render.compositor.pane_morph.is_some());

    // Mid-motion: two placements, and a projection that is not the identity.
    let blits = render.sample_pane_layout(frame_at(origin, 50));
    assert_eq!(blits.len(), 2);
    assert!(render.compositor.pane_morph.is_some(), "still travelling");

    // The last frame still draws the panes — at their destination — and only
    // then is the morph dropped, so nothing is left to re-arm on the next pass.
    let blits = render.sample_pane_layout(frame_at(origin, 100));
    assert_eq!(blits.len(), 2);
    assert!(render.compositor.pane_morph.is_none(), "settled");

    assert!(
        render.sample_pane_layout(frame_at(origin, 150)).is_empty(),
        "a settled frame composes the ordinary way, with no offscreen"
    );
}

#[test]
fn a_disabled_policy_installs_no_morph_at_all() {
    // Not merely a faster path: with no morph there is no offscreen, no
    // per-pane blit, and the pass composes exactly as it did before the
    // feature existed.
    let mut render = empty_render();
    render.compositor.pane_motion = neomacs_display_protocol::motion_spec::MotionSpec::Instant;
    let origin = origin();
    install(
        &mut render,
        &[window(1, rect(0.0, 0.0, 800.0, 600.0))],
        origin,
    );
    install(
        &mut render,
        &[window(1, rect(0.0, 0.0, 400.0, 600.0))],
        origin,
    );
    assert!(render.compositor.pane_morph.is_none());
    assert!(render.sample_pane_layout(frame_at(origin, 0)).is_empty());
}

#[test]
fn the_settled_projection_replaces_the_morphs_on_the_last_frame() {
    // The last frame of a motion and the first frame after it must resolve a
    // click to the same place; a one-pixel shift exactly as the motion ended
    // would be maddening to diagnose from a bug report.
    let mut render = empty_render();
    render.compositor.pane_motion = linear_100ms();
    let origin = origin();
    install(
        &mut render,
        &[window(1, rect(0.0, 0.0, 800.0, 600.0))],
        origin,
    );
    install(
        &mut render,
        &[window(1, rect(400.0, 0.0, 400.0, 600.0))],
        origin,
    );

    let surface = neomacs_display_protocol::GeometryPoint::<
        neomacs_display_protocol::RootSurfaceSpace,
        neomacs_display_protocol::LogicalPixels,
    >::from_px(500.0, 10.0)
    .expect("a finite point");

    render.sample_pane_layout(frame_at(origin, 100));
    let after_last_frame = render
        .compositor
        .interaction
        .as_ref()
        .expect("a projection")
        .map(surface)
        .expect("a mapped point");
    assert!((after_last_frame.x() - 500.0).abs() < 1e-3);
}
