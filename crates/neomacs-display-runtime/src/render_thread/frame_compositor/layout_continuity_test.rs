use super::*;

use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::motion_spec::{MotionDuration, MotionSpec, TweenSpec};
use neomacs_display_protocol::presentation_origin::{
    BufferModiff, InteractionSessionId, PresentationOrigin,
};
use neomacs_display_protocol::scroll_animation::TransitionEasing;
use neomacs_display_protocol::types::{Color, DisplayWindowId, Rect};
use std::time::Duration;

use crate::render_thread::frame_compositor::MeasurementBaseline;

fn now() -> EventTime {
    neomacs_display_protocol::frame_time::observe_platform_now()
}

/// Pane motion is off by default (`PaneMotionConfig::default().enabled` is
/// false) and forced to `Instant` outside full quality, and `try_new` builds no
/// morph for `Instant`. Every test here therefore enables it explicitly, or it
/// would observe "no morph" for a reason that has nothing to do with the origin.
fn tween() -> MotionSpec {
    MotionSpec::Tween(TweenSpec {
        duration: MotionDuration::new(Duration::from_millis(120)).expect("a positive duration"),
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

/// A frame whose single window occupies `bounds`, produced by `origin`.
fn frame_of(presentation: u64, bounds: Rect, origin: PresentationOrigin) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::new();
    frame.presentation_id =
        neomacs_display_protocol::frame_chrome::PresentationId::new(presentation);
    frame.origin = origin;
    frame.background = Color::BLACK;
    frame.window_infos.push(window(1, bounds));
    frame
}

fn during_a_drag() -> PresentationOrigin {
    PresentationOrigin::InteractiveResize {
        session: InteractionSessionId::FIRST,
    }
}

/// A compositor showing one window 200px tall, ready to be measured against.
fn showing_a_window(render: &mut GuiFrameRenderState) {
    render.compositor.pane_motion = tween();
    render.compositor.baseline = Some(MeasurementBaseline {
        presentation: neomacs_display_protocol::frame_chrome::PresentationId::new(1),
        window_infos: vec![window(1, Rect::new(0.0, 0.0, 400.0, 200.0))],
        background: Color::BLACK,
    });
}

fn render_state() -> GuiFrameRenderState {
    GuiFrameRenderState::new_without_device(0x42, false, now())
}

/// The window is 100px shorter than the baseline's, so its pane moved and its
/// viewport shows different text — the two facts a mode-line drag produces.
fn resized(origin: PresentationOrigin) -> FrameGlyphBuffer {
    let mut frame = frame_of(2, Rect::new(0.0, 0.0, 400.0, 100.0), origin);
    frame.window_infos[0].window_start = 400;
    frame
}

fn install(render: &mut GuiFrameRenderState, frame: FrameGlyphBuffer) {
    render.set_current_frame(Some(frame), None, Default::default(), Default::default());
}

#[test]
fn an_ordinary_presentation_that_rearranges_the_panes_builds_a_morph() {
    // The contrast case. Without it every assertion below passes for the wrong
    // reason: pane motion is disabled in a default build, so "no morph" is the
    // answer whatever the origin says.
    let mut render = render_state();
    showing_a_window(&mut render);
    install(&mut render, resized(PresentationOrigin::Ordinary));
    assert!(
        render.compositor.layout.wants_frames(),
        "an ordinary layout change is something to travel to"
    );
}

#[test]
fn a_presentation_composed_during_a_drag_builds_no_pane_morph() {
    // The step's whole purpose: the divider must arrive where the hand put it,
    // not spring toward it a frame later and get overtaken by the next commit.
    let mut render = render_state();
    showing_a_window(&mut render);
    install(&mut render, resized(during_a_drag()));
    assert!(!render.compositor.layout.wants_frames());
}

#[test]
fn a_presentation_composed_during_a_drag_arms_no_scroll_reflow_or_shown_text() {
    // A mode-line drag re-wraps the window it shrinks, so a gate that covered
    // only pane motion would still animate the text sliding inside the pane
    // while the pane itself snapped.
    let mut render = render_state();
    showing_a_window(&mut render);
    install(&mut render, resized(during_a_drag()));
    assert!(render.compositor.pending.scrolls.is_empty());
    assert!(render.compositor.pending.shown_text_replaced.is_empty());
    assert!(render.compositor.pending.reflows.is_empty());
}

#[test]
fn a_drag_discards_layout_motion_an_earlier_install_had_already_armed() {
    // Observations survive an install that no frame drew, and a pane morph
    // survives across installs by design. Skipping the measurement without
    // clearing would animate the drag with the previous commit's motion.
    let mut render = render_state();
    showing_a_window(&mut render);
    install(&mut render, resized(PresentationOrigin::Ordinary));
    assert!(render.compositor.layout.wants_frames());
    assert!(!render.compositor.pending.shown_text_replaced.is_empty());

    install(&mut render, resized(during_a_drag()));
    assert!(!render.compositor.layout.wants_frames());
    assert!(render.compositor.pending.shown_text_replaced.is_empty());
}

#[test]
fn a_theme_change_committed_during_a_drag_is_still_observed() {
    // `suppresses_layout_motion` is about layout. Dropping the cross-fade too
    // would mean a theme changed mid-drag appears with no transition at all.
    let mut render = render_state();
    showing_a_window(&mut render);
    let mut frame = resized(during_a_drag());
    frame.background = Color::WHITE;
    install(&mut render, frame);
    assert!(render.compositor.pending.theme.is_some());
}

#[test]
fn an_ordinary_presentation_arriving_after_a_drag_animates_again() {
    // Suppression is carried on the presentation, so it lifts when the
    // producer stops stamping it — with no end-of-session message that could
    // arrive out of order with the drag's final frames.
    let mut render = render_state();
    showing_a_window(&mut render);
    install(&mut render, resized(during_a_drag()));
    assert!(!render.compositor.layout.wants_frames());

    let mut after = frame_of(
        3,
        Rect::new(0.0, 0.0, 400.0, 300.0),
        PresentationOrigin::Ordinary,
    );
    after.window_infos[0].window_start = 700;
    install(&mut render, after);
    assert!(
        render.compositor.layout.wants_frames(),
        "the drag ended, so the next layout change travels"
    );
}
