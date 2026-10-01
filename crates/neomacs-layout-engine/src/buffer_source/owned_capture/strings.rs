//! Flatten bounded strings with the canonical Lisp-string cursor. Live source
//! operands return to the evaluator for rooting, never to the row worker.
use super::*;
use crate::display_origin::{DisplayOrigin, OverlayStringKind};
use crate::display_source::{
    DisplayItemSource, DisplaySourceContext, LispStringSourceCursor, LispStringSourceOrigin,
};
use crate::display_source_resolver::{
    DisplayDefaultFaceInstallPolicy, DisplaySourcePropertyResolver, DisplaySourceResolveState,
};

pub(super) fn bounded_string(value: Value, max_chars: usize) -> bool {
    if value.is_nil() {
        return true;
    }
    let Some(string) = value.as_lisp_string() else {
        return false;
    };
    if string.schars() > max_chars || string.as_bytes().len() > max_chars.saturating_mul(4) {
        return false;
    }
    let props = neovm_core::emacs_core::value::get_string_text_properties_table_for_value(value);
    let Some(props) = props else {
        return true;
    };
    let unsupported = [
        "invisible",
        "composition",
        "cursor",
        "line-prefix",
        "wrap-prefix",
        "display",
    ]
    .map(Value::symbol);
    !props.has_any_non_nil_property_in_char_range(
        neovm_core::buffer::CharRange::new(CharPos0::ZERO, CharPos0::new(string.schars())),
        &unsupported,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_insertions<B: LayoutBufferView>(
    strings: &super::super::text_source::BufferOverlayStringsItem,
    context: BufferSourceFaceResolutionContext<'_, B>,
    face_ids: &mut FrameFaceAttempt,
    items: &mut Vec<DisplayItem>,
    faces: &mut Vec<PendingDisplaySourceFace>,
    roots: &mut Vec<Value>,
    max_chars: usize,
    max_items: usize,
    cancelled: &impl Fn() -> bool,
) -> Result<(), RowProgramError> {
    let mut bytes = items
        .iter()
        .map(|item| match &item.kind {
            DisplayItemKind::TextRun(run) => run.text.len(),
            DisplayItemKind::SourceMappedText(run) => run.text.len(),
            _ => 0,
        })
        .sum::<usize>();
    for (index, entry) in strings.strings().iter().enumerate() {
        if cancelled() {
            return Err(RowProgramError::Cancelled);
        }
        if !bounded_string(entry.string, max_chars) {
            return Err(RowProgramError::Unsupported);
        }
        bytes = bytes.saturating_add(
            entry
                .string
                .as_lisp_string()
                .ok_or(RowProgramError::Unsupported)?
                .as_bytes()
                .len(),
        );
        if bytes > max_chars.saturating_mul(4) {
            return Err(RowProgramError::Budget);
        }
        if roots.len() >= max_items.saturating_mul(2) {
            return Err(RowProgramError::Budget);
        }
        roots.extend([entry.string, entry.overlay_id]);
        let kind = if entry.after_string_p {
            OverlayStringKind::After
        } else {
            OverlayStringKind::Before
        };
        let origin = DisplayOrigin::OverlayString {
            overlay_id: entry.overlay_id,
            anchor_charpos: strings.anchor_charpos(),
            kind,
        };
        let base = context.resolve_display_string_base_face(
            origin,
            origin.default_base_face_policy(),
            None,
            DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
            face_ids,
        );
        let mut source = LispStringSourceCursor::new_with_box_boundaries(
            1,
            entry.string,
            crate::display_item::RenderFaceRef::FaceId(base.face_id()),
            LispStringSourceOrigin::OverlayString {
                overlay_id: entry.overlay_id,
                kind,
            },
            strings
                .box_boundaries()
                .sequence_member(index, strings.strings().len()),
        )
        .ok_or(RowProgramError::Unsupported)?;
        append_source(
            context,
            &base,
            &mut source,
            face_ids,
            items,
            faces,
            max_items,
            cancelled,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_source<B: LayoutBufferView, S: DisplayItemSource>(
    context: BufferSourceFaceResolutionContext<'_, B>,
    base: &crate::display_source_resolver::DisplayStringBaseFace,
    source: &mut S,
    face_ids: &mut FrameFaceAttempt,
    items: &mut Vec<DisplayItem>,
    faces: &mut Vec<PendingDisplaySourceFace>,
    max_items: usize,
    cancelled: &impl Fn() -> bool,
) -> Result<(), RowProgramError> {
    if let Some(face) = base.pending_face() {
        faces.push(face.clone());
    }
    let mut state = DisplaySourceResolveState::default();
    state.remember_face(base.face_id(), base.face());
    let params = context.string_source_resolve_params(&base);
    let mut resolver = DisplaySourcePropertyResolver::buffer_local(
        context.buffer(),
        params,
        &mut state,
        face_ids,
        faces,
    );
    let mut non_text = Vec::new();
    let mut source_context = DisplaySourceContext::with_face_resolver_and_non_text_area_sink(
        &mut resolver,
        &mut non_text,
        context.buffer().layout_display_target(),
    )
    .with_automatic_composition(params.automatic_composition);
    loop {
        if cancelled() {
            return Err(RowProgramError::Cancelled);
        }
        let Some(item) = source.next_item(&mut source_context) else {
            break;
        };
        if items.len() >= max_items {
            return Err(RowProgramError::Budget);
        }
        if !matches!(
            &item.kind,
            DisplayItemKind::TextRun(_) | DisplayItemKind::SourceMappedText(_)
        ) {
            return Err(RowProgramError::Unsupported);
        }
        items.push(item);
    }
    if !non_text.is_empty() {
        return Err(RowProgramError::Unsupported);
    }
    Ok(())
}

pub(super) fn bounded_replacement(value: Value, max_chars: usize) -> bool {
    value
        .as_lisp_string()
        .is_some_and(|string| string.schars() > 0)
        && bounded_string(value, max_chars)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_replacement<B: LayoutBufferView>(
    replacement: &crate::display_item::BufferDisplayPropertyReplacementItem,
    context: BufferSourceFaceResolutionContext<'_, B>,
    face_ids: &mut FrameFaceAttempt,
    items: &mut Vec<DisplayItem>,
    faces: &mut Vec<PendingDisplaySourceFace>,
    roots: &mut Vec<Value>,
    max_chars: usize,
    max_items: usize,
    cancelled: &impl Fn() -> bool,
) -> Result<(), RowProgramError> {
    let descriptor = replacement.descriptor();
    let classification = descriptor.classification();
    let value = classification.replacement_spec();
    if !matches!(
        classification.replacement(),
        Some(crate::display_property::DisplayReplacementProperty::String)
    ) || !bounded_replacement(value, max_chars)
    {
        return Err(RowProgramError::Unsupported);
    }
    let bytes = items
        .iter()
        .map(|item| match &item.kind {
            DisplayItemKind::TextRun(run) => run.text.len(),
            DisplayItemKind::SourceMappedText(run) => run.text.len(),
            _ => 0,
        })
        .sum::<usize>();
    if bytes.saturating_add(
        value
            .as_lisp_string()
            .ok_or(RowProgramError::Unsupported)?
            .as_bytes()
            .len(),
    ) > max_chars.saturating_mul(4)
        || roots.len() >= max_items.saturating_mul(2)
    {
        return Err(RowProgramError::Budget);
    }
    roots.push(value);
    let origin = DisplayOrigin::DisplayPropertyString {
        anchor_charpos: descriptor.anchor_charpos(),
        source: crate::display_origin::DisplayPropertySource::TextProperty,
    };
    let base = context.resolve_display_string_base_face(
        origin,
        origin.default_base_face_policy(),
        None,
        DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
        face_ids,
    );
    let mut source = crate::display_source::BufferDisplayReplacementStringRequest::new(
        1,
        value,
        descriptor.replacement_source(),
    )
    .with_pointer_appearance(descriptor.pointer_appearance().cloned())
    .with_box_boundaries(descriptor.box_boundaries())
    .into_source(base.face_id())
    .ok_or(RowProgramError::Unsupported)?;
    let (measurement_face, resolved) =
        context.resolve_routed_position_face(face_ids, descriptor.anchor_charpos());
    faces.push(PendingDisplaySourceFace::new(measurement_face, resolved));
    let begin = items.len();
    append_source(
        context,
        &base,
        &mut source,
        face_ids,
        items,
        faces,
        max_items,
        cancelled,
    )?;
    for item in &mut items[begin..] {
        if let DisplayItemKind::SourceMappedText(text) = &mut item.kind {
            text.measurement_face = Some(measurement_face);
        }
    }
    Ok(())
}
