//! The typed producer vocabulary: what an element is, where it came from, and
//! what its glyphs get stamped with.
//!
//! GNU keeps glyph provenance as a `(charpos, object)` PAIR (`struct glyph`,
//! dispextern.h:460-483) and two position tracks on the iterator —
//! `it->current.pos`, the honest buffer position that feeds row min/max, versus
//! `it->position`, what actually lands on the glyph (a string index while a
//! string is being displayed, xdisp.c:9609-9613). [`ProducedGlyphProvenance`]
//! keeps that coordinate space and its source object inseparable. The final
//! row representation registers the string occurrence once and gives each glyph
//! a compact typed token plus its string index.

use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayRowBreakReason, DisplaySourcePosition,
    DisplayStretchWidth, RenderFaceRef, SourceSpan,
};
use crate::display_source::{DisplaySourceStepItem, DisplaySourceTextPosition};
pub(crate) use neomacs_display_protocol::glyph_matrix::{
    GlyphStringBufferRange, GlyphStringId as ProducedStringId, RedisplayGlyphProvenance,
};

/// Producer-side provenance before a string occurrence is assigned its
/// compact row-local token.
///
/// This is the layout equivalent of GNU's `(charpos, object)` pair.  It keeps
/// the VM/session string identity and exact replacement coverage while an item
/// is in flight; row construction moves those occurrence-wide fields into one
/// side-table entry and stores only the token plus index on each glyph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProducedGlyphProvenance {
    Buffer {
        charpos: usize,
    },
    Str {
        string: ProducedStringId,
        index: usize,
        covered_buffer: Option<GlyphStringBufferRange>,
    },
    Redisplay(RedisplayGlyphProvenance),
}

impl ProducedGlyphProvenance {
    pub(crate) const fn buffer(charpos: usize) -> Self {
        Self::Buffer { charpos }
    }

    pub(crate) const fn string(string: ProducedStringId, index: usize) -> Self {
        Self::Str {
            string,
            index,
            covered_buffer: None,
        }
    }

    pub(crate) const fn string_replacement(
        string: ProducedStringId,
        index: usize,
        covered_buffer: GlyphStringBufferRange,
    ) -> Self {
        Self::Str {
            string,
            index,
            covered_buffer: Some(covered_buffer),
        }
    }

    pub(crate) const fn line_end() -> Self {
        Self::Redisplay(RedisplayGlyphProvenance::LineEnd)
    }

    pub(crate) const fn mark() -> Self {
        Self::Redisplay(RedisplayGlyphProvenance::Mark)
    }

    pub(crate) const fn empty_line_newline(charpos: usize) -> Self {
        Self::Redisplay(RedisplayGlyphProvenance::EmptyLineNewline { charpos })
    }

    pub(crate) const fn buffer_charpos(self) -> Option<usize> {
        match self {
            Self::Buffer { charpos } => Some(charpos),
            Self::Str { .. } | Self::Redisplay(_) => None,
        }
    }

    pub(crate) const fn advanced_by(self, char_offset: usize) -> Self {
        match self {
            Self::Buffer { charpos } => Self::buffer(charpos.saturating_add(char_offset)),
            Self::Str {
                string,
                index,
                covered_buffer,
            } => Self::Str {
                string,
                index: index.saturating_add(char_offset),
                covered_buffer,
            },
            Self::Redisplay(provenance) => Self::Redisplay(provenance),
        }
    }

    pub(crate) const fn legacy_charpos(self) -> usize {
        match self {
            Self::Buffer { charpos } => charpos,
            Self::Str { index, .. } => index,
            Self::Redisplay(RedisplayGlyphProvenance::EmptyLineNewline { charpos }) => charpos,
            Self::Redisplay(RedisplayGlyphProvenance::LineEnd | RedisplayGlyphProvenance::Mark) => {
                neomacs_display_protocol::glyph_matrix::NO_BUFFER_POSITION_CHARPOS
            }
        }
    }
}

/// The producer's scan track: charpos plus byte index, ALWAYS through buffer
/// text (GNU `it->current.pos`). The walk's existing position type — the
/// producer does not get a second, drifting copy.
pub(crate) type BufferScanPos = DisplaySourceTextPosition;

/// What a produced element, and every glyph it makes, is attributed to. GNU's
/// `(charpos, object)` pair as one value, so a charpos can never be read in the
/// wrong coordinate space.
/// Mirrors the renderer's private `DisplayTextSourceMapping` (builder.rs): do a
/// run's glyphs advance the stamp per character, or does every glyph carry the
/// run's start?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunStamping {
    /// Buffer text: glyph N carries start + N.
    NaturalText,
    /// Generic mapped text without a Lisp-string source: every glyph carries
    /// the covered start, however many glyphs the replacement produced.
    Covered,
}

/// Convert an item source position into the producer's closed glyph source.
pub(crate) fn provenance_from_source_position(
    position: &DisplaySourcePosition,
) -> ProducedGlyphProvenance {
    match position {
        DisplaySourcePosition::Buffer { char_pos, .. } => {
            ProducedGlyphProvenance::buffer(char_pos.get())
        }
        DisplaySourcePosition::LispString {
            source_id,
            char_index,
            ..
        } => ProducedGlyphProvenance::string(ProducedStringId::new(source_id.get()), *char_index),
        DisplaySourcePosition::Synthetic { .. } => ProducedGlyphProvenance::mark(),
    }
}

pub(crate) fn natural_text_glyph(
    span_start: &DisplaySourcePosition,
    char_offset: usize,
) -> ProducedGlyphProvenance {
    provenance_from_source_position(span_start).advanced_by(char_offset)
}

pub(crate) fn covered_text_glyph(span_start: &DisplaySourcePosition) -> ProducedGlyphProvenance {
    provenance_from_source_position(span_start)
}

/// Provenance for one glyph from source-mapped text.
///
/// Generic mapped text is frozen at its buffer anchor. A Lisp replacement
/// string advances in string coordinates while retaining its exact covered
/// buffer range as a separate, typed field.
pub(crate) fn source_mapped_text_glyph(
    span: &SourceSpan,
    glyph_string_start: Option<&DisplaySourcePosition>,
    char_offset: usize,
) -> ProducedGlyphProvenance {
    let Some(glyph_string_start) = glyph_string_start else {
        return covered_text_glyph(&span.start);
    };
    let provenance = provenance_from_source_position(glyph_string_start).advanced_by(char_offset);
    let (
        ProducedGlyphProvenance::Str { string, index, .. },
        DisplaySourcePosition::Buffer {
            char_pos: covered_start,
            ..
        },
        DisplaySourcePosition::Buffer {
            char_pos: covered_end,
            ..
        },
    ) = (provenance, &span.start, &span.end)
    else {
        return provenance;
    };
    ProducedGlyphProvenance::string_replacement(
        string,
        index,
        GlyphStringBufferRange::new(covered_start.get(), covered_end.get()),
    )
}

/// GNU `it->current.pos` and `it->position` as one struct, so they can never be
/// advanced independently by accident. `scan` always walks buffer text; `stamp`
/// is what the next element's glyphs carry, and differs from `scan` exactly
/// while a producer frame is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProducerPosition {
    scan: BufferScanPos,
    stamp: ProducedGlyphProvenance,
}

impl ProducerPosition {
    /// Producing buffer text: the stamp IS the scan position.
    pub(crate) fn buffer_at(scan: BufferScanPos) -> Self {
        Self {
            scan,
            stamp: ProducedGlyphProvenance::buffer(scan.charpos().max(0) as usize),
        }
    }

    pub(crate) const fn with_stamp(scan: BufferScanPos, stamp: ProducedGlyphProvenance) -> Self {
        Self { scan, stamp }
    }

    pub(crate) const fn scan(self) -> BufferScanPos {
        self.scan
    }

    pub(crate) const fn stamp(self) -> ProducedGlyphProvenance {
        self.stamp
    }

    /// The scan and stamp today's pipeline item carries.
    pub(crate) fn from_step_item(item: &DisplaySourceStepItem) -> Self {
        let step_char = item.source_step_char();
        let scan = BufferScanPos::new(step_char.start_byte_idx(), step_char.start_charpos());
        Self::with_stamp(
            scan,
            provenance_from_source_position(&item.item().span.start),
        )
    }
}

/// One display element: the typed output of GNU's `get_next_display_element`.
/// Payloads start at what the legacy bridge below can fill and grow per rung.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProducedElement {
    /// One character, buffer- or string-stamped per its position.
    Char(ProducedChar),
    /// A homogeneous run — a pure batching optimization over `Char`, never a
    /// different meaning: consuming k characters and asking again must yield
    /// the rest.
    Run(ProducedRun),
    /// A stretch glyph (`(space ...)` specs). Buffer-stamped, design 4.3.
    Stretch(ProducedStretch),
    /// A line end. Provenance distinguishes a buffer newline from a
    /// string-supplied one (GNU `ends_in_newline_from_string_p`).
    RowBreak(ProducedRowBreak),
    /// End of the visible text window.
    EndOfText,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProducedChar {
    position: ProducerPosition,
    ch: char,
    face: RenderFaceRef,
    avoid_cursor: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProducedRun {
    position: ProducerPosition,
    text: Box<str>,
    face: RenderFaceRef,
    stamping: RunStamping,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProducedStretch {
    position: ProducerPosition,
    width: DisplayStretchWidth,
    face: RenderFaceRef,
    avoid_cursor: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProducedRowBreak {
    position: ProducerPosition,
    reason: DisplayRowBreakReason,
}

impl ProducedChar {
    pub(crate) const fn position(&self) -> ProducerPosition {
        self.position
    }

    pub(crate) const fn ch(&self) -> char {
        self.ch
    }

    pub(crate) const fn face(&self) -> RenderFaceRef {
        self.face
    }

    /// GNU `avoid_cursor_p` (xdisp.c:32693): the cursor never lands here.
    pub(crate) const fn avoid_cursor(&self) -> bool {
        self.avoid_cursor
    }
}

impl ProducedRun {
    pub(crate) const fn position(&self) -> ProducerPosition {
        self.position
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) const fn face(&self) -> RenderFaceRef {
        self.face
    }

    pub(crate) fn is_covered_provenance(&self) -> bool {
        self.stamping == RunStamping::Covered
    }

    /// Provenance of the run's `char_offset`-th glyph, following the same rule
    /// the append path uses.
    pub(crate) fn glyph_provenance(&self, char_offset: usize) -> ProducedGlyphProvenance {
        match self.stamping {
            RunStamping::NaturalText => self.position.stamp().advanced_by(char_offset),
            RunStamping::Covered => self.position.stamp(),
        }
    }
}

impl ProducedStretch {
    pub(crate) const fn position(&self) -> ProducerPosition {
        self.position
    }

    pub(crate) fn width(&self) -> &DisplayStretchWidth {
        &self.width
    }

    pub(crate) const fn face(&self) -> RenderFaceRef {
        self.face
    }

    pub(crate) const fn avoid_cursor(&self) -> bool {
        self.avoid_cursor
    }
}

impl ProducedRowBreak {
    pub(crate) const fn position(&self) -> ProducerPosition {
        self.position
    }

    pub(crate) const fn reason(&self) -> DisplayRowBreakReason {
        self.reason
    }
}

impl ProducedElement {
    /// Bridge from today's item vocabulary at a known scan position. Kinds the
    /// element vocabulary does not model yet (glyphless, media replacements)
    /// return `None` and keep flowing through the legacy item path until their
    /// rung, rather than being given an invented element shape.
    pub(crate) fn from_item(item: &DisplayItem, scan: BufferScanPos) -> Option<Self> {
        let buffer_position = || {
            ProducerPosition::with_stamp(scan, provenance_from_source_position(&item.span.start))
        };
        match &item.kind {
            DisplayItemKind::TextRun(run) => Some(Self::Run(ProducedRun {
                position: buffer_position(),
                text: run.text.clone(),
                face: item.face,
                stamping: RunStamping::NaturalText,
            })),
            DisplayItemKind::SourceMappedText(text) => Some(Self::Run(ProducedRun {
                position: ProducerPosition::with_stamp(
                    scan,
                    source_mapped_text_glyph(&item.span, text.glyph_string_start.as_ref(), 0),
                ),
                text: text.text.clone(),
                face: item.face,
                stamping: if text.glyph_string_start.is_some() {
                    RunStamping::NaturalText
                } else {
                    RunStamping::Covered
                },
            })),
            DisplayItemKind::ControlChar { ch } => Some(Self::Char(ProducedChar {
                position: buffer_position(),
                ch: *ch,
                face: item.face,
                avoid_cursor: false,
            })),
            DisplayItemKind::Stretch(stretch) => Some(Self::Stretch(ProducedStretch {
                position: buffer_position(),
                width: stretch.width.clone(),
                face: item.face,
                avoid_cursor: false,
            })),
            DisplayItemKind::RowBreak(row_break) => Some(Self::RowBreak(ProducedRowBreak {
                position: buffer_position(),
                reason: row_break.reason,
            })),
            DisplayItemKind::Glyphless(_) | DisplayItemKind::MediaReplacement(_) => None,
        }
    }

    /// Bridge from today's pipeline step item, whose step char supplies the
    /// scan position.
    pub(crate) fn from_step_item(item: &DisplaySourceStepItem) -> Option<Self> {
        Self::from_item(item.item(), ProducerPosition::from_step_item(item).scan())
    }
}

#[cfg(test)]
#[path = "vocabulary_test.rs"]
mod tests;
