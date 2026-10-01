//! Join adjacent, already measured source fragments on the worker. The main
//! thread never retains unrooted source items across idle boundaries.
use super::*;
use std::hash::Hash;

impl Operation {
    fn span(&self) -> &SourceSpan {
        match self {
            Self::Text(input) => &input.span,
            Self::Mapped(input) => &input.span,
            Self::Space(input) => input.source_span(),
            Self::Break { span, .. } => span,
        }
    }
}

impl RowProgram {
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self.operations.last(), Some((Operation::Break { .. }, _)))
    }

    /// Fragment reservations were checked at mailbox submission. Assembly
    /// preserves their sum and the same batch ceilings. Joined text remains
    /// bounded by that reservation; native font handles never move between jobs.
    pub(crate) fn append_measured_fragment(
        &mut self,
        mut fragment: Self,
        maximum: RowProgramLimits,
    ) -> Result<(), RowProgramError> {
        if self.is_complete()
            || self.geometry != fragment.geometry
            || self.deferred_fonts.is_some()
            || fragment.deferred_fonts.is_some()
            || self.operations.last().map(|(op, _)| &op.span().end)
                != fragment.operations.first().map(|(op, _)| &op.span().start)
            || self.measurements.backend != fragment.measurements.backend
            || self.measurements.char_width != fragment.measurements.char_width
        {
            return Err(RowProgramError::Unsupported);
        }
        let limits = RowProgramLimits {
            items: self
                .limits
                .items
                .checked_add(fragment.limits.items)
                .ok_or(RowProgramError::Budget)?,
            text_bytes: self
                .limits
                .text_bytes
                .checked_add(fragment.limits.text_bytes)
                .ok_or(RowProgramError::Budget)?,
            glyphs: self
                .limits
                .glyphs
                .checked_add(fragment.limits.glyphs)
                .ok_or(RowProgramError::Budget)?,
        };
        if limits.items > maximum.items
            || limits.text_bytes > maximum.text_bytes
            || limits.glyphs > maximum.glyphs
        {
            return Err(RowProgramError::Budget);
        }
        for face in fragment.faces {
            if let Some(previous) = self
                .faces
                .iter()
                .find(|previous| previous.face_id == face.face_id)
            {
                if previous.render_face() != face.render_face() || previous.metrics != face.metrics
                {
                    return Err(RowProgramError::Cancelled);
                }
            } else {
                self.faces.push(face);
            }
        }
        merge_measurements(
            &mut self.measurements.advances,
            fragment.measurements.advances,
        )?;
        merge_measurements(
            &mut self.measurements.vertical,
            fragment.measurements.vertical,
        )?;
        merge_measurements(&mut self.measurements.faces, fragment.measurements.faces)?;
        self.measurements.missing |= fragment.measurements.missing;
        // Restore the producer run split only by the acquisition budget.
        // Keeping this artificial boundary would change whole-run admission
        // and scalar fallback near a visual edge (notably emoji metrics).
        if let (
            Some((Operation::Text(left), left_plan)),
            Some((Operation::Text(right), right_plan)),
        ) = (self.operations.last_mut(), fragment.operations.first())
            && self.trailing_text_continues
            && left.span.end == right.span.start
            && left.face == right.face
            && left.run.composition == right.run.composition
            && left.layout == right.layout
            && left.pointer_appearance == right.pointer_appearance
            && left.box_run_membership == right.box_run_membership
            && (!left.box_run_membership.is_boxed()
                || (!left.box_vertical_edges.owns_right() && !right.box_vertical_edges.owns_left()))
        {
            let chars = left.run.text.chars().count();
            let bytes = left.run.text.len();
            let mut advances =
                match std::mem::replace(left_plan, DisplayTextRunMeasurement::PerChar) {
                    DisplayTextRunMeasurement::Measured(advances) => advances,
                    DisplayTextRunMeasurement::PerChar => Vec::new(),
                };
            advances.extend(
                right_plan
                    .measured_advances()
                    .unwrap_or_default()
                    .iter()
                    .cloned()
                    .map(|mut advance| {
                        advance.char_offset += chars;
                        advance.byte_offset += bytes;
                        advance
                    }),
            );
            if !advances.is_empty() {
                *left_plan = DisplayTextRunMeasurement::Measured(advances);
            }
            left.run.text = format!("{}{}", left.run.text, right.run.text).into();
            left.span.end = right.span.end.clone();
            left.box_vertical_edges =
                neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
                    left.box_vertical_edges.owns_left(),
                    right.box_vertical_edges.owns_right(),
                );
            fragment.operations.remove(0);
        }
        self.trailing_text_continues = fragment.trailing_text_continues;
        self.operations.extend(fragment.operations);
        self.limits = limits;
        Ok(())
    }
}

fn merge_measurements<K: Eq + Hash, V: PartialEq>(
    destination: &mut FxHashMap<K, V>,
    source: FxHashMap<K, V>,
) -> Result<(), RowProgramError> {
    use std::collections::hash_map::Entry;
    for (key, value) in source {
        match destination.entry(key) {
            Entry::Occupied(entry) if entry.get() != &value => {
                return Err(RowProgramError::Cancelled);
            }
            Entry::Occupied(_) => {}
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
        }
    }
    Ok(())
}
