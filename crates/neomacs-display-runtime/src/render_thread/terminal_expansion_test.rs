use super::*;
use crate::core::frame_glyphs::{DisplaySlotId, GlyphRowRole};
use crate::core::types::{Color, DisplayWindowId, Px};

fn generated_char(face_id: FaceId, x: f32) -> FrameGlyph {
    let window_id = DisplayWindowId::new(1);
    FrameGlyph::Char {
        window_id,
        row_role: GlyphRowRole::Text,
        clip_rect: None,
        slot_id: DisplaySlotId::from_pixels(window_id, Px(x), Px(0.0), Px(8.0), Px(16.0)),
        bidi_level: 0,
        char: 'x',
        composed: None,
        x,
        y: 0.0,
        baseline: 12.0,
        width: 8.0,
        height: 16.0,
        ascent: 12.0,
        face_id,
        box_vertical_edges: Default::default(),
    }
}

#[test]
fn merge_remaps_a_conflicting_generated_face_and_its_glyphs_together() {
    let original_id = FaceId::new(TERMINAL_FACE_ID_BASE | 7);
    let mut opaque = Face::new(original_id);
    opaque.foreground = Color::WHITE;
    let mut translucent = Face::new(original_id);
    translucent.foreground = Color::new(1.0, 1.0, 1.0, 0.5);
    let mut expansion = TerminalExpansion::new(
        vec![generated_char(original_id, 0.0)],
        HashMap::from([(original_id, opaque)]),
    );

    expansion.merge(TerminalExpansion::new(
        vec![generated_char(original_id, 8.0)],
        HashMap::from([(original_id, translucent)]),
    ));

    let face_ids: Vec<_> = expansion
        .glyphs()
        .iter()
        .filter_map(FrameGlyph::face_id)
        .collect();
    assert_eq!(face_ids.len(), 2);
    assert_ne!(face_ids[0], face_ids[1]);
    assert_eq!(expansion.faces()[&face_ids[0]].foreground.a, 1.0);
    assert_eq!(expansion.faces()[&face_ids[1]].foreground.a, 0.5);
}
