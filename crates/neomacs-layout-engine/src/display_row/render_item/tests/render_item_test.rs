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

    let remainder = clipped_display_item_remainder_after_chars(source, 1)
        .expect("two string characters remain");

    assert_eq!(
        remainder.kind,
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::from_string_run(
            "bc",
            DisplaySourcePosition::lisp_string(7, 5, 10),
        )),
        "the next row must continue in the same string coordinate space"
    );
}
