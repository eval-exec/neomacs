use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayTextComposition, DisplayTextRun, SourceSpan,
};
#[cfg(test)]
use crate::display_item::{DisplaySourceMappedText, DisplaySourcePosition, RenderFaceRef};
use crate::display_row::builder::DisplayRowAppendProgress;

pub(crate) struct DisplayRowRenderItem {
    source_item: DisplayItem,
    row_item: DisplayItem,
}

impl DisplayRowRenderItem {
    pub(crate) fn from_source_item(source_item: DisplayItem) -> Self {
        // Preserve media as one row item.  The row writer now emits a typed
        // media glyph that owns both its layout metrics and drawable identity.
        let row_item = source_item.clone();
        Self {
            source_item,
            row_item,
        }
    }

    pub(crate) fn source_item(&self) -> &DisplayItem {
        &self.source_item
    }

    #[cfg(test)]
    pub(crate) fn row_face(&self) -> RenderFaceRef {
        self.row_item.face
    }

    pub(crate) fn row_item(&self) -> &DisplayItem {
        &self.row_item
    }

    pub(crate) fn row_item_for_write(&self) -> DisplayItem {
        self.row_item.clone()
    }

    pub(crate) fn clipped_remainder(
        self,
        progress: &DisplayRowAppendProgress,
    ) -> Option<DisplayItem> {
        clipped_display_item_remainder(self.source_item, progress)
    }
}

fn clipped_display_item_remainder(
    item: DisplayItem,
    progress: &DisplayRowAppendProgress,
) -> Option<DisplayItem> {
    clipped_display_item_remainder_after_chars(item, progress.slots().len())
}

fn clipped_display_item_remainder_after_chars(
    item: DisplayItem,
    emitted_chars: usize,
) -> Option<DisplayItem> {
    let DisplayItem {
        span,
        face,
        kind,
        layout,
        pointer_appearance,
        box_vertical_edges,
        box_run_membership,
    } = item;
    let remainder_edges = neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
        false,
        box_vertical_edges.owns_right(),
    );
    match kind {
        DisplayItemKind::TextRun(run) => {
            if emitted_chars > 0 && matches!(&run.composition, DisplayTextComposition::Automatic(_))
            {
                return None;
            }
            let (split_byte, remaining) = clipped_text_remainder(run.text.as_ref(), emitted_chars)?;
            Some(DisplayItem {
                span: SourceSpan::new(span.start.advanced_by(emitted_chars, split_byte), span.end),
                face,
                kind: DisplayItemKind::TextRun(DisplayTextRun::with_composition(
                    remaining,
                    run.composition,
                )),
                layout,
                pointer_appearance,
                box_vertical_edges: remainder_edges,
                box_run_membership,
            })
        }
        DisplayItemKind::SourceMappedText(text) => {
            let remainder = text.into_remainder_after(emitted_chars)?;
            Some(DisplayItem {
                span,
                face,
                kind: DisplayItemKind::SourceMappedText(remainder),
                layout,
                pointer_appearance,
                box_vertical_edges: remainder_edges,
                box_run_membership,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_item::{
        DisplayImageItem, DisplayMediaReplacement, DisplayMediaReplacementKind,
    };

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
}

fn clipped_text_remainder(text: &str, emitted_chars: usize) -> Option<(usize, String)> {
    if emitted_chars >= text.chars().count() {
        return None;
    }
    let split_byte = text
        .char_indices()
        .nth(emitted_chars)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len());
    Some((split_byte, text[split_byte..].to_string()))
}
