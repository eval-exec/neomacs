use crate::buffer_source::producer::frame::ReplacementCoveredSpan;
use crate::display_property::DisplayPropertyClassification;
use crate::display_source_overflow::DisplayXwidgetOverflowAction;
use neomacs_display_protocol::face::{BoxRunMembership, BoxVerticalEdges};
use neomacs_display_protocol::glyph_matrix::TerminalComposition;
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::{
    Px, WebViewId, XwidgetContentExtent, XwidgetId, XwidgetLayoutAdvance,
};
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::emacs_char::EmacsChar;
use neovm_core::face::LispFaceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplaySourceId(u64);

impl DisplaySourceId {
    pub(crate) const fn new(id: u64) -> Self {
        Self(id)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for DisplaySourceId {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DisplaySourcePosition {
    Buffer {
        buffer_id: BufferId,
        char_pos: CharPos0,
        byte_pos: EmacsBytePos,
    },
    LispString {
        source_id: DisplaySourceId,
        char_index: usize,
        byte_index: usize,
    },
    Synthetic {
        source_id: DisplaySourceId,
        offset: usize,
    },
}

impl DisplaySourcePosition {
    pub(crate) const fn buffer(
        buffer_id: BufferId,
        char_pos: CharPos0,
        byte_pos: EmacsBytePos,
    ) -> Self {
        Self::Buffer {
            buffer_id,
            char_pos,
            byte_pos,
        }
    }

    pub(crate) const fn lisp_string(source_id: u64, char_index: usize, byte_index: usize) -> Self {
        Self::LispString {
            source_id: DisplaySourceId::new(source_id),
            char_index,
            byte_index,
        }
    }

    pub(crate) const fn synthetic(source_id: u64, offset: usize) -> Self {
        Self::Synthetic {
            source_id: DisplaySourceId::new(source_id),
            offset,
        }
    }

    pub(crate) fn lisp_string_char_index(&self) -> Option<usize> {
        match self {
            Self::LispString { char_index, .. } => Some(*char_index),
            _ => None,
        }
    }

    /// Advance within this source without changing its coordinate space.
    ///
    /// Buffer positions, Lisp-string indices, and synthetic offsets are
    /// intentionally separate enum arms.  Fragmentation code therefore
    /// cannot advance a string remainder as though it were buffer text.
    pub(crate) fn advanced_by(&self, char_offset: usize, byte_offset: usize) -> Self {
        match self {
            Self::Buffer {
                buffer_id,
                char_pos,
                byte_pos,
            } => Self::buffer(
                *buffer_id,
                CharPos0::new(char_pos.get().saturating_add(char_offset)),
                EmacsBytePos::new(byte_pos.get().saturating_add(byte_offset)),
            ),
            Self::LispString {
                source_id,
                char_index,
                byte_index,
            } => Self::lisp_string(
                source_id.get(),
                char_index.saturating_add(char_offset),
                byte_index.saturating_add(byte_offset),
            ),
            Self::Synthetic { source_id, offset } => {
                Self::synthetic(source_id.get(), offset.saturating_add(char_offset))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SourceSpan {
    pub(crate) start: DisplaySourcePosition,
    pub(crate) end: DisplaySourcePosition,
}

impl SourceSpan {
    pub(crate) const fn new(start: DisplaySourcePosition, end: DisplaySourcePosition) -> Self {
        Self { start, end }
    }

    pub(crate) fn buffer_end_charpos(&self) -> Option<CharPos0> {
        let DisplaySourcePosition::Buffer { char_pos, .. } = self.end else {
            return None;
        };
        Some(char_pos)
    }

    pub(crate) fn buffer_byte_len(&self) -> Option<usize> {
        let DisplaySourcePosition::Buffer {
            byte_pos: start, ..
        } = self.start
        else {
            return None;
        };
        let DisplaySourcePosition::Buffer { byte_pos: end, .. } = self.end else {
            return None;
        };
        end.get().checked_sub(start.get())
    }

    #[cfg(test)]
    pub(crate) const fn lisp_string(
        source_id: u64,
        start_char: usize,
        end_char: usize,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        Self::new(
            DisplaySourcePosition::lisp_string(source_id, start_char, start_byte),
            DisplaySourcePosition::lisp_string(source_id, end_char, end_byte),
        )
    }

    pub(crate) const fn synthetic(source_id: u64, start_offset: usize, end_offset: usize) -> Self {
        Self::new(
            DisplaySourcePosition::synthetic(source_id, start_offset),
            DisplaySourcePosition::synthetic(source_id, end_offset),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RenderFaceRef {
    #[allow(dead_code)]
    Inherit,
    FaceId(FaceId),
}

/// Whether one side of a display-string occurrence continues a boxed run.
/// `StringBase` is retained only for insertion strings whose outside source is
/// definitionally their inherited base; buffer replacements carry a resolved
/// `Boxed`/`Unboxed` fact so an `Inherit` face can never be reinterpreted
/// against the replacement's different base later.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DisplayStringBoxBoundary {
    #[default]
    Unboxed,
    Boxed,
    StringBase,
}

impl DisplayStringBoxBoundary {
    pub(crate) const fn known(boxed: bool) -> Self {
        if boxed { Self::Boxed } else { Self::Unboxed }
    }

    pub(crate) const fn is_boxed(self, string_base_boxed: bool) -> bool {
        match self {
            Self::Unboxed => false,
            Self::Boxed => true,
            Self::StringBase => string_base_boxed,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DisplayStringBoxBoundaries {
    before: DisplayStringBoxBoundary,
    after: DisplayStringBoxBoundary,
}

impl DisplayStringBoxBoundaries {
    pub(crate) const fn known(before_boxed: bool, after_boxed: bool) -> Self {
        Self {
            before: DisplayStringBoxBoundary::known(before_boxed),
            after: DisplayStringBoxBoundary::known(after_boxed),
        }
    }

    pub(crate) const fn string_base() -> Self {
        Self {
            before: DisplayStringBoxBoundary::StringBase,
            after: DisplayStringBoxBoundary::StringBase,
        }
    }

    pub(crate) const fn before_is_boxed(self, string_base_boxed: bool) -> bool {
        self.before.is_boxed(string_base_boxed)
    }

    pub(crate) const fn after_is_boxed(self, string_base_boxed: bool) -> bool {
        self.after.is_boxed(string_base_boxed)
    }

    /// Boundary view for one independently pushed string in an ordered
    /// overlay-string sequence. Between occurrences GNU restores a boxed
    /// iterator; only the first can inherit the source entry terminal and only
    /// the last can own the source exit terminal.
    pub(crate) const fn sequence_member(self, index: usize, len: usize) -> Self {
        Self {
            before: if index == 0 {
                self.before
            } else {
                DisplayStringBoxBoundary::StringBase
            },
            after: if index + 1 == len {
                self.after
            } else {
                DisplayStringBoxBoundary::StringBase
            },
        }
    }
}

/// Semantic source range whose rendered primitives share one transient
/// `mouse-face` appearance.  The end position (together with the source
/// identity) is stable when a run is clipped and resumed on a wrapped row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DisplayPointerSourceRange {
    source: DisplaySourcePosition,
    start_char_index: usize,
    end_char_index: usize,
    overlay_owner: Option<Value>,
    occurrence: DisplayPointerOccurrence,
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
        Self {
            source,
            start_char_index: 0,
            end_char_index,
            overlay_owner: None,
            occurrence: DisplayPointerOccurrence::Source,
        }
    }

    pub(crate) fn effective(
        source: DisplaySourcePosition,
        start_char_index: usize,
        end_char_index: usize,
        overlay_owner: Option<Value>,
    ) -> Self {
        Self {
            source,
            start_char_index,
            end_char_index,
            overlay_owner,
            occurrence: DisplayPointerOccurrence::Source,
        }
    }

    pub(crate) fn in_occurrence(mut self, occurrence: DisplayPointerOccurrence) -> Self {
        self.occurrence = occurrence;
        self
    }

    #[cfg(test)]
    pub(crate) fn buffer_id(&self) -> Option<BufferId> {
        match self.source {
            DisplaySourcePosition::Buffer { buffer_id, .. } => Some(buffer_id),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn source_id(&self) -> Option<DisplaySourceId> {
        match self.source {
            DisplaySourcePosition::LispString { source_id, .. }
            | DisplaySourcePosition::Synthetic { source_id, .. } => Some(source_id),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn start_char_index(&self) -> usize {
        self.start_char_index
    }

    #[cfg(test)]
    pub(crate) const fn end_char_index(&self) -> usize {
        self.end_char_index
    }

    fn protocol_identity(
        &self,
    ) -> neomacs_display_protocol::glyph_matrix::GlyphPointerSourceIdentity {
        use neomacs_display_protocol::glyph_matrix::{
            GlyphPointerOccurrenceIdentity, GlyphPointerSourceIdentity, GlyphPointerSourceKind,
        };
        let (kind, source_id) = match self.source {
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
        let occurrence = match self.occurrence {
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
        GlyphPointerSourceIdentity {
            kind,
            source_id,
            range_start: self.start_char_index as u64,
            range_end: self.end_char_index as u64,
            property_owner: self.overlay_owner.map_or(0, |owner| owner.bits() as u64),
            occurrence,
        }
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayItem {
    pub(crate) span: SourceSpan,
    pub(crate) face: RenderFaceRef,
    pub(crate) kind: DisplayItemKind,
    pub(crate) layout: DisplayItemLayout,
    pub(crate) pointer_appearance: Option<DisplayPointerAppearance>,
    /// GNU `start_of_box_run_p` / `end_of_box_run_p` for this logical
    /// source item.  Layout resolves these from the adjacent *source* faces;
    /// the renderer must never reconstruct them from visible glyph adjacency.
    pub(crate) box_vertical_edges: BoxVerticalEdges,
    /// Boxed/unboxed membership is not derivable from terminal ownership: an
    /// interior member of an open run owns neither side.
    pub(crate) box_run_membership: BoxRunMembership,
}

impl DisplayItem {
    pub(crate) const fn new(span: SourceSpan, face: RenderFaceRef, kind: DisplayItemKind) -> Self {
        Self {
            span,
            face,
            kind,
            layout: DisplayItemLayout {
                raise: None,
                height: None,
                space_width: None,
                break_after_row: false,
            },
            pointer_appearance: None,
            // Synthetic items are closed runs unless their source owner gives
            // us stronger continuation facts.  Buffer and Lisp-string cursors
            // always replace this default with source-derived ownership.
            box_vertical_edges: BoxVerticalEdges::Both,
            box_run_membership: BoxRunMembership::Unboxed,
        }
    }

    pub(crate) const fn with_layout(mut self, layout: DisplayItemLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Mark this item so the current display row ends immediately after it.
    /// See [`DisplayItemLayout::break_after_row`].
    pub(crate) const fn with_break_after_row(mut self) -> Self {
        self.layout.break_after_row = true;
        self
    }

    pub(crate) fn with_pointer_appearance(
        mut self,
        appearance: Option<DisplayPointerAppearance>,
    ) -> Self {
        self.pointer_appearance = appearance;
        self
    }

    pub(crate) const fn with_box_vertical_edges(mut self, edges: BoxVerticalEdges) -> Self {
        self.box_vertical_edges = edges;
        if edges.owns_left() || edges.owns_right() {
            self.box_run_membership = BoxRunMembership::Boxed;
        }
        self
    }

    pub(crate) const fn with_box_run_topology(
        mut self,
        boxed: bool,
        edges: BoxVerticalEdges,
    ) -> Self {
        self.box_run_membership = BoxRunMembership::from_boxed(boxed);
        self.box_vertical_edges = edges;
        self
    }

    pub(crate) fn is_display_table_vector(&self) -> bool {
        matches!(
            &self.kind,
            DisplayItemKind::SourceMappedText(mapped) if mapped.is_display_table_vector()
        )
    }

    #[cfg(test)]
    pub(crate) fn pointer_appearance(&self) -> Option<&DisplayPointerAppearance> {
        self.pointer_appearance.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DisplayItemLayout {
    pub(crate) raise: Option<f32>,
    pub(crate) height: Option<f32>,
    pub(crate) space_width: Option<f32>,
    /// End the current display row immediately after this item, without
    /// consuming another buffer character. Set for a `SourceMappedText` that
    /// stands in for a display-table entry whose glyph vector ends in a newline
    /// (e.g. whitespace-mode's `[$ \n]` on `?\n`): GNU treats the trailing `\n`
    /// glyph as its own end-of-line display element (`ITERATOR_AT_END_OF_LINE_P`
    /// tests `it->c == '\n'` for display-vector elements too, xdisp.c), so the
    /// leading glyphs render and then the row breaks.
    pub(crate) break_after_row: bool,
}

impl DisplayItemLayout {
    pub(crate) fn horizontal_advance_px(self, ch: char, advance_px: f32) -> f32 {
        if ch != ' ' {
            return advance_px;
        }
        self.space_width
            .filter(|factor| factor.is_finite() && *factor > 0.0)
            .map(|factor| advance_px * factor)
            .unwrap_or(advance_px)
    }

    pub(crate) fn vertical_offset_px(self, row_height_px: f32) -> f32 {
        self.raise
            .filter(|factor| factor.is_finite())
            // GNU stores `it->voffset` as an integer.  The floating product
            // is therefore truncated toward zero before it reaches glyph
            // metrics or drawing (xdisp.c `handle_single_display_spec`).
            .map(|factor| -(factor * row_height_px.max(1.0)).trunc())
            .unwrap_or(0.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayItemKind {
    TextRun(DisplayTextRun),
    SourceMappedText(DisplaySourceMappedText),
    ControlChar { ch: char },
    Glyphless(DisplayGlyphless),
    Stretch(DisplayStretch),
    MediaReplacement(DisplayMediaReplacement),
    RowBreak(DisplayRowBreak),
}

/// The named GNU face merged over an item's source face because of the
/// item's display semantics rather than a text property.
///
/// Keeping this closed and adjacent to [`DisplayItemKind`] makes a newly
/// introduced special-character kind choose its face behavior explicitly;
/// source pipelines only consume the decision and cannot drift apart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayItemFaceOverlay {
    EscapeGlyph,
    GlyphlessChar,
}

impl DisplayItemFaceOverlay {
    pub(crate) const fn face_name(self) -> &'static str {
        match self {
            Self::EscapeGlyph => "escape-glyph",
            Self::GlyphlessChar => "glyphless-char",
        }
    }
}

impl DisplayItemKind {
    pub(crate) const fn semantic_face_overlay(&self) -> Option<DisplayItemFaceOverlay> {
        match self {
            Self::ControlChar { .. } => Some(DisplayItemFaceOverlay::EscapeGlyph),
            Self::Glyphless(DisplayGlyphless {
                method: GlyphlessMethod::ZeroWidth,
                ..
            }) => None,
            Self::Glyphless(DisplayGlyphless {
                method:
                    GlyphlessMethod::ThinSpace
                    | GlyphlessMethod::HexCode
                    | GlyphlessMethod::EmptyBox
                    | GlyphlessMethod::Acronym(_),
                ..
            }) => Some(DisplayItemFaceOverlay::GlyphlessChar),
            Self::SourceMappedText(mapped) => mapped.semantic_face_overlay(),
            Self::TextRun(_) | Self::Stretch(_) | Self::MediaReplacement(_) | Self::RowBreak(_) => {
                None
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BufferDisplayReplacementSource {
    buffer_id: BufferId,
    char_pos: CharPos0,
    byte_pos: EmacsBytePos,
    end_char_pos: CharPos0,
    end_byte_pos: EmacsBytePos,
}

impl BufferDisplayReplacementSource {
    #[cfg(test)]
    pub(crate) fn new(buffer_id: BufferId, char_pos: CharPos0, byte_pos: EmacsBytePos) -> Self {
        Self {
            buffer_id,
            char_pos,
            byte_pos,
            end_char_pos: char_pos.add_len(neovm_core::buffer::CharLen::new(1)),
            end_byte_pos: byte_pos,
        }
    }

    pub(crate) fn spanning(
        buffer_id: BufferId,
        char_pos: CharPos0,
        byte_pos: EmacsBytePos,
        end_char_pos: CharPos0,
        end_byte_pos: EmacsBytePos,
    ) -> Self {
        Self {
            buffer_id,
            char_pos,
            byte_pos,
            end_char_pos,
            end_byte_pos,
        }
    }

    pub(crate) fn buffer_id(self) -> BufferId {
        self.buffer_id
    }

    pub(crate) const fn pointer_occurrence(self) -> DisplayPointerOccurrence {
        DisplayPointerOccurrence::BufferDisplayReplacement {
            buffer_id: self.buffer_id,
            anchor_charpos: self.char_pos,
        }
    }

    fn span(self) -> SourceSpan {
        SourceSpan::new(
            DisplaySourcePosition::buffer(self.buffer_id, self.char_pos, self.byte_pos),
            DisplaySourcePosition::buffer(self.buffer_id, self.end_char_pos, self.end_byte_pos),
        )
    }

    fn item(self, face_id: FaceId, kind: DisplayItemKind) -> DisplayItem {
        self.item_with_face(RenderFaceRef::FaceId(face_id), kind)
    }

    pub(crate) fn display_item(self, face_id: FaceId, kind: DisplayItemKind) -> DisplayItem {
        self.item(face_id, kind)
    }

    fn item_with_face(self, face: RenderFaceRef, kind: DisplayItemKind) -> DisplayItem {
        DisplayItem::new(self.span(), face, kind)
    }

    pub(crate) fn item_from_replacement_string_item(self, item: DisplayItem) -> DisplayItem {
        let glyph_string_start = item.span.start.clone();
        let box_vertical_edges = item.box_vertical_edges;
        let box_run_membership = item.box_run_membership;
        let kind = match item.kind {
            DisplayItemKind::TextRun(run) => DisplayItemKind::SourceMappedText(
                DisplaySourceMappedText::from_string_run(run.text, glyph_string_start),
            ),
            kind => kind,
        };
        self.item_with_face(item.face, kind)
            .with_layout(item.layout)
            .with_box_run_topology(box_run_membership.is_boxed(), box_vertical_edges)
            .with_pointer_appearance(item.pointer_appearance)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayPropertyReplacementDescriptor {
    value: Value,
    classification: DisplayPropertyClassification,
    replacement_source: BufferDisplayReplacementSource,
    /// The buffer range this replacement stands for, derived once by the
    /// producer. `covered.start()` is GNU's B (what the glyphs are stamped
    /// with) and `covered.resume()` is GNU's E (where the walk continues).
    /// Consumers READ the resume; nobody outside
    /// [`ReplacementCoveredSpan`] derives one.
    covered: ReplacementCoveredSpan,
    pointer_appearance: Option<DisplayPointerAppearance>,
    box_vertical_edges: BoxVerticalEdges,
    box_boundaries: DisplayStringBoxBoundaries,
}

impl DisplayPropertyReplacementDescriptor {
    pub(crate) fn new(
        value: Value,
        classification: DisplayPropertyClassification,
        replacement_source: BufferDisplayReplacementSource,
        covered: ReplacementCoveredSpan,
    ) -> Self {
        Self {
            value,
            classification,
            replacement_source,
            covered,
            pointer_appearance: None,
            box_vertical_edges: BoxVerticalEdges::Both,
            box_boundaries: DisplayStringBoxBoundaries::default(),
        }
    }

    pub(crate) fn classification(&self) -> &DisplayPropertyClassification {
        &self.classification
    }

    pub(crate) fn replacement_source(&self) -> BufferDisplayReplacementSource {
        self.replacement_source
    }

    pub(crate) fn anchor_charpos(&self) -> CharPos0 {
        self.covered.start()
    }

    /// GNU's E. The renderer APPLIES this to the walk once the replacement is
    /// appended; it is the producer's answer, not the renderer's.
    pub(crate) fn resume_charpos(&self) -> i64 {
        self.covered.resume().get() as i64
    }

    pub(crate) fn pointer_appearance(&self) -> Option<&DisplayPointerAppearance> {
        self.pointer_appearance.as_ref()
    }

    pub(crate) const fn box_vertical_edges(&self) -> BoxVerticalEdges {
        self.box_vertical_edges
    }

    pub(crate) const fn box_boundaries(&self) -> DisplayStringBoxBoundaries {
        self.box_boundaries
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BufferDisplayPropertyReplacementItem {
    descriptor: DisplayPropertyReplacementDescriptor,
    start_byte_pos: EmacsBytePos,
    end_byte_pos: EmacsBytePos,
    covered: ReplacementCoveredSpan,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BufferDisplayPropertyFallbackItem {
    item: DisplayItem,
    start_byte_idx: usize,
    start_charpos: i64,
    source_char: Option<char>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BufferDisplayPropertyReplacementAnchor {
    byte_idx: usize,
    charpos: i64,
}

impl BufferDisplayPropertyReplacementItem {
    pub(crate) fn new(
        value: Value,
        classification: DisplayPropertyClassification,
        replacement_source: BufferDisplayReplacementSource,
        start_byte_pos: EmacsBytePos,
        end_byte_pos: EmacsBytePos,
        covered: ReplacementCoveredSpan,
    ) -> Self {
        Self {
            descriptor: DisplayPropertyReplacementDescriptor::new(
                value,
                classification,
                replacement_source,
                covered,
            ),
            start_byte_pos,
            end_byte_pos,
            covered,
        }
    }

    pub(crate) fn descriptor(&self) -> &DisplayPropertyReplacementDescriptor {
        &self.descriptor
    }

    pub(crate) fn with_pointer_appearance(
        mut self,
        appearance: Option<DisplayPointerAppearance>,
    ) -> Self {
        self.descriptor.pointer_appearance = appearance;
        self
    }

    pub(crate) fn with_box_vertical_edges(mut self, edges: BoxVerticalEdges) -> Self {
        self.descriptor.box_vertical_edges = edges;
        self
    }

    pub(crate) fn with_box_boundaries(mut self, boundaries: DisplayStringBoxBoundaries) -> Self {
        self.descriptor.box_boundaries = boundaries;
        self
    }

    pub(crate) fn start_byte_idx(&self, text_start_byte: usize) -> Option<usize> {
        self.start_byte_pos.get().checked_sub(text_start_byte)
    }

    pub(crate) fn source_anchor(
        &self,
        text_start_byte: usize,
    ) -> Option<BufferDisplayPropertyReplacementAnchor> {
        Some(BufferDisplayPropertyReplacementAnchor {
            byte_idx: self.start_byte_idx(text_start_byte)?,
            charpos: self.start_charpos(),
        })
    }

    pub(crate) fn start_charpos(&self) -> i64 {
        self.covered.start().get() as i64
    }

    pub(crate) fn source_text<'a>(
        &self,
        text_start_byte: usize,
        text: &'a [u8],
    ) -> Option<&'a [u8]> {
        text.get(self.start_byte_idx(text_start_byte)?..)
    }

    pub(crate) fn fallback_display_item(
        &self,
        text_start_byte: usize,
        text: &[u8],
        face: RenderFaceRef,
    ) -> Option<BufferDisplayPropertyFallbackItem> {
        let start_byte_idx = self.start_byte_idx(text_start_byte)?;
        let end_byte_idx = self.end_byte_pos.get().checked_sub(text_start_byte)?;
        let source_text = std::str::from_utf8(text.get(start_byte_idx..end_byte_idx)?).ok()?;
        if source_text.is_empty() {
            return None;
        }
        let source_char = source_text.chars().next();
        let replacement_source = self.descriptor.replacement_source();
        let item = DisplayItem::new(
            SourceSpan::new(
                DisplaySourcePosition::buffer(
                    replacement_source.buffer_id(),
                    self.covered.start(),
                    self.start_byte_pos,
                ),
                DisplaySourcePosition::buffer(
                    replacement_source.buffer_id(),
                    self.covered.resume(),
                    self.end_byte_pos,
                ),
            ),
            face,
            DisplayItemKind::TextRun(DisplayTextRun::independent(source_text.to_owned())),
        )
        .with_box_vertical_edges(self.descriptor.box_vertical_edges())
        .with_pointer_appearance(self.descriptor.pointer_appearance().cloned());
        Some(BufferDisplayPropertyFallbackItem {
            item,
            start_byte_idx,
            start_charpos: self.start_charpos(),
            source_char,
        })
    }
}

impl BufferDisplayPropertyFallbackItem {
    pub(crate) fn into_parts(self) -> (DisplayItem, usize, i64, Option<char>) {
        (
            self.item,
            self.start_byte_idx,
            self.start_charpos,
            self.source_char,
        )
    }
}

impl BufferDisplayPropertyReplacementAnchor {
    pub(crate) fn matches(self, byte_idx: usize, charpos: i64) -> bool {
        self.byte_idx == byte_idx && self.charpos == charpos
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayTextRun {
    pub(crate) text: Box<str>,
    pub(crate) composition: DisplayTextComposition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum DisplayTextComposition {
    #[default]
    UnicodeFallback,
    /// Buffer text known not to be inside a Lisp-selected automatic
    /// composition.  Zero-width characters remain independent display
    /// elements instead of being heuristically merged by Unicode width.
    Independent,
    Automatic(TerminalComposition),
}

impl DisplayTextRun {
    pub(crate) fn with_composition(
        text: impl Into<Box<str>>,
        composition: DisplayTextComposition,
    ) -> Self {
        Self {
            text: text.into(),
            composition,
        }
    }

    pub(crate) fn new(text: impl Into<Box<str>>) -> Self {
        Self::with_composition(text, DisplayTextComposition::UnicodeFallback)
    }

    pub(crate) fn independent(text: impl Into<Box<str>>) -> Self {
        Self::with_composition(text, DisplayTextComposition::Independent)
    }

    pub(crate) fn automatic(text: impl Into<Box<str>>) -> Self {
        let text = text.into();
        let terminal = neovm_core::emacs_core::composite::automatic_composition_for_terminal(&text);
        Self {
            text,
            composition: DisplayTextComposition::Automatic(terminal),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplaySourceMappedFaceRun {
    pub(crate) char_len: usize,
    pub(crate) lisp_face_id: Option<LispFaceId>,
}

impl DisplaySourceMappedFaceRun {
    pub(crate) fn new(char_len: usize, lisp_face_id: Option<LispFaceId>) -> Self {
        Self {
            char_len,
            lisp_face_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplaySourceMappedText {
    pub(crate) text: Box<str>,
    /// A semantic face introduced by the mapping itself rather than by a text
    /// property. Octal escapes own `escape-glyph` here so the source character
    /// need not be representable as a Rust `char` downstream.
    semantic_face_overlay: Option<DisplayItemFaceOverlay>,
    /// Source of glyph indices when buffer coverage and glyph provenance are
    /// intentionally different (a string-valued `display` replacement).
    /// `None` retains the covered-start rule used by escape/composition
    /// expansions.
    pub(crate) glyph_string_start: Option<DisplaySourcePosition>,
    /// Per-glyph GNU Lisp face identities carried by a display-table vector,
    /// run-length encoded in text-character coordinates. `None` means ordinary
    /// mapped text; `Some([])` is an empty display vector; a run whose face is
    /// `None` explicitly resets those glyphs to the saved face.
    lisp_face_runs: Option<Box<[DisplaySourceMappedFaceRun]>>,
}

impl DisplaySourceMappedText {
    pub(crate) fn new(text: impl Into<Box<str>>) -> Self {
        Self {
            text: text.into(),
            semantic_face_overlay: None,
            glyph_string_start: None,
            lisp_face_runs: None,
        }
    }

    pub(crate) fn from_string_run(
        text: impl Into<Box<str>>,
        glyph_string_start: DisplaySourcePosition,
    ) -> Self {
        debug_assert!(matches!(
            glyph_string_start,
            DisplaySourcePosition::LispString { .. }
        ));
        Self {
            text: text.into(),
            semantic_face_overlay: None,
            glyph_string_start: Some(glyph_string_start),
            lisp_face_runs: None,
        }
    }

    pub(crate) fn with_lisp_face_runs(
        mut self,
        face_runs: impl Into<Box<[DisplaySourceMappedFaceRun]>>,
    ) -> Self {
        let face_runs = face_runs.into();
        assert!(
            face_runs.iter().all(|run| run.char_len > 0)
                && face_runs.iter().map(|run| run.char_len).sum::<usize>()
                    == self.text.chars().count(),
            "display-source face runs must cover the mapped text exactly"
        );
        self.lisp_face_runs = Some(face_runs);
        self
    }

    pub(crate) fn with_semantic_face_overlay(mut self, overlay: DisplayItemFaceOverlay) -> Self {
        self.semantic_face_overlay = Some(overlay);
        self
    }

    pub(crate) const fn semantic_face_overlay(&self) -> Option<DisplayItemFaceOverlay> {
        self.semantic_face_overlay
    }

    pub(crate) fn face_segment(
        text: impl Into<Box<str>>,
        glyph_string_start: Option<DisplaySourcePosition>,
    ) -> Self {
        Self {
            text: text.into(),
            semantic_face_overlay: None,
            glyph_string_start,
            lisp_face_runs: None,
        }
    }

    pub(crate) const fn is_display_table_vector(&self) -> bool {
        self.lisp_face_runs.is_some()
    }

    pub(crate) fn lisp_face_runs(&self) -> &[DisplaySourceMappedFaceRun] {
        match &self.lisp_face_runs {
            Some(runs) => runs,
            None => &[],
        }
    }

    /// Remove a display-vector's terminal newline glyph while keeping its
    /// per-glyph face metadata aligned with the visible prefix.
    pub(crate) fn into_prefix_without_last_char(mut self) -> Self {
        let char_len = self.text.chars().count();
        let keep_chars = char_len.saturating_sub(1);
        let keep_bytes = self
            .text
            .char_indices()
            .nth(keep_chars)
            .map_or(self.text.len(), |(byte, _)| byte);
        self.text = self.text[..keep_bytes].into();
        self.lisp_face_runs = self.lisp_face_runs.map(|runs| {
            let (prefix, _) = split_face_runs_at(&runs, keep_chars);
            prefix.into()
        });
        self
    }

    /// Keep the displayed text and its glyph-coordinate origin transactional
    /// when a row clips this item and carries the remainder forward.
    pub(crate) fn into_remainder_after(self, emitted_chars: usize) -> Option<Self> {
        let split_byte = self
            .text
            .char_indices()
            .nth(emitted_chars)
            .map(|(byte, _)| byte)?;
        Some(Self {
            text: self.text[split_byte..].into(),
            semantic_face_overlay: self.semantic_face_overlay,
            glyph_string_start: self
                .glyph_string_start
                .map(|start| start.advanced_by(emitted_chars, split_byte)),
            lisp_face_runs: self
                .lisp_face_runs
                .map(|runs| split_face_runs_at(&runs, emitted_chars).1.into()),
        })
    }
}

fn split_face_runs_at(
    runs: &[DisplaySourceMappedFaceRun],
    mut prefix_chars: usize,
) -> (
    Vec<DisplaySourceMappedFaceRun>,
    Vec<DisplaySourceMappedFaceRun>,
) {
    let mut prefix = Vec::new();
    let mut remainder = Vec::new();
    for run in runs {
        if prefix_chars >= run.char_len {
            prefix.push(run.clone());
            prefix_chars -= run.char_len;
            continue;
        }
        if prefix_chars > 0 {
            prefix.push(DisplaySourceMappedFaceRun::new(
                prefix_chars,
                run.lisp_face_id,
            ));
        }
        remainder.push(DisplaySourceMappedFaceRun::new(
            run.char_len - prefix_chars,
            run.lisp_face_id,
        ));
        prefix_chars = 0;
    }
    (prefix, remainder)
}

/// GNU accepts at most six ASCII bytes for a glyphless-character acronym
/// (`term.c:produce_glyphless_glyph`).  Encoding that bound in the value keeps
/// arbitrary Lisp strings out of the rendering pipeline and lets the method
/// remain `Copy` like the other display policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GlyphlessAcronym {
    bytes: [u8; 6],
    len: u8,
}

impl GlyphlessAcronym {
    pub(crate) fn from_ascii(text: &str) -> Option<Self> {
        if !text.is_ascii() {
            return None;
        }
        let mut bytes = [0; 6];
        let source = text
            .as_bytes()
            .split(|byte| *byte == 0)
            .next()
            .unwrap_or_default();
        let len = source.len().min(bytes.len());
        bytes[..len].copy_from_slice(&source[..len]);
        Some(Self {
            bytes,
            len: len as u8,
        })
    }

    pub(crate) fn tty_text(self) -> String {
        let bytes = &self.bytes[..usize::from(self.len)];
        let acronym = std::str::from_utf8(bytes).expect("glyphless acronym is ASCII");
        if self.len == 1 {
            acronym.to_owned()
        } else {
            format!("[{acronym}]")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlyphlessMethod {
    ZeroWidth,
    ThinSpace,
    HexCode,
    EmptyBox,
    Acronym(GlyphlessAcronym),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlyphlessJoinerPolicy {
    ClassifyAsGlyphless,
    PreserveForComposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplayGlyphless {
    pub(crate) ch: char,
    pub(crate) method: GlyphlessMethod,
}

pub(crate) fn control_char_caret_char(ch: char) -> Option<char> {
    match ch {
        '\u{0000}'..='\u{001f}' => Some(char::from((ch as u8) + b'@')),
        '\u{007f}' => Some('?'),
        _ => None,
    }
}

pub(crate) fn glyphless_method_for_char(
    ch: char,
    joiner_policy: GlyphlessJoinerPolicy,
) -> Option<GlyphlessMethod> {
    if joiner_policy == GlyphlessJoinerPolicy::PreserveForComposition
        && crate::composition::is_composition_joiner(ch)
    {
        return None;
    }

    let cp = ch as u32;
    match cp {
        // NB: the C1 controls (U+0080..U+009F) and unassigned specials
        // (U+FFF0..U+FFF8) are NON-PRINTABLE, so GNU escapes them as `\`+octal in
        // the escape-glyph face -- they are classified by `is_escape_glyph_octal`
        // (see `classify_text_source_char`) BEFORE this table is consulted, so
        // they are intentionally absent here (was `GlyphlessMethod::HexCode`).
        0xfffc => Some(GlyphlessMethod::EmptyBox),
        // Fast paths for the common invisible chars: the format-control chars in
        // the arm below (ZWSP/ZWJ/LRM/tags -- all `Cf`, so also caught by the
        // category check) plus the variation selectors (`Mn`) and line/paragraph
        // separators (`Zl`/`Zp`), which are NOT `Cf` and so need listing here.
        0xfeff
        | 0x200b..=0x200f
        | 0x2028..=0x2029
        | 0xe0001..=0xe007f
        | 0xe0100..=0xe01ef
        | 0xfe00..=0xfe0f => Some(GlyphlessMethod::ZeroWidth),
        // GNU's `format-control` group (glyphless-char-display-control default
        // `thin-space`): general-category `Cf` chars, rendered invisible
        // (ZeroWidth) like the ZWSP/ZWJ/LRM fast paths above -- otherwise a `Cf`
        // char a font can't draw (e.g. U+FFF9..U+FFFB interlinear annotations)
        // falls through to a `.notdef` box.
        //
        // BUT only the `Cf` chars that are also `Default_Ignorable_Code_Point`:
        // Unicode/GNU RENDER the non-ignorable format controls (they carry
        // visible or shaping meaning), so they must NOT be hidden -- see
        // `is_non_ignorable_format_control`. The one in etc/HELLO is U+180E
        // MONGOLIAN VOWEL SEPARATOR (removed from Default_Ignorable in Unicode
        // 6.3): GNU emits it as part of the Mongolian text (composed clusters
        // bypass the glyphless path, xdisp.c `get_next_display_element`), so a
        // blanket "all `Cf` -> ZeroWidth" wrongly dropped it and diverged from
        // GNU on the TTY. Also excludes U+00AD (SHY, has a visible glyph).
        // Guarded on `cp >= 0x80` so ASCII (never `Cf`, the hot path) skips the
        // category lookup that `is_escape_glyph_octal` already fast-paths past.
        _ if cp >= 0x80
            && cp != 0xad
            && is_format_control(cp)
            && !is_non_ignorable_format_control(cp) =>
        {
            Some(GlyphlessMethod::ZeroWidth)
        }
        _ => None,
    }
}

/// True if `cp` is a general-category `Cf` (format-control) character -- GNU's
/// `format-control` glyphless group. ASCII is never `Cf`; callers fast-path it.
fn is_format_control(cp: u32) -> bool {
    use neovm_core::emacs_core::emacs_char::{UnicodeCategory, char_general_category};
    char_general_category(cp) == Some(UnicodeCategory::Format as i64)
}

/// The general-category `Cf` characters that are NOT
/// `Default_Ignorable_Code_Point`, i.e. the format controls Unicode/GNU still
/// *render* rather than hide. Within `Cf` this is exactly the
/// `Prepended_Concatenation_Mark` set (Arabic/Syriac/Kaithi number & ayah
/// signs, which prepend to following digits), the Egyptian Hieroglyph format
/// controls (which drive hieroglyph quadrat layout), and U+180E MONGOLIAN
/// VOWEL SEPARATOR (removed from `Default_Ignorable` in Unicode 6.3). Keeping
/// these out of the glyphless ZeroWidth rule matches GNU, which emits them
/// (via a font glyph or as part of a composed cluster) instead of hiding them.
fn is_non_ignorable_format_control(cp: u32) -> bool {
    matches!(cp,
        0x0600..=0x0605   // Arabic number/year/footnote/etc. signs
        | 0x06DD          // Arabic end of ayah
        | 0x070F          // Syriac abbreviation mark
        | 0x0890..=0x0891 // Arabic pound / piastre marks
        | 0x08E2          // Arabic disputed end of ayah
        | 0x110BD | 0x110CD // Kaithi number sign / number sign above
        | 0x180E          // Mongolian vowel separator
        | 0x13430..=0x1343F // Egyptian Hieroglyph format controls
    )
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayLength {
    #[allow(dead_code)]
    Columns(u16),
    Pixels(f32),
    Em(f32),
    /// A `(space :width/:height/:ascent …)` operand kept verbatim as Lisp.
    ///
    /// GNU stores the property object and evaluates it with
    /// `calc_pixel_width_or_height` (xdisp.c:30355); there is no second,
    /// typed decode of the expression grammar. Keeping the operand as Lisp
    /// means every form GNU accepts reaches the evaluator — including forms
    /// a typed mirror would have to enumerate, such as `(NUM . EXPR)`
    /// products and `(image …)` operands (issue #204).
    Expr(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayStretchWidth {
    Length(DisplayLength),
    /// GNU's `:relative-width` is relative to the first covered source
    /// character, not to the canonical `em`.  Carrying that character in the
    /// render item makes it impossible for row layout to silently substitute
    /// the frame column width (the lossy behavior that shifted Doom's TTY
    /// modeline).
    RelativeToSource {
        factor: f32,
        source: EmacsChar,
    },
    /// `:align-to` operand, kept verbatim as Lisp — see [`DisplayLength::Expr`].
    AlignTo(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayStretch {
    pub(crate) width: DisplayStretchWidth,
    pub(crate) height: Option<DisplayLength>,
    pub(crate) ascent: Option<DisplayLength>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayImageItem {
    pub(crate) image_id: i32,
    pub(crate) source_rect: neomacs_display_protocol::ImageSourceRect,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
    pub(crate) horizontal_margin: f32,
    pub(crate) vertical_margin: f32,
    pub(crate) opaque_background: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayVideoItem {
    pub(crate) video_id: neomacs_display_protocol::VideoId,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayXwidgetItem {
    pub(crate) xwidget_id: XwidgetId,
    pub(crate) webview_id: WebViewId,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySurfaceItem {
    pub(crate) surface_id: i32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayMediaReplacement {
    pub(crate) kind: DisplayMediaReplacementKind,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
    positive_box_line_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayMediaReplacementKind {
    Image {
        image_id: u32,
        source_rect: neomacs_display_protocol::ImageSourceRect,
        margin_left: f32,
        margin_right: f32,
        margin_top: f32,
        margin_bottom: f32,
        opaque_background: Option<u32>,
    },
    /// A valid image replacement whose GNU slice resolves to no pixels. The
    /// source text is still consumed, but no placeholder or drawable glyph is
    /// emitted.
    EmptyImageSlice,
    Video {
        video_id: neomacs_display_protocol::VideoId,
        opacity: f32,
    },
    /// `content` is GNU `xw->width`/`xw->height`; the outer `width` is the
    /// layout advance, which the right-edge crop may narrow below it.
    Xwidget {
        xwidget_id: XwidgetId,
        webview_id: WebViewId,
        content: XwidgetContentExtent,
    },
    Surface {
        surface_id: u32,
    },
}

/// A media replacement proven to be an xwidget.
///
/// GNU's right-edge policy is meaningful only for xwidgets.  Extracting this
/// capability from the generic media enum makes applying that policy a typed
/// operation instead of relying on a debug-only assertion that the caller did
/// not accidentally crop an image, video, or shader surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayXwidgetReplacement(DisplayMediaReplacement);

impl DisplayXwidgetReplacement {
    pub(crate) fn layout_advance(self) -> XwidgetLayoutAdvance {
        XwidgetLayoutAdvance::new(Px(self.0.width))
            .expect("an xwidget replacement always has a positive finite advance")
    }

    pub(crate) fn apply_overflow(mut self, action: DisplayXwidgetOverflowAction) -> Self {
        match action {
            DisplayXwidgetOverflowAction::Fits | DisplayXwidgetOverflowAction::LeaveWhole => {}
            DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth { advance }
                if advance.px().get() < self.0.width =>
            {
                self.0.width = advance.px().get();
            }
            DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth { .. } => {}
        }
        self
    }

    pub(crate) const fn into_media(self) -> DisplayMediaReplacement {
        self.0
    }
}

impl DisplayMediaReplacement {
    pub(crate) fn replacement_stretch(self) -> DisplayStretch {
        DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(self.width)),
            height: Some(DisplayLength::Pixels(self.height)),
            ascent: Some(DisplayLength::Pixels(self.ascent)),
        }
    }

    pub(crate) fn image(image: DisplayImageItem) -> Self {
        let horizontal_margin = display_replacement_margin(image.horizontal_margin);
        let vertical_margin = display_replacement_margin(image.vertical_margin);
        Self {
            kind: DisplayMediaReplacementKind::Image {
                image_id: image.image_id.max(0) as u32,
                source_rect: image.source_rect,
                margin_left: horizontal_margin,
                margin_right: horizontal_margin,
                margin_top: vertical_margin,
                margin_bottom: vertical_margin,
                opaque_background: image.opaque_background,
            },
            width: display_replacement_dimension(image.width) + 2.0 * horizontal_margin,
            height: display_replacement_dimension(image.height) + 2.0 * vertical_margin,
            ascent: display_replacement_ascent(image.ascent) + vertical_margin,
            positive_box_line_width: 0.0,
        }
    }

    /// GNU positive `:box :line-width` reserves space outside the media
    /// content.  Fold that inset into the durable image placement now, while
    /// the realized face is available, so layout advance and renderer replay
    /// cannot disagree or paint the border over the outer image pixels.
    pub(crate) fn with_positive_box_line_width(mut self, per_edge: f32) -> Self {
        if !per_edge.is_finite() || per_edge <= 0.0 {
            return self;
        }
        self.positive_box_line_width = per_edge;
        self
    }

    /// Apply the deferred positive box inset only at image boundaries which
    /// GNU's current slice and affine source terminals actually own.
    pub(crate) fn apply_positive_box_expansion(mut self, edges: BoxVerticalEdges) -> Self {
        let per_edge = self.positive_box_line_width;
        if per_edge <= 0.0 {
            return self;
        }
        if let DisplayMediaReplacementKind::Image {
            source_rect,
            margin_left,
            margin_right,
            margin_top,
            margin_bottom,
            ..
        } = &mut self.kind
        {
            const EDGE_EPSILON: f32 = 1.0 / 65_536.0;
            let left = source_rect.x() <= EDGE_EPSILON && edges.owns_left();
            let right =
                source_rect.x() + source_rect.width() >= 1.0 - EDGE_EPSILON && edges.owns_right();
            let top = source_rect.y() <= EDGE_EPSILON;
            let bottom = source_rect.y() + source_rect.height() >= 1.0 - EDGE_EPSILON;
            if left {
                *margin_left += per_edge;
                self.width += per_edge;
            }
            if right {
                *margin_right += per_edge;
                self.width += per_edge;
            }
            if top {
                *margin_top += per_edge;
                self.height += per_edge;
                self.ascent += per_edge;
            }
            if bottom {
                *margin_bottom += per_edge;
                self.height += per_edge;
            }
        }
        self
    }

    pub(crate) const fn empty_image_slice() -> Self {
        Self {
            kind: DisplayMediaReplacementKind::EmptyImageSlice,
            width: 0.0,
            height: 0.0,
            ascent: 0.0,
            positive_box_line_width: 0.0,
        }
    }

    pub(crate) fn video(video: DisplayVideoItem) -> Self {
        Self {
            kind: DisplayMediaReplacementKind::Video {
                video_id: video.video_id,
                opacity: video.opacity,
            },
            width: display_replacement_dimension(video.width),
            height: display_replacement_dimension(video.height),
            ascent: display_replacement_ascent(video.height),
            positive_box_line_width: 0.0,
        }
    }

    pub(crate) fn xwidget(xwidget: DisplayXwidgetItem) -> Self {
        let width = display_replacement_dimension(xwidget.width);
        let height = display_replacement_dimension(xwidget.height);
        Self {
            kind: DisplayMediaReplacementKind::Xwidget {
                xwidget_id: xwidget.xwidget_id,
                webview_id: xwidget.webview_id,
                content: XwidgetContentExtent::new(width, height)
                    .expect("display_replacement_dimension yields finite widths of at least 1"),
            },
            width,
            height,
            ascent: display_replacement_ascent(xwidget.height),
            positive_box_line_width: 0.0,
        }
    }

    /// Prove this generic media replacement is an xwidget before exposing
    /// GNU's xwidget-only overflow operation.
    pub(crate) fn into_xwidget(self) -> Result<DisplayXwidgetReplacement, Self> {
        if matches!(self.kind, DisplayMediaReplacementKind::Xwidget { .. }) {
            Ok(DisplayXwidgetReplacement(self))
        } else {
            Err(self)
        }
    }

    pub(crate) fn surface(surface: DisplaySurfaceItem) -> Self {
        Self {
            kind: DisplayMediaReplacementKind::Surface {
                surface_id: surface.surface_id.max(0) as u32,
            },
            width: display_replacement_dimension(surface.width),
            height: display_replacement_dimension(surface.height),
            ascent: display_replacement_ascent(surface.height),
            positive_box_line_width: 0.0,
        }
    }
}

fn display_replacement_dimension(value: f32) -> f32 {
    if value.is_finite() {
        value.max(1.0)
    } else {
        1.0
    }
}

fn display_replacement_margin(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn display_replacement_ascent(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowBreak {
    pub(crate) reason: DisplayRowBreakReason,
    pub(crate) line_height: DisplayLineHeightPolicy,
    pub(crate) line_spacing: DisplayLineSpacingPolicy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DisplayLineHeightPolicy {
    /// The newline contributes its face's normal height and configured line
    /// spacing to the display row.
    #[default]
    Default,
    /// GNU `line-height t`: the newline contributes no default height or line
    /// spacing; visible row contents alone determine the row geometry.
    ContentOnly,
}

impl DisplayLineHeightPolicy {
    pub(crate) fn from_property(value: Option<Value>) -> Self {
        if value.is_some_and(|value| value.is_t()) {
            Self::ContentOnly
        } else {
            Self::Default
        }
    }
}

/// String-local `line-spacing` carried until the newline's realized face
/// height is known.  A fractional value is relative to that face, not to the
/// surrounding window's default face.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum DisplayLineSpacingPolicy {
    #[default]
    Inherit,
    Pixels(f32),
    Scale {
        factor: f32,
        reference: DisplayLineSpacingReference,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum DisplayLineSpacingReference {
    #[default]
    DefaultFace,
    CurrentFace,
    NamedFace(Value),
}

impl DisplayLineSpacingPolicy {
    pub(crate) fn from_property(value: Option<Value>) -> Self {
        match value {
            Some(value) if value.is_fixnum() => {
                Self::Pixels(value.as_fixnum().unwrap_or_default() as f32)
            }
            Some(value) if value.is_float() => Self::Scale {
                factor: value.xfloat() as f32,
                reference: DisplayLineSpacingReference::DefaultFace,
            },
            Some(value) if value.is_cons() => {
                let face = value.cons_car();
                let amount = value.cons_cdr();
                let factor = if amount.is_float() {
                    amount.xfloat() as f32
                } else if amount.is_fixnum() {
                    amount.as_fixnum().unwrap_or(1) as f32
                } else {
                    1.0
                };
                let reference = if face.is_nil() {
                    DisplayLineSpacingReference::CurrentFace
                } else {
                    DisplayLineSpacingReference::NamedFace(face)
                };
                Self::Scale { factor, reference }
            }
            _ => Self::Inherit,
        }
    }

    pub(crate) fn resolve(self, base_height: f32, inherited: f32) -> f32 {
        let value = match self {
            Self::Inherit => inherited,
            Self::Pixels(value) => value,
            Self::Scale { factor, .. } => base_height * factor,
        };
        if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        }
    }
}

impl DisplayRowBreak {
    pub(crate) const fn explicit_newline() -> Self {
        Self {
            reason: DisplayRowBreakReason::ExplicitNewline,
            line_height: DisplayLineHeightPolicy::Default,
            line_spacing: DisplayLineSpacingPolicy::Inherit,
        }
    }

    pub(crate) const fn with_line_height(mut self, line_height: DisplayLineHeightPolicy) -> Self {
        self.line_height = line_height;
        self
    }

    pub(crate) const fn with_line_spacing(
        mut self,
        line_spacing: DisplayLineSpacingPolicy,
    ) -> Self {
        self.line_spacing = line_spacing;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayRowBreakReason {
    ExplicitNewline,
    #[allow(dead_code)]
    Wrap,
    #[allow(dead_code)]
    Truncate,
    #[allow(dead_code)]
    EndOfSource,
}

#[cfg(test)]
#[path = "display_item_test.rs"]
mod tests;
