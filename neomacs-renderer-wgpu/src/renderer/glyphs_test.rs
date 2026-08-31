use super::{
    CursorCellAlignment, CursorCellContract, CursorInlineDirection, GlyphCellRect,
    RenderedCharBounds, ResolvedCursorRect, char_overlap, cursor_cell_alignment,
    cursor_glyph_slot_rect, frame_default_glyph_metrics, log_cursor_glyph_alignment,
};
use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::frame_glyphs::{
    CursorStyle, DisplaySlotId, FrameGlyph, FrameGlyphBuffer, GlyphRowRole, WindowCursor,
};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, DisplayWindowId, Rect};

#[test]
fn adjacent_box_paints_merge_across_interleaved_complements() {
    use crate::renderer::frame_pass::{BoxPaintPolicy, BoxSpan, BoxSpanAccumulator};
    use neomacs_display_protocol::types::Rect;

    let face_id = FaceId::new(9);
    let clip = Some(Rect::new(0.0, 0.0, 20.0, 10.0));
    let make = |x, face_id, clip, box_vertical_edges| BoxSpan {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        face_id,
        row_role: GlyphRowRole::Text,
        bg: Some(Color::BLACK),
        clip,
        policy: BoxPaintPolicy::Rounded,
        requires_background_fill: false,
        box_vertical_edges,
    };
    let mut spans = BoxSpanAccumulator::default();
    spans.push(make(0.0, face_id, clip, BoxVerticalEdges::Left));
    spans.push(make(
        0.0,
        FaceId::new(1),
        Some(Rect::new(20.0, 0.0, 10.0, 10.0)),
        BoxVerticalEdges::Both,
    ));
    spans.push(make(10.0, face_id, clip, BoxVerticalEdges::Right));

    assert_eq!(
        spans.group_count(),
        2,
        "one indexed entry per semantic group"
    );
    let spans = spans.finish();
    let alternate = spans.iter().find(|span| span.face_id == face_id).unwrap();
    assert_eq!((alternate.x, alternate.width), (0.0, 20.0));
}

#[test]
fn adjacent_same_face_box_runs_keep_explicit_internal_terminals() {
    use crate::renderer::frame_pass::{BoxPaintPolicy, BoxSpan, BoxSpanAccumulator};

    let face_id = FaceId::new(9);
    let make = |x, edges| BoxSpan {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        face_id,
        row_role: GlyphRowRole::Text,
        bg: Some(Color::BLACK),
        clip: None,
        policy: BoxPaintPolicy::Sharp,
        requires_background_fill: false,
        box_vertical_edges: edges,
    };
    let mut spans = BoxSpanAccumulator::default();
    spans.push(make(0.0, BoxVerticalEdges::Both));
    spans.push(make(10.0, BoxVerticalEdges::Both));

    let spans = spans.finish();
    assert_eq!(spans.len(), 2, "Right|Left is an explicit run boundary");
    assert_eq!(spans[0].box_vertical_edges, BoxVerticalEdges::Both);
    assert_eq!(spans[1].box_vertical_edges, BoxVerticalEdges::Both);
}

#[test]
fn sharp_chrome_keeps_distinct_face_materials_while_edges_supply_continuity() {
    use crate::renderer::frame_pass::{BoxPaintPolicy, BoxSpan, BoxSpanAccumulator};

    let make = |x, face_id, row_role, policy| BoxSpan {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        face_id,
        row_role,
        bg: Some(Color::BLACK),
        clip: None,
        policy,
        requires_background_fill: false,
        box_vertical_edges: if x == 0.0 {
            BoxVerticalEdges::Left
        } else {
            BoxVerticalEdges::Right
        },
    };
    let first = FaceId::new(1);
    let second = FaceId::new(2);

    let mut continuous = BoxSpanAccumulator::default();
    continuous.push(make(
        0.0,
        first,
        GlyphRowRole::ModeLine,
        BoxPaintPolicy::Sharp,
    ));
    continuous.push(make(
        10.0,
        second,
        GlyphRowRole::ModeLine,
        BoxPaintPolicy::Sharp,
    ));
    let continuous = continuous.finish();
    assert_eq!(continuous.len(), 2);
    assert_eq!(continuous[0].box_vertical_edges, BoxVerticalEdges::Left);
    assert_eq!(continuous[1].box_vertical_edges, BoxVerticalEdges::Right);

    for policy in [BoxPaintPolicy::Rounded, BoxPaintPolicy::Sharp] {
        let mut separate = BoxSpanAccumulator::default();
        separate.push(make(0.0, first, GlyphRowRole::ModeLine, policy));
        separate.push(make(10.0, second, GlyphRowRole::ModeLine, policy));
        assert_eq!(separate.finish().len(), 2);
    }
}

#[test]
fn rounded_box_background_suppression_matches_exact_face_paint() {
    use crate::renderer::frame_pass::{BoxPaintPolicy, BoxSpan};
    use neomacs_display_protocol::face::{BoxType, Face};
    use neomacs_display_protocol::types::Rect;
    use std::collections::HashMap;

    let face_id = FaceId::new(7);
    let mut face = Face::new(face_id);
    face.box_type = BoxType::Line;
    face.box_line_width = 1.into();
    face.box_corner_radius = 4;
    let faces = HashMap::from([(face_id, face)]);
    let spans = [BoxSpan {
        x: 0.0,
        y: 0.0,
        width: 30.0,
        height: 10.0,
        face_id,
        row_role: GlyphRowRole::Text,
        bg: Some(Color::BLACK),
        clip: Some(Rect::new(10.0, 0.0, 10.0, 10.0)),
        policy: BoxPaintPolicy::Rounded,
        requires_background_fill: false,
        box_vertical_edges: BoxVerticalEdges::Both,
    }];

    let alternate_clip = Rect::new(10.0, 0.0, 10.0, 10.0);
    assert!(super::WgpuRenderer::paint_has_rounded_box_span(
        0.0,
        0.0,
        30.0,
        10.0,
        face_id,
        Some(&alternate_clip),
        GlyphRowRole::Text,
        &spans,
        &faces,
    ));

    let base_face_id = FaceId::new(1);
    for base_clip in [
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(20.0, 0.0, 10.0, 10.0),
    ] {
        assert!(!super::WgpuRenderer::paint_has_rounded_box_span(
            0.0,
            0.0,
            30.0,
            10.0,
            base_face_id,
            Some(&base_clip),
            GlyphRowRole::Text,
            &spans,
            &faces,
        ));
    }
}

#[test]
fn merged_box_span_keeps_first_left_and_last_right_edge_ownership() {
    use crate::renderer::frame_pass::{BoxPaintPolicy, BoxSpan, BoxSpanAccumulator};

    let mut spans = BoxSpanAccumulator::default();
    let span = |x, edges| BoxSpan {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        face_id: FaceId::new(7),
        row_role: GlyphRowRole::Text,
        bg: Some(Color::BLACK),
        clip: None,
        policy: BoxPaintPolicy::Rounded,
        requires_background_fill: false,
        box_vertical_edges: edges,
    };
    spans.push(span(0.0, BoxVerticalEdges::Left));
    spans.push(span(10.0, BoxVerticalEdges::Right));

    let spans = spans.finish();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].width, 20.0);
    assert_eq!(spans[0].box_vertical_edges, BoxVerticalEdges::Both);
}

#[test]
fn transient_face_box_owns_only_the_mouse_highlight_run_terminals() {
    use crate::renderer::frame_pass::{
        BoxPaintPolicy, BoxSpan, BoxSpanAccumulator, box_edges_for_face_paint,
    };

    let base = FaceId::new(4);
    assert_eq!(
        box_edges_for_face_paint(base, base, BoxVerticalEdges::Neither, false, false),
        BoxVerticalEdges::Neither
    );
    let painted = FaceId::new(5);
    let first = box_edges_for_face_paint(base, painted, BoxVerticalEdges::Neither, false, true);
    let second = box_edges_for_face_paint(base, painted, BoxVerticalEdges::Neither, true, false);
    assert_eq!(
        (first, second),
        (BoxVerticalEdges::Left, BoxVerticalEdges::Right),
        "a multi-glyph transient face owns only the highlight span's terminals"
    );

    let mut spans = BoxSpanAccumulator::default();
    let span = |x, box_vertical_edges| BoxSpan {
        x,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        face_id: painted,
        row_role: GlyphRowRole::Text,
        bg: Some(Color::BLACK),
        clip: None,
        policy: BoxPaintPolicy::Sharp,
        requires_background_fill: false,
        box_vertical_edges,
    };
    spans.push(span(0.0, first));
    spans.push(span(10.0, second));
    let spans = spans.finish();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].box_vertical_edges, BoxVerticalEdges::Both);
}

#[test]
fn media_glyphs_participate_in_the_shared_main_and_child_box_collection() {
    use crate::renderer::frame_pass::collect_frame_box_spans;
    use crate::renderer::pointer_override::PointerOverrideResolver;
    use neomacs_display_protocol::face::{BoxType, Face};
    use neomacs_display_protocol::{ImageId, ImageSourceRect, Rect};

    let face_id = FaceId::new(13);
    let mut face = Face::new(face_id);
    face.box_type = BoxType::Line;
    face.box_line_width = 1.into();
    let mut frame = FrameGlyphBuffer::new();
    frame.faces.insert(face_id, face);
    frame.glyphs.push(FrameGlyph::Image {
        window_id: DisplayWindowId::new(2),
        row_role: GlyphRowRole::Text,
        clip_rect: Some(Rect::new(0.0, 0.0, 40.0, 20.0)),
        slot_id: None,
        image_id: ImageId::new(7),
        source_rect: ImageSourceRect::FULL,
        slot_rect: Rect::new(1.0, 2.0, 24.0, 14.0),
        box_rect: Rect::new(1.0, 0.0, 24.0, 20.0),
        x: 3.0,
        y: 4.0,
        width: 20.0,
        height: 10.0,
        face_id,
        box_vertical_edges: BoxVerticalEdges::Right,
    });

    let pointer_override = PointerOverrideResolver::new(&frame, None);
    let spans = collect_frame_box_spans(&frame, &frame.faces, &pointer_override);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].face_id, face_id);
    assert_eq!(spans[0].box_vertical_edges, BoxVerticalEdges::Right);
    assert_eq!(
        (spans[0].x, spans[0].y, spans[0].width, spans[0].height),
        (1.0, 0.0, 24.0, 20.0),
        "boxes use the full GNU row-height glyph string, not intrinsic texture bounds"
    );
    assert!(spans[0].requires_background_fill);
}

#[test]
fn image_mouse_face_enters_shared_box_topology() {
    use crate::renderer::frame_pass::collect_frame_box_spans;
    use crate::renderer::pointer_override::PointerOverrideResolver;
    use neomacs_display_protocol::face::{BoxType, Face};
    use neomacs_display_protocol::{
        FrameRect, ImageId, ImageSourceRect, PointerAppearanceId, PointerAppearancePhase,
        PointerAppearanceSelection, PointerDrawMode, PresentedPaintSpan,
        PresentedPointerAppearance, PresentedPointerRegion, PresentedPrimitiveKind, Rect,
    };

    let base_id = FaceId::new(13);
    let hover_id = FaceId::new(14);
    let mut hover = Face::new(hover_id);
    hover.box_type = BoxType::Line;
    hover.box_line_width = 1.into();
    let mut frame = FrameGlyphBuffer::with_size(64.0, 32.0);
    frame.faces.insert(base_id, Face::new(base_id));
    frame.faces.insert(hover_id, hover);
    frame.glyphs.push(FrameGlyph::Image {
        window_id: DisplayWindowId::new(2),
        row_role: GlyphRowRole::Text,
        clip_rect: None,
        slot_id: None,
        image_id: ImageId::new(7),
        source_rect: ImageSourceRect::FULL,
        slot_rect: Rect::new(1.0, 2.0, 24.0, 14.0),
        box_rect: Rect::new(1.0, 0.0, 24.0, 20.0),
        x: 3.0,
        y: 4.0,
        width: 20.0,
        height: 10.0,
        face_id: base_id,
        box_vertical_edges: BoxVerticalEdges::Neither,
    });
    let appearance = PointerAppearanceId::try_from(0usize).unwrap();
    frame
        .install_presented_pointer(
            vec![PresentedPointerRegion::new(
                FrameRect::new(1.0, 0.0, 24.0, 20.0).unwrap(),
                None,
                Some(appearance),
            )],
            vec![PresentedPointerAppearance::new(
                vec![PresentedPaintSpan::new(
                    PresentedPrimitiveKind::Image,
                    0,
                    1,
                    FrameRect::new(1.0, 0.0, 24.0, 20.0).unwrap(),
                )],
                PointerDrawMode::Face(hover_id),
                PointerDrawMode::Face(hover_id),
            )],
        )
        .unwrap();
    let pointer_override = PointerOverrideResolver::new(
        &frame,
        Some(PointerAppearanceSelection::new(
            appearance,
            PointerAppearancePhase::Hover,
        )),
    );

    let spans = collect_frame_box_spans(&frame, &frame.faces, &pointer_override);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].face_id, hover_id);
    assert_eq!(spans[0].box_vertical_edges, BoxVerticalEdges::Both);
}

#[test]
fn mixed_image_text_mouse_face_uses_box_rect_adjacency() {
    use crate::renderer::frame_pass::collect_frame_box_spans;
    use crate::renderer::pointer_override::PointerOverrideResolver;
    use neomacs_display_protocol::face::{BoxType, Face};
    use neomacs_display_protocol::{
        DisplaySlotId, FrameRect, ImageId, ImageSourceRect, PointerAppearanceId,
        PointerAppearancePhase, PointerAppearanceSelection, PointerDrawMode, PresentedPaintSpan,
        PresentedPointerAppearance, PresentedPointerRegion, PresentedPrimitiveKind, Rect,
    };

    let base_id = FaceId::new(15);
    let hover_id = FaceId::new(16);
    let mut hover = Face::new(hover_id);
    hover.box_type = BoxType::Line;
    hover.box_line_width = 1.into();
    let window_id = DisplayWindowId::new(2);
    let mut frame = FrameGlyphBuffer::with_size(64.0, 32.0);
    frame.faces.insert(base_id, Face::new(base_id));
    frame.faces.insert(hover_id, hover);
    frame.glyphs.push(FrameGlyph::Image {
        window_id,
        row_role: GlyphRowRole::Text,
        clip_rect: None,
        slot_id: None,
        image_id: ImageId::new(7),
        source_rect: ImageSourceRect::FULL,
        slot_rect: Rect::new(1.0, 2.0, 24.0, 14.0),
        box_rect: Rect::new(1.0, 0.0, 24.0, 20.0),
        x: 3.0,
        y: 4.0,
        width: 20.0,
        height: 10.0,
        face_id: base_id,
        box_vertical_edges: BoxVerticalEdges::Neither,
    });
    frame.glyphs.push(FrameGlyph::Char {
        window_id,
        row_role: GlyphRowRole::Text,
        clip_rect: None,
        slot_id: DisplaySlotId {
            window_id,
            row: 0,
            col: 1,
        },
        bidi_level: 0,
        char: 'x',
        composed: None,
        x: 25.0,
        y: 0.0,
        baseline: 15.0,
        width: 8.0,
        height: 20.0,
        ascent: 15.0,
        face_id: base_id,
        box_vertical_edges: BoxVerticalEdges::Neither,
    });
    let appearance = PointerAppearanceId::try_from(0usize).unwrap();
    frame
        .install_presented_pointer(
            vec![PresentedPointerRegion::new(
                FrameRect::new(1.0, 0.0, 32.0, 20.0).unwrap(),
                None,
                Some(appearance),
            )],
            vec![PresentedPointerAppearance::new(
                vec![
                    PresentedPaintSpan::new(
                        PresentedPrimitiveKind::Image,
                        0,
                        1,
                        FrameRect::new(1.0, 0.0, 24.0, 20.0).unwrap(),
                    ),
                    PresentedPaintSpan::new(
                        PresentedPrimitiveKind::Glyph,
                        1,
                        1,
                        FrameRect::new(25.0, 0.0, 8.0, 20.0).unwrap(),
                    ),
                ],
                PointerDrawMode::Face(hover_id),
                PointerDrawMode::Face(hover_id),
            )],
        )
        .unwrap();
    let pointer_override = PointerOverrideResolver::new(
        &frame,
        Some(PointerAppearanceSelection::new(
            appearance,
            PointerAppearancePhase::Hover,
        )),
    );

    let spans = collect_frame_box_spans(&frame, &frame.faces, &pointer_override);
    assert_eq!(
        spans.len(),
        2,
        "media background ownership keeps paint batches distinct"
    );
    assert_eq!(spans[0].box_vertical_edges, BoxVerticalEdges::Left);
    assert_eq!(spans[1].box_vertical_edges, BoxVerticalEdges::Right);
}

fn make_cursor(
    slot_id: DisplaySlotId,
    x: f32,
    y: f32,
    width: f32,
    style: CursorStyle,
) -> WindowCursor {
    WindowCursor {
        window_id: slot_id.window_id,
        slot_id,
        x,
        y,
        width,
        height: 16.0,
        style,
        color: Color::WHITE,
        cursor_fg: Color::BLACK,
        ascent: 12.0,
        active: true,
    }
}

#[test]
fn rtl_bar_cursor_uses_right_edge_of_char_slot() {
    let mut frame = FrameGlyphBuffer::new();
    frame.set_draw_context(DisplayWindowId::new(1), GlyphRowRole::Text, None);
    frame.add_char('א', 10.0, 20.0, 12.0, 16.0, 12.0, false);
    let slot_id = frame.glyphs[0].slot_id().expect("slot id");
    if let FrameGlyph::Char { bidi_level, .. } = &mut frame.glyphs[0] {
        *bidi_level = 1;
    }

    let cursor = make_cursor(slot_id, 10.0, 20.0, 2.0, CursorStyle::Bar(2.0));
    assert_eq!(
        cursor_glyph_slot_rect(&frame, &cursor),
        (20.0, 20.0, 2.0, 16.0)
    );
}

#[test]
fn rtl_hbar_cursor_uses_right_edge_of_stretch_slot() {
    let mut frame = FrameGlyphBuffer::new();
    frame.set_draw_context(DisplayWindowId::new(2), GlyphRowRole::Text, None);
    frame.add_stretch(30.0, 40.0, 24.0, 16.0, Color::BLACK, FaceId::new(0), false);
    let slot_id = frame.glyphs[0].slot_id().expect("slot id");
    if let FrameGlyph::Stretch { bidi_level, .. } = &mut frame.glyphs[0] {
        *bidi_level = 1;
    }

    let cursor = make_cursor(slot_id, 30.0, 40.0, 8.0, CursorStyle::Hbar(2.0));
    assert_eq!(
        cursor_glyph_slot_rect(&frame, &cursor),
        (46.0, 40.0, 8.0, 16.0)
    );
}

#[test]
fn filled_box_cursor_keeps_slot_origin_in_rtl_runs() {
    let mut frame = FrameGlyphBuffer::new();
    frame.set_draw_context(DisplayWindowId::new(3), GlyphRowRole::Text, None);
    frame.add_char('א', 50.0, 60.0, 12.0, 16.0, 12.0, false);
    let slot_id = frame.glyphs[0].slot_id().expect("slot id");
    if let FrameGlyph::Char { bidi_level, .. } = &mut frame.glyphs[0] {
        *bidi_level = 1;
    }

    let cursor = make_cursor(slot_id, 50.0, 60.0, 8.0, CursorStyle::FilledBox);
    assert_eq!(
        cursor_glyph_slot_rect(&frame, &cursor),
        (50.0, 60.0, 12.0, 16.0)
    );
}

#[test]
#[tracing_test::traced_test]
fn vertical_bar_cursor_aligned_to_its_glyph_cell_is_not_a_mismatch() {
    let window_id = DisplayWindowId::new(1);
    let slot_id = DisplaySlotId {
        window_id,
        row: 6,
        col: 6,
    };
    let mut frame = FrameGlyphBuffer::new();
    frame.char_width = 13.0;
    frame.char_height = 18.0;
    frame.set_draw_context(window_id, GlyphRowRole::Text, None);
    frame.add_char('葭', 233.0, 123.0, 13.0, 18.0, 18.0, false);
    if let FrameGlyph::Char {
        slot_id: glyph_slot,
        ..
    } = &mut frame.glyphs[0]
    {
        *glyph_slot = slot_id;
    }
    frame.window_cursors.push(make_cursor(
        slot_id,
        233.0,
        123.0,
        2.0,
        CursorStyle::Bar(2.0),
    ));
    frame.window_cursors[0].height = 18.0;
    frame.window_cursors[0].ascent = 18.0;

    let chars = [RenderedCharBounds {
        glyph_index: 0,
        window_id: window_id.get(),
        row_role: GlyphRowRole::Text,
        slot_id,
        label: "葭".to_owned(),
        face_id: FaceId::new(25),
        font_size: 13.0,
        cell_x: 233.0,
        cell_y: 123.0,
        cell_w: 13.0,
        cell_h: 18.0,
        glyph_x: 233.5,
        glyph_y: 125.5,
        glyph_w: 12.5,
        glyph_h: 13.0,
        left_overhang: 0.0,
        right_overhang: 0.0,
        top_overhang: 0.0,
        bottom_overhang: 0.0,
    }];

    log_cursor_glyph_alignment(4_294_967_296, "text", &frame, &chars);

    assert!(
        !logs_contain("cursor_glyph_mismatch"),
        "a GNU-style vertical bar occupies one cell edge; it must not contain the glyph bitmap"
    );
}

#[test]
fn cursor_cell_alignment_models_gnu_cursor_shapes() {
    let cell = GlyphCellRect(Rect::new(100.0, 40.0, 13.0, 18.0));
    let full_cell = ResolvedCursorRect(cell.0);
    let left_bar = ResolvedCursorRect(Rect::new(100.0, 40.0, 2.0, 18.0));
    let right_bar = ResolvedCursorRect(Rect::new(111.0, 40.0, 2.0, 18.0));

    for style in [
        CursorStyle::FilledBox,
        CursorStyle::Hbar(2.0),
        CursorStyle::Hollow,
    ] {
        assert_eq!(
            cursor_cell_alignment(
                style,
                CursorInlineDirection::LeftToRight,
                full_cell,
                cell,
                0.01,
            ),
            CursorCellAlignment::Aligned,
            "{style:?} resolves from the complete cell rectangle"
        );
    }
    assert_eq!(
        cursor_cell_alignment(
            CursorStyle::Bar(2.0),
            CursorInlineDirection::LeftToRight,
            left_bar,
            cell,
            0.01,
        ),
        CursorCellAlignment::Aligned
    );
    assert_eq!(
        cursor_cell_alignment(
            CursorStyle::Bar(2.0),
            CursorInlineDirection::RightToLeft,
            right_bar,
            cell,
            0.01,
        ),
        CursorCellAlignment::Aligned
    );
}

#[test]
fn vertical_bar_away_from_both_cell_edges_is_misaligned() {
    let cell = GlyphCellRect(Rect::new(100.0, 40.0, 13.0, 18.0));
    let interior_bar = ResolvedCursorRect(Rect::new(105.0, 40.0, 2.0, 18.0));

    assert_eq!(
        cursor_cell_alignment(
            CursorStyle::Bar(2.0),
            CursorInlineDirection::LeftToRight,
            interior_bar,
            cell,
            0.01,
        ),
        CursorCellAlignment::Misaligned {
            expected: CursorCellContract::VerticalLeadingEdge(CursorInlineDirection::LeftToRight),
        }
    );
}

#[test]
fn vertical_bar_on_ltr_trailing_edge_is_misaligned() {
    let cell = GlyphCellRect(Rect::new(100.0, 40.0, 13.0, 18.0));
    let trailing_bar = ResolvedCursorRect(Rect::new(111.0, 40.0, 2.0, 18.0));

    assert_eq!(
        cursor_cell_alignment(
            CursorStyle::Bar(2.0),
            CursorInlineDirection::LeftToRight,
            trailing_bar,
            cell,
            0.01,
        ),
        CursorCellAlignment::Misaligned {
            expected: CursorCellContract::VerticalLeadingEdge(CursorInlineDirection::LeftToRight),
        },
        "GNU permits the right cell edge only for an RTL glyph"
    );
}

#[test]
fn vertical_bar_on_rtl_trailing_edge_is_misaligned() {
    let cell = GlyphCellRect(Rect::new(100.0, 40.0, 13.0, 18.0));
    let trailing_bar = ResolvedCursorRect(Rect::new(100.0, 40.0, 2.0, 18.0));

    assert_eq!(
        cursor_cell_alignment(
            CursorStyle::Bar(2.0),
            CursorInlineDirection::RightToLeft,
            trailing_bar,
            cell,
            0.01,
        ),
        CursorCellAlignment::Misaligned {
            expected: CursorCellContract::VerticalLeadingEdge(CursorInlineDirection::RightToLeft),
        },
        "GNU permits the left cell edge only for an LTR glyph"
    );
}

#[test]
fn full_cell_cursor_displaced_from_its_cell_is_misaligned() {
    let cell = GlyphCellRect(Rect::new(100.0, 40.0, 13.0, 18.0));
    let displaced = ResolvedCursorRect(Rect::new(99.0, 40.0, 13.0, 18.0));

    assert_eq!(
        cursor_cell_alignment(
            CursorStyle::FilledBox,
            CursorInlineDirection::LeftToRight,
            displaced,
            cell,
            0.01,
        ),
        CursorCellAlignment::Misaligned {
            expected: CursorCellContract::FullCell,
        }
    );
}

#[test]
fn frame_default_glyph_metrics_use_frame_font_and_line_height() {
    let mut frame = FrameGlyphBuffer::new();
    frame.font_pixel_size = 27.0;
    frame.char_height = 33.0;

    assert_eq!(frame_default_glyph_metrics(&frame), Some((27.0, 33.0)));
}

#[test]
fn frame_default_glyph_metrics_none_without_font_size() {
    // A frame without a resolved font size must not produce invented
    // metrics: set_metrics clears the whole glyph atlas on a default-metric
    // change, so a fabricated fallback from a synthetic frame (e.g. a
    // filled-box cursor mini-frame) would evict every cached glyph twice.
    let mut frame = FrameGlyphBuffer::new();
    frame.font_pixel_size = f32::NAN;
    frame.char_height = 0.0;

    assert_eq!(frame_default_glyph_metrics(&frame), None);
}

#[test]
fn frame_default_glyph_metrics_derive_line_height_from_font_size() {
    let mut frame = FrameGlyphBuffer::new();
    frame.font_pixel_size = 14.0;
    frame.char_height = 0.0;

    let (font_size, line_height) =
        frame_default_glyph_metrics(&frame).expect("real font size yields metrics");
    assert_eq!(font_size, 14.0);
    assert!((line_height - 16.8).abs() < 0.001);
}

fn char_bounds(label: &str, x: f32, y: f32, width: f32, height: f32) -> RenderedCharBounds {
    RenderedCharBounds {
        glyph_index: 0,
        window_id: 1,
        row_role: GlyphRowRole::Text,
        slot_id: DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
            row: 0,
            col: 0,
        },
        label: label.to_string(),
        face_id: FaceId::new(0),
        font_size: 14.0,
        cell_x: x,
        cell_y: y,
        cell_w: width,
        cell_h: height,
        glyph_x: x,
        glyph_y: y,
        glyph_w: width,
        glyph_h: height,
        left_overhang: 0.0,
        right_overhang: 0.0,
        top_overhang: 0.0,
        bottom_overhang: 0.0,
    }
}

#[test]
fn char_overlap_detects_intersecting_rendered_bitmaps() {
    let a = char_bounds("A", 0.0, 0.0, 10.0, 12.0);
    let b = char_bounds("B", 9.0, 4.0, 10.0, 12.0);

    let overlap = char_overlap(&a, &b).expect("overlap");
    assert_eq!(overlap.x, 9.0);
    assert_eq!(overlap.y, 4.0);
    assert_eq!(overlap.width, 1.0);
    assert_eq!(overlap.height, 8.0);
    assert!(!overlap.expected_by_overhang);
}

#[test]
fn char_overlap_ignores_touching_edges_and_subpixel_noise() {
    let a = char_bounds("A", 0.0, 0.0, 10.0, 12.0);
    let touching = char_bounds("B", 10.0, 0.0, 10.0, 12.0);
    let tiny = char_bounds("C", 9.75, 0.0, 10.0, 12.0);

    assert!(char_overlap(&a, &touching).is_none());
    assert!(char_overlap(&a, &tiny).is_none());
}

#[test]
fn char_overlap_classifies_font_overhang_separately() {
    let mut f = char_bounds("f", 0.0, 0.0, 9.0, 12.0);
    f.glyph_w = 11.0;
    f.right_overhang = 2.0;
    let next = char_bounds("a", 9.0, 0.0, 12.0, 12.0);

    let overlap = char_overlap(&f, &next).expect("overhang overlap");
    assert_eq!(overlap.x, 9.0);
    assert_eq!(overlap.width, 2.0);
    assert!(overlap.expected_by_overhang);
}

#[test]
fn char_overlap_classifies_subpixel_boundary_overhang_separately() {
    let mut slash = char_bounds("/", 775.8, 698.0, 14.0, 31.0);
    slash.glyph_x = 776.0;
    slash.glyph_y = 701.7;
    slash.glyph_w = 14.3;
    slash.glyph_h = 22.3;
    slash.right_overhang = (slash.glyph_x + slash.glyph_w - (slash.cell_x + slash.cell_w)).max(0.0);

    let mut m = char_bounds("m", 789.8, 698.0, 14.0, 31.0);
    m.glyph_x = 789.7;
    m.glyph_y = 708.0;
    m.glyph_w = 13.7;
    m.glyph_h = 13.7;
    m.left_overhang = (m.cell_x - m.glyph_x).max(0.0);

    let overlap = char_overlap(&slash, &m).expect("subpixel boundary overhang");
    assert!(overlap.expected_by_overhang);
}

#[test]
fn char_overlap_classifies_adjacent_vertical_overhang_separately() {
    let mut upper = char_bounds("│", 861.0, 76.0, 9.0, 19.0);
    upper.glyph_x = 865.0;
    upper.glyph_y = 75.0;
    upper.glyph_w = 2.0;
    upper.glyph_h = 23.0;
    upper.top_overhang = 1.0;
    upper.bottom_overhang = 3.0;

    let mut lower = char_bounds("│", 861.0, 95.0, 9.0, 19.0);
    lower.glyph_x = 865.0;
    lower.glyph_y = 94.0;
    lower.glyph_w = 2.0;
    lower.glyph_h = 23.0;
    lower.top_overhang = 1.0;
    lower.bottom_overhang = 3.0;

    let overlap = char_overlap(&upper, &lower).expect("vertical overhang overlap");
    assert_eq!(overlap.y, 94.0);
    assert_eq!(overlap.height, 4.0);
    assert!(overlap.expected_by_overhang);
}

#[test]
fn char_overlap_classifies_box_drawing_corner_join_separately() {
    let mut corner = char_bounds("╮", 1734.0, 95.0, 9.0, 19.0);
    corner.slot_id.row = 5;
    corner.slot_id.col = 2;
    corner.glyph_x = 1733.0;
    corner.glyph_y = 103.0;
    corner.glyph_w = 7.0;
    corner.glyph_h = 14.0;
    corner.left_overhang = 1.0;

    let mut stem = char_bounds("│", 1734.0, 114.0, 9.0, 19.0);
    stem.slot_id.row = 6;
    stem.slot_id.col = 2;
    stem.glyph_x = 1738.0;
    stem.glyph_y = 113.0;
    stem.glyph_w = 2.0;
    stem.glyph_h = 23.0;

    let overlap = char_overlap(&corner, &stem).expect("box-drawing join overlap");
    assert_eq!(overlap.x, 1738.0);
    assert_eq!(overlap.y, 113.0);
    assert_eq!(overlap.width, 2.0);
    assert_eq!(overlap.height, 4.0);
    assert!(overlap.expected_by_overhang);
}

#[test]
fn char_overlap_classifies_adjacent_dual_bearing_overhang_separately() {
    let mut w = char_bounds("w", 888.0, 384.0, 16.0, 33.0);
    w.glyph_x = 889.0;
    w.glyph_y = 395.0;
    w.glyph_w = 17.0;
    w.glyph_h = 15.0;
    w.right_overhang = 2.0;

    let mut x = char_bounds("x", 904.0, 384.0, 16.0, 33.0);
    x.glyph_x = 903.0;
    x.glyph_y = 395.0;
    x.glyph_w = 18.0;
    x.glyph_h = 15.0;
    x.left_overhang = 1.0;
    x.right_overhang = 1.0;

    let overlap = char_overlap(&w, &x).expect("dual-bearing overhang overlap");
    assert_eq!(overlap.x, 903.0);
    assert_eq!(overlap.width, 3.0);
    assert!(overlap.expected_by_overhang);
}
