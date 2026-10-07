use super::*;
use crate::display_item::{
    DisplayItemFaceOverlay, DisplaySourceMappedFaceRun, DisplaySourceMappedText, RenderFaceRef,
};

#[test]
fn unresolved_mapped_faces_return_the_original_item() {
    for mapped in [
        DisplaySourceMappedText::new("ab")
            .with_semantic_face_overlay(DisplayItemFaceOverlay::EscapeGlyph),
        DisplaySourceMappedText::new("ab")
            .with_lisp_face_runs(vec![DisplaySourceMappedFaceRun::new(2, None)]),
    ] {
        let item = DisplayItem::new(
            SourceSpan::synthetic(2, 0, 1),
            RenderFaceRef::Inherit,
            DisplayItemKind::SourceMappedText(mapped),
        );
        let original = item.clone();
        assert_eq!(
            ResolvedMappedTextInput::capture(item, FaceId::new(7)),
            Err(original)
        );
    }
}
