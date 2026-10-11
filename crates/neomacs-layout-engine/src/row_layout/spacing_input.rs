use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayItemLayout, DisplayLength, DisplayPointerAppearance,
    DisplayStretch, DisplayStretchWidth, RenderFaceRef, SourceSpan,
};
use neomacs_display_protocol::face::{BoxRunMembership, BoxVerticalEdges};
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::emacs_char::EmacsChar;

/// A literal length. Unlike DisplayLength, this cannot retain a Lisp operand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ResolvedLength {
    Pixels(f32),
    Em(f32),
}

impl ResolvedLength {
    fn capture(length: &DisplayLength) -> Option<Self> {
        match length {
            DisplayLength::Pixels(value) => Some(Self::Pixels(*value)),
            DisplayLength::Em(value) => Some(Self::Em(*value)),
            DisplayLength::Expr(_) => None,
        }
    }

    fn into_display_length(self) -> DisplayLength {
        match self {
            Self::Pixels(value) => DisplayLength::Pixels(value),
            Self::Em(value) => DisplayLength::Em(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ResolvedStretchWidth {
    Length(ResolvedLength),
    RelativeToSource { factor: f32, source: EmacsChar },
}

/// Spacing with source semantics resolved and no evaluator operands. Relative
/// dimensions still use the job's captured row/font measurement environment.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedSpacingInput {
    span: SourceSpan,
    face: FaceId,
    width: ResolvedStretchWidth,
    height: Option<ResolvedLength>,
    ascent: Option<ResolvedLength>,
    layout: DisplayItemLayout,
    pointer_appearance: Option<DisplayPointerAppearance>,
    box_vertical_edges: BoxVerticalEdges,
    box_run_membership: BoxRunMembership,
}

impl ResolvedSpacingInput {
    pub(crate) fn source_span(&self) -> &SourceSpan {
        &self.span
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn capture(item: DisplayItem, base_face: FaceId) -> Result<Self, DisplayItem> {
        let DisplayItemKind::Stretch(stretch) = &item.kind else {
            return Err(item);
        };
        let width = match &stretch.width {
            DisplayStretchWidth::Length(length) => {
                let Some(length) = ResolvedLength::capture(length) else {
                    return Err(item);
                };
                ResolvedStretchWidth::Length(length)
            }
            DisplayStretchWidth::RelativeToSource { factor, source } => {
                ResolvedStretchWidth::RelativeToSource {
                    factor: *factor,
                    source: *source,
                }
            }
            DisplayStretchWidth::AlignTo(_) => return Err(item),
        };
        let capture_optional = |length: &Option<DisplayLength>| match length {
            None => Some(None),
            Some(length) => ResolvedLength::capture(length).map(Some),
        };
        let (Some(height), Some(ascent)) = (
            capture_optional(&stretch.height),
            capture_optional(&stretch.ascent),
        ) else {
            return Err(item);
        };
        Ok(Self {
            span: item.span,
            face: render_face_ref_id(item.face, base_face),
            width,
            height,
            ascent,
            layout: item.layout,
            pointer_appearance: item.pointer_appearance,
            box_vertical_edges: item.box_vertical_edges,
            box_run_membership: item.box_run_membership,
        })
    }

    pub(crate) fn into_display_item(self) -> DisplayItem {
        let width = match self.width {
            ResolvedStretchWidth::Length(length) => {
                DisplayStretchWidth::Length(length.into_display_length())
            }
            ResolvedStretchWidth::RelativeToSource { factor, source } => {
                DisplayStretchWidth::RelativeToSource { factor, source }
            }
        };
        DisplayItem {
            span: self.span,
            face: RenderFaceRef::FaceId(self.face),
            kind: DisplayItemKind::Stretch(DisplayStretch {
                width,
                height: self.height.map(ResolvedLength::into_display_length),
                ascent: self.ascent.map(ResolvedLength::into_display_length),
            }),
            layout: self.layout,
            pointer_appearance: self.pointer_appearance,
            box_vertical_edges: self.box_vertical_edges,
            box_run_membership: self.box_run_membership,
        }
    }
}

#[cfg(test)]
#[path = "tests/spacing_input_test.rs"]
mod tests;
