use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayItemLayout, DisplayPointerAppearance, DisplayTextRun,
    SourceSpan,
};
use neomacs_display_protocol::face::{BoxRunMembership, BoxVerticalEdges};
use neomacs_display_protocol::types::FaceId;

/// An owned text append with no live Lisp objects or inherited text face.
///
/// Face and source IDs belong to the capturing presentation's namespaces.
/// They are opaque tokens here, not proof that a later job is still current.
/// Font realization and generation validation remain the caller's duties.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedTextInput {
    pub(crate) span: SourceSpan,
    pub(crate) face: FaceId,
    pub(crate) run: DisplayTextRun,
    pub(crate) layout: DisplayItemLayout,
    pub(crate) pointer_appearance: Option<DisplayPointerAppearance>,
    pub(crate) box_vertical_edges: BoxVerticalEdges,
    pub(crate) box_run_membership: BoxRunMembership,
}

impl ResolvedTextInput {
    /// Move supported text into the owned representation. Unsupported items
    /// are returned intact for canonical synchronous processing; no partial
    /// resolution or fallback guesses are installed in an owned text input.
    #[allow(clippy::result_large_err)]
    pub(crate) fn capture(item: DisplayItem, base_face: FaceId) -> Result<Self, DisplayItem> {
        let DisplayItemKind::TextRun(run) = item.kind else {
            return Err(item);
        };
        Ok(Self {
            span: item.span,
            face: render_face_ref_id(item.face, base_face),
            run,
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
    use crate::display_item::{DisplayLength, DisplayStretch, DisplayStretchWidth, RenderFaceRef};

    #[test]
    fn owned_text_resolves_inheritance_and_moves_across_threads() {
        fn require_send_sync<T: Send + Sync + 'static>() {}
        require_send_sync::<ResolvedTextInput>();
        let source = SourceSpan::synthetic(4, 0, 3);
        let input = ResolvedTextInput::capture(
            DisplayItem::new(
                source.clone(),
                RenderFaceRef::Inherit,
                DisplayItemKind::TextRun(DisplayTextRun::independent("aλ中")),
            ),
            FaceId::new(7),
        )
        .expect("ordinary text is supported");
        let input = std::thread::spawn(move || input).join().unwrap();
        assert_eq!(input.face, FaceId::new(7));
        assert_eq!(input.span, source);
        assert_eq!(input.run.text.as_ref(), "aλ中");
    }

    #[test]
    fn non_text_input_returns_unchanged_for_synchronous_processing() {
        let item = DisplayItem::new(
            SourceSpan::synthetic(2, 0, 1),
            RenderFaceRef::Inherit,
            DisplayItemKind::Stretch(DisplayStretch {
                width: DisplayStretchWidth::Length(DisplayLength::Pixels(12.0)),
                height: None,
                ascent: None,
            }),
        );
        let original = item.clone();
        assert_eq!(
            ResolvedTextInput::capture(item, FaceId::new(7)),
            Err(original)
        );
    }
}
