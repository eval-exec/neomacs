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

/// The placement of a window that is leaving, which reads the previous picture.
fn departing(sample: &LayoutSample, id: i64) -> PanePlacement {
    *sample
        .panes
        .iter()
        .find(|placement| {
            placement.window == live(id)
                && placement.source == neomacs_renderer_wgpu::PaneSource::Previous
        })
        .expect("the window was placed as departing")
}

/// The destination-sourced placement of `window` in `sample`.
///
/// A resizing pane also contributes a fading ghost of its old wrapping; that
/// one is a picture on its way out rather than "where the pane is", so it is
/// `all_placed` that sees it.
fn placed(sample: &LayoutSample, id: i64) -> PanePlacement {
    *sample
        .panes
        .iter()
        .find(|placement| {
            placement.window == live(id)
                && placement.source == neomacs_renderer_wgpu::PaneSource::Destination
        })
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
fn an_overshooting_pane_never_gets_a_negative_extent_or_a_strip_it_is_not_owed() {
    // Overshoot is not hypothetical and does not need a spring: a motion that
    // is interrupted and resumed carries its entry rate in, and the tween bump
    // peaks around progress 1.3. Two things break past 1.0, and neither is
    // visible at rest, so both are asserted at an explicit sample.
    let motion = MotionSample {
        progress: 1.3,
        content_mix: neomacs_display_protocol::motion_spec::UnitInterval::clamp(1.0),
        rate: 0.0,
        finished: false,
    };

    // A pane that GROWS must never emit a vacated strip. The guard used to read
    // the instantaneous width, so past 1.0 a growing pane briefly measured
    // wider than its destination and published an opaque slab of stale
    // pre-change pixels -- drawn last, over the neighbour it had just uncovered.
    let mut grew = Vec::new();
    place(
        PaneChange::Persisted {
            window: live(1),
            from: rect(0.0, 0.0, 400.0, 600.0),
            to: rect(0.0, 0.0, 800.0, 600.0),
        },
        motion,
        &[],
        &mut grew,
    );
    // Its reflow ghost is legitimate -- the pane's width IS changing, so its
    // old wrapping crossfades over the area it keeps. What must not exist is
    // old picture drawn PAST the destination, which is what a vacated strip is
    // and what a growing pane never earns.
    for placement in &grew {
        if placement.source != neomacs_renderer_wgpu::PaneSource::Previous {
            continue;
        }
        assert!(
            placement.bounds.x + placement.bounds.width <= 800.0 + 1e-3,
            "a growing pane was handed a vacated strip: {:?}",
            placement.bounds
        );
    }

    // A pane collapsing to near nothing must not invert. A negative extent is
    // rejected by `GeometryRect::new`, which drops the pane from the projection
    // and silently falls the hit test back to identity -- the exact
    // render/hit-test divergence the projection exists to prevent.
    let mut shrank = Vec::new();
    place(
        PaneChange::Persisted {
            window: live(1),
            from: rect(0.0, 0.0, 600.0, 600.0),
            to: rect(0.0, 0.0, 20.0, 600.0),
        },
        motion,
        &[],
        &mut shrank,
    );
    for placement in &shrank {
        assert!(
            placement.bounds.width >= 0.0 && placement.bounds.height >= 0.0,
            "inverted rect {:?}",
            placement.bounds
        );
        assert!(
            placement.painted.width >= 0.0 && placement.painted.height >= 0.0,
            "inverted painted rect {:?}",
            placement.painted
        );
    }
}

#[test]
fn a_click_on_the_area_a_pane_has_not_given_up_does_not_resolve_off_the_frame() {
    // The pane draws only what it owns: `layout_pass` clamps its quad to
    // its own rect, so a shrinking pane paints its destination content at
    // its destination size and the vacated strip beside it shows the old
    // picture. The projection was built from the *unclamped* rect, so it
    // claimed the strip too -- and translated a click there by the pane's
    // whole travel, off the end of the frame.
    //
    // 800x600 at x=0 becoming 400x600 at x=400: a pane that shrinks *and*
    // moves, which is the only shape that separates the two rects. Halfway,
    // the pane draws [200, 600) and the strip covers [600, 800).
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let origin = origin();
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let sample = morph.sample(frame_at(origin, 50));
    let projection = sample.projection(neomacs_display_protocol::PresentationId::new(3));

    let surface = neomacs_display_protocol::GeometryPoint::<
        neomacs_display_protocol::RootSurfaceSpace,
        neomacs_display_protocol::LogicalPixels,
    >::from_px(700.0, 10.0)
    .expect("a finite point");
    let mapped = projection.map(surface).expect("a mapped point");
    assert!(
        mapped.x() <= 800.0,
        "a click at 700 on an 800px frame resolved to {}",
        mapped.x()
    );
    // It is over the old picture, which names no position in the destination,
    // so the destination's own layout answers: identity.
    assert!(
        (mapped.x() - 700.0).abs() < 1e-3,
        "resolved to {}",
        mapped.x()
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
        !render.compositor.layout.wants_frames(),
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
    assert!(render.compositor.layout.wants_frames());

    // Mid-motion: two placements, and a projection that is not the identity.
    let blits = render
        .sample_pane_layout(
            &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
            frame_at(origin, 50),
        )
        .blits;
    // Four, not two: window 1 narrows from 800 to 400, so alongside the two
    // destination panes it contributes its old wrapping (crossfading) and the
    // area it has not yet vacated (held opaque).
    assert_eq!(blits.len(), 4);
    assert!(render.compositor.layout.wants_frames(), "still travelling");

    // The last frame still draws the panes — at their destination — and only
    // then is the morph dropped, so nothing is left to re-arm on the next pass.
    let blits = render
        .sample_pane_layout(
            &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
            frame_at(origin, 100),
        )
        .blits;
    assert_eq!(blits.len(), 3);
    assert!(!render.compositor.layout.wants_frames(), "settled");

    assert!(
        render
            .sample_pane_layout(
                &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
                frame_at(origin, 150)
            )
            .blits
            .is_empty(),
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
    assert!(!render.compositor.layout.wants_frames());
    assert!(
        render
            .sample_pane_layout(
                &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
                frame_at(origin, 0)
            )
            .blits
            .is_empty()
    );
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

    let composition = render.sample_pane_layout(
        &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
        frame_at(origin, 100),
    );
    render.publish_presented_projection(composition.projection);
    let after_last_frame = render
        .compositor
        .interaction
        .as_ref()
        .expect("a projection")
        .map(surface)
        .expect("a mapped point");
    assert!((after_last_frame.x() - 500.0).abs() < 1e-3);
}

// =======================================================================
// Interruption: a layout change arriving mid-motion
// =======================================================================

#[test]
fn a_layout_arriving_mid_motion_carries_the_panes_on_from_where_they_are() {
    // The defect this replaces: the morph was rebuilt from the *committed*
    // layout, which is the destination the old motion was still travelling
    // toward. Every pane snapped forward to a rect it had not reached, then
    // animated away from it — a jump in the wrong direction, mid-animation.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let mut morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");

    // Halfway: the pane is 600 wide.
    let midpoint = placed(&morph.sample(frame_at(origin, 50)), 1).bounds.width;
    assert!((midpoint - 600.0).abs() < 1e-3);

    // A second layout change arrives, wanting 200 wide.
    let retargeted = [window(1, rect(0.0, 0.0, 200.0, 600.0))];
    morph.retarget(
        &retargeted,
        linear_100ms(),
        origin.plus(Duration::from_millis(50)),
    );

    let spliced = morph
        .spliced(frame_at(origin, 50))
        .expect("the retarget still has ground to cover");
    let resumed = placed(&spliced.sample(frame_at(origin, 50)), 1)
        .bounds
        .width;
    assert!(
        (resumed - midpoint).abs() < 1e-3,
        "the spliced motion must start at the width the panes actually had \
         ({midpoint}), not at the committed 400: got {resumed}"
    );
}

#[test]
fn a_spliced_motion_does_not_stall_at_the_splice() {
    // Restarting from rest is visible as a hitch: the panes are travelling,
    // then stop dead for a frame, then accelerate again. The entry rate is
    // what carries the speed across.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let mut morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let at_splice = morph.sample(frame_at(origin, 50));
    assert!(
        at_splice.motion.rate > 0.0,
        "the motion is moving before it is cut"
    );

    morph.retarget(
        &[window(1, rect(0.0, 0.0, 200.0, 600.0))],
        linear_100ms(),
        origin.plus(Duration::from_millis(50)),
    );
    let spliced = morph
        .spliced(frame_at(origin, 50))
        .expect("a spliced morph");
    let just_after = spliced.sample(frame_at(origin, 51));
    assert!(
        just_after.motion.rate > 0.0,
        "the spliced motion left the splice already moving, rather than from rest"
    );
}

#[test]
fn a_retarget_to_where_the_panes_already_are_ends_the_motion() {
    // Not every interruption has ground left to cover. A layout that matches
    // where the panes have arrived should finish, not start a zero-length
    // motion that still asks for frames.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let mut morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    // Let it arrive, then retarget to the destination it already reached.
    morph.retarget(
        &after,
        linear_100ms(),
        origin.plus(Duration::from_millis(100)),
    );
    assert!(morph.has_pending_retarget());
    assert!(
        morph.spliced(frame_at(origin, 100)).is_none(),
        "nothing left to animate"
    );
}

#[test]
fn a_window_the_new_layout_drops_keeps_the_position_it_had_reached() {
    // Its record must not come from a presentation two commits old, or step 8
    // will fade it out from somewhere it never was.
    let origin = origin();
    let before = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let after = [
        window(1, rect(0.0, 0.0, 200.0, 600.0)),
        window(2, rect(200.0, 0.0, 600.0, 600.0)),
    ];
    let mut morph =
        PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let travelled = placed(&morph.sample(frame_at(origin, 50)), 2).bounds;

    morph.retarget(
        &[window(1, rect(0.0, 0.0, 800.0, 600.0))],
        linear_100ms(),
        origin.plus(Duration::from_millis(50)),
    );
    let spliced = morph
        .spliced(frame_at(origin, 50))
        .expect("a spliced morph");
    let exited = spliced
        .changes()
        .find(|change| matches!(change, PaneChange::Exited { window, .. } if *window == live(2)))
        .expect("window 2 is recorded as leaving");
    assert!(
        matches!(exited, PaneChange::Exited { from, .. } if (from.x - travelled.x).abs() < 1e-3),
        "the leaving pane is recorded where it had travelled to, not where it started"
    );
}

#[test]
fn a_sampled_projection_is_not_visible_to_a_hit_test_until_it_has_been_presented() {
    // Sampling places the panes for a frame that may never reach the screen:
    // the render pass can still lose the surface, or abandon the frame for
    // want of content, after this point. Publishing at sample time would leave
    // hit testing answering about pixels nobody saw, which is the one thing
    // the projection exists to prevent.
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

    let composition = render.sample_pane_layout(
        &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
        frame_at(origin, 50),
    );
    assert!(!composition.blits.is_empty(), "the panes are in motion");
    assert!(
        composition.projection.is_some(),
        "the sample computed one, it just has not been published"
    );
    assert!(
        render.compositor.interaction.is_none(),
        "nothing was presented, so no projection is in force"
    );

    render.publish_presented_projection(composition.projection);
    assert!(
        render.compositor.interaction.is_some(),
        "presenting the frame is what puts its projection in force"
    );
}

// =======================================================================
// Entering and leaving
// =======================================================================

#[test]
fn a_leaving_pane_reads_the_previous_composition_because_the_new_one_has_none_of_it() {
    // Its window is absent from the destination presentation entirely. Reading
    // the composed picture at its old rect would blit whatever replaced it,
    // wearing the departing pane's geometry.
    let origin = origin();
    let before = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let after = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let leaving = departing(&morph.sample(frame_at(origin, 50)), 2);
    assert_eq!(leaving.source, neomacs_renderer_wgpu::PaneSource::Previous);
    // Trimmed to the ground window 1 has not reached. Halfway through, window 1
    // spans [0, 600), so the only part of the frame still showing the deleted
    // window is [600, 800).
    //
    // It is not "held at the rect it had": a departing pane draws the old
    // picture opaquely, and `Previous` placements draw *over* the destination,
    // so an untrimmed one covers the very pane taking its place -- the
    // survivor's arrival stays invisible until the last frame and then pops.
    assert_eq!(leaving.bounds, rect(600.0, 0.0, 200.0, 600.0));
    assert_eq!(
        leaving.content_origin,
        (600.0, 0.0),
        "and reads the old picture from the columns it still covers"
    );
}

#[test]
fn an_entering_pane_fades_in_and_a_leaving_one_is_uncovered_rather_than_faded() {
    // An entering pane fades: appearing at full opacity is indistinguishable
    // from the frame simply being redrawn, which is the jump the morph exists
    // to remove.
    let origin = origin();
    let split_before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let split_after = [
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ];
    let entering = PaneLayoutMorph::try_new(&split_before, &split_after, linear_100ms(), origin)
        .expect("a morph");
    let early = placed(&entering.sample(frame_at(origin, 10)), 2).opacity;
    let late = placed(&entering.sample(frame_at(origin, 90)), 2).opacity;
    assert!(
        early < late,
        "an entering pane fades in: {early} then {late}"
    );

    // A leaving pane does not fade. It is the old picture, and the old picture
    // is opaque until something takes the ground it stands on -- so it shrinks
    // as its replacement grows and is gone when that replacement arrives.
    //
    // Fading it looks like the same double exposure a crossfaded vacated strip
    // produces: the deleted window and the pane replacing it both half-visible
    // for the length of the motion, over a destination backdrop that already
    // shows the settled layout.
    let delete_after = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let leaving = PaneLayoutMorph::try_new(&split_after, &delete_after, linear_100ms(), origin)
        .expect("a morph");
    let early = departing(&leaving.sample(frame_at(origin, 10)), 2);
    let late = departing(&leaving.sample(frame_at(origin, 90)), 2);
    assert_eq!(early.opacity, 1.0, "opaque throughout");
    assert_eq!(late.opacity, 1.0);
    assert!(
        late.bounds.width < early.bounds.width,
        "and uncovered rather than faded: {} then {}",
        early.bounds.width,
        late.bounds.width
    );
}

#[test]
fn a_persisted_pane_is_always_fully_opaque_and_reads_the_destination() {
    // Only the lifecycle changes are translucent. Fading a pane that merely
    // moved would show the frame through the text it is carrying.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    for ms in [0, 50, 100] {
        let pane = placed(&morph.sample(frame_at(origin, ms)), 1);
        assert_eq!(pane.opacity, 1.0);
        assert_eq!(pane.source, neomacs_renderer_wgpu::PaneSource::Destination);
    }
}

// =======================================================================
// Reflow: a pane whose width changed
// =======================================================================

/// Every placement for `id`, in draw order.
fn all_placed(sample: &LayoutSample, id: i64) -> Vec<PanePlacement> {
    sample
        .panes
        .iter()
        .filter(|placement| placement.window == live(id))
        .copied()
        .collect()
}

#[test]
fn a_pane_whose_width_changed_shows_its_old_wrapping_while_it_is_still_the_old_shape() {
    // The destination picture holds the NEW line breaks. Drawing only that
    // means the text rewraps on the very first frame while the geometry spends
    // the whole motion catching up — the pane is 800px wide showing text
    // wrapped for 400.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");

    let placements = all_placed(&morph.sample(frame_at(origin, 50)), 1);
    assert_eq!(
        placements.len(),
        3,
        "the old wrapping, the area not yet vacated, and the new one"
    );
    assert_eq!(
        placements[0].source,
        neomacs_renderer_wgpu::PaneSource::Previous,
        "the outgoing wrapping is drawn first, under the destination"
    );
    assert_eq!(
        placements[2].source,
        neomacs_renderer_wgpu::PaneSource::Destination
    );
    assert!(
        placements[0].opacity < 1.0 && placements[0].opacity > 0.0,
        "mid-crossfade"
    );
    // The crossfade covers only what the pane keeps. Beyond that it is not
    // rewrapping, it is vacating, and that area is the strip below.
    assert_eq!(placements[0].bounds.width, 400.0);

    // The strip: the area the pane still covers but will not keep, held at
    // full opacity. Fading it would leave the old text and the pane that
    // replaces it both half-visible for the length of the motion, which reads
    // as a dissolve rather than as a divider sweeping across.
    let strip = placements[1];
    assert_eq!(strip.source, neomacs_renderer_wgpu::PaneSource::Previous);
    assert_eq!(strip.opacity, 1.0, "held, not faded");
    assert_eq!(strip.bounds.x, 400.0, "starts where the pane will end");
    assert_eq!(
        strip.bounds.width,
        placements[2].bounds.width - 400.0,
        "and runs to the pane's travelling edge"
    );
    assert_eq!(
        strip.content_origin.0, 400.0,
        "showing the old picture from the columns it covered"
    );
}

#[test]
fn a_pane_that_only_moved_does_not_crossfade_anything() {
    // Its text did not rewrap, so the destination picture is correct for it at
    // every instant. Crossfading would cost a texture and soften the glyphs for
    // the duration, to show two pictures that are the same.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let placements = all_placed(&morph.sample(frame_at(origin, 50)), 1);
    assert_eq!(placements.len(), 1);
    assert_eq!(
        placements[0].source,
        neomacs_renderer_wgpu::PaneSource::Destination
    );
    assert_eq!(placements[0].opacity, 1.0);
}

#[test]
fn the_outgoing_wrapping_is_anchored_where_the_reader_last_saw_it() {
    // Anchoring it at the destination origin instead would slide the old text
    // sideways as it faded, so the reader would watch their lines move twice.
    let origin = origin();
    let before = [window(1, rect(100.0, 0.0, 700.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    for ms in [0, 50, 100] {
        let ghost = all_placed(&morph.sample(frame_at(origin, ms)), 1)[0];
        assert_eq!(
            ghost.content_origin,
            (100.0, 0.0),
            "the old picture is sampled where it was, at every instant"
        );
    }
}

#[test]
fn a_reflow_ghost_never_answers_a_hit_test() {
    // It shows the PREVIOUS presentation's wrapping, so a point inside it does
    // not name a position in the destination at all. Including it would let a
    // click resolve against text on its way off screen.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let sample = morph.sample(frame_at(origin, 50));
    let projection = sample.projection(neomacs_display_protocol::PresentationId::new(3));
    assert_eq!(
        projection.panes().len(),
        1,
        "one pane, one transform — the ghost is not a place you can click"
    );
}

#[test]
fn a_pane_moving_under_a_still_pointer_changes_what_the_pointer_is_over() {
    // The reason hover has to be re-resolved per frame while panes move.
    // Hover is otherwise computed when a pointer event arrives; if the user
    // holds still during a `split-window`, no event fires, and the shader
    // keeps the u/v of a pane position that has since travelled. The
    // projection is what makes that observable: the same surface point maps
    // to a different content position at each instant of the motion.
    let origin = origin();
    let before = [window(1, rect(0.0, 0.0, 800.0, 600.0))];
    let after = [window(1, rect(400.0, 0.0, 400.0, 600.0))];
    let morph = PaneLayoutMorph::try_new(&before, &after, linear_100ms(), origin).expect("a morph");
    let presentation = neomacs_display_protocol::PresentationId::new(5);
    let still = neomacs_display_protocol::GeometryPoint::<
        neomacs_display_protocol::RootSurfaceSpace,
        neomacs_display_protocol::LogicalPixels,
    >::from_px(500.0, 10.0)
    .expect("a finite point");

    let early = morph
        .sample(frame_at(origin, 25))
        .projection(presentation)
        .map(still)
        .expect("mapped")
        .x();
    let late = morph
        .sample(frame_at(origin, 75))
        .projection(presentation)
        .map(still)
        .expect("mapped")
        .x();
    assert!(
        (early - late).abs() > 1.0,
        "an unmoved pointer is over different content as the pane travels \
         ({early} then {late}); resolving hover only on pointer events would \
         report the first answer for the whole motion"
    );
}
