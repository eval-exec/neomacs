//! Unicode utility functions for the Rust layout engine.
//!
//! Pure functions for UTF-8 decoding, character width classification,
//! grapheme cluster collection, and glyphless character detection.

/// Decode one UTF-8 character from a byte slice.
/// Returns (char, bytes_consumed).
pub(crate) fn decode_utf8(bytes: &[u8]) -> (char, usize) {
    if bytes.is_empty() {
        return ('\0', 0);
    }

    let b0 = bytes[0];
    if b0 < 0x80 {
        (b0 as char, 1)
    } else if b0 < 0xC0 {
        // Invalid continuation byte — treat as replacement
        ('\u{FFFD}', 1)
    } else if b0 < 0xE0 {
        if bytes.len() < 2 {
            return ('\u{FFFD}', 1);
        }
        let cp = ((b0 as u32 & 0x1F) << 6) | (bytes[1] as u32 & 0x3F);
        (char::from_u32(cp).unwrap_or('\u{FFFD}'), 2)
    } else if b0 < 0xF0 {
        if bytes.len() < 3 {
            return ('\u{FFFD}', 1);
        }
        let cp =
            ((b0 as u32 & 0x0F) << 12) | ((bytes[1] as u32 & 0x3F) << 6) | (bytes[2] as u32 & 0x3F);
        (char::from_u32(cp).unwrap_or('\u{FFFD}'), 3)
    } else {
        if bytes.len() < 4 {
            return ('\u{FFFD}', 1);
        }
        let cp = ((b0 as u32 & 0x07) << 18)
            | ((bytes[1] as u32 & 0x3F) << 12)
            | ((bytes[2] as u32 & 0x3F) << 6)
            | (bytes[3] as u32 & 0x3F);
        (char::from_u32(cp).unwrap_or('\u{FFFD}'), 4)
    }
}

/// Check if a character is a wide (CJK, emoji, …) character that
/// occupies 2 display columns.
///
/// Delegates to [`neovm_core::encoding::char_width`] so the layout
/// engine and the elisp `char-width` builtin share the same Unicode
/// tables (transcribed from `lisp/international/characters.el`'s
/// default `char-width-table`). Keep them unified here rather than
/// maintaining a second copy of the ranges.
pub(crate) fn is_wide_char(ch: char) -> bool {
    neovm_core::encoding::char_width(ch) == 2
}

/// Check if a codepoint is an emoji that should have wide (2-column) presentation.
#[cfg(test)]
pub(crate) fn is_emoji_presentation(cp: u32) -> bool {
    // Emoticons
    (0x1F600..=0x1F64F).contains(&cp)
    // Miscellaneous Symbols and Pictographs
    || (0x1F300..=0x1F5FF).contains(&cp)
    // Transport and Map Symbols
    || (0x1F680..=0x1F6FF).contains(&cp)
    // Supplemental Symbols and Pictographs
    || (0x1F900..=0x1F9FF).contains(&cp)
    // Symbols and Pictographs Extended-A
    || (0x1FA00..=0x1FA6F).contains(&cp)
    || (0x1FA70..=0x1FAFF).contains(&cp)
    // Dingbats (selected emoji)
    || (0x2702..=0x27B0).contains(&cp)
    // Regional Indicator Symbols
    || (0x1F1E0..=0x1F1FF).contains(&cp)
    // Playing cards, mahjong, dominos
    || cp == 0x1F004  // mahjong red dragon
    || cp == 0x1F0CF // playing card black joker
    // Skin tone modifiers (display-width 0 when following emoji, but 2 when standalone)
    // We'll treat them as part of clusters, so this is just for standalone
}

/// Check if a character is a grapheme cluster extender: it should be
/// bundled with the preceding base character for proper rendering.
///
/// Delegates to [`neovm_core::encoding::char_width`] for the 0-width
/// check (matching GNU Emacs's default `char-width-table`), plus a
/// few explicit codepoints that GNU does not mark zero-width in the
/// char-width-table but which still cluster with the preceding base
/// character on the terminal: the ZWJ `U+200D`, the Combining
/// Enclosing Keycap `U+20E3`, and the skin-tone Emoji Modifier range
/// `U+1F3FB..U+1F3FF`.
pub(crate) fn is_cluster_extender(ch: char) -> bool {
    if neovm_core::encoding::char_width(ch) == 0 {
        return true;
    }
    let cp = ch as u32;
    cp == 0x200D || cp == 0x20E3 || (0x1F3FB..=0x1F3FF).contains(&cp)
}

/// Check if a codepoint is a Regional Indicator Symbol. A pair of these
/// forms a flag emoji; the layout walk composes the second one into the
/// first's grapheme cluster (see `engine.rs` cluster-continuation logic).
pub(crate) fn is_regional_indicator(cp: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&cp)
}

/// Collect a grapheme cluster starting with the base character `base_ch`.
/// Peeks at subsequent bytes in `remaining` to find cluster extenders.
///
/// Returns (cluster_string, extra_bytes_consumed, extra_chars_consumed).
/// If there are no extenders, returns (None, 0, 0) — use single-char path.
#[cfg(test)]
pub(crate) fn collect_grapheme_cluster(
    base_ch: char,
    remaining: &[u8],
) -> (Option<String>, usize, usize) {
    let mut extra_bytes = 0usize;
    let mut extra_chars = 0usize;
    let mut cluster = String::new();
    cluster.push(base_ch);

    let mut peek = 0usize;
    let base_is_ri = is_regional_indicator(base_ch as u32);

    loop {
        if peek >= remaining.len() {
            break;
        }
        let (next_ch, next_len) = decode_utf8(&remaining[peek..]);

        if next_ch == '\u{200D}' {
            // ZWJ: consume it AND the next character (emoji ZWJ sequence)
            cluster.push(next_ch);
            peek += next_len;
            extra_bytes += next_len;
            extra_chars += 1;

            // Consume the character after ZWJ
            if peek < remaining.len() {
                let (zjoin_ch, zjoin_len) = decode_utf8(&remaining[peek..]);
                cluster.push(zjoin_ch);
                peek += zjoin_len;
                extra_bytes += zjoin_len;
                extra_chars += 1;
            }
        } else if is_cluster_extender(next_ch) && next_ch != '\u{200D}' {
            // Combining mark, variation selector, skin tone modifier, etc.
            cluster.push(next_ch);
            peek += next_len;
            extra_bytes += next_len;
            extra_chars += 1;
        } else if base_is_ri
            && is_regional_indicator(next_ch as u32)
            && cluster.chars().count() == 1
        {
            // Second regional indicator forms a flag pair
            cluster.push(next_ch);
            extra_bytes += next_len;
            extra_chars += 1;
            break; // Flags are exactly 2 regional indicators
        } else {
            break;
        }
    }

    if extra_chars > 0 {
        (Some(cluster), extra_bytes, extra_chars)
    } else {
        (None, 0, 0)
    }
}

/// Check if a character is potentially glyphless and should be looked up
/// in the glyphless-char-display char-table.
/// This is a fast pre-filter — only chars in these ranges trigger the FFI call.
#[cfg(test)]
pub(crate) fn is_potentially_glyphless(ch: char) -> bool {
    let cp = ch as u32;
    // C1 control characters (0x80-0x9F)
    (0x80..=0x9F).contains(&cp)
    // Soft hyphen (sometimes glyphless)
    || cp == 0xAD
    // Unicode format/control characters
    || (0x200B..=0x200F).contains(&cp)  // ZWSP, ZWNJ, ZWJ, LRM, RLM
    || (0x202A..=0x202E).contains(&cp)  // bidi embedding
    || (0x2060..=0x2069).contains(&cp)  // word joiner, invisible separators
    || (0x2028..=0x2029).contains(&cp)  // line/paragraph separator
    || cp == 0xFEFF                      // BOM / ZWNBSP
    || (0xFFF0..=0xFFFD).contains(&cp)  // specials (interlinear annotation, replacement)
    // Emacs raw bytes (BYTE8 encoding: 0x3FFF80..0x3FFFFF)
    || (0x3FFF80..=0x3FFFFF).contains(&cp)
    // Unassigned/private use — only very high ranges
    || (0xE0000..=0xE007F).contains(&cp) // tags block
}

#[cfg(test)]
#[path = "unicode_test.rs"]
mod tests;
