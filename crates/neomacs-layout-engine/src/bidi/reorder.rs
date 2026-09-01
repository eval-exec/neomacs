//! Visual reordering of bidi-resolved text (UAX#9 L2-L4).

use super::tables::bidi_mirror;

/// Reorder characters into visual order based on resolved embedding levels.
///
/// Implements UAX#9 rule L2: reverse each maximal sequence of characters
/// at the same or higher level, starting from the highest level.
///
/// Returns a vector of indices into the original character array, in visual order.
pub fn reorder_visual(levels: &[u8]) -> Vec<usize> {
    let n = levels.len();
    if n == 0 {
        return Vec::new();
    }

    // Create initial logical order
    let mut order: Vec<usize> = (0..n).collect();

    if levels.iter().all(|&l| l == 0) {
        return order; // Fast path: all LTR
    }

    // Find the highest level
    let max_level = *levels.iter().max().unwrap();
    let min_odd_level = {
        let mut min = max_level;
        for &l in levels {
            if l > 0 && l % 2 == 1 && l < min {
                min = l;
            }
        }
        if min.is_multiple_of(2) { min + 1 } else { min }
    };

    // L2: From the highest level down to the lowest odd level,
    // reverse any contiguous sequence of characters at that level or higher.
    let mut level = max_level;
    while level >= min_odd_level {
        let mut i = 0;
        while i < n {
            if levels[order[i]] >= level {
                // Find the end of this run
                let start = i;
                while i < n && levels[order[i]] >= level {
                    i += 1;
                }
                // Reverse the run
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
        if level == 0 {
            break;
        }
        level -= 1;
    }

    order
}

/// Apply character mirroring for RTL characters (L4).
///
/// Returns a new vector of characters with mirrored glyphs where appropriate.
/// Characters at odd embedding levels get their mirrored counterparts.
pub fn apply_mirroring(chars: &[char], levels: &[u8]) -> Vec<char> {
    chars
        .iter()
        .zip(levels.iter())
        .map(|(&ch, &level)| {
            if level % 2 == 1 {
                bidi_mirror(ch).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

/// Reorder a line of text into visual order, applying mirroring.
///
/// This is the high-level entry point for display: given text and resolved levels,
/// produce the visual character sequence.
pub fn reorder_line(chars: &[char], levels: &[u8]) -> Vec<char> {
    let mirrored = apply_mirroring(chars, levels);
    let visual_order = reorder_visual(levels);
    visual_order.iter().map(|&i| mirrored[i]).collect()
}

#[cfg(test)]
#[path = "reorder_test.rs"]
mod tests;
