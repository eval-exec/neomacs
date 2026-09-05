use crate::display_item::{
    BufferDisplayReplacementSource, DisplayGlyphless, DisplayItem, DisplayItemKind,
    DisplayItemLayout, DisplayLength, DisplayLineHeightPolicy, DisplayLineSpacingPolicy,
    DisplayMediaReplacement, DisplayPointerAppearance, DisplayPointerOccurrence,
    DisplayPointerSourceRange, DisplayRowBreak, DisplayRowBreakReason, DisplaySourceMappedText,
    DisplaySourcePosition, DisplayStretch, DisplayStretchWidth, DisplayStringBoxBoundaries,
    DisplayTextComposition, DisplayTextRun, GlyphlessJoinerPolicy, GlyphlessMethod, RenderFaceRef,
    SourceSpan, glyphless_method_for_char,
};
use crate::display_origin::{DisplayOrigin, DisplayPropertySource, OverlayStringKind};
use crate::display_property::{
    DisplayMarginContent, DisplayMarginSide, DisplayPropertyClassification,
    DisplayReplacementProperty, classify_display_property,
    classify_display_property_modifiers_only,
};
use crate::display_row::append_context::DisplayRowAppendKind;
use crate::display_row::append_context::DisplayRowTextNaturalAdvanceKind;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_source_append_plan::{
    DisplaySourceAppendMeasurementKind, DisplaySourceAppendRenderPlan, DisplaySourceFallbackWidth,
};
use crate::display_spec::{DisplaySpaceKey, display_space_positive_number};
use crate::neovm_bridge::{LayoutBufferView, OrderedFaceSources, TtyGlyphlessCharDisplay};
use crate::types::{NobreakDisplayMode, WindowParams};
use crate::unicode::{EmacsTextStorage, decode_emacs_char, decode_utf8};
use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::{
    BufferId, CharLen, CharPos0, EmacsBytePos, text_props::TextPropertyTable,
};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::composite::composition_display_text_for_property;
use neovm_core::emacs_core::emacs_char::EmacsChar;
use neovm_core::emacs_core::value::{get_string_text_properties_table_for_value, list_to_vec};
use neovm_core::face::LispFaceId;

pub(crate) struct DisplaySourceContext<'a> {
    face_resolver: Option<&'a mut dyn DisplayItemFaceResolver>,
    /// Typed side channel for output that does not belong to the text area.
    ///
    /// GNU's iterator changes `glyph_row_area` for margin/fringe display specs;
    /// it does not turn them into zero-width text.  Keeping the placement in an
    /// exhaustive enum prevents a newly supported non-text area from silently
    /// degrading to `Empty` while walking nested Lisp/overlay strings.
    non_text_area_sink: Option<&'a mut Vec<DisplayNonTextAreaEmission>>,
}

impl<'a> DisplaySourceContext<'a> {
    pub(crate) const fn empty() -> Self {
        Self {
            face_resolver: None,
            non_text_area_sink: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_face_resolver(resolver: &'a mut dyn DisplayItemFaceResolver) -> Self {
        Self {
            face_resolver: Some(resolver),
            non_text_area_sink: None,
        }
    }

    pub(crate) fn with_face_resolver_and_non_text_area_sink(
        resolver: &'a mut dyn DisplayItemFaceResolver,
        non_text_area_sink: &'a mut Vec<DisplayNonTextAreaEmission>,
    ) -> Self {
        Self {
            face_resolver: Some(resolver),
            non_text_area_sink: Some(non_text_area_sink),
        }
    }

    fn collect_fringe(&mut self, layout: crate::display_spec::DisplayFringeLayout) {
        if let Some(sink) = self.non_text_area_sink.as_mut() {
            sink.push(DisplayNonTextAreaEmission::Fringe(layout));
        }
    }

    fn collect_margin(&mut self, emission: DisplayMarginEmission) {
        if let Some(sink) = self.non_text_area_sink.as_mut() {
            sink.push(DisplayNonTextAreaEmission::Margin(emission));
        }
    }

    pub(crate) fn resolve_face_ref(
        &mut self,
        base: RenderFaceRef,
        face_value: Value,
    ) -> RenderFaceRef {
        self.face_resolver
            .as_mut()
            .map(|resolver| resolver.resolve_face_ref(base, face_value))
            .unwrap_or(base)
    }

    pub(crate) fn resolve_face_sources(
        &mut self,
        base: RenderFaceRef,
        sources: &OrderedFaceSources,
    ) -> RenderFaceRef {
        self.face_resolver
            .as_mut()
            .map(|resolver| resolver.resolve_face_sources(base, sources))
            .unwrap_or(base)
    }

    pub(crate) fn resolve_lisp_face_ref(
        &mut self,
        base: RenderFaceRef,
        lisp_face_id: LispFaceId,
    ) -> RenderFaceRef {
        self.face_resolver
            .as_mut()
            .map(|resolver| resolver.resolve_lisp_face_ref(base, lisp_face_id))
            .unwrap_or(base)
    }

    pub(crate) fn resolve_pointer_face_ref(
        &mut self,
        base: RenderFaceRef,
        face_value: Value,
    ) -> Option<RenderFaceRef> {
        self.face_resolver
            .as_mut()
            .and_then(|resolver| resolver.resolve_pointer_face_ref(base, face_value))
    }

    pub(crate) fn face_has_box(&mut self, face: RenderFaceRef) -> bool {
        self.face_resolver
            .as_mut()
            .is_some_and(|resolver| resolver.face_has_box(face))
    }

    fn resolve_display_media_replacement(
        &mut self,
        display_prop: Value,
        image_slice: Option<crate::display_spec::DisplayImageSliceSpec>,
        face: RenderFaceRef,
    ) -> Option<DisplayMediaReplacement> {
        self.face_resolver.as_mut().and_then(|resolver| {
            resolver.resolve_display_media_replacement(display_prop, image_slice, face)
        })
    }
}

/// Resolved output whose placement is outside the ordinary text flow.
///
/// This mirrors GNU's closed `glyph_row_area` routing decision.  Consumers
/// must handle every placement explicitly instead of interpreting absence of
/// an inline glyph as absence of output.
#[derive(Clone, Debug)]
pub(crate) enum DisplayNonTextAreaEmission {
    Fringe(crate::display_spec::DisplayFringeLayout),
    Margin(DisplayMarginEmission),
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayMarginEmission {
    side: DisplayMarginSide,
    content: DisplayMarginEmissionContent,
}

impl DisplayMarginEmission {
    pub(crate) fn new(side: DisplayMarginSide, content: DisplayMarginEmissionContent) -> Self {
        Self { side, content }
    }

    pub(crate) fn side(&self) -> DisplayMarginSide {
        self.side
    }

    pub(crate) fn content(&self) -> &DisplayMarginEmissionContent {
        &self.content
    }
}

#[derive(Clone, Debug)]
pub(crate) enum DisplayMarginEmissionContent {
    String(Value),
    Item(DisplayItemKind),
}

impl Default for DisplaySourceContext<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

pub(crate) trait DisplayItemSource {
    fn next_item(&mut self, context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplaySourceAppendContinuation {
    Rendered,
    Stopped,
}

impl DisplaySourceAppendContinuation {
    pub(crate) fn should_break(self) -> bool {
        matches!(self, Self::Stopped)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceRangeItemAppendRequest {
    item: DisplayItem,
    append_kind: DisplayRowAppendKind,
}

impl DisplaySourceRangeItemAppendRequest {
    pub(crate) fn new(item: DisplayItem, append_kind: DisplayRowAppendKind) -> Self {
        Self { item, append_kind }
    }

    #[cfg(test)]
    pub(crate) fn append_kind(&self) -> DisplayRowAppendKind {
        self.append_kind
    }

    pub(crate) fn into_item(self) -> DisplayItem {
        self.item
    }
}

/// Source for one logical display item.
///
/// Most items are yielded once. A display-table `SourceMappedText` remains one
/// source element to the buffer walk, but is exposed here as contiguous face
/// segments so row measurement and rendering use each glyph's GNU lface.
pub(crate) struct DisplayItemSegmentSource {
    item: Option<DisplayItem>,
    face_run_index: usize,
    text_char_offset: usize,
    text_byte_offset: usize,
}

impl DisplayItemSegmentSource {
    pub(crate) fn new(item: DisplayItem) -> Self {
        Self {
            item: Some(item),
            face_run_index: 0,
            text_char_offset: 0,
            text_byte_offset: 0,
        }
    }
}

impl DisplayItemSource for DisplayItemSegmentSource {
    fn next_item(&mut self, context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        let item = self.item.as_ref()?;
        let DisplayItemKind::SourceMappedText(mapped) = &item.kind else {
            return self.item.take();
        };
        if mapped.lisp_face_runs().is_empty() {
            return self.item.take();
        }

        let run = mapped.lisp_face_runs().get(self.face_run_index)?.clone();
        let remaining = mapped.text.get(self.text_byte_offset..)?;
        let segment_byte_len = remaining
            .char_indices()
            .nth(run.char_len)
            .map_or(remaining.len(), |(byte, _)| byte);
        let segment_text: Box<str> = remaining[..segment_byte_len].into();
        let glyph_string_start = mapped.glyph_string_start.as_ref().map(|start| {
            start
                .clone()
                .advanced_by(self.text_char_offset, self.text_byte_offset)
        });
        let face = run
            .lisp_face_id
            .map(|id| context.resolve_lisp_face_ref(item.face, id))
            .unwrap_or(item.face);
        let current_boxed = context.face_has_box(face);
        let previous_boxed = if self.face_run_index == 0 {
            // GNU saves the face of the replaced source character before
            // entering a display vector. The vector's first realized face is
            // compared with that saved face, not with the previous buffer
            // character outside this item.
            context.face_has_box(item.face)
        } else {
            let previous = &mapped.lisp_face_runs()[self.face_run_index - 1];
            let previous_face = previous
                .lisp_face_id
                .map(|id| context.resolve_lisp_face_ref(item.face, id))
                .unwrap_or(item.face);
            context.face_has_box(previous_face)
        };
        let next_boxed = if self.face_run_index + 1 == mapped.lisp_face_runs().len() {
            // The same saved source face is restored after the vector, so it
            // is the authoritative successor at the final mapped run.
            context.face_has_box(item.face)
        } else {
            let next = &mapped.lisp_face_runs()[self.face_run_index + 1];
            let next_face = next
                .lisp_face_id
                .map(|id| context.resolve_lisp_face_ref(item.face, id))
                .unwrap_or(item.face);
            context.face_has_box(next_face)
        };
        let segment = DisplayItem {
            span: item.span.clone(),
            face,
            kind: DisplayItemKind::SourceMappedText(DisplaySourceMappedText::face_segment(
                segment_text,
                glyph_string_start,
            )),
            layout: item.layout,
            pointer_appearance: item.pointer_appearance.clone(),
            box_vertical_edges: BoxVerticalEdges::from_ownership(
                current_boxed && !previous_boxed,
                current_boxed && !next_boxed,
            ),
            box_run_membership: neomacs_display_protocol::face::BoxRunMembership::from_boxed(
                current_boxed,
            ),
        };

        self.face_run_index += 1;
        self.text_char_offset = self.text_char_offset.saturating_add(run.char_len);
        self.text_byte_offset = self.text_byte_offset.saturating_add(segment_byte_len);
        if self.face_run_index == mapped.lisp_face_runs().len() {
            self.item = None;
        }
        Some(segment)
    }
}

pub(crate) trait DisplayItemFaceResolver {
    fn resolve_face_ref(&mut self, base: RenderFaceRef, face_value: Value) -> RenderFaceRef;

    fn resolve_lisp_face_ref(
        &mut self,
        base: RenderFaceRef,
        _lisp_face_id: LispFaceId,
    ) -> RenderFaceRef {
        base
    }

    fn resolve_face_sources(
        &mut self,
        base: RenderFaceRef,
        sources: &OrderedFaceSources,
    ) -> RenderFaceRef;

    /// Whether the fully resolved face participates in GNU's box-run
    /// topology.  This is required rather than defaulting to `false`: source
    /// producers use it to publish affine start/end ownership, so a resolver
    /// that forgets the capability would silently erase every box terminal.
    fn face_has_box(&self, face: RenderFaceRef) -> bool;

    fn resolve_pointer_face_ref(
        &mut self,
        base: RenderFaceRef,
        face_value: Value,
    ) -> Option<RenderFaceRef> {
        let resolved = self.resolve_face_ref(base, face_value);
        (resolved != base).then_some(resolved)
    }

    fn resolve_display_media_replacement(
        &mut self,
        _display_prop: Value,
        _image_slice: Option<crate::display_spec::DisplayImageSliceSpec>,
        _face: RenderFaceRef,
    ) -> Option<DisplayMediaReplacement> {
        None
    }
}

pub(crate) struct SyntheticTextItemSource {
    item: Option<DisplayItem>,
}

impl SyntheticTextItemSource {
    pub(crate) fn new(
        source_id: u64,
        text: impl Into<Box<str>>,
        face: RenderFaceRef,
        start_offset: usize,
    ) -> Self {
        let text = text.into();
        let end_offset = start_offset.saturating_add(text.chars().count());
        let item = DisplayItem::new(
            SourceSpan::synthetic(source_id, start_offset, end_offset),
            face,
            DisplayItemKind::TextRun(DisplayTextRun::new(text)),
        );
        Self { item: Some(item) }
    }
}

impl DisplayItemSource for SyntheticTextItemSource {
    fn next_item(&mut self, _context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.item.take()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BufferTextItemSource {
    buffer_id: BufferId,
    start_char: CharPos0,
    start_byte: EmacsBytePos,
    end_char: CharPos0,
    end_byte: EmacsBytePos,
}

impl BufferTextItemSource {
    pub(crate) const fn new(
        buffer_id: BufferId,
        start_char: CharPos0,
        start_byte: EmacsBytePos,
        end_char: CharPos0,
        end_byte: EmacsBytePos,
    ) -> Self {
        Self {
            buffer_id,
            start_char,
            start_byte,
            end_char,
            end_byte,
        }
    }

    #[cfg(test)]
    pub(crate) fn single_char(
        buffer_id: BufferId,
        char_pos: CharPos0,
        start_byte: EmacsBytePos,
        end_byte: EmacsBytePos,
    ) -> Self {
        Self::new(
            buffer_id,
            char_pos,
            start_byte,
            char_pos.add_len(CharLen::new(1)),
            end_byte,
        )
    }

    fn span(self) -> SourceSpan {
        SourceSpan::new(
            DisplaySourcePosition::buffer(self.buffer_id, self.start_char, self.start_byte),
            DisplaySourcePosition::buffer(self.buffer_id, self.end_char, self.end_byte),
        )
    }

    pub(crate) fn item(self, face: RenderFaceRef, kind: DisplayItemKind) -> DisplayItem {
        DisplayItem::new(self.span(), face, kind)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplaySourceTextRange {
    start: CharPos0,
    end: CharPos0,
}

impl DisplaySourceTextRange {
    pub(crate) fn new(start: CharPos0, end: CharPos0) -> Self {
        Self { start, end }
    }

    pub(crate) fn single_char(start: CharPos0) -> Self {
        Self::new(start, start.add_len(CharLen::new(1)))
    }

    pub(crate) fn start(self) -> CharPos0 {
        self.start
    }

    pub(crate) fn end(self) -> CharPos0 {
        self.end
    }

    #[cfg(test)]
    pub(crate) fn is_single_char(self) -> bool {
        self.end == self.start.add_len(CharLen::new(1))
    }

    pub(crate) fn is_empty_or_reversed(self) -> bool {
        self.end <= self.start
    }
}

/// Buffer-absolute byte origin of the window-local text slice.
///
/// This type is the explicit conversion boundary between absolute buffer byte
/// positions and the relative byte indices consumed by a display walk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisplaySourceTextOrigin {
    buffer_byte: usize,
}

impl DisplaySourceTextOrigin {
    pub(crate) const fn new(buffer_byte: usize) -> Self {
        Self { buffer_byte }
    }

    pub(crate) const fn buffer_byte(self) -> usize {
        self.buffer_byte
    }

    pub(crate) fn position_from_buffer(
        self,
        byte_pos: EmacsBytePos,
        char_pos: CharPos0,
    ) -> Option<DisplaySourceTextPosition> {
        Some(DisplaySourceTextPosition::new(
            byte_pos.get().checked_sub(self.buffer_byte)?,
            char_pos.get() as i64,
        ))
    }
}

/// Position in the window-local text slice consumed by one display walk.
///
/// `byte_idx` is deliberately not an [`EmacsBytePos`]: it is relative to the
/// walk's [`DisplaySourceTextOrigin`], while `charpos` remains buffer-absolute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisplaySourceTextPosition {
    byte_idx: usize,
    charpos: i64,
}

impl DisplaySourceTextPosition {
    pub(crate) const fn new(byte_idx: usize, charpos: i64) -> Self {
        Self { byte_idx, charpos }
    }

    pub(crate) const fn byte_idx(self) -> usize {
        self.byte_idx
    }

    pub(crate) const fn charpos(self) -> i64 {
        self.charpos
    }

    pub(crate) const fn with_charpos(self, charpos: i64) -> Self {
        Self {
            byte_idx: self.byte_idx,
            charpos,
        }
    }

    pub(crate) fn advance_byte_idx_to(&mut self, byte_idx: usize) {
        self.byte_idx = byte_idx;
    }

    pub(crate) fn advance_charpos_by_one(&mut self) {
        self.charpos = self.charpos.saturating_add(1);
    }

    pub(crate) fn advance_one_char(&mut self, ch_len: usize) {
        self.byte_idx = self.byte_idx.saturating_add(ch_len);
        self.charpos = self.charpos.saturating_add(1);
    }

    pub(crate) fn matches(self, byte_idx: usize, charpos: i64) -> bool {
        self.byte_idx == byte_idx && self.charpos == charpos
    }

    pub(crate) fn consume_step_char(&mut self, text: &[u8]) -> Option<DisplaySourceStepChar> {
        if self.byte_idx >= text.len() {
            return None;
        }
        let start_byte_idx = self.byte_idx;
        let start_charpos = self.charpos;
        let (ch, ch_len) = decode_utf8(&text[start_byte_idx..]);
        if ch_len == 0 {
            return None;
        }
        self.advance_one_char(ch_len);
        Some(DisplaySourceStepChar::new(
            ch,
            start_byte_idx,
            start_charpos,
        ))
    }

    pub(crate) fn skip_chars_until(&mut self, text: &[u8], charpos: i64) {
        while self.charpos < charpos && self.byte_idx < text.len() {
            if self.consume_step_char(text).is_none() {
                break;
            }
        }
    }

    pub(crate) fn consume_until_line_break(&mut self, text: &[u8]) -> bool {
        while self.byte_idx < text.len() {
            let Some(source_char) = self.consume_step_char(text) else {
                break;
            };
            if source_char.ch() == '\n' {
                return true;
            }
        }
        false
    }

    pub(crate) fn consume_one_then_until_line_break(&mut self, text: &[u8]) -> bool {
        let Some(source_char) = self.consume_step_char(text) else {
            return false;
        };
        source_char.ch() == '\n' || self.consume_until_line_break(text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplaySourceStepChar {
    ch: char,
    start_byte_idx: usize,
    start_charpos: i64,
}

impl DisplaySourceStepChar {
    pub(crate) const fn new(ch: char, start_byte_idx: usize, start_charpos: i64) -> Self {
        Self {
            ch,
            start_byte_idx,
            start_charpos,
        }
    }

    pub(crate) fn ch(self) -> char {
        self.ch
    }

    pub(crate) fn start_byte_idx(self) -> usize {
        self.start_byte_idx
    }

    pub(crate) fn start_charpos(self) -> i64 {
        self.start_charpos
    }

    pub(crate) fn source_range(self) -> DisplaySourceTextRange {
        DisplaySourceTextRange::single_char(CharPos0::new(self.start_charpos as usize))
    }

    pub(crate) fn source_char(
        self,
        nobreak_display_policy: NobreakDisplayMode,
    ) -> DisplaySourceTextChar {
        DisplaySourceTextChar::new(self.ch, self.source_range().start(), nobreak_display_policy)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceItem {
    item: DisplayItem,
    start_byte_idx: usize,
    start_charpos: i64,
    source_char: Option<char>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceStepItem {
    item: DisplayItem,
    source_step_char: DisplaySourceStepChar,
    source_end_charpos: Option<i64>,
    source_end_byte_idx: Option<usize>,
    is_explicit_line_break: bool,
}

impl DisplaySourceStepItem {
    pub(crate) fn new(source_item: DisplaySourceItem, text_start_byte: usize) -> Option<Self> {
        let is_explicit_line_break = source_item.is_explicit_line_break();
        let source_step_char = source_item.source_step_char()?;
        let source_end_charpos = source_item.buffer_end_charpos();
        let source_end_byte_idx = source_item.end_byte_idx(text_start_byte);
        Some(Self {
            item: source_item.into_item(),
            source_step_char,
            source_end_charpos,
            source_end_byte_idx,
            is_explicit_line_break,
        })
    }

    pub(crate) fn source_step_char(&self) -> DisplaySourceStepChar {
        self.source_step_char
    }

    pub(crate) fn source_end_charpos(&self) -> Option<i64> {
        self.source_end_charpos
    }

    #[cfg(test)]
    pub(crate) fn end_charpos(&self) -> i64 {
        self.source_end_charpos
            .unwrap_or_else(|| self.source_step_char.start_charpos().saturating_add(1))
    }

    pub(crate) fn source_end_byte_idx(&self) -> Option<usize> {
        self.source_end_byte_idx
    }

    pub(crate) fn is_explicit_line_break(&self) -> bool {
        self.is_explicit_line_break
    }

    pub(crate) fn item(&self) -> &DisplayItem {
        &self.item
    }

    pub(crate) fn item_mut(&mut self) -> &mut DisplayItem {
        &mut self.item
    }

    #[cfg(test)]
    pub(crate) fn is_multi_char_text_run(&self) -> bool {
        let DisplayItemKind::TextRun(run) = &self.item.kind else {
            return false;
        };
        if matches!(&run.composition, DisplayTextComposition::Automatic(_)) {
            return false;
        }
        let mut chars = run.text.chars();
        chars.next().is_some() && chars.next().is_some()
    }

    pub(crate) fn text_run(&self) -> Option<&str> {
        let DisplayItemKind::TextRun(run) = &self.item.kind else {
            return None;
        };
        if matches!(&run.composition, DisplayTextComposition::Automatic(_)) {
            return None;
        }
        let text = &*run.text;
        let mut chars = text.chars();
        chars.next()?;
        chars.next()?;
        // Exclude any char that needs a precluster Special substitute (newline,
        // CR, tab, and every nobreak space/hyphen GNU highlights). Such chars
        // must break the multi-char fast path so they get their own single-char
        // item -- their nobreak face is keyed on the per-item source char.
        text.chars()
            .all(|ch| {
                !matches!(ch, '\n' | '\r' | '\t') && !nonascii_space_p(ch) && !nonascii_hyphen_p(ch)
            })
            .then_some(text)
    }

    pub(crate) fn raw_text_run(&self) -> Option<&str> {
        let DisplayItemKind::TextRun(run) = &self.item.kind else {
            return None;
        };
        Some(&run.text)
    }

    /// The first character of a multi-character text run as its own step item.
    /// See [`DisplaySourceItem::first_text_run_char`].
    pub(crate) fn first_text_run_char(self, text_start_byte: usize) -> Option<Self> {
        let source_step_char = self.source_step_char;
        let source_item = DisplaySourceItem::new(
            self.item,
            source_step_char.start_byte_idx(),
            source_step_char.start_charpos(),
            Some(source_step_char.ch()),
        );
        let first = source_item.first_text_run_char(text_start_byte)?;
        DisplaySourceStepItem::new(first, text_start_byte)
    }

    #[cfg(test)]
    pub(crate) fn split_text_run_items(
        self,
        text_start_byte: usize,
    ) -> Option<(Self, Vec<DisplaySourceStepItem>)> {
        let source_step_char = self.source_step_char;
        let source_item = DisplaySourceItem::new(
            self.item,
            source_step_char.start_byte_idx(),
            source_step_char.start_charpos(),
            Some(source_step_char.ch()),
        );
        let (first, pending) = source_item.split_text_run_items(text_start_byte)?;
        let first = DisplaySourceStepItem::new(first, text_start_byte)?;
        let pending = pending
            .into_iter()
            .filter_map(|item| DisplaySourceStepItem::new(item, text_start_byte))
            .collect();
        Some((first, pending))
    }

    pub(crate) fn split_text_run_at_charpos(
        self,
        split_charpos: i64,
        text_start_byte: usize,
    ) -> Option<(Self, Self)> {
        let source_step_char = self.source_step_char;
        let source_item = DisplaySourceItem::new(
            self.item,
            source_step_char.start_byte_idx(),
            source_step_char.start_charpos(),
            Some(source_step_char.ch()),
        );
        let (prefix, suffix) =
            source_item.split_text_run_at_charpos(split_charpos, text_start_byte)?;
        Some((
            DisplaySourceStepItem::new(prefix, text_start_byte)?,
            DisplaySourceStepItem::new(suffix, text_start_byte)?,
        ))
    }

    pub(crate) fn into_render_parts(
        self,
    ) -> (
        DisplaySourceStepChar,
        Option<i64>,
        Option<usize>,
        DisplayItem,
    ) {
        (
            self.source_step_char,
            self.source_end_charpos,
            self.source_end_byte_idx,
            self.item,
        )
    }

    #[cfg(test)]
    pub(crate) fn into_test_render_parts(self) -> Option<(DisplaySourceStepChar, DisplayItem)> {
        Some((self.source_step_char, self.item))
    }
}

impl DisplaySourceItem {
    pub(crate) fn new(
        item: DisplayItem,
        start_byte_idx: usize,
        start_charpos: i64,
        source_char: Option<char>,
    ) -> Self {
        Self {
            item,
            start_byte_idx,
            start_charpos,
            source_char,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        item: DisplayItem,
        start_byte_idx: usize,
        start_charpos: i64,
        source_char: Option<char>,
    ) -> Self {
        Self::new(item, start_byte_idx, start_charpos, source_char)
    }

    pub(crate) fn direct_source_char(&self) -> Option<char> {
        match &self.item.kind {
            DisplayItemKind::TextRun(run) => run.text.chars().next(),
            DisplayItemKind::RowBreak(row_break)
                if row_break.reason == DisplayRowBreakReason::ExplicitNewline =>
            {
                Some('\n')
            }
            DisplayItemKind::ControlChar { ch } => Some(*ch),
            DisplayItemKind::Glyphless(glyphless) => Some(glyphless.ch),
            DisplayItemKind::SourceMappedText(mapped) => self.source_char.or_else(|| {
                mapped
                    .semantic_face_overlay()
                    .is_some_and(|overlay| {
                        overlay == crate::display_item::DisplayItemFaceOverlay::EscapeGlyph
                    })
                    .then(|| mapped.text.chars().next())
                    .flatten()
            }),
            _ => None,
        }
    }

    pub(crate) fn start_byte_idx(&self) -> usize {
        self.start_byte_idx
    }

    pub(crate) fn start_charpos(&self) -> i64 {
        self.start_charpos
    }

    #[cfg(test)]
    pub(crate) fn item(&self) -> &DisplayItem {
        &self.item
    }

    pub(crate) fn buffer_byte_len(&self) -> Option<usize> {
        self.item.span.buffer_byte_len()
    }

    // The `Err` arm returns the unconsumed item to the caller (retry protocol),
    // so it is deliberately the same large type as `Ok`; boxing it is a perf
    // hint deferred out of the lint gate.
    #[allow(clippy::result_large_err)]
    pub(crate) fn consume_for_render(
        self,
        position: &mut DisplaySourceTextPosition,
    ) -> Result<Self, Self> {
        if !position.matches(self.start_byte_idx(), self.start_charpos()) {
            tracing::error!(
                "DisplaySourceItem: validated source item at byte {} charpos {} \
                 did not match source walk byte {} charpos {}",
                self.start_byte_idx(),
                self.start_charpos(),
                position.byte_idx(),
                position.charpos()
            );
            return Err(self);
        }
        let Some(ch) = self.direct_source_char() else {
            return Err(self);
        };
        let byte_len = self.buffer_byte_len().unwrap_or_else(|| ch.len_utf8());
        position.advance_byte_idx_to(self.start_byte_idx().saturating_add(byte_len));
        Ok(self)
    }

    pub(crate) fn source_step_char(&self) -> Option<DisplaySourceStepChar> {
        Some(DisplaySourceStepChar::new(
            self.direct_source_char()?,
            self.start_byte_idx,
            self.start_charpos,
        ))
    }

    pub(crate) fn is_explicit_line_break(&self) -> bool {
        matches!(
            self.item.kind,
            DisplayItemKind::RowBreak(row_break)
                if row_break.reason == DisplayRowBreakReason::ExplicitNewline
        )
    }

    #[cfg(test)]
    pub(crate) fn end_charpos(&self) -> i64 {
        self.item
            .span
            .buffer_end_charpos()
            .map(|char_pos| char_pos.get() as i64)
            .unwrap_or_else(|| self.start_charpos.saturating_add(1))
    }

    pub(crate) fn end_byte_idx(&self, text_start_byte: usize) -> Option<usize> {
        display_item_buffer_end_byte_idx(&self.item, text_start_byte)
    }

    pub(crate) fn is_multi_char_text_run(&self) -> bool {
        let DisplayItemKind::TextRun(run) = &self.item.kind else {
            return false;
        };
        if matches!(&run.composition, DisplayTextComposition::Automatic(_)) {
            return false;
        }
        let mut chars = run.text.chars();
        chars.next().is_some() && chars.next().is_some()
    }

    /// The FIRST character of a multi-character text run, as its own item.
    ///
    /// The renderer takes one character and the producer's position carries the
    /// rest: the next production reads the remainder straight from the cursor,
    /// so no remainder is materialized here (the whole-run split into N items
    /// existed only to feed them back through a queue).
    pub(crate) fn first_text_run_char(self, text_start_byte: usize) -> Option<Self> {
        if !self.is_multi_char_text_run() {
            return None;
        }
        let DisplayItem {
            span,
            face,
            kind,
            layout,
            pointer_appearance,
            box_vertical_edges,
            box_run_membership,
        } = self.item;
        let DisplayItemKind::TextRun(run) = kind else {
            return None;
        };
        let DisplaySourcePosition::Buffer { buffer_id, .. } = span.start else {
            return None;
        };
        let ch = run.text.chars().next()?;
        let item = direct_text_run_char_item(
            buffer_id,
            face,
            layout,
            text_start_byte,
            self.start_byte_idx,
            self.start_charpos,
            ch,
            run.composition,
        );
        Some(DisplaySourceItem::new(
            item.with_pointer_appearance(pointer_appearance)
                .with_box_run_topology(
                    box_run_membership.is_boxed(),
                    BoxVerticalEdges::from_ownership(box_vertical_edges.owns_left(), false),
                ),
            self.start_byte_idx,
            self.start_charpos,
            Some(ch),
        ))
    }

    #[cfg(test)]
    pub(crate) fn split_text_run_items(
        self,
        text_start_byte: usize,
    ) -> Option<(Self, Vec<DisplaySourceItem>)> {
        if !self.is_multi_char_text_run() {
            return None;
        }
        let DisplayItem {
            span,
            face,
            kind,
            layout,
            pointer_appearance,
            box_vertical_edges,
            box_run_membership,
        } = self.item;
        let DisplayItemKind::TextRun(run) = kind else {
            return None;
        };
        let DisplaySourcePosition::Buffer { buffer_id, .. } = span.start else {
            return None;
        };
        let mut byte_idx = self.start_byte_idx;
        let mut charpos = self.start_charpos;
        let mut items = Vec::new();
        let composition = run.composition;
        let chars = run.text.chars().collect::<Vec<_>>();
        let last_index = chars.len().saturating_sub(1);
        for (index, ch) in chars.into_iter().enumerate() {
            let ch_len = ch.len_utf8();
            let item = direct_text_run_char_item(
                buffer_id,
                face,
                layout,
                text_start_byte,
                byte_idx,
                charpos,
                ch,
                composition.clone(),
            );
            items.push(DisplaySourceItem::new(
                item.with_pointer_appearance(pointer_appearance.clone())
                    .with_box_run_topology(
                        box_run_membership.is_boxed(),
                        BoxVerticalEdges::from_ownership(
                            index == 0 && box_vertical_edges.owns_left(),
                            index == last_index && box_vertical_edges.owns_right(),
                        ),
                    ),
                byte_idx,
                charpos,
                Some(ch),
            ));
            byte_idx = byte_idx.saturating_add(ch_len);
            charpos = charpos.saturating_add(1);
        }
        if items.len() <= 1 {
            return None;
        }
        let mut iter = items.into_iter();
        let first = iter.next()?;
        let pending = iter.collect();
        Some((first, pending))
    }

    pub(crate) fn split_text_run_at_charpos(
        self,
        split_charpos: i64,
        text_start_byte: usize,
    ) -> Option<(Self, Self)> {
        let DisplayItem {
            span,
            face,
            kind,
            layout,
            pointer_appearance,
            box_vertical_edges,
            box_run_membership,
        } = self.item;
        let DisplayItemKind::TextRun(run) = kind else {
            return None;
        };
        let DisplaySourcePosition::Buffer {
            buffer_id,
            char_pos: start_char_pos,
            byte_pos: start_byte_pos,
        } = span.start
        else {
            return None;
        };
        let DisplaySourcePosition::Buffer {
            char_pos: end_char_pos,
            byte_pos: end_byte_pos,
            ..
        } = span.end
        else {
            return None;
        };
        let start_charpos = start_char_pos.get() as i64;
        let end_charpos = end_char_pos.get() as i64;
        if split_charpos <= start_charpos || split_charpos >= end_charpos {
            return None;
        }

        let split_char_offset = split_charpos.checked_sub(start_charpos)? as usize;
        let split_text_byte_offset = run
            .text
            .char_indices()
            .nth(split_char_offset)
            .map(|(idx, _)| idx)?;
        if split_text_byte_offset == 0 || split_text_byte_offset >= run.text.len() {
            return None;
        }
        let split_byte_idx = self.start_byte_idx.checked_add(split_text_byte_offset)?;
        let split_byte_pos = EmacsBytePos::new(text_start_byte.checked_add(split_byte_idx)?);
        let split_char_pos = CharPos0::new(split_charpos as usize);
        let prefix_text = run.text.get(..split_text_byte_offset)?.to_owned();
        let suffix_text = run.text.get(split_text_byte_offset..)?.to_owned();
        let composition = run.composition;

        let prefix = DisplayItem::new(
            SourceSpan::new(
                DisplaySourcePosition::buffer(buffer_id, start_char_pos, start_byte_pos),
                DisplaySourcePosition::buffer(buffer_id, split_char_pos, split_byte_pos),
            ),
            face,
            DisplayItemKind::TextRun(DisplayTextRun::with_composition(
                prefix_text,
                composition.clone(),
            )),
        )
        .with_layout(layout)
        .with_box_run_topology(
            box_run_membership.is_boxed(),
            BoxVerticalEdges::from_ownership(box_vertical_edges.owns_left(), false),
        )
        .with_pointer_appearance(pointer_appearance.clone());
        let suffix = DisplayItem::new(
            SourceSpan::new(
                DisplaySourcePosition::buffer(buffer_id, split_char_pos, split_byte_pos),
                DisplaySourcePosition::buffer(buffer_id, end_char_pos, end_byte_pos),
            ),
            face,
            DisplayItemKind::TextRun(DisplayTextRun::with_composition(suffix_text, composition)),
        )
        .with_layout(layout)
        .with_box_run_topology(
            box_run_membership.is_boxed(),
            BoxVerticalEdges::from_ownership(false, box_vertical_edges.owns_right()),
        )
        .with_pointer_appearance(pointer_appearance);
        Some((
            DisplaySourceItem::new(prefix, self.start_byte_idx, self.start_charpos, None),
            DisplaySourceItem::new(suffix, split_byte_idx, split_charpos, None),
        ))
    }

    #[cfg(test)]
    pub(crate) fn into_render_parts(self) -> Option<(DisplaySourceStepChar, DisplayItem)> {
        let source_step_char = self.source_step_char()?;
        Some((source_step_char, self.item))
    }

    pub(crate) fn buffer_end_charpos(&self) -> Option<i64> {
        self.item
            .span
            .buffer_end_charpos()
            .map(|char_pos| char_pos.get() as i64)
    }

    pub(crate) fn into_item(self) -> DisplayItem {
        self.item
    }
}

fn display_item_buffer_end_byte_idx(item: &DisplayItem, text_start_byte: usize) -> Option<usize> {
    let DisplaySourcePosition::Buffer {
        byte_pos: end_byte_pos,
        ..
    } = item.span.end
    else {
        return None;
    };
    end_byte_pos.get().checked_sub(text_start_byte)
}

fn direct_text_run_char_item(
    buffer_id: BufferId,
    face: RenderFaceRef,
    layout: DisplayItemLayout,
    text_start_byte: usize,
    start_byte_idx: usize,
    start_charpos: i64,
    ch: char,
    composition: DisplayTextComposition,
) -> DisplayItem {
    let end_byte_idx = start_byte_idx.saturating_add(ch.len_utf8());
    let end_charpos = start_charpos.saturating_add(1);
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::buffer(
                buffer_id,
                CharPos0::new(start_charpos.max(0) as usize),
                EmacsBytePos::new(text_start_byte.saturating_add(start_byte_idx)),
            ),
            DisplaySourcePosition::buffer(
                buffer_id,
                CharPos0::new(end_charpos.max(0) as usize),
                EmacsBytePos::new(text_start_byte.saturating_add(end_byte_idx)),
            ),
        ),
        face,
        DisplayItemKind::TextRun(DisplayTextRun::with_composition(
            ch.to_string(),
            composition,
        )),
    )
    .with_layout(layout)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplaySourceAppendItem {
    ControlChar { ch: char },
    SourceMappedText { text: Box<str> },
    Glyphless { ch: char, method: GlyphlessMethod },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplaySourceSpecialDisplay {
    Control(DisplaySourceAppendItem),
    Nobreak(DisplaySourceAppendItem),
    Glyphless(DisplaySourceAppendItem),
}

impl DisplaySourceSpecialDisplay {
    pub(crate) fn for_precluster_char(
        ch: char,
        nobreak_display_policy: NobreakDisplayMode,
    ) -> Option<Self> {
        if Self::is_control_char(ch) {
            Some(Self::Control(DisplaySourceAppendItem::ControlChar { ch }))
        } else {
            DisplaySourceAppendItem::nobreak_display(ch, nobreak_display_policy).map(Self::Nobreak)
        }
    }

    pub(crate) fn for_cluster_state(cluster: DisplaySourceClusterState) -> Option<Self> {
        DisplaySourceAppendItem::glyphless_display(cluster).map(Self::Glyphless)
    }

    pub(crate) fn into_append_item(self) -> DisplaySourceAppendItem {
        match self {
            Self::Control(item) | Self::Nobreak(item) | Self::Glyphless(item) => item,
        }
    }

    pub(crate) fn is_control(&self) -> bool {
        matches!(self, Self::Control(_))
    }

    #[cfg(test)]
    pub(crate) fn is_nobreak(&self) -> bool {
        matches!(self, Self::Nobreak(_))
    }

    fn is_control_char(ch: char) -> bool {
        (ch < ' ' && ch != '\n' && ch != '\t') || ch == '\x7F'
    }

    pub(crate) fn kind(&self) -> DisplaySourceSpecialDisplayKind {
        match self {
            Self::Control(_) => DisplaySourceSpecialDisplayKind::Control,
            Self::Nobreak(_) => DisplaySourceSpecialDisplayKind::Nobreak,
            Self::Glyphless(_) => DisplaySourceSpecialDisplayKind::Glyphless,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplaySourceSpecialDisplayKind {
    Control,
    Nobreak,
    Glyphless,
}

impl DisplaySourceSpecialDisplayKind {
    pub(crate) fn invalidates_face_after_append(self) -> bool {
        matches!(self, Self::Control | Self::Nobreak)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceTextChar {
    ch: char,
    range: DisplaySourceTextRange,
    precluster_special_display: Option<DisplaySourceSpecialDisplay>,
}

impl DisplaySourceTextChar {
    pub(crate) fn new(
        ch: char,
        start: CharPos0,
        nobreak_display_policy: NobreakDisplayMode,
    ) -> Self {
        Self {
            ch,
            range: DisplaySourceTextRange::single_char(start),
            precluster_special_display: DisplaySourceSpecialDisplay::for_precluster_char(
                ch,
                nobreak_display_policy,
            ),
        }
    }

    pub(crate) fn range(&self) -> DisplaySourceTextRange {
        self.range
    }

    pub(crate) fn precluster_special_display(&self) -> Option<&DisplaySourceSpecialDisplay> {
        self.precluster_special_display.as_ref()
    }

    pub(crate) fn cluster_state(&self, tail: Option<(char, bool)>) -> DisplaySourceClusterState {
        DisplaySourceClusterState::for_char(self.ch, tail)
    }

    pub(crate) fn cluster_special_display(
        &self,
        tail: Option<(char, bool)>,
    ) -> Option<DisplaySourceSpecialDisplay> {
        DisplaySourceSpecialDisplay::for_cluster_state(self.cluster_state(tail))
    }

    fn special_request_for_display(
        &self,
        display: DisplaySourceSpecialDisplay,
    ) -> DisplaySpecialSourceCharRequest {
        DisplaySpecialSourceCharRequest::new(self, display)
    }

    #[cfg(test)]
    pub(crate) fn control_special_request(&self) -> Option<DisplaySpecialSourceCharRequest> {
        self.precluster_special_display()
            .filter(|display| display.is_control())
            .cloned()
            .map(|display| self.special_request_for_display(display))
    }

    #[cfg(test)]
    pub(crate) fn nobreak_special_request(&self) -> Option<DisplaySpecialSourceCharRequest> {
        self.precluster_special_display()
            .filter(|display| display.is_nobreak())
            .cloned()
            .map(|display| self.special_request_for_display(display))
    }

    pub(crate) fn cluster_special_request(
        &self,
        tail: Option<(char, bool)>,
    ) -> Option<DisplaySpecialSourceCharRequest> {
        self.cluster_special_display(tail)
            .map(|display| self.special_request_for_display(display))
    }

    pub(crate) fn special_request(
        &self,
        tail: Option<(char, bool)>,
    ) -> Option<DisplaySpecialSourceCharRequest> {
        self.precluster_special_display()
            .cloned()
            .map(|display| self.special_request_for_display(display))
            .or_else(|| self.cluster_special_request(tail))
    }

    pub(crate) fn advance_request<'text>(
        &self,
        text: &'text [u8],
        byte_idx: usize,
        tail: Option<(char, bool)>,
    ) -> DisplaySourceRenderPlanRequest<'text> {
        DisplaySourceRenderPlanRequest::new(text, byte_idx, self.range(), self.cluster_state(tail))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySpecialSourceCharRequest {
    range: DisplaySourceTextRange,
    special_display: DisplaySourceSpecialDisplay,
}

impl DisplaySpecialSourceCharRequest {
    pub(crate) fn new(
        source_char: &DisplaySourceTextChar,
        special_display: DisplaySourceSpecialDisplay,
    ) -> Self {
        Self {
            range: source_char.range(),
            special_display,
        }
    }

    pub(crate) fn kind(&self) -> DisplaySourceSpecialDisplayKind {
        self.special_display.kind()
    }

    pub(crate) fn requires_overflow_measurement(&self) -> bool {
        self.special_display.is_control()
    }

    pub(crate) fn source_item_request(&self) -> DisplaySourceItemRequest {
        DisplaySourceItemRequest::new(self.range, self.special_display.clone().into_append_item())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySourceTextItemRequest {
    range: DisplaySourceTextRange,
    ch: char,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySourceTextRequest {
    source_item: DisplaySourceTextItemRequest,
    render_plan: DisplaySourceAppendRenderPlan,
}

impl DisplaySourceTextRequest {
    #[cfg(test)]
    pub(crate) fn new(
        range: DisplaySourceTextRange,
        source_char: char,
        render_plan: DisplaySourceAppendRenderPlan,
    ) -> Self {
        Self {
            source_item: DisplaySourceTextItemRequest::new(range, source_char),
            render_plan,
        }
    }

    pub(crate) fn from_source_item(
        source_item: DisplaySourceTextItemRequest,
        render_plan: DisplaySourceAppendRenderPlan,
    ) -> Self {
        Self {
            source_item,
            render_plan,
        }
    }

    pub(crate) fn source_item(self) -> DisplaySourceTextItemRequest {
        self.source_item
    }

    pub(crate) fn render_plan(self) -> DisplaySourceAppendRenderPlan {
        self.render_plan
    }

    pub(crate) fn advance_px(self) -> f32 {
        self.render_plan.advance_px()
    }
}

impl DisplaySourceTextItemRequest {
    pub(crate) fn new(range: DisplaySourceTextRange, ch: char) -> Self {
        Self { range, ch }
    }

    pub(crate) fn for_range_and_cluster(
        range: DisplaySourceTextRange,
        cluster: DisplaySourceClusterState,
    ) -> Self {
        Self::new(range, cluster.ch())
    }

    #[cfg(test)]
    pub(crate) fn range(self) -> DisplaySourceTextRange {
        self.range
    }

    pub(crate) fn source_char(self) -> char {
        self.ch
    }

    #[cfg(test)]
    pub(crate) fn into_display_item_kind(self) -> DisplayItemKind {
        DisplayItemKind::TextRun(DisplayTextRun::new(self.ch.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn into_display_item<B: LayoutBufferView + ?Sized>(
        self,
        buffer_id: BufferId,
        buffer: &B,
        face: RenderFaceRef,
    ) -> Option<DisplayItem> {
        let range = self.range();
        if !range.is_single_char() {
            return None;
        }

        let start = range.start();
        let end = range.end();
        Some(
            BufferTextItemSource::single_char(
                buffer_id,
                start,
                buffer.layout_char_pos_to_emacs_byte_pos(start),
                buffer.layout_char_pos_to_emacs_byte_pos(end),
            )
            .item(face, self.into_display_item_kind()),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySourceItemRequest {
    range: DisplaySourceTextRange,
    item: DisplaySourceAppendItem,
}

impl DisplaySourceItemRequest {
    pub(crate) fn new(range: DisplaySourceTextRange, item: DisplaySourceAppendItem) -> Self {
        Self { range, item }
    }

    pub(crate) fn range(&self) -> DisplaySourceTextRange {
        self.range
    }

    pub(crate) fn item(&self) -> &DisplaySourceAppendItem {
        &self.item
    }

    pub(crate) fn fallback_width(&self) -> DisplaySourceFallbackWidth {
        self.item.fallback_width()
    }

    pub(crate) fn into_display_item_kind(self) -> DisplayItemKind {
        self.item.into_display_item_kind()
    }

    pub(crate) fn into_display_item<B: LayoutBufferView + ?Sized>(
        self,
        buffer_id: BufferId,
        buffer: &B,
        face: RenderFaceRef,
    ) -> Option<DisplayItem> {
        let range = self.range();
        if range.is_empty_or_reversed() {
            return None;
        }

        let start = range.start();
        let end = range.end();
        Some(
            BufferTextItemSource::new(
                buffer_id,
                start,
                buffer.layout_char_pos_to_emacs_byte_pos(start),
                end,
                buffer.layout_char_pos_to_emacs_byte_pos(end),
            )
            .item(face, self.into_display_item_kind()),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplaySourceClusterState {
    ch: char,
    tail: Option<(char, bool)>,
    is_cluster_continuation: bool,
}

impl DisplaySourceClusterState {
    pub(crate) fn for_char(ch: char, tail: Option<(char, bool)>) -> Self {
        Self {
            ch,
            tail,
            is_cluster_continuation: crate::composition::continues_cluster(ch, tail),
        }
    }

    pub(crate) fn is_cluster_continuation(self) -> bool {
        self.is_cluster_continuation
    }

    pub(crate) fn ch(self) -> char {
        self.ch
    }

    pub(crate) fn has_tail(self) -> bool {
        self.tail.is_some()
    }

    pub(crate) fn append_measurement_kind(self) -> DisplaySourceAppendMeasurementKind {
        DisplaySourceAppendMeasurementKind::for_char(self.ch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySourceRenderPlanRequest<'text> {
    text: &'text [u8],
    byte_idx: usize,
    range: DisplaySourceTextRange,
    cluster: DisplaySourceClusterState,
}

impl<'text> DisplaySourceRenderPlanRequest<'text> {
    pub(crate) fn new(
        text: &'text [u8],
        byte_idx: usize,
        range: DisplaySourceTextRange,
        cluster: DisplaySourceClusterState,
    ) -> Self {
        Self {
            text,
            byte_idx,
            range,
            cluster,
        }
    }

    pub(crate) fn text(self) -> &'text [u8] {
        self.text
    }

    pub(crate) fn byte_idx(self) -> usize {
        self.byte_idx
    }

    pub(crate) fn range(self) -> DisplaySourceTextRange {
        self.range
    }

    pub(crate) fn cluster(self) -> DisplaySourceClusterState {
        self.cluster
    }

    pub(crate) fn measurement_kind(self) -> DisplaySourceAppendMeasurementKind {
        self.cluster.append_measurement_kind()
    }

    pub(crate) fn into_text_request(
        self,
        render_plan: DisplaySourceAppendRenderPlan,
    ) -> DisplaySourceTextRequest {
        DisplaySourceTextRequest::from_source_item(
            DisplaySourceTextItemRequest::for_range_and_cluster(self.range, self.cluster),
            render_plan,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySourceNaturalMeasurementRequest {
    source_item: DisplaySourceTextItemRequest,
    fallback: DisplayRowTextNaturalAdvanceKind,
}

impl DisplayRowTextNaturalAdvanceKind {
    pub(crate) fn for_cluster_state(cluster: DisplaySourceClusterState) -> Self {
        Self::for_source_char(cluster.ch(), cluster.is_cluster_continuation())
    }
}

impl DisplaySourceNaturalMeasurementRequest {
    pub(crate) fn for_range_and_cluster(
        range: DisplaySourceTextRange,
        cluster: DisplaySourceClusterState,
    ) -> Self {
        Self {
            source_item: DisplaySourceTextItemRequest::for_range_and_cluster(range, cluster),
            fallback: DisplayRowTextNaturalAdvanceKind::for_cluster_state(cluster),
        }
    }

    pub(crate) fn source_item(self) -> DisplaySourceTextItemRequest {
        self.source_item
    }

    pub(crate) fn fallback(self) -> DisplayRowTextNaturalAdvanceKind {
        self.fallback
    }
}

/// GNU `nonascii_space_p` (xdisp.c:8526, `blankp (c)`): a non-ASCII char in the
/// Unicode Zs (separator, space) category. nbsp U+00A0 is the common case; we
/// also accept the other fixed-width Zs spaces GNU highlights.
pub(crate) fn nonascii_space_p(ch: char) -> bool {
    matches!(
        ch,
        '\u{00A0}' // NO-BREAK SPACE
            | '\u{1680}' // OGHAM SPACE MARK
            | '\u{2000}'
            ..='\u{200A}' // EN QUAD .. HAIR SPACE
            | '\u{202F}' // NARROW NO-BREAK SPACE
            | '\u{205F}' // MEDIUM MATHEMATICAL SPACE
            | '\u{3000}' // IDEOGRAPHIC SPACE
    )
}

/// GNU `nonascii_hyphen_p` (xdisp.c:8527): SOFT_HYPHEN, HYPHEN or
/// NON_BREAKING_HYPHEN (character.h:71-75).
pub(crate) fn nonascii_hyphen_p(ch: char) -> bool {
    matches!(ch, '\u{00AD}' | '\u{2010}' | '\u{2011}')
}

/// GNU displays a NON-PRINTABLE character -- one whose general category is
/// `Cc`/`Cs`/`Cn` (`!CHAR_PRINTABLE_P`, xdisp.c:8552) -- as `\` followed by its
/// codepoint in octal, painted in the `escape-glyph` face (xdisp.c:8645-8661,
/// `\%03o`). This is the escape path that also renders raw bytes; it covers
/// noncharacters (U+FFFE/U+FFFF, U+FDD0..U+FDEF), unassigned codepoints, and the
/// C1 controls U+0080..U+009F -- everything a font would otherwise draw as
/// `.notdef`. ASCII control chars (< 0x20, 0x7F) take the caret/`^X` path and
/// nobreak space/hyphen their own; both are classified earlier, so this only
/// needs the printable test. ASCII (< 0x80) is fast-pathed as always printable.
pub(crate) fn is_escape_glyph_octal(ch: char) -> bool {
    let cp = ch as u32;
    if cp < 0x80 {
        return false;
    }
    use neovm_core::emacs_core::emacs_char::{char_general_category, printablep};
    char_general_category(cp).is_some_and(|cat| !printablep(cat))
}

/// The `\`+octal escape substitute string for a non-printable char (see
/// [`is_escape_glyph_octal`]): GNU `sprintf(str, "%03o", c)` prefixed with the
/// escape glyph (xdisp.c:8654). U+FFFF -> `\177777`, U+0080 -> `\200`.
pub(crate) fn escape_glyph_octal_text(code: u32) -> String {
    format!("\\{code:03o}")
}

impl DisplaySourceAppendItem {
    pub(crate) fn nobreak_display(ch: char, display_policy: NobreakDisplayMode) -> Option<Self> {
        // GNU `get_next_display_element` (xdisp.c:8594-8643): in highlight mode
        // GNU preserves the original character by default.  Only the separate
        // `nobreak-char-ascii-display` boolean selects an ASCII lookalike.
        // Any non-nil, non-t `nobreak-char-display` value selects escape form.
        let text = match display_policy {
            NobreakDisplayMode::HighlightOriginal
                if nonascii_space_p(ch) || nonascii_hyphen_p(ch) =>
            {
                return Some(Self::SourceMappedText {
                    text: ch.to_string().into(),
                });
            }
            NobreakDisplayMode::HighlightAscii if nonascii_space_p(ch) => " ",
            NobreakDisplayMode::HighlightAscii if nonascii_hyphen_p(ch) => "-",
            NobreakDisplayMode::Escape if nonascii_space_p(ch) => "\\ ",
            NobreakDisplayMode::Escape if nonascii_hyphen_p(ch) => "\\-",
            _ => return None,
        };
        Some(Self::SourceMappedText { text: text.into() })
    }

    pub(crate) fn glyphless_display(cluster: DisplaySourceClusterState) -> Option<Self> {
        let ch = cluster.ch();
        if cluster.has_tail() && crate::composition::is_composition_joiner(ch) {
            return None;
        }
        let method = glyphless_method_for_char(ch, GlyphlessJoinerPolicy::ClassifyAsGlyphless)?;
        Some(Self::Glyphless { ch, method })
    }

    pub(crate) fn fallback_width(&self) -> DisplaySourceFallbackWidth {
        let columns = match self {
            Self::ControlChar { .. } => 2,
            Self::SourceMappedText { text } => text.chars().count().max(1),
            Self::Glyphless { .. } => 1,
        };
        DisplaySourceFallbackWidth::columns(columns)
    }

    pub(crate) fn into_display_item_kind(self) -> DisplayItemKind {
        match self {
            Self::ControlChar { ch } => DisplayItemKind::ControlChar { ch },
            Self::SourceMappedText { text } => {
                DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new(text))
            }
            Self::Glyphless { ch, method } => {
                DisplayItemKind::Glyphless(DisplayGlyphless { ch, method })
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DisplayReplacementBox {
    width_px: f32,
    height_px: f32,
    ascent_px: f32,
}

impl DisplayReplacementBox {
    pub(crate) fn new(width_px: f32, height_px: f32, ascent_px: f32) -> Self {
        Self {
            width_px: width_px.max(0.0),
            height_px: height_px.max(0.0),
            ascent_px: ascent_px.max(0.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayReplacementSourceMappedTextItem {
    text: Box<str>,
}

impl DisplayReplacementSourceMappedTextItem {
    pub(crate) fn new(text: impl Into<Box<str>>) -> Self {
        Self { text: text.into() }
    }

    pub(crate) fn into_text(self) -> Box<str> {
        self.text
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DisplayReplacementStretchSourceItem {
    geometry: DisplayReplacementBox,
    width_px: f32,
    height_px: f32,
    ascent_px: f32,
    cursor_slot_width_px: f32,
}

/// Geometry resolved from GNU's `(space ...)` display spec.  This is shared
/// by buffer display replacements and line/wrap prefixes; cursor policy stays
/// in `DisplayReplacementStretchSourceItem`, where it belongs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySpaceGeometry {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) ascent: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DisplaySpaceWidthPolicy {
    Explicit(Value),
    Relative { factor: f32 },
    AlignTo(Value),
    Default,
}

impl DisplaySpaceWidthPolicy {
    pub(crate) fn from_items(items: &[Value]) -> Self {
        if let Some(prop) = display_space_plist_value(items, DisplaySpaceKey::Width)
            && !prop.is_nil()
        {
            Self::Explicit(prop)
        } else if let Some(prop) = display_space_plist_value(items, DisplaySpaceKey::RelativeWidth)
            && let Some(factor) = display_space_positive_number(prop)
        {
            Self::Relative { factor }
        } else if let Some(prop) = display_space_plist_value(items, DisplaySpaceKey::AlignTo)
            && !prop.is_nil()
        {
            Self::AlignTo(prop)
        } else {
            Self::Default
        }
    }

    fn zero_width_allowed(self) -> bool {
        matches!(self, Self::AlignTo(_))
    }

    fn resolve(
        self,
        pctx: &crate::display_pixel_calc::PixelCalcContext,
        current_x: f32,
        content_x: f32,
        display_char_width: f32,
        default_width: f32,
    ) -> f32 {
        use crate::display_pixel_calc::calc_pixel_width_or_height;

        match self {
            Self::Explicit(prop) => calc_pixel_width_or_height(pctx, &prop, true, None)
                .map(|pixels| pixels as f32)
                .unwrap_or(default_width),
            // GNU `produce_stretch_glyph` assigns the product to its integer
            // pixel-width field (`xdisp.c`), truncating before row fitting.
            // Keep that conversion inside the typed Relative branch: callers
            // must not choose their own rounding policy for this width kind.
            Self::Relative { factor } => (factor * display_char_width.max(0.0)).trunc(),
            Self::AlignTo(prop) => {
                let mut align_to: i32 = -1;
                if let Some(pixels) =
                    calc_pixel_width_or_height(pctx, &prop, true, Some(&mut align_to))
                {
                    let target_x = if align_to >= 0 {
                        align_to as f32 + pixels as f32
                    } else {
                        content_x + pixels as f32
                    };
                    (target_x - current_x).max(0.0)
                } else {
                    default_width
                }
            }
            Self::Default => default_width,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DisplaySpaceHeightPolicy {
    Explicit(Value),
    Relative { factor: f32 },
    Default,
}

impl DisplaySpaceHeightPolicy {
    pub(crate) fn from_items(items: &[Value]) -> Self {
        if let Some(prop) = display_space_plist_value(items, DisplaySpaceKey::Height)
            && !prop.is_nil()
        {
            Self::Explicit(prop)
        } else if let Some(prop) = display_space_plist_value(items, DisplaySpaceKey::RelativeHeight)
            && let Some(factor) = display_space_positive_number(prop)
        {
            Self::Relative { factor }
        } else {
            Self::Default
        }
    }

    fn zero_height_allowed(self) -> bool {
        matches!(self, Self::Explicit(_))
    }

    fn resolve(
        self,
        pctx: &crate::display_pixel_calc::PixelCalcContext,
        default_height: f32,
    ) -> f32 {
        use crate::display_pixel_calc::calc_pixel_width_or_height;

        match self {
            Self::Explicit(prop) => calc_pixel_width_or_height(pctx, &prop, false, None)
                .map(|pixels| pixels as f32)
                .unwrap_or(default_height),
            Self::Relative { factor } => default_height * factor,
            Self::Default => default_height,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DisplaySpaceAscentPolicy {
    Percent { percent: f32 },
    Pixel(Value),
    Default,
}

impl DisplaySpaceAscentPolicy {
    pub(crate) fn from_items(items: &[Value]) -> Self {
        let Some(prop) = display_space_plist_value(items, DisplaySpaceKey::Ascent) else {
            return Self::Default;
        };
        if let Some(percent) = display_space_positive_number(prop)
            && percent <= 100.0
        {
            Self::Percent { percent }
        } else if !prop.is_nil() {
            Self::Pixel(prop)
        } else {
            Self::Default
        }
    }

    fn resolve(
        self,
        pctx: &crate::display_pixel_calc::PixelCalcContext,
        height: f32,
        default_ascent: f32,
        default_height: f32,
    ) -> f32 {
        use crate::display_pixel_calc::calc_pixel_width_or_height;

        match self {
            Self::Percent { percent } => height * percent / 100.0,
            Self::Pixel(prop) => calc_pixel_width_or_height(pctx, &prop, false, None)
                .map(|pixels| (pixels as f32).max(0.0).min(height))
                .unwrap_or_else(|| Self::default_ascent(height, default_ascent, default_height)),
            Self::Default => Self::default_ascent(height, default_ascent, default_height),
        }
    }

    fn default_ascent(height: f32, default_ascent: f32, default_height: f32) -> f32 {
        height * default_ascent / default_height
    }
}

fn display_space_plist_value(items: &[Value], wanted: DisplaySpaceKey) -> Option<Value> {
    let mut i = 1;
    while i + 1 < items.len() {
        if DisplaySpaceKey::from_lisp_value(items[i]) == Some(wanted) {
            return Some(items[i + 1]);
        }
        i += 2;
    }
    None
}

impl DisplayReplacementStretchSourceItem {
    pub(crate) fn from_extents(width_px: f32, height_px: f32, ascent_px: f32) -> Self {
        let width_px = width_px.max(0.0);
        let height_px = height_px.max(0.0);
        let ascent_px = ascent_px.max(0.0);
        Self {
            geometry: DisplayReplacementBox::new(width_px, height_px, ascent_px),
            width_px,
            height_px,
            ascent_px,
            cursor_slot_width_px: width_px,
        }
    }

    pub(crate) fn from_space_extents(
        width_px: f32,
        height_px: f32,
        ascent_px: f32,
        fallback_cursor_width_px: f32,
    ) -> Self {
        let mut item = Self::from_extents(width_px, height_px, ascent_px);
        item.cursor_slot_width_px = item.width_px.max(fallback_cursor_width_px);
        item
    }
}

impl DisplaySpaceGeometry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_display_space_spec(
        spec: &Value,
        current_x: f32,
        content_x: f32,
        face_char_w: f32,
        display_char_width: f32,
        default_height: f32,
        default_ascent: f32,
        params: &WindowParams,
    ) -> Self {
        use crate::display_pixel_calc::PixelCalcContext;

        let default_width = params.char_width.max(1.0);
        let default_height = if params.window_system {
            default_height.max(1.0)
        } else {
            params.char_height.max(1.0)
        };
        let default_ascent = if params.window_system {
            default_ascent.max(0.0).min(default_height)
        } else {
            default_height
        };
        let Some(items) = list_to_vec(spec) else {
            return Self {
                width: default_width,
                height: default_height,
                ascent: default_ascent,
            };
        };

        let pctx = PixelCalcContext {
            frame_column_width: params.char_width.max(1.0) as f64,
            frame_line_height: params.char_height.max(1.0) as f64,
            frame_res_x: 96.0,
            frame_res_y: 96.0,
            face_font_height: default_height as f64,
            face_font_width: face_char_w.round().max(1.0) as f64,
            text_area_left: params.text_bounds.x as f64,
            text_area_right: (params.text_bounds.x + params.text_bounds.width) as f64,
            text_area_width: params.text_bounds.width as f64,
            left_margin_left: (params.text_bounds.x
                - params.left_fringe_width
                - params.left_margin_width) as f64,
            left_margin_width: params.left_margin_width as f64,
            right_margin_left: (params.text_bounds.x
                + params.text_bounds.width
                + params.right_fringe_width) as f64,
            right_margin_width: params.right_margin_width as f64,
            left_fringe_width: params.left_fringe_width as f64,
            right_fringe_width: params.right_fringe_width as f64,
            fringes_outside_margins: false,
            scroll_bar_width: 0.0,
            scroll_bar_on_left: false,
            line_number_pixel_width: 0.0,
            symbol_values: std::collections::HashMap::new(),
            image_sizes: crate::display_pixel_calc::PixelCalcImageSizes::resolve_for_space_spec(
                spec,
                &params.space_image_inputs(),
            ),
        };

        let width_policy = DisplaySpaceWidthPolicy::from_items(&items);
        let mut width = width_policy.resolve(
            &pctx,
            current_x,
            content_x,
            display_char_width,
            default_width,
        );
        if width <= 0.0 && (width < 0.0 || !width_policy.zero_width_allowed()) {
            width = 1.0;
        }

        let (height, ascent) = if params.window_system {
            let height_policy = DisplaySpaceHeightPolicy::from_items(&items);
            let mut height = height_policy.resolve(&pctx, default_height);
            if height <= 0.0 && (height < 0.0 || !height_policy.zero_height_allowed()) {
                height = 1.0;
            }

            let ascent = DisplaySpaceAscentPolicy::from_items(&items).resolve(
                &pctx,
                height,
                default_ascent,
                default_height,
            );
            (height, ascent)
        } else {
            (1.0, 1.0)
        };

        Self {
            width,
            height,
            ascent: ascent.max(0.0).min(height),
        }
    }

    pub(crate) fn width_px(self) -> f32 {
        self.width
    }

    pub(crate) fn display_item_kind(self) -> DisplayItemKind {
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(self.width)),
            height: Some(DisplayLength::Pixels(self.height)),
            ascent: Some(DisplayLength::Pixels(self.ascent)),
        })
    }
}

impl DisplayReplacementStretchSourceItem {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_display_space_spec(
        spec: &Value,
        current_x: f32,
        content_x: f32,
        face_char_w: f32,
        display_char_width: f32,
        default_height: f32,
        default_ascent: f32,
        fallback_cursor_width_px: f32,
        params: &WindowParams,
    ) -> Self {
        let geometry = DisplaySpaceGeometry::from_display_space_spec(
            spec,
            current_x,
            content_x,
            face_char_w,
            display_char_width,
            default_height,
            default_ascent,
            params,
        );
        Self::from_space_extents(
            geometry.width,
            geometry.height,
            geometry.ascent,
            fallback_cursor_width_px,
        )
    }

    pub(crate) fn width_px(self) -> f32 {
        self.width_px
    }

    pub(crate) fn height_px(self) -> f32 {
        self.height_px
    }

    pub(crate) fn ascent_px(self) -> f32 {
        self.ascent_px
    }

    pub(crate) fn cursor_slot_width_px(self) -> f32 {
        self.cursor_slot_width_px
    }

    pub(crate) fn geometry(self) -> DisplayReplacementBox {
        self.geometry
    }

    pub(crate) fn display_item_kind(self) -> DisplayItemKind {
        let geometry = self.geometry();
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(geometry.width_px)),
            height: Some(DisplayLength::Pixels(geometry.height_px)),
            ascent: Some(DisplayLength::Pixels(geometry.ascent_px)),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayReplacementMediaSourceItem {
    media: DisplayMediaReplacement,
    cursor_face_height: f32,
    cursor_face_ascent: f32,
}

impl DisplayReplacementMediaSourceItem {
    pub(crate) fn new(
        media: DisplayMediaReplacement,
        face_height: f32,
        face_ascent: f32,
        uses_xwidget_cursor_extents: bool,
    ) -> Self {
        let (cursor_face_height, cursor_face_ascent) = if uses_xwidget_cursor_extents {
            (media.height.max(face_height), media.height.max(face_ascent))
        } else {
            (media.height, media.ascent)
        };
        Self {
            media,
            cursor_face_height,
            cursor_face_ascent,
        }
    }

    pub(crate) fn media(self) -> DisplayMediaReplacement {
        self.media
    }

    pub(crate) fn width_px(self) -> f32 {
        self.media.width
    }

    pub(crate) fn display_height_px(self) -> f32 {
        self.media.height
    }

    pub(crate) fn display_ascent_px(self) -> f32 {
        self.media.ascent
    }

    pub(crate) fn cursor_face_height_px(self) -> f32 {
        self.cursor_face_height
    }

    pub(crate) fn cursor_face_ascent_px(self) -> f32 {
        self.cursor_face_ascent
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayReplacementMediaSourceResolution {
    Media(DisplayReplacementMediaSourceItem),
    Placeholder(DisplayReplacementSourceMappedTextItem),
}

#[derive(Clone)]
pub(crate) struct DisplayReplacementStringSourceItem {
    value: Value,
    origin: DisplayOrigin,
    source_id: u64,
    cursor_slot_width_px: f32,
    is_empty: bool,
}

impl DisplayReplacementStringSourceItem {
    pub(crate) fn display_property_string(
        value: Value,
        anchor_charpos: CharPos0,
        source: DisplayPropertySource,
        source_id: u64,
        cursor_slot_width_px: f32,
    ) -> Option<Self> {
        let replacement = value.as_utf8_str()?;
        Some(Self {
            value,
            origin: DisplayOrigin::DisplayPropertyString {
                anchor_charpos,
                source,
            },
            source_id,
            cursor_slot_width_px,
            is_empty: replacement.is_empty(),
        })
    }

    pub(crate) fn value(&self) -> Value {
        self.value
    }

    pub(crate) fn source_id(&self) -> u64 {
        self.source_id
    }

    pub(crate) fn cursor_slot_width_px(&self) -> f32 {
        self.cursor_slot_width_px
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.is_empty
    }

    pub(crate) fn origin(&self) -> DisplayOrigin {
        self.origin
    }

    #[cfg(test)]
    pub(crate) fn base_face_policy(&self) -> crate::display_face_policy::BaseFacePolicy {
        self.origin.default_base_face_policy()
    }
}

#[derive(Clone)]
pub(crate) enum DisplayPropertyReplacementSourceItem {
    /// A replacement that produces no inline glyph and consumes the covered
    /// text with zero inline width, e.g. a `(left-fringe …)` display spec. GNU
    /// draws the bitmap in the fringe; the text area shows nothing.
    Empty,
    /// A replacement routed to a structural margin area. It consumes the
    /// covered source with zero inline width while retaining typed output.
    Margin(DisplayMarginEmission),
    String(DisplayReplacementStringSourceItem),
    Stretch(DisplayReplacementStretchSourceItem),
    Media(DisplayReplacementMediaSourceResolution),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayPropertyReplacementCursorPolicy {
    TextSlot {
        width_px: f32,
        stretch_like: bool,
    },
    DisplayBox {
        width_px: f32,
        cursor_face_height_px: f32,
        cursor_face_ascent_px: f32,
    },
    FaceChar,
}

impl DisplayPropertyReplacementSourceItem {
    pub(crate) fn cursor_policy(&self) -> DisplayPropertyReplacementCursorPolicy {
        match self {
            Self::Empty => DisplayPropertyReplacementCursorPolicy::TextSlot {
                width_px: 0.0,
                stretch_like: false,
            },
            Self::Margin(_) => DisplayPropertyReplacementCursorPolicy::TextSlot {
                width_px: 0.0,
                stretch_like: false,
            },
            Self::String(item) => DisplayPropertyReplacementCursorPolicy::TextSlot {
                width_px: item.cursor_slot_width_px(),
                stretch_like: false,
            },
            Self::Stretch(item) => DisplayPropertyReplacementCursorPolicy::TextSlot {
                width_px: item.cursor_slot_width_px(),
                stretch_like: true,
            },
            Self::Media(DisplayReplacementMediaSourceResolution::Media(item)) => {
                DisplayPropertyReplacementCursorPolicy::DisplayBox {
                    width_px: item.width_px(),
                    cursor_face_height_px: item.cursor_face_height_px(),
                    cursor_face_ascent_px: item.cursor_face_ascent_px(),
                }
            }
            Self::Media(DisplayReplacementMediaSourceResolution::Placeholder(_)) => {
                DisplayPropertyReplacementCursorPolicy::FaceChar
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayPropertyReplacementSourceInputs {
    string_cursor_slot_width_px: Option<f32>,
    stretch_display_char_width_px: Option<f32>,
    media: Option<DisplayReplacementMediaSourceResolution>,
}

impl DisplayPropertyReplacementSourceInputs {
    pub(crate) const fn empty() -> Self {
        Self {
            string_cursor_slot_width_px: None,
            stretch_display_char_width_px: None,
            media: None,
        }
    }

    pub(crate) fn with_string_cursor_slot_width_px(mut self, width_px: f32) -> Self {
        self.string_cursor_slot_width_px = Some(width_px);
        self
    }

    pub(crate) fn with_stretch_display_char_width_px(mut self, width_px: f32) -> Self {
        self.stretch_display_char_width_px = Some(width_px);
        self
    }

    pub(crate) fn with_media(mut self, media: DisplayReplacementMediaSourceResolution) -> Self {
        self.media = Some(media);
        self
    }
}

impl DisplayPropertyReplacementSourceItem {
    pub(crate) fn from_display_property_parts(
        display_property: &DisplayPropertyClassification,
        anchor_charpos: CharPos0,
        current_x: f32,
        content_x: f32,
        params: &WindowParams,
        metrics: DisplayRowFallbackMetrics,
        inputs: DisplayPropertyReplacementSourceInputs,
    ) -> Option<Self> {
        // The Lisp payload comes from the SPEC that produced the replacement, not
        // from the whole `display` value: for `["X"]`, `("X")` or
        // `(when t . "X")` those are different objects.
        let value = display_property.replacement_spec();
        match display_property.replacement()? {
            DisplayReplacementProperty::String => {
                DisplayReplacementStringSourceItem::display_property_string(
                    value,
                    anchor_charpos,
                    DisplayPropertySource::TextProperty,
                    1,
                    inputs.string_cursor_slot_width_px?,
                )
                .map(Self::String)
            }
            DisplayReplacementProperty::Stretch(_) => Some(Self::Stretch(
                DisplayReplacementStretchSourceItem::from_display_space_spec(
                    &value,
                    current_x,
                    content_x,
                    metrics.char_width(),
                    inputs.stretch_display_char_width_px?,
                    metrics.row_height(),
                    metrics.ascent(),
                    metrics.char_width(),
                    params,
                ),
            )),
            DisplayReplacementProperty::Media(_) => inputs.media.map(Self::Media),
            DisplayReplacementProperty::Fringe(_) => Some(Self::Empty),
            // Margin output discovered in the descriptor path is rendered by
            // the row-area append plan, never as inline text.
            // Margin descriptors are resolved earlier, where media and source
            // face services are available.
            DisplayReplacementProperty::Margin(_) => None,
        }
    }
}

pub(crate) struct BufferDisplayReplacementStringSource<S> {
    replacement_source: BufferDisplayReplacementSource,
    source: S,
    inherited_pointer_appearance: Option<DisplayPointerAppearance>,
}

#[derive(Clone, Debug)]
pub(crate) struct BufferDisplayReplacementStringRequest {
    source_id: u64,
    value: Value,
    replacement_source: BufferDisplayReplacementSource,
    inherited_pointer_appearance: Option<DisplayPointerAppearance>,
    box_boundaries: DisplayStringBoxBoundaries,
}

impl BufferDisplayReplacementStringRequest {
    pub(crate) fn new(
        source_id: u64,
        value: Value,
        replacement_source: BufferDisplayReplacementSource,
    ) -> Self {
        Self {
            source_id,
            value,
            replacement_source,
            inherited_pointer_appearance: None,
            box_boundaries: DisplayStringBoxBoundaries::default(),
        }
    }

    pub(crate) fn with_pointer_appearance(
        mut self,
        appearance: Option<DisplayPointerAppearance>,
    ) -> Self {
        self.inherited_pointer_appearance = appearance;
        self
    }

    pub(crate) fn with_box_boundaries(mut self, boundaries: DisplayStringBoxBoundaries) -> Self {
        self.box_boundaries = boundaries;
        self
    }

    pub(crate) fn into_source(
        self,
        fallback_face_id: FaceId,
    ) -> Option<BufferDisplayReplacementStringSource<LispStringSourceCursor>> {
        let string_source = LispStringSourceCursor::new_with_box_boundaries(
            self.source_id,
            self.value,
            RenderFaceRef::FaceId(fallback_face_id),
            LispStringSourceOrigin::BufferDisplayReplacement(self.replacement_source),
            self.box_boundaries,
        )?;
        Some(BufferDisplayReplacementStringSource::new(
            self.replacement_source,
            string_source,
            self.inherited_pointer_appearance,
        ))
    }
}

impl<S> BufferDisplayReplacementStringSource<S> {
    pub(crate) const fn new(
        replacement_source: BufferDisplayReplacementSource,
        source: S,
        inherited_pointer_appearance: Option<DisplayPointerAppearance>,
    ) -> Self {
        Self {
            replacement_source,
            source,
            inherited_pointer_appearance,
        }
    }
}

impl<S: DisplayItemSource> DisplayItemSource for BufferDisplayReplacementStringSource<S> {
    fn next_item(&mut self, context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        let mut item = self.source.next_item(context)?;
        if item.pointer_appearance.is_none() {
            item.pointer_appearance = self.inherited_pointer_appearance.clone();
        }
        Some(
            self.replacement_source
                .item_from_replacement_string_item(item),
        )
    }
}

pub(crate) struct LispStringSourceCursor {
    stack: LispStringSourceStack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LispStringSourceOrigin {
    Normal,
    /// Content reached through `((margin SIDE) STRING)`.  Like every display
    /// replacement string in GNU, its own replacing `display` properties are
    /// inert while ordinary face/modifier properties still apply.
    MarginDisplayReplacement,
    BufferDisplayReplacement(BufferDisplayReplacementSource),
    OverlayString {
        overlay_id: Value,
        kind: OverlayStringKind,
    },
}

/// Whether a string frame's OWN `display` properties are handled.
///
/// GNU draws this line at `it->string_from_display_prop_p`, not at "is this a
/// string": `handle_display_prop` declines to recurse only when the string it
/// is walking came from a `display` property (xdisp.c:5934-5942, 6334-6335).
/// So an overlay string is ordinary displayable text whose own `display`
/// properties apply, while a display string's are inert.
///
/// Pinned by
/// `a_replacing_display_spec_is_honored_in_an_overlay_string_but_not_below_it`
/// (engine_test.rs), including the transitive half — see
/// [`LispStringSourceStack::push_with_replacement_source`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NestedDisplayPolicy {
    /// Ordinary displayable text: handle a `display` property here, including a
    /// replacing one.
    Handle,
    /// Reached FROM a `display` property: this string's own `display`
    /// properties contribute modifiers only and can never replace.
    ModifiersOnly,
}

impl LispStringSourceOrigin {
    const fn nested_display_policy(self) -> NestedDisplayPolicy {
        match self {
            Self::MarginDisplayReplacement | Self::BufferDisplayReplacement(_) => {
                NestedDisplayPolicy::ModifiersOnly
            }
            Self::Normal | Self::OverlayString { .. } => NestedDisplayPolicy::Handle,
        }
    }

    const fn pointer_occurrence(self) -> DisplayPointerOccurrence {
        match self {
            Self::OverlayString { overlay_id, kind } => {
                DisplayPointerOccurrence::OverlayString { overlay_id, kind }
            }
            Self::BufferDisplayReplacement(source) => source.pointer_occurrence(),
            Self::Normal | Self::MarginDisplayReplacement => DisplayPointerOccurrence::Source,
        }
    }
}

impl LispStringSourceCursor {
    pub(crate) fn new(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        origin: LispStringSourceOrigin,
    ) -> Option<Self> {
        Some(Self {
            stack: LispStringSourceStack::with_root(source_id, value, base_face, origin)?,
        })
    }

    pub(crate) fn new_with_box_boundaries(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        origin: LispStringSourceOrigin,
        box_boundaries: DisplayStringBoxBoundaries,
    ) -> Option<Self> {
        Some(Self {
            stack: LispStringSourceStack::with_root_box_boundaries(
                source_id,
                value,
                base_face,
                origin,
                box_boundaries,
            )?,
        })
    }

    pub(crate) fn with_tty_glyphless_char_display(
        mut self,
        display: TtyGlyphlessCharDisplay,
    ) -> Self {
        self.stack = self.stack.with_tty_glyphless_char_display(display);
        self
    }

    pub(crate) fn discard_until_row_break(&mut self) -> bool {
        let mut context = DisplaySourceContext::empty();
        while let Some(item) = self.next_item(&mut context) {
            if matches!(item.kind, DisplayItemKind::RowBreak(_)) {
                return true;
            }
        }
        false
    }
}

impl DisplayItemSource for LispStringSourceCursor {
    fn next_item(&mut self, context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.stack.next_item(context)
    }
}

// The action transports complete display items without allocating in the
// string-source walk.
#[allow(clippy::large_enum_variant)]
enum LispStringAction {
    PopFrame,
    PushReplacement {
        value: Value,
        base_face: RenderFaceRef,
        box_boundaries: DisplayStringBoxBoundaries,
    },
    Emit(DisplayItem),
    /// The covered chars were consumed by a no-inline-output display spec
    /// (e.g. `(left-fringe …)`); produce no glyph and continue.
    Skip,
}

/// What a frame can do once any display property at the position has already
/// been resolved: emit the position's own glyphs, or run out of string.
///
/// The point of the type is what it CANNOT say. It has no `PushReplacement`,
/// so any path that reaches an action through here is structurally incapable
/// of nesting a replacement — which is the ban GNU states as
/// `it->string_from_display_prop_p` (xdisp.c:5934-5942, 6334-6335): a display
/// string's own display properties are inert, while an overlay string's apply.
/// [`NestedDisplayPolicy::ModifiersOnly`] used to enforce that with a runtime
/// branch that simply declined to look; now the arm has no `return` of its own
/// and can only finish through [`LispStringSourceFrame::resolved_action`], so
/// the compiler enforces it and the P4.7 nested-ban pins pin behaviour the
/// type already guarantees.
enum LispStringResolvedAction {
    PopFrame,
    Emit(DisplayItem),
}

impl From<LispStringResolvedAction> for LispStringAction {
    fn from(action: LispStringResolvedAction) -> Self {
        match action {
            LispStringResolvedAction::PopFrame => Self::PopFrame,
            LispStringResolvedAction::Emit(item) => Self::Emit(item),
        }
    }
}

/// The position facts both policies compute before any display property is
/// consulted, handed to [`LispStringSourceFrame::resolved_action`] so the two
/// arms share one tail instead of two copies of it.
struct LispStringResolvedSpan {
    start: usize,
    property_end: usize,
    face: RenderFaceRef,
    pointer_appearance: Option<DisplayPointerAppearance>,
    item_layout: DisplayItemLayout,
}

pub(crate) struct LispStringSourceStack {
    frames: Vec<LispStringSourceFrame>,
    next_source_id: u64,
    tty_glyphless_char_display: TtyGlyphlessCharDisplay,
}

impl LispStringSourceStack {
    pub(crate) fn empty(next_source_id: u64) -> Self {
        Self {
            frames: Vec::new(),
            next_source_id,
            tty_glyphless_char_display: TtyGlyphlessCharDisplay::default(),
        }
    }

    fn with_root(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        origin: LispStringSourceOrigin,
    ) -> Option<Self> {
        let frame = LispStringSourceFrame::new(source_id, value, base_face, origin)?;
        Some(Self {
            frames: vec![frame],
            next_source_id: source_id.saturating_add(1),
            tty_glyphless_char_display: TtyGlyphlessCharDisplay::default(),
        })
    }

    fn with_root_box_boundaries(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        origin: LispStringSourceOrigin,
        box_boundaries: DisplayStringBoxBoundaries,
    ) -> Option<Self> {
        let frame = LispStringSourceFrame::new_with_occurrence(
            source_id,
            value,
            base_face,
            None,
            origin.nested_display_policy(),
            origin.pointer_occurrence(),
            box_boundaries,
        )?;
        Some(Self {
            frames: vec![frame],
            next_source_id: source_id.saturating_add(1),
            tty_glyphless_char_display: TtyGlyphlessCharDisplay::default(),
        })
    }

    pub(crate) fn with_tty_glyphless_char_display(
        mut self,
        display: TtyGlyphlessCharDisplay,
    ) -> Self {
        self.tty_glyphless_char_display = display;
        self
    }

    pub(crate) fn push_with_replacement_source(
        &mut self,
        value: Value,
        base_face: RenderFaceRef,
        replacement_source: Option<BufferDisplayReplacementSource>,
        box_boundaries: DisplayStringBoxBoundaries,
    ) {
        let source_id = self.allocate_source_id();
        let occurrence = replacement_source
            .map(BufferDisplayReplacementSource::pointer_occurrence)
            .or_else(|| self.frames.last().map(|frame| frame.pointer_occurrence))
            .unwrap_or_default();
        // TRANSITIVE by construction: whatever string we were walking, the one
        // we are pushing was reached THROUGH a `display` property, so its own
        // display properties are inert. This is what makes GNU's ban hold at
        // every depth rather than only one level below the buffer.
        if let Some(frame) = LispStringSourceFrame::new_with_occurrence(
            source_id,
            value,
            base_face,
            replacement_source,
            NestedDisplayPolicy::ModifiersOnly,
            occurrence,
            box_boundaries,
        ) {
            self.frames.push(frame);
        }
    }

    pub(crate) fn next_item(
        &mut self,
        context: &mut DisplaySourceContext<'_>,
    ) -> Option<DisplayItem> {
        loop {
            let (action, replacement_source) = {
                let frame = self.frames.last_mut()?;
                (
                    frame.next_action(context, self.tty_glyphless_char_display),
                    frame.replacement_source,
                )
            };

            match action {
                LispStringAction::PopFrame => {
                    self.frames.pop();
                }
                LispStringAction::PushReplacement {
                    value,
                    base_face,
                    box_boundaries,
                } => {
                    self.push_with_replacement_source(
                        value,
                        base_face,
                        replacement_source,
                        box_boundaries,
                    );
                }
                LispStringAction::Emit(item) => {
                    return Some(match replacement_source {
                        Some(source) => source.item_from_replacement_string_item(item),
                        None => item,
                    });
                }
                LispStringAction::Skip => {}
            }
        }
    }

    /// Where the innermost live replacement string is reading from. Reached
    /// through `BufferTextSourceCursor::source_position`, which is itself
    /// `#[cfg(test)]`.
    #[cfg(test)]
    pub(crate) fn source_position(&self) -> DisplaySourcePosition {
        self.frames
            .last()
            .map(LispStringSourceFrame::source_position)
            .unwrap_or_else(|| DisplaySourcePosition::synthetic(0, 0))
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    fn allocate_source_id(&mut self) -> u64 {
        let id = self.next_source_id;
        self.next_source_id = self.next_source_id.saturating_add(1);
        id
    }
}

struct LispStringSourceFrame {
    source_id: u64,
    text: Vec<u8>,
    storage: EmacsTextStorage,
    char_byte_offsets: Vec<usize>,
    props: Option<TextPropertyTable>,
    char_index: usize,
    base_face: RenderFaceRef,
    replacement_source: Option<BufferDisplayReplacementSource>,
    nested_display_policy: NestedDisplayPolicy,
    pointer_occurrence: DisplayPointerOccurrence,
    /// Face immediately outside this string occurrence.  Replacement and
    /// overlay strings can continue an underlying box run; standalone chrome
    /// strings have an object boundary and therefore use `None`.
    box_boundaries: DisplayStringBoxBoundaries,
}

impl LispStringSourceFrame {
    fn new(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        origin: LispStringSourceOrigin,
    ) -> Option<Self> {
        Self::new_with_replacement_source(source_id, value, base_face, None, origin)
    }

    fn new_with_replacement_source(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        replacement_source: Option<BufferDisplayReplacementSource>,
        origin: LispStringSourceOrigin,
    ) -> Option<Self> {
        Self::new_with_occurrence(
            source_id,
            value,
            base_face,
            replacement_source,
            origin.nested_display_policy(),
            origin.pointer_occurrence(),
            match origin {
                LispStringSourceOrigin::BufferDisplayReplacement(_)
                | LispStringSourceOrigin::OverlayString { .. } => {
                    DisplayStringBoxBoundaries::string_base()
                }
                LispStringSourceOrigin::Normal
                | LispStringSourceOrigin::MarginDisplayReplacement => {
                    DisplayStringBoxBoundaries::default()
                }
            },
        )
    }

    fn new_with_occurrence(
        source_id: u64,
        value: Value,
        base_face: RenderFaceRef,
        replacement_source: Option<BufferDisplayReplacementSource>,
        nested_display_policy: NestedDisplayPolicy,
        pointer_occurrence: DisplayPointerOccurrence,
        box_boundaries: DisplayStringBoxBoundaries,
    ) -> Option<Self> {
        let string = value.as_lisp_string()?;
        let text = string.as_bytes().to_vec();
        let storage = if string.is_multibyte() {
            EmacsTextStorage::Multibyte
        } else {
            EmacsTextStorage::Unibyte
        };
        let mut char_byte_offsets = Vec::with_capacity(string.schars().saturating_add(1));
        let mut byte_offset = 0;
        while byte_offset < text.len() {
            char_byte_offsets.push(byte_offset);
            let (_, len) = decode_emacs_char(&text[byte_offset..], storage)?;
            byte_offset = byte_offset.saturating_add(len);
        }
        char_byte_offsets.push(text.len());
        Some(Self {
            source_id,
            text,
            storage,
            char_byte_offsets,
            props: get_string_text_properties_table_for_value(value),
            char_index: 0,
            base_face,
            replacement_source,
            nested_display_policy,
            pointer_occurrence,
            box_boundaries,
        })
    }

    fn next_action(
        &mut self,
        context: &mut DisplaySourceContext<'_>,
        tty_glyphless_char_display: TtyGlyphlessCharDisplay,
    ) -> LispStringAction {
        if self.char_index >= self.char_count() {
            return LispStringAction::PopFrame;
        }

        let start = self.char_index;
        let Some(source_char) = self.char_at(start) else {
            return LispStringAction::PopFrame;
        };
        let property_end = self.next_property_change(start).max(start + 1);
        let face = self.face_at(start, context);
        let pointer_appearance = self.pointer_appearance_at(start, property_end, face, context);
        let span = self.span(start, property_end);

        let mut item_layout = DisplayItemLayout::default();
        if let Some(display_prop) = self.display_prop_at(start) {
            // The two policies differ only here, and only the `Handle` arm may
            // return: the `ModifiersOnly` arm falls through to
            // `resolved_action`, whose type has no `PushReplacement`, so the
            // nested-replacement ban is a property of the control flow rather
            // than of a check someone has to remember to keep.
            if self.nested_display_policy == NestedDisplayPolicy::ModifiersOnly {
                self.char_index = property_end;
                item_layout = classify_display_property_modifiers_only(display_prop);
            } else {
                let display_property = DisplayPropertySourcePlan::new(display_prop);
                let display_end = if display_property.replacement().is_some() {
                    self.display_value_extent(display_prop, property_end)
                } else {
                    property_end
                };
                let display_span = if display_end == property_end {
                    span
                } else {
                    self.span(start, display_end)
                };
                match display_property.cursor_action(
                    context,
                    display_span,
                    DisplayPropertySourceFaces::LispString {
                        effective: face,
                        underlying: self.base_face,
                    },
                    source_char,
                ) {
                    DisplayPropertySourceCursorAction::PushReplacement { value, base_face } => {
                        self.char_index = display_end;
                        let box_boundaries =
                            self.box_boundaries_for_range(start, display_end, context);
                        return LispStringAction::PushReplacement {
                            value,
                            base_face,
                            box_boundaries,
                        };
                    }
                    DisplayPropertySourceCursorAction::Emit(item) => {
                        self.char_index = display_end;
                        let item_face = item.face;
                        let edges = self.box_vertical_edges_for_range(
                            start,
                            display_end,
                            item_face,
                            context,
                        );
                        return LispStringAction::Emit(
                            item.with_pointer_appearance(pointer_appearance)
                                .with_box_run_topology(context.face_has_box(item_face), edges),
                        );
                    }
                    DisplayPropertySourceCursorAction::Skip => {
                        self.char_index = display_end;
                        // `(left-fringe …)`: the covered chars produce no glyph.
                        return LispStringAction::Skip;
                    }
                    DisplayPropertySourceCursorAction::FallThrough { layout } => {
                        self.char_index = property_end;
                        item_layout = layout;
                    }
                }
            }
        }

        self.resolved_action(
            LispStringResolvedSpan {
                start,
                property_end,
                face,
                pointer_appearance,
                item_layout,
            },
            context,
            tty_glyphless_char_display,
        )
        .into()
    }

    /// Emit the glyphs for a position whose display property (if any) has
    /// already been resolved. Returns [`LispStringResolvedAction`], which is
    /// the whole point: no caller of this can produce a nested replacement.
    fn resolved_action(
        &mut self,
        span: LispStringResolvedSpan,
        context: &mut DisplaySourceContext<'_>,
        tty_glyphless_char_display: TtyGlyphlessCharDisplay,
    ) -> LispStringResolvedAction {
        let LispStringResolvedSpan {
            start,
            property_end,
            face,
            pointer_appearance,
            item_layout,
        } = span;
        let Some(character) = self.char_at(start) else {
            return LispStringResolvedAction::PopFrame;
        };
        if let Some(composition) = self
            .composition_prop_at(start)
            .and_then(composition_display_text_for_property)
        {
            let end = start.saturating_add(composition.char_len());
            if end <= property_end && end <= self.char_count() {
                self.char_index = end;
                return LispStringResolvedAction::Emit(
                    DisplayItem::new(
                        self.span(start, end),
                        face,
                        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new(
                            composition.text().to_owned(),
                        )),
                    )
                    .with_layout(item_layout)
                    .with_pointer_appearance(pointer_appearance)
                    .with_box_run_topology(
                        context.face_has_box(face),
                        self.box_vertical_edges_for_range(start, end, face, context),
                    ),
                );
            }
        }
        if let Some(mut kind) = display_item_kind_for_text_source_char_with_tty_mapping(
            character,
            tty_glyphless_char_display.method_for(character),
        ) {
            if let DisplayItemKind::RowBreak(row_break) = &mut kind {
                let property = |name| {
                    self.props.as_ref().and_then(|props| {
                        props.get_property_at_char_pos(CharPos0::new(start), Value::symbol(name))
                    })
                };
                *row_break = row_break
                    .with_line_height(DisplayLineHeightPolicy::from_property(property(
                        "line-height",
                    )))
                    .with_line_spacing(DisplayLineSpacingPolicy::from_property(property(
                        "line-spacing",
                    )));
            }
            self.char_index = start + 1;
            return LispStringResolvedAction::Emit(
                DisplayItem::new(self.span(start, start + 1), face, kind)
                    .with_layout(item_layout)
                    .with_pointer_appearance(pointer_appearance)
                    .with_box_run_topology(
                        context.face_has_box(face),
                        self.box_vertical_edges_for_range(start, start + 1, face, context),
                    ),
            );
        }

        let end = self.next_text_run_end(start, property_end, tty_glyphless_char_display);
        self.char_index = end;
        LispStringResolvedAction::Emit(
            DisplayItem::new(
                self.span(start, end),
                face,
                DisplayItemKind::TextRun(DisplayTextRun::independent(self.text_slice(start, end))),
            )
            .with_layout(item_layout)
            .with_pointer_appearance(pointer_appearance)
            .with_box_run_topology(
                context.face_has_box(face),
                self.box_vertical_edges_for_range(start, end, face, context),
            ),
        )
    }

    fn char_count(&self) -> usize {
        self.char_byte_offsets.len().saturating_sub(1)
    }

    #[cfg(test)]
    fn source_position(&self) -> DisplaySourcePosition {
        DisplaySourcePosition::lisp_string(
            self.source_id,
            self.char_index,
            self.byte_offset(self.char_index),
        )
    }

    fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(self.source_id, start, self.byte_offset(start)),
            DisplaySourcePosition::lisp_string(self.source_id, end, self.byte_offset(end)),
        )
    }

    fn byte_offset(&self, char_index: usize) -> usize {
        self.char_byte_offsets
            .get(char_index.min(self.char_count()))
            .copied()
            .unwrap_or(self.text.len())
    }

    fn char_at(&self, char_index: usize) -> Option<EmacsChar> {
        let start = self.byte_offset(char_index);
        let end = self.byte_offset(char_index + 1);
        decode_emacs_char(self.text.get(start..end)?, self.storage).map(|(character, _)| character)
    }

    fn text_slice(&self, start: usize, end: usize) -> String {
        let bytes = self
            .text
            .get(self.byte_offset(start)..self.byte_offset(end))
            .unwrap_or_default();
        let mut text = String::with_capacity(bytes.len());
        let mut offset = 0;
        while offset < bytes.len() {
            let Some((character, len)) = decode_emacs_char(&bytes[offset..], self.storage) else {
                break;
            };
            let TextSourceCharClassification::Text(ch) = classify_text_source_char(character)
            else {
                debug_assert!(false, "special Emacs character entered a plain text run");
                break;
            };
            text.push(ch);
            offset = offset.saturating_add(len);
        }
        text
    }

    fn next_property_change(&self, char_index: usize) -> usize {
        self.props
            .as_ref()
            .and_then(|props| {
                props
                    .next_property_change_after_char_pos(CharPos0::new(char_index))
                    .map(CharPos0::get)
            })
            .unwrap_or_else(|| self.char_count())
            .min(self.char_count())
    }

    fn next_text_run_end(
        &self,
        start: usize,
        limit: usize,
        tty_glyphless_char_display: TtyGlyphlessCharDisplay,
    ) -> usize {
        let mut end = start;
        while end < limit {
            let Some(ch) = self.char_at(end) else {
                break;
            };
            if !matches!(
                classify_text_source_char(ch),
                TextSourceCharClassification::Text(_)
            ) || tty_glyphless_char_display.method_for(ch).is_some()
            {
                break;
            }
            end += 1;
        }
        end.max(start + 1).min(limit)
    }

    fn display_prop_at(&self, char_index: usize) -> Option<Value> {
        self.props
            .as_ref()?
            .get_property_at_char_pos(CharPos0::new(char_index), Value::symbol("display"))
    }

    fn composition_prop_at(&self, char_index: usize) -> Option<Value> {
        self.props
            .as_ref()?
            .get_property_at_char_pos(CharPos0::new(char_index), Value::symbol("composition"))
    }

    fn display_value_extent(&self, value: Value, mut extent: usize) -> usize {
        let char_count = self.char_count();
        while extent < char_count {
            match self.display_prop_at(extent) {
                Some(next) if next.bits() == value.bits() => {
                    extent = self
                        .next_property_change(extent)
                        .max(extent + 1)
                        .min(char_count);
                }
                _ => break,
            }
        }
        extent
    }

    fn face_at(&self, char_index: usize, context: &mut DisplaySourceContext<'_>) -> RenderFaceRef {
        let Some(props) = &self.props else {
            return self.base_face;
        };
        let char_pos = CharPos0::new(char_index);
        let face = props
            .get_property_at_char_pos(char_pos, Value::symbol("face"))
            .or_else(|| props.get_property_at_char_pos(char_pos, Value::symbol("font-lock-face")));
        face.map(|value| context.resolve_face_ref(self.base_face, value))
            .unwrap_or(self.base_face)
    }

    fn box_vertical_edges_for_range(
        &self,
        start: usize,
        end: usize,
        face: RenderFaceRef,
        context: &mut DisplaySourceContext<'_>,
    ) -> BoxVerticalEdges {
        if !context.face_has_box(face) {
            return BoxVerticalEdges::Neither;
        }
        let string_base_boxed = context.face_has_box(self.base_face);
        let previous_boxed = if start > 0 {
            let face = self.face_at(start - 1, context);
            context.face_has_box(face)
        } else {
            self.box_boundaries.before_is_boxed(string_base_boxed)
        };
        let next_boxed = if end < self.char_count() {
            let face = self.face_at(end, context);
            context.face_has_box(face)
        } else {
            self.box_boundaries.after_is_boxed(string_base_boxed)
        };
        BoxVerticalEdges::from_ownership(!previous_boxed, !next_boxed)
    }

    fn box_boundaries_for_range(
        &self,
        start: usize,
        end: usize,
        context: &mut DisplaySourceContext<'_>,
    ) -> DisplayStringBoxBoundaries {
        let string_base_boxed = context.face_has_box(self.base_face);
        let before_boxed = if start > 0 {
            let face = self.face_at(start - 1, context);
            context.face_has_box(face)
        } else {
            self.box_boundaries.before_is_boxed(string_base_boxed)
        };
        let after_boxed = if end < self.char_count() {
            let face = self.face_at(end, context);
            context.face_has_box(face)
        } else {
            self.box_boundaries.after_is_boxed(string_base_boxed)
        };
        DisplayStringBoxBoundaries::known(before_boxed, after_boxed)
    }

    fn pointer_appearance_at(
        &self,
        char_index: usize,
        _property_end: usize,
        face: RenderFaceRef,
        context: &mut DisplaySourceContext<'_>,
    ) -> Option<DisplayPointerAppearance> {
        let props = self.props.as_ref()?;
        let property = Value::symbol("mouse-face");
        let char_pos = CharPos0::new(char_index);
        let value = props.get_property_at_char_pos(char_pos, property)?;
        if value.is_nil() {
            return None;
        }
        let pointer_face = context.resolve_pointer_face_ref(face, value)?;
        let range_start = props
            .previous_single_property_change_before_char_pos(char_pos, property)
            .unwrap_or(CharPos0::ZERO);
        let range_end = props
            .next_single_property_change_after_char_pos(char_pos, property)
            .unwrap_or_else(|| CharPos0::new(self.char_count()));
        let source = DisplayPointerSourceRange::effective(
            DisplaySourcePosition::lisp_string(self.source_id, 0, 0),
            range_start.get(),
            range_end.get(),
            None,
        )
        .in_occurrence(self.pointer_occurrence);
        Some(DisplayPointerAppearance::new(source, pointer_face))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayPropertySourceAction {
    PushReplacement {
        value: Value,
        base_face: RenderFaceRef,
    },
    Emit {
        kind: DisplayItemKind,
        layout: DisplayItemLayout,
    },
    /// Consume the covered text and emit no glyph (e.g. `(left-fringe …)`).
    Skip,
    Ignore {
        layout: DisplayItemLayout,
    },
}

#[derive(Clone, Debug, PartialEq)]
// Cursor actions are ephemeral hot-path values; keep `DisplayItem` inline.
#[allow(clippy::large_enum_variant)]
pub(crate) enum DisplayPropertySourceCursorAction {
    PushReplacement {
        value: Value,
        base_face: RenderFaceRef,
    },
    Emit(DisplayItem),
    /// Consume the covered text and emit no glyph (e.g. `(left-fringe …)`).
    Skip,
    FallThrough {
        layout: DisplayItemLayout,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayPropertySourcePlan {
    value: Value,
    classification: DisplayPropertyClassification,
}

/// The two faces GNU's display-property machinery keeps distinct.
///
/// A non-string replacement (image, stretch, etc.) uses the effective face at
/// the source position.  A replacement string uses that same face for buffer
/// text, but when the source itself is a Lisp string GNU's `underlying_face_id'
/// deliberately bypasses that string's face properties.  Encoding the source
/// kind in an enum makes callers choose the inheritance rule explicitly instead
/// of passing one untyped face that can silently leak across a source boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayPropertySourceFaces {
    Buffer {
        effective: RenderFaceRef,
    },
    LispString {
        effective: RenderFaceRef,
        underlying: RenderFaceRef,
    },
}

impl DisplayPropertySourceFaces {
    const fn effective(self) -> RenderFaceRef {
        match self {
            Self::Buffer { effective } | Self::LispString { effective, .. } => effective,
        }
    }

    const fn replacement_string_base(self) -> RenderFaceRef {
        match self {
            Self::Buffer { effective } => effective,
            Self::LispString { underlying, .. } => underlying,
        }
    }
}

impl DisplayPropertySourcePlan {
    pub(crate) fn new(value: Value) -> Self {
        Self {
            value,
            classification: classify_display_property(value),
        }
    }

    pub(crate) fn replacement(&self) -> Option<&DisplayReplacementProperty> {
        self.classification.replacement()
    }

    pub(crate) fn into_classification(self) -> DisplayPropertyClassification {
        self.classification
    }

    pub(crate) fn source_action(
        &self,
        context: &mut DisplaySourceContext<'_>,
        faces: DisplayPropertySourceFaces,
        source_char: EmacsChar,
    ) -> DisplayPropertySourceAction {
        let effective_face = faces.effective();
        match DisplayPropertySourceReplacement::resolve(
            context,
            self.value,
            &self.classification,
            effective_face,
            source_char,
        ) {
            DisplayPropertySourceReplacement::String(value) => {
                DisplayPropertySourceAction::PushReplacement {
                    value,
                    base_face: faces.replacement_string_base(),
                }
            }
            DisplayPropertySourceReplacement::Item(kind) => DisplayPropertySourceAction::Emit {
                kind,
                layout: self.classification.modifiers(),
            },
            DisplayPropertySourceReplacement::Empty => DisplayPropertySourceAction::Skip,
            DisplayPropertySourceReplacement::Unresolved => DisplayPropertySourceAction::Ignore {
                layout: self.classification.modifiers(),
            },
        }
    }

    pub(crate) fn cursor_action(
        &self,
        context: &mut DisplaySourceContext<'_>,
        span: SourceSpan,
        faces: DisplayPropertySourceFaces,
        source_char: EmacsChar,
    ) -> DisplayPropertySourceCursorAction {
        self.source_action(context, faces, source_char)
            .into_cursor_action(span, faces.effective())
    }
}

impl DisplayPropertySourceAction {
    pub(crate) fn into_cursor_action(
        self,
        span: SourceSpan,
        face: RenderFaceRef,
    ) -> DisplayPropertySourceCursorAction {
        match self {
            Self::PushReplacement { value, base_face } => {
                DisplayPropertySourceCursorAction::PushReplacement { value, base_face }
            }
            Self::Emit { kind, layout } => DisplayPropertySourceCursorAction::Emit(
                DisplayItem::new(span, face, kind).with_layout(layout),
            ),
            Self::Skip => DisplayPropertySourceCursorAction::Skip,
            Self::Ignore { layout } => DisplayPropertySourceCursorAction::FallThrough { layout },
        }
    }
}

enum DisplayPropertySourceReplacement {
    String(Value),
    Item(DisplayItemKind),
    /// `(left-fringe …)` and friends: the covered text is replaced by nothing
    /// inline (GNU draws the bitmap in the fringe). Consume the text, emit no
    /// glyph.
    Empty,
    Unresolved,
}

impl DisplayPropertySourceReplacement {
    fn resolve(
        context: &mut DisplaySourceContext<'_>,
        display_prop: Value,
        classification: &DisplayPropertyClassification,
        face: RenderFaceRef,
        source_char: EmacsChar,
    ) -> Self {
        // Typed arms take their Lisp payload from the SPEC that produced the
        // replacement (`["X"]` and `("X")` are not the string they replace with);
        // only the untyped fallback below still probes the whole `display` value.
        let spec = classification.replacement_spec();
        match classification.replacement() {
            Some(DisplayReplacementProperty::String) => Self::String(spec),
            Some(DisplayReplacementProperty::Stretch(stretch)) => {
                Self::Item(DisplayItemKind::Stretch(stretch.bind_source(source_char)))
            }
            Some(DisplayReplacementProperty::Fringe(layout)) => {
                // Collect the fringe layout for the row-render path; the inline
                // text stays suppressed (Empty).
                context.collect_fringe(*layout);
                Self::Empty
            }
            Some(DisplayReplacementProperty::Margin(margin)) => {
                let content = match margin.content() {
                    DisplayMarginContent::String(value) => {
                        Some(DisplayMarginEmissionContent::String(*value))
                    }
                    DisplayMarginContent::Stretch { layout, .. } => {
                        Some(DisplayMarginEmissionContent::Item(
                            DisplayItemKind::Stretch(layout.bind_source(source_char)),
                        ))
                    }
                    DisplayMarginContent::Media {
                        spec,
                        replacement,
                        image_slice,
                    } => replacement
                        .direct_replacement()
                        .map(DisplayItemKind::MediaReplacement)
                        .or_else(|| {
                            context
                                .resolve_display_media_replacement(*spec, *image_slice, face)
                                .filter(|media| replacement.accepts_media_replacement(media))
                                .map(DisplayItemKind::MediaReplacement)
                        })
                        .map(DisplayMarginEmissionContent::Item),
                };
                if let Some(content) = content {
                    context.collect_margin(DisplayMarginEmission::new(margin.side(), content));
                }
                // A margin display spec replaces its covered source in the text
                // area even if its media payload cannot be resolved.
                Self::Empty
            }
            Some(DisplayReplacementProperty::Media(replacement)) => replacement
                .direct_replacement()
                .map(DisplayItemKind::MediaReplacement)
                .or_else(|| {
                    context
                        .resolve_display_media_replacement(spec, classification.image_slice(), face)
                        .filter(|media| replacement.accepts_media_replacement(media))
                        .map(DisplayItemKind::MediaReplacement)
                })
                .map(Self::Item)
                .unwrap_or(Self::Unresolved),
            None => context
                .resolve_display_media_replacement(display_prop, None, face)
                .map(DisplayItemKind::MediaReplacement)
                .map(Self::Item)
                .unwrap_or(Self::Unresolved),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextSourceCharClassification {
    /// Ordinary renderable text. Carrying the Rust scalar here makes it
    /// impossible for a non-Unicode Emacs character to enter a text run.
    Text(char),
    RowBreak,
    ControlChar {
        ch: char,
    },
    /// A non-printable char shown as `\`+octal in the escape-glyph face
    /// (see [`is_escape_glyph_octal`]); emitted as a single-char
    /// `SourceMappedText` item so its escape-glyph face merges via the hook.
    EscapeOctal {
        displayed_code: u32,
    },
    Glyphless {
        ch: char,
        method: GlyphlessMethod,
    },
}

pub(crate) fn classify_text_source_char(character: EmacsChar) -> TextSourceCharClassification {
    let code = character.code();
    let Some(ch) = character.as_rust_char() else {
        return TextSourceCharClassification::EscapeOctal {
            displayed_code: character.to_byte8().map(u32::from).unwrap_or(code),
        };
    };
    if ch == '\n' {
        return TextSourceCharClassification::RowBreak;
    }
    if is_control_char(ch) {
        return TextSourceCharClassification::ControlChar { ch };
    }
    // A non-printable char (general category Cc/Cs/Cn, `!CHAR_PRINTABLE_P`)
    // displays as `\`+octal in the escape-glyph face (GNU xdisp.c:8552, checked
    // BEFORE glyphless production) -- the noncharacters U+FFFE/U+FFFF and the
    // U+FDD0..U+FDEF range, unassigned specials U+FFF0..U+FFF8, and the C1
    // controls U+0080..U+009F. This precedes the glyphless methods so those all
    // resolve to GNU's octal escape rather than a hex-code box.
    if is_escape_glyph_octal(ch) {
        return TextSourceCharClassification::EscapeOctal {
            displayed_code: code,
        };
    }
    if let Some(method) =
        glyphless_method_for_char(ch, GlyphlessJoinerPolicy::PreserveForComposition)
    {
        return TextSourceCharClassification::Glyphless { ch, method };
    }
    TextSourceCharClassification::Text(ch)
}

pub(crate) fn display_item_kind_for_text_source_char_with_tty_mapping(
    character: EmacsChar,
    tty_mapping: Option<GlyphlessMethod>,
) -> Option<DisplayItemKind> {
    match classify_text_source_char(character) {
        TextSourceCharClassification::Text(ch) => {
            tty_mapping.map(|method| DisplayItemKind::Glyphless(DisplayGlyphless { ch, method }))
        }
        TextSourceCharClassification::RowBreak => Some(DisplayItemKind::RowBreak(
            DisplayRowBreak::explicit_newline(),
        )),
        TextSourceCharClassification::ControlChar { ch } => {
            Some(DisplayItemKind::ControlChar { ch })
        }
        TextSourceCharClassification::EscapeOctal { displayed_code } => {
            Some(DisplayItemKind::SourceMappedText(
                DisplaySourceMappedText::new(escape_glyph_octal_text(displayed_code))
                    .with_semantic_face_overlay(
                        crate::display_item::DisplayItemFaceOverlay::EscapeGlyph,
                    ),
            ))
        }
        TextSourceCharClassification::Glyphless { ch, method } => {
            Some(DisplayItemKind::Glyphless(DisplayGlyphless {
                ch,
                method: tty_mapping.unwrap_or(method),
            }))
        }
    }
}

fn is_control_char(ch: char) -> bool {
    let code = ch as u32;
    (code <= 0x1f && ch != '\n' && ch != '\t') || code == 0x7f
}

#[cfg(test)]
#[path = "display_source_test.rs"]
mod tests;
