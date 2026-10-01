//! Capture pointer-source metadata before row production.
//!
//! Lisp values occur only in capture arguments. Stored source ranges contain
//! the same opaque identities that travel with published glyphs, not live
//! overlay values. Their lifetime remains owned by presentation bookkeeping;
//! capturing a token does not root or extend the lifetime of its Lisp object.

#[cfg(test)]
use super::DisplaySourceId;
use super::{DisplaySourcePosition, RenderFaceRef};
use neomacs_display_protocol::glyph_matrix::{
    GlyphPointerOccurrenceIdentity, GlyphPointerSourceIdentity, GlyphPointerSourceKind,
};
use neovm_core::buffer::{BufferId, CharPos0};
use neovm_core::emacs_core::Value;

/// Semantic source range whose rendered primitives share one transient
/// `mouse-face` appearance.  The end position (together with the source
/// identity) is stable when a run is clipped and resumed on a wrapped row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayPointerSourceRange {
    // Captured on the evaluator thread. The row producer only copies opaque
    // protocol identities; it never dereferences an overlay or a Lisp string.
    identity: GlyphPointerSourceIdentity,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) enum DisplayPointerOccurrence {
    #[default]
    Source,
    OverlayString {
        overlay_id: Value,
        kind: crate::display_origin::OverlayStringKind,
    },
    BufferDisplayReplacement {
        buffer_id: BufferId,
        anchor_charpos: CharPos0,
    },
}

impl DisplayPointerSourceRange {
    #[cfg(test)]
    pub(crate) fn ending_at(source: DisplaySourcePosition, end_char_index: usize) -> Self {
        Self::effective(source, 0, end_char_index, None)
    }

    pub(crate) fn effective(
        source: DisplaySourcePosition,
        start_char_index: usize,
        end_char_index: usize,
        overlay_owner: Option<Value>,
    ) -> Self {
        let (kind, source_id) = match source {
            DisplaySourcePosition::Buffer { buffer_id, .. } => {
                (GlyphPointerSourceKind::Buffer, buffer_id.0)
            }
            DisplaySourcePosition::LispString { source_id, .. } => {
                (GlyphPointerSourceKind::LispString, source_id.get())
            }
            DisplaySourcePosition::Synthetic { source_id, .. } => {
                (GlyphPointerSourceKind::Synthetic, source_id.get())
            }
        };
        Self {
            identity: GlyphPointerSourceIdentity {
                kind,
                source_id,
                range_start: start_char_index as u64,
                range_end: end_char_index as u64,
                property_owner: overlay_owner.map_or(0, |owner| owner.bits() as u64),
                occurrence: GlyphPointerOccurrenceIdentity::Source,
            },
        }
    }

    pub(crate) fn in_occurrence(mut self, occurrence: DisplayPointerOccurrence) -> Self {
        self.identity.occurrence = match occurrence {
            DisplayPointerOccurrence::Source => GlyphPointerOccurrenceIdentity::Source,
            DisplayPointerOccurrence::OverlayString { overlay_id, kind } => {
                GlyphPointerOccurrenceIdentity::OverlayString {
                    overlay_id: overlay_id.bits() as u64,
                    after: matches!(kind, crate::display_origin::OverlayStringKind::After),
                }
            }
            DisplayPointerOccurrence::BufferDisplayReplacement {
                buffer_id,
                anchor_charpos,
            } => GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                buffer_id: buffer_id.0,
                anchor: anchor_charpos.get() as u64,
            },
        };
        self
    }

    #[cfg(test)]
    pub(crate) fn buffer_id(&self) -> Option<BufferId> {
        (self.identity.kind == GlyphPointerSourceKind::Buffer)
            .then_some(BufferId(self.identity.source_id))
    }

    #[cfg(test)]
    pub(crate) fn source_id(&self) -> Option<DisplaySourceId> {
        (self.identity.kind != GlyphPointerSourceKind::Buffer)
            .then_some(DisplaySourceId::new(self.identity.source_id))
    }

    #[cfg(test)]
    pub(crate) const fn start_char_index(&self) -> usize {
        self.identity.range_start as usize
    }

    #[cfg(test)]
    pub(crate) const fn end_char_index(&self) -> usize {
        self.identity.range_end as usize
    }

    fn protocol_identity(&self) -> GlyphPointerSourceIdentity {
        self.identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayPointerAppearance {
    source: DisplayPointerSourceRange,
    face: RenderFaceRef,
}

impl DisplayPointerAppearance {
    pub(crate) const fn new(source: DisplayPointerSourceRange, face: RenderFaceRef) -> Self {
        Self { source, face }
    }

    #[cfg(test)]
    pub(crate) const fn source(&self) -> &DisplayPointerSourceRange {
        &self.source
    }

    #[cfg(test)]
    pub(crate) const fn face(&self) -> RenderFaceRef {
        self.face
    }

    pub(crate) fn glyph_metadata(
        &self,
    ) -> Option<neomacs_display_protocol::glyph_matrix::GlyphPointerAppearance> {
        let RenderFaceRef::FaceId(face_id) = self.face else {
            return None;
        };
        Some(
            neomacs_display_protocol::glyph_matrix::GlyphPointerAppearance {
                source: self.source.protocol_identity(),
                face_id,
            },
        )
    }
}
