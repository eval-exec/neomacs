//! Prior-row paint primitives, admitted for this frame's fresh source walk.
//! No old provenance, pointer token, or box terminal may be installed unchanged:
//! the writer binds those from the current canonical item on every reuse.

use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphType, MatrixRow};
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::CharRange;

use std::sync::Arc;

use super::{ChromeMemo, ChromeRowFingerprint, chrome_row_role};

/// Owns the exact retained row admitted by its window's ChromeMemo. Construction
/// validates a character/paint bijection; callers only receive immutable slices.
#[derive(Clone, Debug)]
pub(crate) struct ChromeGlyphReservoir {
    row: MatrixRow,
    text: IndependentAsciiText,
}

// Captured base faces can own mutator-confined rooted Lisp face state. Keep
// that existing owner's confinement while borrowing its immutable text.
static_assertions::assert_impl_all!(ChromeGlyphReservoir: Clone);
static_assertions::assert_not_impl_any!(ChromeGlyphReservoir: Send, Sync, Copy);
static_assertions::const_assert!(std::mem::size_of::<ChromeGlyphReservoir>() <= 32);

#[derive(Clone, Debug)]
struct IndependentAsciiText {
    owner: Arc<[ChromeRowFingerprint]>,
    fingerprint: ChromeFingerprintIndex,
}

#[derive(Clone, Copy, Debug)]
struct ChromeFingerprintIndex(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrintableAscii<'a>(&'a [u8]);

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("chrome primitive reuse requires printable ASCII")]
struct NotPrintableAscii;

impl<'a> TryFrom<&'a [u8]> for PrintableAscii<'a> {
    type Error = NotPrintableAscii;

    fn try_from(bytes: &'a [u8]) -> Result<Self, Self::Error> {
        if bytes.iter().all(|&byte| matches!(byte, b' '..=b'~')) {
            Ok(Self(bytes))
        } else {
            Err(NotPrintableAscii)
        }
    }
}

/// Both sides of a lookup use this definition, so text, coordinate range and
/// realized face cannot diverge between a cached run and its current request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChromeGlyphRunKey<'a> {
    range: CharRange,
    text: PrintableAscii<'a>,
    face: FaceId,
}

impl<'a> ChromeGlyphRunKey<'a> {
    fn capture(range: CharRange, text: PrintableAscii<'a>, face: FaceId) -> Option<Self> {
        if range.is_empty() || range.len().get() != text.0.len() {
            return None;
        }
        Some(Self { range, text, face })
    }
}

impl ChromeGlyphReservoir {
    /// Select and validate paint and fingerprint data from the same owner.
    pub(super) fn capture(memo: &ChromeMemo, current: &ChromeRowFingerprint) -> Option<Self> {
        let (index, previous) = memo
            .fingerprints
            .iter()
            .enumerate()
            .find(|(_, previous)| previous.kind() == current.kind())?;
        if previous.context != current.context || previous.text.0.len() != current.text.0.len() {
            return None;
        }
        let role = chrome_row_role(current.kind());
        let (_, row) = memo.chrome.rows.iter().find(|(_, row)| row.role == role)?;
        if row.reversed_p {
            return None;
        }
        let text = PrintableAscii::try_from(previous.text.0.as_ref()).ok()?;
        PrintableAscii::try_from(current.text.0.as_ref()).ok()?;
        let glyphs = &row.glyphs[GlyphArea::Text.index()];
        if glyphs.len() != text.0.len() {
            return None;
        }
        for (glyph, byte) in glyphs.iter().zip(text.0) {
            let GlyphType::Char { ch } = glyph.glyph_type else {
                return None;
            };
            if ch != char::from(*byte)
                || glyph.padding
                || glyph.wide
                || glyph.bidi_level != 0
                || !glyph.pixel_width.is_finite()
                || glyph.pixel_width <= 0.0
                || !glyph.pixel_height.is_finite()
                || glyph.pixel_height < 0.0
                || !glyph.pixel_ascent.is_finite()
                || glyph.pixel_ascent < 0.0
                || !glyph.vertical_offset_px.is_finite()
            {
                return None;
            }
        }
        Some(Self {
            row: row.clone(),
            text: IndependentAsciiText {
                owner: memo.fingerprints.clone(),
                fingerprint: ChromeFingerprintIndex(index),
            },
        })
    }

    /// The current source walk has already resolved its face and box topology.
    /// Only unchanged text in this exact root-character interval may reuse paint.
    pub(crate) fn glyphs_for(
        &self,
        range: CharRange,
        text: &str,
        face_id: FaceId,
    ) -> Option<&[Glyph]> {
        let current = ChromeGlyphRunKey::capture(
            range,
            PrintableAscii::try_from(text.as_bytes()).ok()?,
            face_id,
        )?;
        let indices = range.start().get()..range.end().get();
        let text = &self.text.owner.get(self.text.fingerprint.0)?.text.0;
        let bytes = text.get(indices.clone())?;
        let glyphs = self.row.glyphs[GlyphArea::Text.index()].get(indices)?;
        let first = glyphs.first()?;
        if glyphs.iter().any(|glyph| glyph.face_id != first.face_id) {
            return None;
        }
        // A subslice of validated printable ASCII remains printable ASCII.
        let previous = ChromeGlyphRunKey::capture(range, PrintableAscii(bytes), first.face_id)?;
        (previous == current).then_some(glyphs)
    }
}

#[cfg(test)]
#[path = "glyph_reservoir/tests/glyph_reservoir_test.rs"]
mod tests;
