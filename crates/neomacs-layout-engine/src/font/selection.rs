//! Platform-neutral GNU font-entity scoring.
//!
//! GNU `font_score` stores four independent seven-bit distances. Their bit
//! positions make comparison lexicographic in `face-font-selection-order`
//! (by default: width, size, weight, slant); unrelated properties are not an
//! additive bag of penalties. Rust field order plus derived [`Ord`] expresses
//! that policy directly and prevents a refactor from silently reordering it.

use crate::font_backend::PlatformFontSize;
use neovm_core::face::{FontSlant, FontWidth};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PropertyDistance(u32);

/// GNU's default style-property priority, represented without bit arithmetic.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct GnuStyleScore {
    width: PropertyDistance,
    size: PropertyDistance,
    weight: PropertyDistance,
    slant: PropertyDistance,
}

/// Complete candidate score before the stable discovery-order tie break.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CandidateSelectionScore {
    /// Non-style compatibility supplied by the caller (family/spacing).
    compatibility: u32,
    style: GnuStyleScore,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn candidate_selection_score(
    compatibility: u32,
    requested_size_26_6: u32,
    requested_weight: u16,
    requested_slant: FontSlant,
    requested_width: Option<FontWidth>,
    candidate_weight: u16,
    candidate_slant: FontSlant,
    candidate_width: Option<FontWidth>,
    candidate_size: PlatformFontSize,
) -> Option<CandidateSelectionScore> {
    let size = size_distance(requested_size_26_6, candidate_size)?;
    Some(CandidateSelectionScore {
        compatibility,
        style: GnuStyleScore {
            width: PropertyDistance(requested_width.map_or(0, |requested| {
                u32::from(
                    candidate_width
                        .unwrap_or(FontWidth::Normal)
                        .gnu_numeric()
                        .abs_diff(requested.gnu_numeric()),
                )
            })),
            // Scalable entities match every requested size. Fixed bitmap
            // entities carry Fontconfig's concrete pixel size and are scored
            // before the materializer opens anything, as GNU does.
            size,
            weight: PropertyDistance(u32::from(candidate_weight.abs_diff(requested_weight))),
            slant: PropertyDistance(slant_distance(requested_slant, candidate_slant)),
        },
    })
}

/// GNU converts Fontconfig's pixel sizes to integer Lisp fixnums, rejects a
/// fixed entity outside the inclusive 1/2×..2× range, then stores twice the
/// absolute pixel distance in a seven-bit score field.
fn size_distance(
    requested_size_26_6: u32,
    candidate_size: PlatformFontSize,
) -> Option<PropertyDistance> {
    let candidate_size_26_6 = match candidate_size {
        PlatformFontSize::Scalable => return Some(PropertyDistance(0)),
        PlatformFontSize::Fixed { device_ppem_26_6 } => device_ppem_26_6,
        // Unknown is a capability state, not shorthand for scalable. The
        // resolver must classify it before GNU entity scoring.
        PlatformFontSize::Unknown => return None,
    };
    let requested_px = requested_size_26_6 >> 6;
    let candidate_px = candidate_size_26_6 >> 6;
    if requested_px.saturating_mul(2) < candidate_px
        || candidate_px.saturating_mul(2) < requested_px
    {
        return None;
    }
    Some(PropertyDistance(
        requested_px
            .abs_diff(candidate_px)
            .saturating_mul(2)
            .min(127),
    ))
}

fn slant_distance(requested: FontSlant, candidate: FontSlant) -> u32 {
    use FontSlant::{Italic, Normal, Oblique, ReverseItalic, ReverseOblique};
    match (requested, candidate) {
        (Normal, Normal) => 0,
        (Italic, Italic) | (Italic, Oblique) => 0,
        (Oblique, Oblique) | (Oblique, Italic) => 0,
        (ReverseItalic, ReverseItalic) | (ReverseItalic, ReverseOblique) => 0,
        (ReverseOblique, ReverseOblique) | (ReverseOblique, ReverseItalic) => 0,
        (Normal, _) => 350,
        (_, Normal) => 250,
        _ => 75,
    }
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod selection_test;
