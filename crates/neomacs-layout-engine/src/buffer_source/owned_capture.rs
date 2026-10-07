//! Bounded acquisition through the canonical source producer. These values
//! remain on the evaluator thread until RowProgram removes all Lisp operands.

mod invisibility;
mod strings;

use super::consumption::BufferSourceConsumedItem;
use super::face_resolution::BufferSourceFaceResolutionContext;
use super::producer::BufferElementProducer;
use crate::display_item::{DisplayItem, DisplayItemKind};
use crate::display_source::DisplaySourceTextPosition;
use crate::display_source_resolver::PendingDisplaySourceFace;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{LayoutBufferView, LayoutCharPropertyLookup};
use crate::row_layout::program::RowProgramError;
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos, EmacsByteRange};
use neovm_core::emacs_core::Value;

pub(crate) struct CapturedPhysicalLine {
    pub items: Vec<DisplayItem>,
    pub roots: Vec<Value>,
    pub source_start: crate::display_item::DisplaySourcePosition,
    pub faces: Vec<PendingDisplaySourceFace>,
    pub end: CharPos0,
    pub complete: bool,
    pub trailing_text_continues: bool,
}

/// A conservative first source domain: complete physical lines, ordinary
/// resolved faces and literal item geometry. Window policy (prefixes, bidi,
/// display tables, trailing-whitespace and indicators) is checked by the
/// caller before entering this source-only operation.
#[cfg(test)]
pub(crate) fn capture_physical_line<B: LayoutBufferView>(
    buffer_id: BufferId,
    window: u64,
    start: CharPos0,
    max_chars: usize,
    max_items: usize,
    context: BufferSourceFaceResolutionContext<'_, B>,
    face_ids: &mut FrameFaceAttempt,
    cancelled: impl Fn() -> bool,
) -> Result<CapturedPhysicalLine, RowProgramError> {
    let fragment = capture_source_fragment(
        buffer_id, window, start, max_chars, max_items, false, context, face_ids, cancelled,
    )?;
    if fragment.complete {
        Ok(fragment)
    } else {
        Err(RowProgramError::Incomplete)
    }
}

/// Acquire one bounded fragment. Only the caller's preceding owned fragment
/// permits continuation entry; no evaluator operands may survive this call.
#[allow(clippy::too_many_arguments)]
pub(crate) fn capture_source_fragment<B: LayoutBufferView>(
    buffer_id: BufferId,
    window: u64,
    start: CharPos0,
    max_chars: usize,
    max_items: usize,
    continuing: bool,
    context: BufferSourceFaceResolutionContext<'_, B>,
    face_ids: &mut FrameFaceAttempt,
    cancelled: impl Fn() -> bool,
) -> Result<CapturedPhysicalLine, RowProgramError> {
    let buffer = context.buffer();
    let start_byte = buffer.layout_char_pos_to_emacs_byte_pos(start);
    if start_byte < buffer.layout_point_min_emacs_byte_pos() {
        return Err(RowProgramError::Unsupported);
    }
    if !continuing
        && start_byte > buffer.layout_point_min_emacs_byte_pos()
        && buffer.layout_emacs_byte_at_pos(EmacsBytePos::new(start_byte.get() - 1)) != Some(b'\n')
    {
        return Err(RowProgramError::Unsupported);
    }
    let end = CharPos0::new(start.get().saturating_add(max_chars))
        .min(buffer.layout_point_max_char_pos());
    let end_byte = buffer.layout_char_pos_to_emacs_byte_pos(end);
    if start >= end || max_items == 0 {
        return Err(RowProgramError::Budget);
    }
    // Inspect only the physical line this job will consume. Scanning the
    // bounded byte chunks stops at the first newline without copying text;
    // properties on later lines must not veto this line's admission.
    let mut offset = start_byte.get();
    let scan = buffer.layout_try_for_each_emacs_byte_range_chunk(
        EmacsByteRange::new(start_byte, end_byte),
        |chunk| {
            if cancelled() {
                return Err(None);
            }
            if let Some(index) = chunk.iter().position(|byte| *byte == b'\n') {
                return Err(Some(EmacsBytePos::new(offset + index + 1)));
            }
            offset += chunk.len();
            Ok(())
        },
    );
    let end_byte = match scan {
        Err(Some(newline_end)) => newline_end,
        Err(None) => return Err(RowProgramError::Cancelled),
        Ok(()) => end_byte,
    };
    let end = buffer.layout_emacs_byte_pos_to_char_pos(end_byte);
    let display_lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("display"));
    let height_lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("line-height"));
    let spacing_lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("line-spacing"));
    let invisible_lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("invisible"));
    let mut has_invisible = false;
    let window_lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("window"));
    let mut has_scoped_overlay = false;
    let lookups = ["composition", "line-prefix", "wrap-prefix"]
        .map(|name| LayoutCharPropertyLookup::new(buffer, Value::symbol(name)));
    let string_lookups = ["before-string", "after-string"]
        .map(|name| LayoutCharPropertyLookup::new(buffer, Value::symbol(name)));
    // Ordinary overlay faces and pointer metadata use the canonical producer.
    // Bound intersecting overlays before it can collect them or inspect any
    // replacement strings. Category/alias properties use the same effective
    // lookup as visible layout, so indirect replacements cannot bypass this.
    let mut roots = Vec::new();
    let overlays = buffer.layout_overlays();
    for (index, overlay) in overlays
        .iter_overlays_in_accessible_emacs_byte_range(
            EmacsByteRange::new(start_byte, end_byte),
            buffer.layout_point_max_emacs_byte_pos(),
        )
        .enumerate()
    {
        if cancelled() {
            return Err(RowProgramError::Cancelled);
        }
        if index >= max_items {
            return Err(RowProgramError::Budget);
        }
        roots.push(overlay);
        has_scoped_overlay |= window_lookup
            .effective_overlay_value(buffer, overlay)
            .and_then(|value| value.as_window_id())
            .is_some();
        if let Some(value) = invisible_lookup.effective_overlay_value(buffer, overlay) {
            if !value.is_symbol() {
                return Err(RowProgramError::Unsupported);
            }
            has_invisible |= !value.is_nil();
        }
        if overlays.overlay_applies_to_window(overlay, Some(window))
            && (height_lookup
                .effective_overlay_value(buffer, overlay)
                .is_some_and(|value| !literal_line_height(value))
                || spacing_lookup
                    .effective_overlay_value(buffer, overlay)
                    .is_some_and(|value| !literal_line_spacing(value))
                || lookups
                    .iter()
                    .any(|lookup| lookup.effective_overlay_value(buffer, overlay).is_some())
                || string_lookups.iter().any(|lookup| {
                    lookup
                        .effective_overlay_value(buffer, overlay)
                        .is_some_and(|value| !strings::bounded_string(value, max_chars))
                })
                || display_lookup
                    .effective_overlay_value(buffer, overlay)
                    .is_some_and(|value| {
                        !(literal_raise_or_nil(value)
                            || strings::bounded_replacement(value, max_chars))
                    }))
        {
            return Err(RowProgramError::Unsupported);
        }
    }
    // Property boundaries are bounded too: a hostile line with a different
    // property on every character cannot hide unbounded capture work.
    let mut pos = start_byte;
    let mut boundaries = 0;
    while pos < end_byte {
        if cancelled() {
            return Err(RowProgramError::Cancelled);
        }
        boundaries += 1;
        if boundaries > max_items {
            return Err(RowProgramError::Budget);
        }
        if let Some(value) = invisible_lookup.text_value_at(buffer, pos) {
            if !value.is_symbol() {
                return Err(RowProgramError::Unsupported);
            }
            has_invisible |= !value.is_nil();
        }
        if height_lookup
            .text_value_at(buffer, pos)
            .is_some_and(|value| !literal_line_height(value))
            || spacing_lookup
                .text_value_at(buffer, pos)
                .is_some_and(|value| !literal_line_spacing(value))
            || display_lookup
                .text_value_at(buffer, pos)
                .is_some_and(|value| {
                    !(literal_raise_or_nil(value) || strings::bounded_replacement(value, max_chars))
                })
            || lookups.iter().any(|lookup| {
                lookup
                    .text_value_at(buffer, pos)
                    .is_some_and(|value| !value.is_nil())
            })
        {
            return Err(RowProgramError::Unsupported);
        }
        pos = buffer
            .layout_next_text_prop_change_after_emacs_byte_pos(pos)
            .filter(|next| *next > pos)
            .unwrap_or(end_byte)
            .min(end_byte);
    }
    // The visible invisible-text checkpoint currently has no window scope.
    // Do not admit scoped-overlay combinations with different producer and
    // visibility filtering until both paths share that scope explicitly.
    if has_invisible && (has_scoped_overlay || !invisibility::bounded_spec(buffer, max_items)) {
        return Err(RowProgramError::Unsupported);
    }
    let mut active_buffer_face = None;
    let mut producer = BufferElementProducer::new_for_window_range(
        buffer_id,
        buffer,
        Some(window),
        start.get() as i64,
        end,
        start_byte.get(),
    );
    let mut position = DisplaySourceTextPosition::new(0, start.get() as i64);
    let mut items: Vec<DisplayItem> = Vec::new();
    let mut faces = Vec::new();
    let source_start =
        crate::display_item::DisplaySourcePosition::buffer(buffer_id, start, start_byte);
    for _ in 0..max_items {
        if cancelled() {
            return Err(RowProgramError::Cancelled);
        }
        if position.charpos() >= end.get() as i64 && !items.is_empty() {
            // The bounded producer may stop inside a shaping cluster. Retain
            // only spans closed by inspected source, and resume from that
            // certified boundary on the next idle slice.
            let last = items.last_mut().ok_or(RowProgramError::Incomplete)?;
            let DisplayItemKind::TextRun(run) = &mut last.kind else {
                return Err(RowProgramError::Unsupported);
            };
            let (chars, bytes) = if matches!(
                run.composition,
                crate::display_item::DisplayTextComposition::Automatic(_)
            ) {
                // Never trim a selected composition's owned cell plan. Retry
                // the whole final item with lookahead in the next slice.
                (0, 0)
            } else {
                crate::display_text_run_measurement::closed_measurement_prefix(&run.text)
            };
            if chars == 0 {
                items.pop();
            } else {
                run.text = run.text[..bytes].into();
                last.span.end = last.span.start.advanced_by(chars, bytes);
                last.box_vertical_edges =
                    neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
                        last.box_vertical_edges.owns_left(),
                        false,
                    );
            }
            let Some(crate::display_item::DisplaySourcePosition::Buffer { char_pos: end, .. }) =
                items.last().map(|item| &item.span.end)
            else {
                return Err(RowProgramError::Unsupported);
            };
            let end = *end;
            return Ok(CapturedPhysicalLine {
                items,
                roots,
                source_start,
                faces,
                end,
                complete: false,
                trailing_text_continues: chars > 0,
            });
        }
        if items.len() >= max_items {
            break;
        }
        if has_invisible
            && invisibility::capture_skip(
                buffer_id,
                buffer,
                start_byte,
                end,
                &mut position,
                &mut producer,
                active_buffer_face,
                &mut items,
                max_items,
            )?
        {
            continue;
        }
        let step = producer.produce_step(position, context, face_ids);
        if !step.pending_non_text_area.is_empty() {
            return Err(RowProgramError::Unsupported);
        }
        faces.extend(step.pending_faces);
        let Some(item) = step.source_item else {
            return Err(RowProgramError::Incomplete);
        };
        let item = match item {
            BufferSourceConsumedItem::Renderable(item) => item,
            BufferSourceConsumedItem::OverlayStrings(strings) => {
                strings::capture_insertions(
                    &strings, context, face_ids, &mut items, &mut faces, &mut roots, max_chars,
                    max_items, &cancelled,
                )?;
                continue;
            }
            BufferSourceConsumedItem::DisplayPropertyReplacement(replacement) => {
                let resume = CharPos0::new(replacement.descriptor().resume_charpos() as usize);
                // A replacement clipped by the acquisition limit cannot prove
                // its complete covered span, especially across a newline.
                if resume >= end {
                    return Err(RowProgramError::Unsupported);
                }
                strings::capture_replacement(
                    &replacement,
                    context,
                    face_ids,
                    &mut items,
                    &mut faces,
                    &mut roots,
                    max_chars,
                    max_items,
                    &cancelled,
                )?;
                // A pushed string can use another paint face; do not infer
                // the next hidden span's active buffer face from that string.
                active_buffer_face = None;
                position = DisplaySourceTextPosition::new(
                    buffer.layout_char_pos_to_emacs_byte_pos(resume).get() - start_byte.get(),
                    resume.get() as i64,
                );
                continue;
            }
        };
        let (_, end_char, end_byte, item) = item.into_render_parts();
        position = DisplaySourceTextPosition::new(
            end_byte.unwrap_or(step.source_position.byte_idx()),
            end_char.unwrap_or(step.source_position.charpos()),
        );
        if let DisplayItemKind::TextRun(run) = &item.kind
            && run.text.chars().any(|ch| {
                crate::display_source::nonascii_space_p(ch)
                    || crate::display_source::nonascii_hyphen_p(ch)
            })
        {
            // These require the buffer loop's nobreak face/substitution
            // policy, even though their source vocabulary is ordinary text.
            return Err(RowProgramError::Unsupported);
        }
        active_buffer_face = Some(item.face);
        let complete = matches!(item.kind, DisplayItemKind::RowBreak(_));
        if items.len() >= max_items {
            return Err(RowProgramError::Budget);
        }
        let text_bytes = items
            .iter()
            .chain(std::iter::once(&item))
            .map(|item| match &item.kind {
                DisplayItemKind::TextRun(run) => run.text.len(),
                DisplayItemKind::SourceMappedText(run) => run.text.len(),
                _ => 0,
            })
            .sum::<usize>();
        if text_bytes > max_chars.saturating_mul(4) {
            return Err(RowProgramError::Budget);
        }
        items.push(item);
        if complete {
            return Ok(CapturedPhysicalLine {
                items,
                roots,
                source_start,
                faces,
                end: CharPos0::new(position.charpos() as usize),
                complete: true,
                trailing_text_continues: false,
            });
        }
    }
    // A composition-rich line can exhaust the item budget before the byte
    // budget. Every completed producer item ends at a semantic boundary, so
    // retain that bounded prefix and continue acquisition on the next idle
    // step. An insertion-only suffix has no independent buffer resume point.
    let Some(crate::display_item::DisplaySourcePosition::Buffer {
        char_pos: resume, ..
    }) = items.last().map(|item| &item.span.end)
    else {
        return Err(RowProgramError::Budget);
    };
    let resume = *resume;
    if resume <= start || resume >= end {
        return Err(RowProgramError::Budget);
    }
    Ok(CapturedPhysicalLine {
        items,
        roots,
        source_start,
        faces,
        end: resume,
        complete: false,
        trailing_text_continues: false,
    })
}

/// The canonical producer resolves this modifier into owned item geometry.
/// Admit only a finite literal operand: no conditions, expressions, compound
/// specs or replacement objects can cross this bounded preflight shortcut.
fn literal_raise_or_nil(value: Value) -> bool {
    if value.is_nil() {
        return true;
    }
    if !value.is_cons() || !value.cons_car().is_symbol_named("raise") {
        return false;
    }
    let tail = value.cons_cdr();
    tail.is_cons()
        && tail.cons_cdr().is_nil()
        && tail
            .cons_car()
            .as_number_f64()
            .is_some_and(|number| number.is_finite() && (number as f32).is_finite())
}

#[cfg(test)]
#[path = "tests/owned_capture_test.rs"]
mod tests;

fn literal_line_height(value: Value) -> bool {
    value.is_nil()
        || value.is_t()
        || value.is_fixnum()
        || (value.is_float() && (value.xfloat() as f32).is_finite())
}

fn literal_line_spacing(value: Value) -> bool {
    value.is_nil()
        || value.is_fixnum()
        || (value.is_float() && value.xfloat().is_finite() && (value.xfloat() as f32).is_finite())
}
