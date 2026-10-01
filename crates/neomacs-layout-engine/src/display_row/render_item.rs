use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayTextComposition, DisplayTextRun, SourceSpan,
};
#[cfg(test)]
use crate::display_item::{DisplaySourceMappedText, DisplaySourcePosition, RenderFaceRef};
use crate::display_row::builder::DisplayRowAppendProgress;

pub(crate) struct DisplayRowRenderItem {
    source_item: DisplayItem,
}

impl DisplayRowRenderItem {
    pub(crate) fn from_source_item(source_item: DisplayItem) -> Self {
        // Preserve media as one row item.  The row writer now emits a typed
        // media glyph that owns both its layout metrics and drawable identity.
        // Measurement, writing and continuation all use the same item. Keep
        // its owned source representation once; the writer's copy is needed
        // only because a clipped item may still have to resume on another row.
        Self { source_item }
    }

    pub(crate) fn source_item(&self) -> &DisplayItem {
        &self.source_item
    }

    #[cfg(test)]
    pub(crate) fn row_face(&self) -> RenderFaceRef {
        self.source_item.face
    }

    pub(crate) fn row_item(&self) -> &DisplayItem {
        &self.source_item
    }

    pub(crate) fn row_item_for_write(&self) -> DisplayItem {
        self.source_item.clone()
    }

    pub(crate) fn clipped_remainder(
        self,
        progress: &DisplayRowAppendProgress,
    ) -> DisplayRowClippedRemainder {
        clipped_display_item_remainder_after_chars(self.source_item, progress.emitted_glyphs())
    }
}

/// What is left of a display item after the row stopped on it.
///
/// GNU `display_line` has three ways to leave an element that did not fit
/// (src/xdisp.c:26320-26480), and they are not interchangeable: it keeps the
/// glyphs that fit and resumes the element on the next row, it removes the
/// element *whole* so the next row re-produces it, or the element has nothing
/// left to place.  Modelling them as an `Option` made the middle case
/// unrepresentable -- a media glyph the row had just refused read as "nothing
/// to remember" -- so a refused image silently vanished instead of moving
/// down.  `DeferWhole` is that case as a first-class outcome.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayRowClippedRemainder {
    /// Part of the item is on this row; the unrendered tail resumes on the
    /// next row.
    Resume(DisplayItem),
    /// Not one glyph of this item reached the row.  GNU unproduces the element
    /// and restores its iterator to before it (`display_line`,
    /// src/xdisp.c:26448-26475: "Restore positions to values before the
    /// element"), then produces it again at the start of the next row -- where
    /// `hpos == 0`, so a wide image is laid out against a whole row instead of
    /// the fragment left over here.  The item carries over whole, uncropped,
    /// with the face, layout and box topology it was produced with.
    DeferWhole(DisplayItem),
    /// The row placed everything this item could still contribute.
    ///
    /// This is the answer for an item that was fully emitted, and for an
    /// atomic item a structural lane clipped in place: such an item has no
    /// resumable tail, and re-producing it would duplicate the glyph the lane
    /// already drew.
    Nothing,
}

/// `emitted_glyphs` is how much of `item` the row kept: one per emitted glyph,
/// zero when the row refused the item outright.
pub(crate) fn clipped_display_item_remainder_after_chars(
    item: DisplayItem,
    emitted_glyphs: usize,
) -> DisplayRowClippedRemainder {
    let DisplayItem {
        span,
        face,
        kind,
        layout,
        pointer_appearance,
        box_vertical_edges,
        box_run_membership,
    } = item;
    if emitted_glyphs == 0 {
        return DisplayRowClippedRemainder::DeferWhole(DisplayItem {
            span,
            face,
            kind,
            layout,
            pointer_appearance,
            box_vertical_edges,
            box_run_membership,
        });
    }
    let remainder_edges = neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
        false,
        box_vertical_edges.owns_right(),
    );
    match kind {
        DisplayItemKind::TextRun(run) => {
            if matches!(&run.composition, DisplayTextComposition::Automatic(_)) {
                // A composed cluster cannot resume mid-cluster here, so the
                // tail of a partially emitted automatic composition is not
                // carried over.  GNU's multi-glyph rule keeps the glyphs that
                // fit and leaves the rest for the continuation line
                // (src/xdisp.c:26196-26200).
                return DisplayRowClippedRemainder::Nothing;
            }
            let Some((split_byte, remaining)) =
                clipped_text_remainder(run.text.as_ref(), emitted_glyphs)
            else {
                return DisplayRowClippedRemainder::Nothing;
            };
            DisplayRowClippedRemainder::Resume(DisplayItem {
                span: SourceSpan::new(span.start.advanced_by(emitted_glyphs, split_byte), span.end),
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
            let Some(remainder) = text.into_remainder_after(emitted_glyphs) else {
                return DisplayRowClippedRemainder::Nothing;
            };
            DisplayRowClippedRemainder::Resume(DisplayItem {
                span,
                face,
                kind: DisplayItemKind::SourceMappedText(remainder),
                layout,
                pointer_appearance,
                box_vertical_edges: remainder_edges,
                box_run_membership,
            })
        }
        // Media, stretches and the other single-glyph items are atomic: the
        // row has already written its one glyph, so there is no tail to
        // resume.  (The `emitted_glyphs == 0` case above is the refusal.)
        _ => DisplayRowClippedRemainder::Nothing,
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

#[cfg(test)]
#[path = "render_item/tests/render_item_test.rs"]
mod tests;
