use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayItemLayout, DisplayPointerAppearance,
    DisplaySourcePosition, SourceSpan,
};
use neomacs_display_protocol::face::{BoxRunMembership, BoxVerticalEdges};
use neomacs_display_protocol::types::FaceId;

/// Owned replacement text whose covered buffer span differs from its glyph
/// coordinates. Semantic face overlays and display-table faces must already
/// be resolved. Source and face identities still require job admission.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedMappedTextInput {
    pub(crate) span: SourceSpan,
    pub(crate) face: FaceId,
    pub(crate) measurement_face: Option<FaceId>,
    pub(crate) text: Box<str>,
    pub(crate) glyph_string_start: Option<DisplaySourcePosition>,
    pub(crate) layout: DisplayItemLayout,
    pub(crate) pointer_appearance: Option<DisplayPointerAppearance>,
    pub(crate) box_vertical_edges: BoxVerticalEdges,
    pub(crate) box_run_membership: BoxRunMembership,
}

impl ResolvedMappedTextInput {
    #[allow(clippy::result_large_err)]
    pub(crate) fn capture(item: DisplayItem, base_face: FaceId) -> Result<Self, DisplayItem> {
        let DisplayItemKind::SourceMappedText(mapped) = item.kind else {
            return Err(item);
        };
        if mapped.is_display_table_vector() || mapped.semantic_face_overlay().is_some() {
            return Err(DisplayItem {
                kind: DisplayItemKind::SourceMappedText(mapped),
                ..item
            });
        }
        Ok(Self {
            span: item.span,
            face: render_face_ref_id(item.face, base_face),
            measurement_face: mapped.measurement_face,
            text: mapped.text,
            glyph_string_start: mapped.glyph_string_start,
            layout: item.layout,
            pointer_appearance: item.pointer_appearance,
            box_vertical_edges: item.box_vertical_edges,
            box_run_membership: item.box_run_membership,
        })
    }
}

#[cfg(test)]
mod tests {
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
}
