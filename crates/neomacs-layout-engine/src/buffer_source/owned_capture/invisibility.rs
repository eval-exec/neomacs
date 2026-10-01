//! Bounded invisible source spans; glyph provenance stays synthetic while
//! an empty buffer item records the skipped source extent.

use super::*;
use crate::display_item::{DisplaySourcePosition, DisplayTextRun, RenderFaceRef, SourceSpan};
use crate::display_row::source_append::SyntheticTextMarker;
use crate::display_source::SyntheticTextItemSource;
use crate::neovm_bridge::RustTextPropAccess;
use neovm_core::window::WindowLayoutVariable;

pub(super) fn bounded_spec<B: LayoutBufferView>(buffer: &B, budget: usize) -> bool {
    let mut spec = buffer
        .layout_buffer_local_value(WindowLayoutVariable::BufferInvisibilitySpec)
        .unwrap_or(Value::T);
    if spec.is_symbol() {
        return true;
    }
    for _ in 0..budget {
        if spec.is_nil() {
            return true;
        }
        if !spec.is_cons() {
            return false;
        }
        let tag = spec.cons_car();
        if !(tag.is_symbol()
            || (tag.is_cons() && tag.cons_car().is_symbol() && tag.cons_cdr().is_symbol()))
        {
            return false;
        }
        spec = spec.cons_cdr();
    }
    spec.is_nil()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_skip<B: LayoutBufferView>(
    buffer_id: BufferId,
    buffer: &B,
    start_byte: EmacsBytePos,
    limit: CharPos0,
    position: &mut DisplaySourceTextPosition,
    producer: &mut BufferElementProducer<'_, B>,
    active_face: Option<RenderFaceRef>,
    items: &mut Vec<DisplayItem>,
    max_items: usize,
) -> Result<bool, RowProgramError> {
    let props = RustTextPropAccess::new(buffer);
    let start = position.charpos();
    if props.replacing_display_at(start) {
        return Ok(false);
    }
    let (status, end) = props
        .invisible_run_before(start, limit.get() as i64)
        .ok_or(RowProgramError::Unsupported)?;
    if !status.hidden() {
        return Ok(false);
    }
    if status.preserves_boundary_overlay_strings()
        && producer.has_pending_overlay_strings_at(*position)
    {
        return Ok(false);
    }
    // At a fragment entry there may be no known preceding active face yet.
    let face = active_face.ok_or(RowProgramError::Unsupported)?;
    let needed = 1 + usize::from(status.ellipsis());
    if items.len().saturating_add(needed) > max_items {
        return Err(RowProgramError::Budget);
    }
    let begin = CharPos0::new(start as usize);
    let end = CharPos0::new(end as usize);
    let begin_byte = buffer.layout_char_pos_to_emacs_byte_pos(begin);
    let end_byte = buffer.layout_char_pos_to_emacs_byte_pos(end);
    items.push(DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::buffer(buffer_id, begin, begin_byte),
            DisplaySourcePosition::buffer(buffer_id, end, end_byte),
        ),
        face,
        DisplayItemKind::TextRun(DisplayTextRun::independent("")),
    ));
    if status.ellipsis() {
        let marker = SyntheticTextMarker::InvisibleEllipsis;
        // Display tables are excluded by the page policy, so this is the
        // same default synthetic source used by the canonical invisible walk.
        items.push(
            SyntheticTextItemSource::new(marker.source_id(), marker.text(), face, 0)
                .into_item()
                .ok_or(RowProgramError::Unsupported)?,
        );
    }
    *position = DisplaySourceTextPosition::new(end_byte.get() - start_byte.get(), end.get() as i64);
    producer.consume_prefix_to(end.get() as i64);
    Ok(true)
}
