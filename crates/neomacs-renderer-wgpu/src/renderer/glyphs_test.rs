use super::{
    CharOverlapClassification, CursorCellAlignment, CursorCellContract, CursorInlineDirection,
    ExpectedCharOverlap, GlyphCellRect, RenderedCharBounds, RenderedGlyphGeometry,
    ResolvedCursorRect, char_overlap, cursor_cell_alignment, cursor_glyph_slot_rect,
    frame_default_glyph_metrics, log_cursor_glyph_alignment,
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
        row_role: GlyphRowRole::Text,
        slot_id,
        label: "葭".to_owned(),
        face_id: FaceId::new(25),
        font_size: 13.0,
        geometry: RenderedGlyphGeometry::new(
            Rect::new(233.0, 123.0, 13.0, 18.0),
            Rect::new(233.5, 125.5, 12.5, 13.0),
        ),
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
        row_role: GlyphRowRole::Text,
        slot_id: DisplaySlotId {
            window_id: neomacs_display_protocol::types::DisplayWindowId::new(1),
            row: 0,
            col: 0,
        },
        label: label.to_string(),
        face_id: FaceId::new(0),
        font_size: 14.0,
        geometry: RenderedGlyphGeometry::new(
            Rect::new(x, y, width, height),
            Rect::new(x, y, width, height),
        ),
    }
}

fn with_bitmap(mut bounds: RenderedCharBounds, bitmap: Rect) -> RenderedCharBounds {
    bounds.geometry = RenderedGlyphGeometry::new(bounds.geometry.cell, bitmap);
    bounds
}

#[test]
fn char_overlap_detects_intersecting_rendered_bitmaps() {
    let a = char_bounds("A", 0.0, 0.0, 10.0, 12.0);
    let b = char_bounds("B", 9.0, 4.0, 10.0, 12.0);

    let overlap = char_overlap(&a, &b).expect("overlap");
    assert_eq!(overlap.bounds, Rect::new(9.0, 4.0, 1.0, 8.0));
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Unexpected
    );
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
    let f = with_bitmap(
        char_bounds("f", 0.0, 0.0, 9.0, 12.0),
        Rect::new(0.0, 0.0, 11.0, 12.0),
    );
    let next = char_bounds("a", 9.0, 0.0, 12.0, 12.0);

    let overlap = char_overlap(&f, &next).expect("overhang overlap");
    assert_eq!(overlap.bounds.x, 9.0);
    assert_eq!(overlap.bounds.width, 2.0);
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Expected(ExpectedCharOverlap::HorizontalOverhang)
    );
}

#[test]
fn char_overlap_classifies_overhang_past_a_narrower_intervening_cell_separately() {
    // A Nerd Font icon keeps the family's 9 px advance but draws 15 px wide,
    // so its right overhang crosses the 4 px cell after it and lands in the
    // next one. GNU `right_overwritten` walks following glyphs while the
    // overhang still exceeds their summed widths: the reach is one glyph's
    // bearing, not a collision between two cells.
    let mut icon = with_bitmap(
        char_bounds("\u{f48a}", 49.0, 1188.0, 9.0, 20.0),
        Rect::new(49.0, 1192.5, 15.0, 10.0),
    );
    icon.slot_id.col = 6;
    let mut r = with_bitmap(
        char_bounds("R", 62.0, 1188.0, 9.0, 20.0),
        Rect::new(63.0, 1192.0, 7.5, 11.0),
    );
    r.slot_id.col = 8;

    let overlap = char_overlap(&icon, &r).expect("overhang overlap");
    assert_eq!(overlap.bounds, Rect::new(63.0, 1192.5, 1.0, 10.0));
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Expected(ExpectedCharOverlap::HorizontalOverhang)
    );
}

#[test]
fn char_overlap_keeps_a_bitmap_displaced_from_its_own_cell_a_collision() {
    // Ink that never touches its own advance cell is a placement defect (an
    // x-offset, scale or slot bug), whatever it lands on. GNU overhang is
    // `rbearing > width` / `lbearing < 0`: ink that starts in the cell.
    let a = with_bitmap(
        char_bounds("A", 0.0, 0.0, 9.0, 12.0),
        Rect::new(0.5, 0.0, 8.0, 12.0),
    );
    let mut displaced = with_bitmap(
        char_bounds("B", 18.0, 0.0, 9.0, 12.0),
        Rect::new(1.0, 0.0, 7.0, 12.0),
    );
    displaced.slot_id.col = 2;

    let overlap = char_overlap(&a, &displaced).expect("bitmap collision");
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Unexpected
    );
}

#[test]
fn char_overlap_classifies_ink_starting_at_its_cell_edge_as_overhang() {
    // GNU `lbearing >= width` (ink wholly right of the advance, e.g. a
    // zero-width combining mark's cell) is still a bearing; the own-cell rule
    // tolerates the shared threshold rather than demanding ink strictly inside.
    let mut mark = with_bitmap(
        char_bounds("\u{0301}", 9.0, 0.0, 0.0, 12.0),
        Rect::new(9.0, 0.0, 4.0, 12.0),
    );
    mark.slot_id.col = 1;
    let mut next = char_bounds("a", 9.0, 0.0, 9.0, 12.0);
    next.slot_id.col = 2;
    // Cells: mark 9..9 (zero width), a 9..18 — they touch, do not intersect.
    for (a, b) in [(&mark, &next), (&next, &mark)] {
        let overlap = char_overlap(a, b).expect("mark ink over the next glyph");
        assert_eq!(
            overlap.classification,
            CharOverlapClassification::Expected(ExpectedCharOverlap::HorizontalOverhang),
            "classification must not depend on argument order"
        );
    }
}

#[test]
fn char_overlap_keeps_overlapping_cells_a_collision() {
    // Two advance cells that intersect are a layout defect regardless of how
    // far either bitmap reaches: overhang never explains the cells themselves.
    let mut icon = with_bitmap(
        char_bounds("\u{f48a}", 49.0, 1188.0, 9.0, 20.0),
        Rect::new(49.0, 1192.5, 15.0, 10.0),
    );
    icon.slot_id.col = 6;
    let mut r = with_bitmap(
        char_bounds("R", 55.0, 1188.0, 9.0, 20.0),
        Rect::new(56.0, 1192.0, 7.5, 11.0),
    );
    r.slot_id.col = 7;

    let overlap = char_overlap(&icon, &r).expect("bitmap collision");
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Unexpected
    );
}

#[test]
fn char_overlap_horizontal_overhang_requires_one_logical_row() {
    let first = with_bitmap(
        char_bounds("f", 0.0, 0.0, 9.0, 12.0),
        Rect::new(0.0, 0.0, 11.0, 12.0),
    );
    let next = char_bounds("a", 9.0, 0.0, 12.0, 12.0);
    let mut other_window = next.clone();
    other_window.slot_id.window_id = DisplayWindowId::new(2);
    let mut other_role = next.clone();
    other_role.row_role = GlyphRowRole::ModeLine;
    let mut other_row = next;
    other_row.slot_id.row = 1;

    for (case, next) in [
        ("different window", other_window),
        ("different row role", other_role),
        ("different row", other_row),
    ] {
        let overlap = char_overlap(&first, &next).expect("horizontal bitmap collision");
        assert_eq!(
            overlap.classification,
            CharOverlapClassification::Unexpected,
            "{case} must not suppress a genuine collision"
        );
    }
}

#[test]
fn char_overlap_classifies_subpixel_boundary_overhang_separately() {
    let slash = with_bitmap(
        char_bounds("/", 775.8, 698.0, 14.0, 31.0),
        Rect::new(776.0, 701.7, 14.3, 22.3),
    );
    let m = with_bitmap(
        char_bounds("m", 789.8, 698.0, 14.0, 31.0),
        Rect::new(789.7, 708.0, 13.7, 13.7),
    );

    let overlap = char_overlap(&slash, &m).expect("subpixel boundary overhang");
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Expected(ExpectedCharOverlap::HorizontalOverhang)
    );
}

#[test]
fn char_overlap_classifies_adjacent_vertical_overhang_separately() {
    let (upper, lower) = adjacent_vertical_overhang_bounds();

    let overlap = char_overlap(&upper, &lower).expect("vertical overhang overlap");
    assert_eq!(overlap.bounds.y, 94.0);
    assert_eq!(overlap.bounds.height, 4.0);
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Expected(ExpectedCharOverlap::VerticalOverhang)
    );
}

fn adjacent_vertical_overhang_bounds() -> (RenderedCharBounds, RenderedCharBounds) {
    let mut upper = with_bitmap(
        char_bounds("│", 861.0, 76.0, 9.0, 19.0),
        Rect::new(865.0, 75.0, 2.0, 23.0),
    );
    upper.slot_id.row = 5;
    upper.slot_id.col = 2;

    let mut lower = with_bitmap(
        char_bounds("│", 861.0, 95.0, 9.0, 19.0),
        Rect::new(865.0, 94.0, 2.0, 23.0),
    );
    lower.slot_id.row = 6;
    lower.slot_id.col = 2;
    (upper, lower)
}

#[test]
fn char_overlap_vertical_overhang_requires_one_logical_grid_run() {
    let (upper, lower) = adjacent_vertical_overhang_bounds();
    let mut other_window = lower.clone();
    other_window.slot_id.window_id = DisplayWindowId::new(2);
    let mut other_role = lower.clone();
    other_role.row_role = GlyphRowRole::ModeLine;
    let mut other_column = lower.clone();
    other_column.slot_id.col = 3;
    let mut nonadjacent_row = lower;
    nonadjacent_row.slot_id.row = 7;

    for (case, lower) in [
        ("different window", other_window),
        ("different row role", other_role),
        ("different column", other_column),
        ("nonadjacent row", nonadjacent_row),
    ] {
        let overlap = char_overlap(&upper, &lower).expect("vertical bitmap collision");
        assert_eq!(
            overlap.classification,
            CharOverlapClassification::Unexpected,
            "{case} must not suppress a genuine collision"
        );
    }
}

#[test]
fn char_overlap_vertical_overhang_must_intersect_the_shared_row_boundary() {
    let mut upper = with_bitmap(
        char_bounds("upper", 0.0, 0.0, 10.0, 10.0),
        Rect::new(4.0, 0.0, 2.0, 18.0),
    );
    upper.slot_id.row = 5;
    upper.slot_id.col = 2;

    let mut lower = with_bitmap(
        char_bounds("lower", 0.0, 10.0, 10.0, 10.0),
        Rect::new(4.0, 14.0, 2.0, 4.0),
    );
    lower.slot_id.row = 6;
    lower.slot_id.col = 2;

    let overlap = char_overlap(&upper, &lower).expect("deep vertical bitmap collision");
    assert_eq!(overlap.bounds, Rect::new(4.0, 14.0, 2.0, 4.0));
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Unexpected,
        "vertical ink that collides away from the shared row boundary is not a joining overhang"
    );
}

#[test]
fn char_overlap_classifies_box_drawing_corner_join_separately() {
    let mut corner = with_bitmap(
        char_bounds("╮", 1734.0, 95.0, 9.0, 19.0),
        Rect::new(1733.0, 103.0, 7.0, 14.0),
    );
    corner.slot_id.row = 5;
    corner.slot_id.col = 2;

    let mut stem = with_bitmap(
        char_bounds("│", 1734.0, 114.0, 9.0, 19.0),
        Rect::new(1738.0, 113.0, 2.0, 23.0),
    );
    stem.slot_id.row = 6;
    stem.slot_id.col = 2;

    let overlap = char_overlap(&corner, &stem).expect("box-drawing join overlap");
    assert_eq!(overlap.bounds, Rect::new(1738.0, 113.0, 2.0, 4.0));
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Expected(ExpectedCharOverlap::VerticalOverhang)
    );
}

#[test]
fn char_overlap_classifies_adjacent_dual_bearing_overhang_separately() {
    let w = with_bitmap(
        char_bounds("w", 888.0, 384.0, 16.0, 33.0),
        Rect::new(889.0, 395.0, 17.0, 15.0),
    );
    let x = with_bitmap(
        char_bounds("x", 904.0, 384.0, 16.0, 33.0),
        Rect::new(903.0, 395.0, 18.0, 15.0),
    );

    let overlap = char_overlap(&w, &x).expect("dual-bearing overhang overlap");
    assert_eq!(overlap.bounds.x, 903.0);
    assert_eq!(overlap.bounds.width, 3.0);
    assert_eq!(
        overlap.classification,
        CharOverlapClassification::Expected(ExpectedCharOverlap::HorizontalOverhang)
    );
}
