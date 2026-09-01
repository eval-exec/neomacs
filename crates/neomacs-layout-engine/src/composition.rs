//! Central grapheme-cluster composition rules for the layout walks.
//!
//! GNU Emacs groups characters into composed grapheme clusters via its
//! automatic-composition machinery, carrying a `struct composition_it`
//! (src/dispextern.h) on the display iterator so every text-producing path
//! groups clusters identically. neomacs's layout walks historically each
//! made their own ad-hoc `is_cluster_extender` checks; this module is the
//! single source of truth they share.
//!
//! Phase 2 covers grapheme clusters (combining marks, variation selectors,
//! ZWJ emoji sequences, regional-indicator flag pairs) — the cases GNU
//! composes by default. Contextual-shaping scripts (Arabic, Indic) and the
//! glyph-id gstring arrive in a later phase.

use crate::unicode::{is_cluster_extender, is_regional_indicator, is_wide_char};
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphRow, GlyphType};

/// Display columns occupied by a base character before clustering.
///
/// Regional indicators are forced to 2 columns so a composed flag fills a
/// full 2-column cell instead of overlapping the next glyph; everything
/// else defers to the shared char-width table (GNU's default
/// `char-width-table`).
pub(crate) fn base_width_cols(ch: char) -> u8 {
    if is_wide_char(ch) || is_regional_indicator(ch as u32) {
        2
    } else {
        1
    }
}

/// Number of terminal cells a *composed grapheme cluster* (a `Composite`
/// glyph holding several characters) occupies on a TTY.
///
/// GNU advances the redisplay column by a composition's `cmp->width`
/// (src/composite.h), which for an automatic composition is the sum of the
/// component characters' `CHARACTER_WIDTH` (src/composite.c
/// `composition_update_it`/`composition_gstring_width`): combining marks,
/// shaping controls and joiners contribute 0, base letters contribute their
/// char-width. That is exactly what `string-width` computes, so the rendered
/// cell count of a composed cluster must equal `string-width` of its text —
/// the same value `current-column`/`Fcurrent_column` use buffer-side. Without
/// this, the render walk counted the whole cluster as a single cell and any
/// following TAB filled to the wrong stop (Arabic/Indic names in etc/HELLO).
pub(crate) fn composed_cluster_cols(text: &str) -> usize {
    neovm_core::encoding::string_width(text).max(1)
}

/// Whether `ch` continues the grapheme cluster of the previously emitted
/// text glyph, given that glyph's `tail` — `(last_char,
/// is_lone_regional_indicator)` from the current display row, or `None`
/// at a row start.
///
/// A character continues the cluster when it is a cluster extender
/// (combining mark, variation selector, ZWJ, skin-tone modifier), when it
/// follows a ZWJ (a member of an emoji ZWJ sequence), or when it is the
/// second regional indicator after a lone one (a flag pair). This is the
/// single rule every layout char loop consults so clustering is identical
/// across buffer text, overlay strings, and display strings — neomacs's
/// stand-in for GNU's shared `composition_it` walk.
pub(crate) fn continues_cluster(ch: char, tail: Option<(char, bool)>) -> bool {
    is_cluster_extender(ch)
        || matches!(tail, Some((prev, _)) if prev == '\u{200D}')
        || (is_regional_indicator(ch as u32) && matches!(tail, Some((_, true))))
}

/// Return `(last_char, is_lone_regional_indicator)` for the last
/// non-padding text glyph in `row`. This is the shared display-row view
/// used by row builders and live text-window output to decide whether the
/// next source character continues a grapheme cluster or contextual run.
pub(crate) fn last_text_cluster_tail_in_row(row: &GlyphRow) -> Option<(char, bool)> {
    let area = &row.glyphs[GlyphArea::Text.index()];
    last_text_cluster_tail_in_glyphs(area)
}

pub(crate) fn last_text_cluster_tail_in_glyphs(area: &[Glyph]) -> Option<(char, bool)> {
    let glyph = area.iter().rev().find(|g| !g.padding)?;
    match &glyph.glyph_type {
        GlyphType::Char { ch } => Some((*ch, is_regional_indicator(*ch as u32))),
        GlyphType::Composite { text } => text.chars().last().map(|ch| (ch, false)),
        _ => None,
    }
}

/// A zero-width *composition joiner / selector*: a format character that the
/// generic glyphless-char check would skip, but which actually binds an emoji
/// or text composition together and so must be absorbed into the active
/// cluster instead (GNU consumes these via `composition_it`). Covers ZWJ/ZWNJ,
/// variation selectors (incl. the supplement), and tag characters (subdivision
/// flag tag sequences). Deliberately EXCLUDES bidi controls (LRM/RLM), spaces
/// (ZWSP/ZWNBSP), line/paragraph separators, and C1 controls — those stay
/// glyphless so they still display as their own replacement glyph.
pub fn is_composition_joiner(ch: char) -> bool {
    matches!(
        ch as u32,
        0x200C | 0x200D            // ZERO WIDTH NON-JOINER / JOINER
            | 0xFE00..=0xFE0F      // variation selectors 1-16
            | 0xE0001              // language tag
            | 0xE0020..=0xE007F    // tag characters (tag sequences)
            | 0xE0100..=0xE01EF    // variation selectors supplement
    )
}

/// The character that decides which font a cluster resolves through.
///
/// Shared by the layout-side per-char font realization and the renderer's
/// rasterization so both threads make the same fallback decision:
/// - A cluster containing U+FE0F (emoji variation selector) requests EMOJI
///   presentation for the whole cluster, so the font is resolved the way an
///   emoji codepoint is — probe with a canonical emoji (U+1F600) to reach
///   the color emoji font (an emoji keycap is digit + U+FE0F + U+20E3; the
///   digit alone would pick a monochrome font).
/// - Otherwise the first glyph-bearing non-ASCII character, skipping
///   zero-width joiners/selectors that don't determine the font.
/// - `None` means the cluster is ASCII-only: the face's primary font applies.
pub fn representative_char_for_cluster(text: &str) -> Option<char> {
    if text.contains('\u{FE0F}') {
        return Some('\u{1F600}');
    }
    text.chars()
        .find(|&ch| !ch.is_ascii() && !is_composition_joiner(ch))
}

/// Whether a composed payload can be replayed as independent cmap glyphs or
/// must pass through a shaping engine. Keeping this decision typed prevents a
/// bitmap shortcut from silently flattening combining/contextual clusters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompositionGlyphPlan {
    SimpleCopy,
    Shape,
}

pub(crate) fn composition_glyph_plan(text: &str) -> CompositionGlyphPlan {
    if text.chars().all(|ch| {
        !is_cluster_extender(ch) && !is_composition_joiner(ch) && !needs_complex_shaping(ch)
    }) {
        CompositionGlyphPlan::SimpleCopy
    } else {
        CompositionGlyphPlan::Shape
    }
}

/// A contextual-shaping script: one whose letters change form based on
/// neighbours (Arabic joining) or reorder (Indic), so per-character isolated
/// shaping is wrong and the run must be shaped together with `shape_run`.
/// The concrete value only matters for grouping consecutive same-script
/// characters into one shaped run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComplexScript {
    Arabic,
    Syriac,
    Devanagari,
    Bengali,
    Gurmukhi,
    Gujarati,
    Oriya,
    Tamil,
    Telugu,
    Kannada,
    Malayalam,
    Thai,
    Lao,
    Tibetan,
    Myanmar,
}

/// Identify the contextual-shaping script of `ch`, if any. Scripts not listed
/// (Latin, CJK, …) render correctly per character and return `None`, so they
/// stay on the fast per-char path. Mirrors the coverage GNU's default
/// `composition-function-table` provides for these scripts.
pub(crate) fn complex_script(ch: char) -> Option<ComplexScript> {
    use ComplexScript::*;
    let cp = ch as u32;
    Some(match cp {
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Arabic
        }
        0x0700..=0x074F => Syriac,
        0x0900..=0x097F => Devanagari,
        0x0980..=0x09FF => Bengali,
        0x0A00..=0x0A7F => Gurmukhi,
        0x0A80..=0x0AFF => Gujarati,
        0x0B00..=0x0B7F => Oriya,
        0x0B80..=0x0BFF => Tamil,
        0x0C00..=0x0C7F => Telugu,
        0x0C80..=0x0CFF => Kannada,
        0x0D00..=0x0D7F => Malayalam,
        0x0E00..=0x0E7F => Thai,
        0x0E80..=0x0EFF => Lao,
        0x0F00..=0x0FFF => Tibetan,
        0x1000..=0x109F => Myanmar,
        _ => return None,
    })
}

/// Whether `ch` continues a contextual-shaping run started by the previously
/// emitted glyph: both belong to the SAME complex script. Such a run is grown
/// into one composed `Composite` glyph plus one padding cell per following
/// character, so the renderer shapes (joins / reorders) the whole run as a
/// unit while the per-char padding keeps per-letter cursor positions.
///
/// Unlike [`continues_cluster`], a run member is NOT zero-width — it occupies
/// its own column. `tail` is the previously emitted glyph's
/// `(last_char, _)` from the current display row.
pub(crate) fn continues_complex_run(ch: char, tail: Option<(char, bool)>) -> bool {
    match (complex_script(ch), tail) {
        (Some(script), Some((prev, _))) => complex_script(prev) == Some(script),
        _ => false,
    }
}

/// Whether `ch` belongs to a script that needs run-level contextual shaping.
/// Gates the run-shaping advance path in the layout walk.
pub(crate) fn needs_complex_shaping(ch: char) -> bool {
    complex_script(ch).is_some()
}

#[cfg(test)]
#[path = "composition_test.rs"]
mod tests;
