use super::*;
use crate::display_item::{DisplayImageItem, DisplayMediaReplacement, DisplayMediaReplacementKind};

#[test]
fn image_margin_expands_the_slot_but_keeps_media_bounds_on_image_content() {
    let replacement = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 7,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        width: 20.0,
        height: 10.0,
        ascent: 8.0,
        horizontal_margin: 3.0,
        vertical_margin: 2.0,
        opaque_background: Some(0x12_34_56),
    });
    assert_eq!(replacement.width, 26.0);
    assert_eq!(replacement.height, 14.0);
    assert_eq!(replacement.ascent, 10.0);

    assert!(matches!(
        replacement.kind,
        DisplayMediaReplacementKind::Image {
            margin_left: 3.0,
            margin_right: 3.0,
            margin_top: 2.0,
            margin_bottom: 2.0,
            ..
        }
    ));
}

#[test]
fn media_replacement_remains_one_authoritative_row_item() {
    let replacement = DisplayMediaReplacement::image(DisplayImageItem {
        image_id: 7,
        source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
        width: 20.0,
        height: 10.0,
        ascent: 8.0,
        horizontal_margin: 3.0,
        vertical_margin: 2.0,
        opaque_background: Some(0x12_34_56),
    });
    let source = DisplayItem {
        span: SourceSpan::synthetic(1, 0, 1),
        face: RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(1)),
        kind: DisplayItemKind::MediaReplacement(replacement),
        layout: Default::default(),
        pointer_appearance: None,
        box_vertical_edges: Default::default(),
        box_run_membership: Default::default(),
    };

    let rendered = DisplayRowRenderItem::from_source_item(source);

    assert!(matches!(
        rendered.row_item().kind,
        DisplayItemKind::MediaReplacement(actual) if actual == replacement
    ));
}

#[test]
fn writer_clipping_retains_original_source_for_next_row() {
    use crate::display_row::builder::{
        DisplayRowAppendStatus, DisplayRowLayout, DisplayRowPosition, DisplayRowProgressWriter,
        DisplayTabPolicy,
    };
    use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
    use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphRow};
    use neomacs_display_protocol::types::FaceId;

    let face = RenderFaceRef::FaceId(FaceId::new(3));
    let source = DisplayItem::new(
        SourceSpan::lisp_string(7, 4, 8, 4, 8),
        face,
        DisplayItemKind::TextRun(DisplayTextRun::independent("abcd")),
    );
    let rendered = DisplayRowRenderItem::from_source_item(source);
    let layout = DisplayRowLayout {
        role: GlyphRowRole::Text,
        y_px: 0.0,
        height_px: 16.0,
        ascent_px: 12.0,
        char_width_px: 8.0,
        tab_policy: DisplayTabPolicy::every(4),
        line_number_width_px: 0.0,
        base_face: face,
        pixel_calc: crate::display_pixel_calc::PixelCalcContext::for_chrome_row(
            16.0,
            8.0,
            16.0,
            std::collections::HashMap::new(),
        ),
        space_image_params: None,
        line_wrap: crate::display_row::append_context::DisplayRowLineWrap::chrome_row(),
    };
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let progress =
        DisplayRowProgressWriter::new(&layout, &mut row, DisplayRowPosition::new(0.0, 0), 16.0)
            .push_item(rendered.row_item_for_write());
    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(row.glyphs[GlyphArea::Text.index()].len(), 2);
    let DisplayRowClippedRemainder::Resume(remainder) = rendered.clipped_remainder(&progress)
    else {
        panic!("the unrendered text must continue on the next row");
    };
    assert_eq!(remainder.span, SourceSpan::lisp_string(7, 6, 8, 6, 8));
    assert_eq!(remainder.face, face);
    assert_eq!(
        remainder.kind,
        DisplayItemKind::TextRun(DisplayTextRun::independent("cd"))
    );
}

#[test]
fn clipped_string_mapped_text_preserves_and_advances_its_string_origin() {
    let source = DisplayItem {
        span: SourceSpan::synthetic(1, 10, 12),
        face: RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(1)),
        kind: DisplayItemKind::SourceMappedText(DisplaySourceMappedText::from_string_run(
            "αbc",
            DisplaySourcePosition::lisp_string(7, 4, 8),
        )),
        layout: Default::default(),
        pointer_appearance: None,
        box_vertical_edges: Default::default(),
        box_run_membership: Default::default(),
    };

    let DisplayRowClippedRemainder::Resume(remainder) =
        clipped_display_item_remainder_after_chars(source, 1)
    else {
        panic!("two string characters remain, so the tail resumes");
    };

    assert_eq!(
        remainder.kind,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::from_string_run(
            "bc",
            DisplaySourcePosition::lisp_string(7, 5, 10),
        )),
        "the next row must continue in the same string coordinate space"
    );
}

fn media_source_item(image_id: i32, width: f32, height: f32) -> DisplayItem {
    DisplayItem {
        span: SourceSpan::synthetic(9, 0, 1),
        face: RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(3)),
        kind: DisplayItemKind::MediaReplacement(DisplayMediaReplacement::image(DisplayImageItem {
            image_id,
            source_rect: neomacs_display_protocol::ImageSourceRect::FULL,
            width,
            height,
            ascent: height,
            horizontal_margin: 0.0,
            vertical_margin: 0.0,
            opaque_background: None,
        })),
        layout: Default::default(),
        pointer_appearance: None,
        box_vertical_edges: Default::default(),
        box_run_membership: Default::default(),
    }
}

/// GNU `display_line` unproduces a display element that draws past the right
/// edge of a continued row and re-produces it at the start of the next row
/// (src/xdisp.c:26448-26475).  A refused image therefore carries over WHOLE --
/// the old `Option` remainder answered "nothing to remember" for it, which is
/// how the image silently disappeared instead of moving down.
#[test]
fn refused_media_item_is_deferred_whole() {
    let source = media_source_item(42, 200.0, 80.0);
    let remainder = clipped_display_item_remainder_after_chars(source.clone(), 0);
    let DisplayRowClippedRemainder::DeferWhole(deferred) = remainder else {
        panic!("a media glyph that reached no part of the row is deferred whole");
    };
    assert_eq!(
        deferred, source,
        "the deferred item keeps its own face, layout and media identity"
    );
}

/// An atomic item the row did place -- a structural lane clipped it in place
/// -- has no resumable tail; re-producing it would draw the glyph twice.
#[test]
fn media_item_already_on_the_row_has_no_remainder() {
    let remainder =
        clipped_display_item_remainder_after_chars(media_source_item(42, 200.0, 80.0), 1);
    assert_eq!(remainder, DisplayRowClippedRemainder::Nothing);
}

/// A text run the row kept part of resumes from its unrendered tail, and one
/// it kept whole has nothing left.
#[test]
fn text_run_remainder_resumes_and_then_exhausts() {
    let source = DisplayItem {
        span: SourceSpan::synthetic(5, 0, 3),
        face: RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(1)),
        kind: DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
        layout: Default::default(),
        pointer_appearance: None,
        box_vertical_edges: Default::default(),
        box_run_membership: Default::default(),
    };
    let DisplayRowClippedRemainder::Resume(remainder) =
        clipped_display_item_remainder_after_chars(source.clone(), 1)
    else {
        panic!("one character of three is left");
    };
    assert_eq!(
        remainder.kind,
        DisplayItemKind::TextRun(DisplayTextRun::new("bc"))
    );
    assert_eq!(
        clipped_display_item_remainder_after_chars(source, 3),
        DisplayRowClippedRemainder::Nothing
    );
}

/// A run the row refused outright is the same item produced against the next
/// row, not a rebuilt text tail: it keeps its span, layout and box topology.
#[test]
fn refused_text_run_is_deferred_whole_with_its_box_topology() {
    let mut source = DisplayItem {
        span: SourceSpan::synthetic(5, 0, 3),
        face: RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(1)),
        kind: DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
        layout: Default::default(),
        pointer_appearance: None,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
            true, true,
        ),
        box_run_membership: Default::default(),
    };
    source.layout.raise = Some(0.25);
    let DisplayRowClippedRemainder::DeferWhole(deferred) =
        clipped_display_item_remainder_after_chars(source.clone(), 0)
    else {
        panic!("a refused text run is deferred whole");
    };
    assert_eq!(deferred, source);
    assert!(matches!(
        deferred.box_vertical_edges,
        neomacs_display_protocol::face::BoxVerticalEdges::Both
    ));
}
