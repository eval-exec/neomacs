use super::*;
use neomacs_display_protocol::presentation_origin::BufferModiff;
use neomacs_display_protocol::types::{Color, DisplayWindowId};

fn frame(background: Color) -> FrameGlyphBuffer {
    let mut frame = FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.background = background;
    frame
}

/// The last composed presentation, as the measurements see it.
fn baseline(background: Color) -> crate::render_thread::frame_compositor::MeasurementBaseline {
    crate::render_thread::frame_compositor::MeasurementBaseline {
        presentation: neomacs_display_protocol::PresentationId::new(1),
        window_infos: Vec::new(),
        background,
    }
}

fn minibuffer_at(y: f32) -> neomacs_display_protocol::frame_glyphs::WindowInfo {
    neomacs_display_protocol::frame_glyphs::WindowInfo {
        window_id: DisplayWindowId::new(9),
        buffer_id: 2,
        window_start: 1,
        window_end: 1,
        buffer_size: 0,
        buffer_modiff: BufferModiff::new(1),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, y, 800.0, 20.0),
        geometry: neomacs_display_protocol::PresentedWindowGeometry::default(),
        line_number_field: None,
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: false,
        is_minibuffer: true,
        char_height: 16.0,
        buffer_name: String::from(" *Minibuf-0*"),
        buffer_file_name: String::new(),
        modified: false,
    }
}

const DARK: Color = Color {
    r: 0.1,
    g: 0.1,
    b: 0.1,
    a: 1.0,
};

#[test]
fn a_background_change_past_the_threshold_is_a_theme_change() {
    let light = Color {
        r: 0.9,
        g: 0.9,
        b: 0.9,
        a: 1.0,
    };
    let change = theme_change(&baseline(DARK), &frame(light)).expect("a theme change");
    assert_eq!(
        change.bounds,
        neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
        "with no minibuffer the whole frame crossfades"
    );
}

#[test]
fn a_sub_threshold_drift_is_not_a_theme_change() {
    // A face recomputation can nudge a channel without the user changing
    // anything; crossfading the frame for that is worse than doing nothing.
    let nudged = Color {
        r: DARK.r + 0.01,
        g: DARK.g,
        b: DARK.b,
        a: 1.0,
    };
    assert_eq!(theme_change(&baseline(DARK), &frame(nudged)), None);
}

#[test]
fn an_alpha_only_change_is_not_a_theme_change() {
    // Frame opacity is a window-manager property a user may animate on its own.
    // Treating it as a theme change would crossfade every time the frame faded.
    let translucent = Color { a: 0.4, ..DARK };
    assert_eq!(theme_change(&baseline(DARK), &frame(translucent)), None);
}

#[test]
fn an_identical_background_is_not_a_theme_change() {
    assert_eq!(theme_change(&baseline(DARK), &frame(DARK)), None);
}

#[test]
fn the_crossfade_stops_above_the_minibuffer() {
    let light = Color {
        r: 0.9,
        g: 0.9,
        b: 0.9,
        a: 1.0,
    };
    let mut next = frame(light);
    next.window_infos.push(minibuffer_at(580.0));
    let change = theme_change(&baseline(DARK), &next).expect("a theme change");
    assert_eq!(
        change.bounds.height, 580.0,
        "the echo area draws from its own state and is not part of the fade"
    );
}

#[test]
fn each_channel_can_trigger_the_change_on_its_own() {
    for shifted in [
        Color { r: 0.5, ..DARK },
        Color { g: 0.5, ..DARK },
        Color { b: 0.5, ..DARK },
    ] {
        assert!(
            theme_change(&baseline(DARK), &frame(shifted)).is_some(),
            "a single channel moving past the threshold is a theme change"
        );
    }
}
