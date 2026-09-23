use crate::core::frame_glyphs::{CursorStyle, DisplaySlotId, FrameGlyphBuffer, WindowCursor};
use crate::render_thread::state::{PointerAppearanceState, PresentedAppearanceKey};
use neomacs_display_protocol::frame_chrome::PresentationId;
use neomacs_display_protocol::{
    Color, DisplayWindowId, EffectsConfig, FrameRate, PointerAppearanceId,
};

fn filled_box_frame(effects: EffectsConfig) -> (FrameGlyphBuffer, DisplayWindowId) {
    let window_id = DisplayWindowId::new(1);
    let mut frame = FrameGlyphBuffer::with_size(20.0, 20.0);
    frame.window_cursors.push(WindowCursor {
        window_id,
        slot_id: DisplaySlotId::ZERO,
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 0.0,
        active: true,
    });
    frame.set_window_cursor_effects(window_id, effects);
    (frame, window_id)
}

// If this claim is false, a hovered or pressed pointer appearance is composited
// from a retained scene captured before the appearance changed: the cell under
// the pointer keeps its old look until the next scene commit, and the cursor
// cells retained for that scene redraw the stale appearance over the composite.
#[test]
fn a_hovered_or_pressed_pointer_appearance_forces_the_full_render_and_retains_no_cursor_cells() {
    let mut pointer = PointerAppearanceState::default();
    let mut frame = FrameGlyphBuffer::with_size(20.0, 20.0);
    frame.window_cursors.push(WindowCursor {
        window_id: DisplayWindowId::new(1),
        slot_id: DisplaySlotId::ZERO,
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        style: CursorStyle::FilledBox,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 0.0,
        active: true,
    });
    assert!(super::pointer_appearance_allowed(&pointer));
    assert_eq!(
        super::build_filled_box_cursor_cells(&frame, 1.0, &pointer).len(),
        1
    );

    let key = PresentedAppearanceKey::new(
        PresentationId::new(7),
        PointerAppearanceId::try_from(0usize).unwrap(),
    );
    pointer.hover(Some(key));
    assert!(!super::pointer_appearance_allowed(&pointer));
    assert!(super::build_filled_box_cursor_cells(&frame, 1.0, &pointer).is_empty());

    pointer.press();
    assert!(!super::pointer_appearance_allowed(&pointer));
    assert!(super::build_filled_box_cursor_cells(&frame, 1.0, &pointer).is_empty());
}

// If this claim is false, a window with its own cursor-effect profile loses it
// the moment the frame takes the retained-static path: the mini-frame falls back
// to the global profile, so the cursor changes colour-cycle behaviour depending
// on which composition strategy the frame happened to pick.
#[test]
fn a_retained_filled_box_cursor_cell_resolves_the_same_local_effect_profile_as_a_full_render() {
    let pointer = PointerAppearanceState::default();

    for local_enabled in [false, true] {
        let mut local = EffectsConfig::cursor_profile_baseline();
        local.cursor_color_cycle.enabled = local_enabled;
        local.cursor_color_cycle.fps = FrameRate::new(12).unwrap();
        let (frame, window_id) = filled_box_frame(local.clone());

        let mut global = EffectsConfig::default();
        global.cursor_color_cycle.enabled = !local_enabled;
        global.cursor_color_cycle.fps = FrameRate::new(60).unwrap();

        let cells = super::build_filled_box_cursor_cells(&frame, 1.0, &pointer);
        assert_eq!(cells.len(), 1);
        assert_eq!(
            cells[0]
                .mini
                .effective_window_cursor_effects(window_id, &global),
            &local,
            "the retained cursor-only path must resolve the same local profile as the full renderer"
        );
    }
}
