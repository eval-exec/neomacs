use super::*;
use crate::render_thread::render_quality::WindowAnimationSpecs;
use neomacs_display_protocol::motion_spec::MotionSpec;
use neomacs_display_protocol::motion_spec::{MotionDuration, TweenSpec};
use neomacs_display_protocol::presentation_origin::BufferModiff;
use neomacs_display_protocol::scroll_animation::TransitionEasing;
use neomacs_display_protocol::types::{DisplayWindowId, Rect};
use std::time::Duration;

fn origin() -> EventTime {
    neomacs_display_protocol::frame_time::observe_platform_now()
}

fn frame_at(origin: EventTime, millis: u64) -> FrameSample {
    FrameSample::new(origin.plus(Duration::from_millis(millis)), Duration::ZERO)
}

/// Every role on one 100ms linear curve.
///
/// The tests below are about the morph's mechanics, not about the four slots
/// having different shapes, so giving each role the same curve keeps their
/// arithmetic readable: a sample's progress is the elapsed fraction.
fn tween_100ms() -> WindowAnimationSpecs {
    let tween = MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::new(Duration::from_millis(100)).expect("a positive duration"),
        easing: TransitionEasing::Linear,
        bezier: None,
    });
    WindowAnimationSpecs {
        resize: tween,
        movement: tween,
        open: tween,
        close: tween,
    }
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

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

fn presentation() -> PresentationId {
    PresentationId::new(1)
}

/// Deliver a commit to `driver`.
fn commit(
    driver: LayoutDriver,
    previous: &[WindowInfo],
    next: &[WindowInfo],
    at: EventTime,
) -> LayoutDriver {
    driver.on_commit(LayoutDelta { previous, next }, tween_100ms(), at)
}

fn one_pane() -> Vec<WindowInfo> {
    vec![window(1, rect(0.0, 0.0, 800.0, 600.0))]
}

fn two_panes() -> Vec<WindowInfo> {
    vec![
        window(1, rect(0.0, 0.0, 400.0, 600.0)),
        window(2, rect(400.0, 0.0, 400.0, 600.0)),
    ]
}

// =======================================================================
// Settled + a commit
// =======================================================================

#[test]
fn a_commit_that_rearranges_the_panes_starts_a_motion() {
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin());
    assert!(driver.wants_frames());
}

#[test]
fn a_commit_that_moves_nothing_leaves_the_compositor_settled() {
    // Almost every commit is this one — a keystroke, an echo-area clear, a
    // mode-line clock tick. Starting a motion for each would animate the frame
    // continuously while nothing on it had moved.
    let driver = commit(LayoutDriver::Settled, &one_pane(), &one_pane(), origin());
    assert!(!driver.wants_frames());
}

// =======================================================================
// Animating + a commit — the two cells that were bugs
// =======================================================================

#[test]
fn a_commit_that_moves_nothing_does_not_cancel_a_motion_in_flight() {
    // The first of the two bugs this state machine exists to make
    // unrepresentable: assigning the "no morph" answer through cancelled
    // whatever was running, so the panes snapped to their destination on the
    // first keypress after a split.
    let origin = origin();
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin);
    let driver = commit(
        driver,
        &two_panes(),
        &two_panes(),
        origin.plus(Duration::from_millis(10)),
    );
    assert!(driver.wants_frames(), "still travelling");
}

#[test]
fn a_commit_that_moves_nothing_does_not_restart_a_motion_in_flight() {
    // The second bug, and subtler: a retarget restarts the motion from the
    // current instant, so retargeting on every commit pinned progress near zero
    // and the panes crawled — the animation looked like it never happened, and
    // then the layout arrived. Verified by progress, not by state: a restarted
    // motion is still `Animating`.
    let origin = origin();
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin);

    // Commits keep arriving while the motion runs, none of them moving a pane.
    let mut driver = driver;
    for ms in [10, 20, 30, 40, 50] {
        driver = commit(
            driver,
            &two_panes(),
            &two_panes(),
            origin.plus(Duration::from_millis(ms)),
        );
    }

    // Halfway through a 100ms tween the pane must be halfway, not back at the
    // start. 800 -> 400 means it still covers 600 at the midpoint.
    let (_, composition) = driver.on_frame(presentation(), frame_at(origin, 50));

    // Measured from the vacated strip, not from the pane's own quad. A blit
    // carries the rect it *paints*, and a shrinking pane paints its
    // destination size (400) from the first frame onward — the travel shows up
    // as the strip beside it, which is exactly the area it has not yet given
    // up. So the strip's width is what says how far along the motion is, and a
    // restart would collapse it toward zero.
    // A shrinking pane contributes two `Previous` placements: the reflow ghost,
    // which starts at the pane's own origin, and the strip, which starts where
    // the pane will end. Only the second one travels.
    let strip = composition
        .blits
        .iter()
        .filter(|blit| blit.source == neomacs_renderer_wgpu::PaneSource::Previous)
        .find(|blit| blit.bounds.x > 0.0)
        .expect("a shrinking pane leaves a strip while it travels");
    let travelling_edge = strip.bounds.x + strip.bounds.width;
    assert!(
        (travelling_edge - 600.0).abs() < 5.0,
        "the motion was restarted by commits that moved nothing: the pane still \
         reaches {travelling_edge} at the midpoint, expected 600"
    );
}

#[test]
fn a_commit_that_wants_the_panes_elsewhere_retargets() {
    let origin = origin();
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin);
    let elsewhere = vec![
        window(1, rect(0.0, 0.0, 200.0, 600.0)),
        window(2, rect(200.0, 0.0, 600.0, 600.0)),
    ];
    let driver = commit(
        driver,
        &two_panes(),
        &elsewhere,
        origin.plus(Duration::from_millis(50)),
    );
    let (_, composition) = driver.on_frame(presentation(), frame_at(origin, 150));
    assert!(
        !composition.blits.is_empty(),
        "the retarget is still carrying the panes to the new destination"
    );
}

// =======================================================================
// A frame
// =======================================================================

#[test]
fn a_settled_compositor_places_nothing_and_names_no_transform() {
    let (driver, composition) =
        LayoutDriver::Settled.on_frame(presentation(), frame_at(origin(), 0));
    assert!(!driver.wants_frames());
    assert!(composition.blits.is_empty());
    assert!(composition.projection.is_none());
}

#[test]
fn the_last_frame_of_a_motion_still_places_the_panes_and_then_settles() {
    // Settling before drawing would leave the final frame composed from the
    // previous placement, one frame short of the destination.
    let origin = origin();
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin);
    let (driver, composition) = driver.on_frame(presentation(), frame_at(origin, 100));
    assert!(
        !composition.blits.is_empty(),
        "the destination frame is drawn"
    );
    assert!(composition.projection.is_some());
    assert!(!driver.wants_frames(), "and then it is over");
}

#[test]
fn a_frame_with_no_presentation_settles_rather_than_animating_nothing() {
    // There is no presentation to name in a projection, so there is nothing
    // this frame could correctly place.
    let origin = origin();
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin);
    assert!(driver.wants_frames());
    // The compositor-level adapter settles the driver when it has no frame;
    // this asserts the driver itself is safe to ask.
    let (_, composition) = driver.on_frame(presentation(), frame_at(origin, 50));
    assert!(composition.projection.is_some());
}

// =======================================================================
// Frame demand
// =======================================================================

#[test]
fn only_an_animating_driver_asks_for_frames() {
    // The frame coordinator schedules on standing demands. A morph's state
    // lives here rather than in a renderer effect list, so it was invisible to
    // every demand and advanced only on frames some other activity happened to
    // schedule — 400ms of motion in the middle of a 3s animation. Asking the
    // driver is what keeps a future animating state from repeating that.
    assert!(!LayoutDriver::Settled.wants_frames());
    let driver = commit(LayoutDriver::Settled, &one_pane(), &two_panes(), origin());
    assert!(driver.wants_frames());
}

#[test]
fn a_retarget_onto_where_the_panes_already_are_settles_but_still_names_the_transform() {
    // This composition places nothing, so the render pass takes its
    // compositor-only fast path — and that path used to return no projection at
    // all, leaving hit testing on the morph's last mid-motion transform for
    // every frame afterwards. Reachable only through a real retarget: one pane
    // is travelling 800 -> 400 and, at the moment it happens to be 600 wide, a
    // commit asks for 600.
    let origin = origin();
    let narrower = vec![window(1, rect(0.0, 0.0, 400.0, 600.0))];
    let driver = commit(LayoutDriver::Settled, &one_pane(), &narrower, origin);

    let already_there = vec![window(1, rect(0.0, 0.0, 600.0, 600.0))];
    let driver = commit(
        driver,
        &narrower,
        &already_there,
        origin.plus(Duration::from_millis(50)),
    );

    let (driver, composition) = driver.on_frame(presentation(), frame_at(origin, 50));
    assert!(
        composition.blits.is_empty(),
        "the pane is already where the new layout wants it"
    );
    assert!(
        composition.projection.is_some(),
        "and yet there is a transform the next hit test must use"
    );
    assert!(!driver.wants_frames(), "with nothing left to animate");
}
