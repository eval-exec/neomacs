//! Composition facts the layout engine and the renderer must agree on.
//!
//! Both threads resolve a composed cluster's font through the same
//! character, so the rule lives here, below both of them (MILESTONES M4).

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
