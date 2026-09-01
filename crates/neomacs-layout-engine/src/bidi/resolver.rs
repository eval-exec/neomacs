//! Unicode Bidirectional Algorithm (UAX#9) resolver.
//!
//! Resolves embedding levels for a sequence of characters according to
//! the Unicode Bidirectional Algorithm. Processes text line-by-line.

use super::tables::{bidi_class, bracket_type, canonical_bracket};
use super::types::*;

/// Resolve bidi embedding levels for a line of text.
///
/// Returns a vector of resolved embedding levels, one per character in `text`.
/// `base_dir` specifies the paragraph direction (LTR, RTL, or Auto for P2-P3).
pub fn resolve_levels(text: &str, base_dir: BidiDir) -> Vec<u8> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut classes: Vec<BidiClass> = chars.iter().map(|&ch| bidi_class(ch)).collect();
    let original_classes: Vec<BidiClass> = classes.clone();

    // P2-P3: Determine paragraph level
    let paragraph_level = match base_dir {
        BidiDir::LTR => 0,
        BidiDir::RTL => 1,
        BidiDir::Auto => determine_paragraph_level(&classes),
    };

    // X1-X8: Resolve explicit embedding levels
    let mut levels = resolve_explicit(&mut classes, paragraph_level);

    // X9: Remove explicit formatting characters from consideration
    // (We keep them but mark them as BN for algorithm purposes)
    for i in 0..classes.len() {
        if original_classes[i].is_removed_by_x9() {
            classes[i] = BidiClass::BN;
            levels[i] = levels
                .get(i.wrapping_sub(1))
                .copied()
                .unwrap_or(paragraph_level);
        }
    }

    // Find isolating run sequences and process W/N/I rules for each
    let sequences = find_run_sequences(&levels, &classes, paragraph_level);
    for seq in &sequences {
        // W1-W7: Resolve weak types
        resolve_weak(&mut classes, seq, &original_classes);

        // N0: Resolve paired brackets
        resolve_brackets(&mut classes, &chars, seq, &levels, seq.sos);

        // N1-N2: Resolve neutrals
        resolve_neutral(&mut classes, seq);
    }

    // I1-I2: Resolve implicit levels
    resolve_implicit(&mut levels, &classes, paragraph_level);

    // L1: Reset whitespace/separator levels at line end
    resolve_whitespace(&mut levels, &original_classes, paragraph_level);

    levels
}

/// Paragraph embedding level (0 = left-to-right, 1 = right-to-left) that
/// [`resolve_levels`] uses for `text` under `base_dir`. Exposed so the
/// glyph-row reorderer can right-align right-to-left paragraphs the way GNU
/// displays them with reversed (`reversed_p`) rows flush to the right margin.
pub fn paragraph_base_level(text: &str, base_dir: BidiDir) -> u8 {
    match base_dir {
        BidiDir::LTR => 0,
        BidiDir::RTL => 1,
        BidiDir::Auto => {
            let classes: Vec<BidiClass> = text.chars().map(bidi_class).collect();
            determine_paragraph_level(&classes)
        }
    }
}

/// P2-P3: Determine paragraph embedding level from first strong character.
fn determine_paragraph_level(classes: &[BidiClass]) -> u8 {
    let mut isolate_depth = 0u32;
    for &class in classes {
        match class {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => {
                isolate_depth += 1;
            }
            BidiClass::PDI => {
                isolate_depth = isolate_depth.saturating_sub(1);
            }
            BidiClass::L if isolate_depth == 0 => return 0,
            BidiClass::R | BidiClass::AL if isolate_depth == 0 => return 1,
            _ => {}
        }
    }
    0 // Default to LTR
}

/// X1-X8: Resolve explicit embedding levels.
fn resolve_explicit(classes: &mut [BidiClass], paragraph_level: u8) -> Vec<u8> {
    let n = classes.len();
    let mut levels = vec![paragraph_level; n];

    let mut stack: Vec<DirectionalStatus> = Vec::with_capacity(MAX_DEPTH as usize + 2);
    stack.push(DirectionalStatus {
        level: paragraph_level,
        override_status: Override::Neutral,
        isolate_status: false,
    });

    let mut overflow_isolate_count: u32 = 0;
    let mut overflow_embedding_count: u32 = 0;
    let mut valid_isolate_count: u32 = 0;

    for i in 0..n {
        let class = classes[i];
        let current = stack.last().unwrap();
        let current_level = current.level;
        let current_override = current.override_status;

        match class {
            // X2: RLE
            BidiClass::RLE => {
                let new_level = least_odd_greater_than(current_level);
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::Neutral,
                        isolate_status: false,
                    });
                } else if overflow_isolate_count == 0 {
                    overflow_embedding_count += 1;
                }
                levels[i] = current_level;
            }
            // X3: LRE
            BidiClass::LRE => {
                let new_level = least_even_greater_than(current_level);
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::Neutral,
                        isolate_status: false,
                    });
                } else if overflow_isolate_count == 0 {
                    overflow_embedding_count += 1;
                }
                levels[i] = current_level;
            }
            // X4: RLO
            BidiClass::RLO => {
                let new_level = least_odd_greater_than(current_level);
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::RTL,
                        isolate_status: false,
                    });
                } else if overflow_isolate_count == 0 {
                    overflow_embedding_count += 1;
                }
                levels[i] = current_level;
            }
            // X5: LRO
            BidiClass::LRO => {
                let new_level = least_even_greater_than(current_level);
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::LTR,
                        isolate_status: false,
                    });
                } else if overflow_isolate_count == 0 {
                    overflow_embedding_count += 1;
                }
                levels[i] = current_level;
            }
            // X5a: RLI
            BidiClass::RLI => {
                levels[i] = current_level;
                if current_override == Override::LTR {
                    classes[i] = BidiClass::L;
                } else if current_override == Override::RTL {
                    classes[i] = BidiClass::R;
                }
                let new_level = least_odd_greater_than(current_level);
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    valid_isolate_count += 1;
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::Neutral,
                        isolate_status: true,
                    });
                } else {
                    overflow_isolate_count += 1;
                }
            }
            // X5b: LRI
            BidiClass::LRI => {
                levels[i] = current_level;
                if current_override == Override::LTR {
                    classes[i] = BidiClass::L;
                } else if current_override == Override::RTL {
                    classes[i] = BidiClass::R;
                }
                let new_level = least_even_greater_than(current_level);
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    valid_isolate_count += 1;
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::Neutral,
                        isolate_status: true,
                    });
                } else {
                    overflow_isolate_count += 1;
                }
            }
            // X5c: FSI — determine direction from content, then act as LRI or RLI
            BidiClass::FSI => {
                let sub_level = determine_fsi_level(classes, i + 1);
                levels[i] = current_level;
                if current_override == Override::LTR {
                    classes[i] = BidiClass::L;
                } else if current_override == Override::RTL {
                    classes[i] = BidiClass::R;
                }
                let new_level = if sub_level == 1 {
                    least_odd_greater_than(current_level)
                } else {
                    least_even_greater_than(current_level)
                };
                if new_level <= MAX_DEPTH
                    && overflow_isolate_count == 0
                    && overflow_embedding_count == 0
                {
                    valid_isolate_count += 1;
                    stack.push(DirectionalStatus {
                        level: new_level,
                        override_status: Override::Neutral,
                        isolate_status: true,
                    });
                } else {
                    overflow_isolate_count += 1;
                }
            }
            // X6a: PDI
            BidiClass::PDI => {
                if overflow_isolate_count > 0 {
                    overflow_isolate_count -= 1;
                } else if valid_isolate_count > 0 {
                    overflow_embedding_count = 0;
                    // Pop until isolate status entry found
                    while stack.len() > 1 && !stack.last().unwrap().isolate_status {
                        stack.pop();
                    }
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    valid_isolate_count -= 1;
                }
                let current = stack.last().unwrap();
                levels[i] = current.level;
                if current.override_status == Override::LTR {
                    classes[i] = BidiClass::L;
                } else if current.override_status == Override::RTL {
                    classes[i] = BidiClass::R;
                }
            }
            // X7: PDF
            BidiClass::PDF => {
                if overflow_isolate_count > 0 {
                    // Do nothing
                } else if overflow_embedding_count > 0 {
                    overflow_embedding_count -= 1;
                } else if stack.len() > 1 && !stack.last().unwrap().isolate_status {
                    stack.pop();
                }
                levels[i] = stack.last().unwrap().level;
            }
            // X6: All other characters
            _ => {
                levels[i] = current_level;
                if current_override == Override::LTR {
                    classes[i] = BidiClass::L;
                } else if current_override == Override::RTL {
                    classes[i] = BidiClass::R;
                }
            }
        }
    }

    levels
}

/// Determine the embedding level for an FSI from the text following it.
fn determine_fsi_level(classes: &[BidiClass], start: usize) -> u8 {
    let mut isolate_depth = 0u32;
    for &class in &classes[start..] {
        match class {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => {
                isolate_depth += 1;
            }
            BidiClass::PDI => {
                if isolate_depth > 0 {
                    isolate_depth -= 1;
                } else {
                    // Matching PDI for the FSI
                    break;
                }
            }
            BidiClass::L if isolate_depth == 0 => return 0,
            BidiClass::R | BidiClass::AL if isolate_depth == 0 => return 1,
            _ => {}
        }
    }
    0 // Default LTR
}

/// An isolating run sequence — a maximal sequence of level runs at the same level,
/// connected by isolate initiator/PDI pairs.
struct RunSequence {
    /// Indices into the character array, in logical order.
    runs: Vec<BidiRun>,
    /// Level of this sequence.
    level: u8,
    /// Start-of-sequence type (sos).
    sos: BidiClass,
    /// End-of-sequence type (eos).
    eos: BidiClass,
}

impl RunSequence {
    /// Iterate over character indices in this sequence.
    fn indices(&self) -> Vec<usize> {
        let mut result = Vec::new();
        for run in &self.runs {
            for i in run.start..run.end {
                result.push(i);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BidiRun {
    start: usize,
    end: usize,
    level: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BracketPair {
    open_seq: usize,
    close_seq: usize,
}

/// Find level runs and group them into isolating run sequences (X10).
fn find_run_sequences(
    levels: &[u8],
    classes: &[BidiClass],
    paragraph_level: u8,
) -> Vec<RunSequence> {
    let n = levels.len();
    if n == 0 {
        return Vec::new();
    }

    // Find level runs: maximal subsequences at same level
    let mut runs: Vec<BidiRun> = Vec::new();
    let mut run_start = 0;
    for i in 1..=n {
        if i == n || levels[i] != levels[run_start] {
            runs.push(BidiRun {
                start: run_start,
                end: i,
                level: levels[run_start],
            });
            if i < n {
                run_start = i;
            }
        }
    }

    // For simplicity, treat each level run as its own isolating run sequence.
    // Full UAX#9 groups runs connected by isolate initiator/PDI pairs,
    // but for most practical text this simpler approach works correctly.
    let mut sequences = Vec::new();
    for (idx, run) in runs.iter().enumerate() {
        // Determine sos: max(level, preceding level)
        let prev_level = if run.start == 0 {
            paragraph_level
        } else {
            levels[run.start - 1]
        };
        let sos = if run.level.max(prev_level) % 2 == 0 {
            BidiClass::L
        } else {
            BidiClass::R
        };

        // Determine eos: max(level, following level)
        let next_level = if run.end >= n {
            paragraph_level
        } else {
            levels[run.end]
        };
        let eos = if run.level.max(next_level) % 2 == 0 {
            BidiClass::L
        } else {
            BidiClass::R
        };

        // Skip runs that are entirely X9-removed characters
        let has_content = (run.start..run.end).any(|i| !classes[i].is_removed_by_x9());
        if has_content || idx == runs.len() - 1 {
            sequences.push(RunSequence {
                runs: vec![*run],
                level: run.level,
                sos,
                eos,
            });
        }
    }

    sequences
}

/// W1-W7: Resolve weak types within an isolating run sequence.
fn resolve_weak(classes: &mut [BidiClass], seq: &RunSequence, _original_classes: &[BidiClass]) {
    let indices = seq.indices();
    if indices.is_empty() {
        return;
    }

    // W1: NSM → type of preceding character (or sos if at start)
    let mut prev_type = seq.sos;
    for &i in &indices {
        if classes[i] == BidiClass::NSM {
            match prev_type {
                BidiClass::LRI | BidiClass::RLI | BidiClass::FSI | BidiClass::PDI => {
                    classes[i] = BidiClass::ON;
                }
                _ => {
                    classes[i] = prev_type;
                }
            }
        }
        prev_type = classes[i];
    }

    // W2: EN → AN when preceding strong type is AL
    let mut last_strong = seq.sos;
    for &i in &indices {
        match classes[i] {
            BidiClass::L | BidiClass::R | BidiClass::AL => {
                last_strong = classes[i];
            }
            BidiClass::EN if last_strong == BidiClass::AL => {
                classes[i] = BidiClass::AN;
            }
            _ => {}
        }
    }

    // W3: AL → R
    for &i in &indices {
        if classes[i] == BidiClass::AL {
            classes[i] = BidiClass::R;
        }
    }

    // W4: Single ES between EN → EN; single CS between same number type → that type
    for idx in 1..indices.len().saturating_sub(1) {
        let i = indices[idx];
        let prev_i = indices[idx - 1];
        let next_i = indices[idx + 1];
        match classes[i] {
            BidiClass::ES
                if classes[prev_i] == BidiClass::EN && classes[next_i] == BidiClass::EN =>
            {
                classes[i] = BidiClass::EN;
            }
            BidiClass::CS => {
                if classes[prev_i] == BidiClass::EN && classes[next_i] == BidiClass::EN {
                    classes[i] = BidiClass::EN;
                } else if classes[prev_i] == BidiClass::AN && classes[next_i] == BidiClass::AN {
                    classes[i] = BidiClass::AN;
                }
            }
            _ => {}
        }
    }

    // W5: ET adjacent to EN → EN
    // Forward pass: ET after EN → EN
    let mut prev_en = false;
    for &i in &indices {
        match classes[i] {
            BidiClass::EN => {
                prev_en = true;
            }
            BidiClass::ET if prev_en => {
                classes[i] = BidiClass::EN;
            }
            _ => {
                prev_en = false;
            }
        }
    }
    // Backward pass: ET before EN → EN
    let mut next_en = false;
    for &i in indices.iter().rev() {
        match classes[i] {
            BidiClass::EN => {
                next_en = true;
            }
            BidiClass::ET if next_en => {
                classes[i] = BidiClass::EN;
            }
            _ => {
                next_en = false;
            }
        }
    }

    // W6: ES, ET, CS → ON (remaining ones not converted by W4/W5)
    for &i in &indices {
        match classes[i] {
            BidiClass::ES | BidiClass::ET | BidiClass::CS => {
                classes[i] = BidiClass::ON;
            }
            _ => {}
        }
    }

    // W7: EN → L when last strong type is L (or sos is L)
    let mut last_strong = seq.sos;
    for &i in &indices {
        match classes[i] {
            BidiClass::L | BidiClass::R => {
                last_strong = classes[i];
            }
            BidiClass::EN if last_strong == BidiClass::L => {
                classes[i] = BidiClass::L;
            }
            _ => {}
        }
    }
}

/// N0: Resolve paired brackets using the Paired Bracket Algorithm (BPA).
fn resolve_brackets(
    classes: &mut [BidiClass],
    chars: &[char],
    seq: &RunSequence,
    _levels: &[u8],
    sos: BidiClass,
) {
    let indices = seq.indices();
    if indices.is_empty() {
        return;
    }

    // BPA stack: (index_in_indices, canonical_closing_bracket)
    let mut stack: Vec<(usize, char)> = Vec::new();
    let mut pairs: Vec<BracketPair> = Vec::new();

    for (seq_idx, &char_idx) in indices.iter().enumerate() {
        let ch = canonical_bracket(chars[char_idx]);
        match bracket_type(ch) {
            BracketType::Open(close) => {
                if stack.len() >= MAX_BPA_STACK {
                    break; // Stack overflow — stop processing
                }
                stack.push((seq_idx, close));
            }
            BracketType::Close(_open) => {
                // Find matching opening bracket on stack
                let mut found = None;
                for j in (0..stack.len()).rev() {
                    if stack[j].1 == ch {
                        found = Some(j);
                        break;
                    }
                }
                if let Some(j) = found {
                    pairs.push(BracketPair {
                        open_seq: stack[j].0,
                        close_seq: seq_idx,
                    });
                    stack.truncate(j); // Pop everything above
                }
            }
            BracketType::None => {}
        }
    }

    // Sort pairs by opening position
    pairs.sort_by_key(|pair| pair.open_seq);

    // Resolve each pair per N0b-d
    let embedding_dir = if seq.level.is_multiple_of(2) {
        BidiClass::L
    } else {
        BidiClass::R
    };

    for pair in &pairs {
        let open_seq = pair.open_seq;
        let close_seq = pair.close_seq;
        let open_idx = indices[open_seq];
        let close_idx = indices[close_seq];

        // Find strong types inside the bracket pair
        let mut inside_strong = None;
        for &i in &indices[(open_seq + 1)..close_seq] {
            let strong = classes[i].to_strong_for_neutral();
            if strong == BidiClass::L || strong == BidiClass::R {
                if inside_strong.is_none() {
                    inside_strong = Some(strong);
                } else if inside_strong != Some(strong) {
                    // Mixed — have both L and R inside
                    inside_strong = Some(if embedding_dir == BidiClass::L {
                        BidiClass::L
                    } else {
                        BidiClass::R
                    });
                    break;
                }
            }
        }

        // N0b: If strong type inside matches embedding direction
        if inside_strong == Some(embedding_dir) {
            classes[open_idx] = embedding_dir;
            classes[close_idx] = embedding_dir;
            continue;
        }

        // N0c: If strong type inside is opposite to embedding direction
        if let Some(strong) = inside_strong
            && strong != embedding_dir
        {
            // Check context before the opening bracket
            let mut context_dir = sos;
            for &i in indices[..open_seq].iter().rev() {
                let s = classes[i].to_strong_for_neutral();
                if s == BidiClass::L || s == BidiClass::R {
                    context_dir = s;
                    break;
                }
            }
            let resolved = if context_dir == embedding_dir {
                embedding_dir
            } else {
                strong
            };
            classes[open_idx] = resolved;
            classes[close_idx] = resolved;
            continue;
        }

        // N0d: No strong type inside — leave as ON
    }
}

/// N1-N2: Resolve neutral types.
fn resolve_neutral(classes: &mut [BidiClass], seq: &RunSequence) {
    let indices = seq.indices();
    if indices.is_empty() {
        return;
    }

    // N1: Between two strong types of the same direction → that direction
    // N2: Otherwise → embedding direction

    let embedding_dir = if seq.level.is_multiple_of(2) {
        BidiClass::L
    } else {
        BidiClass::R
    };

    // Find runs of neutrals and BNs, then resolve them
    let mut i = 0;
    while i < indices.len() {
        let class = classes[indices[i]];
        if class == BidiClass::ON
            || class == BidiClass::WS
            || class == BidiClass::B
            || class == BidiClass::S
            || class == BidiClass::BN
        {
            // Start of neutral run
            let run_start = i;

            // Find end of neutral run
            while i < indices.len() {
                let c = classes[indices[i]];
                if c != BidiClass::ON
                    && c != BidiClass::WS
                    && c != BidiClass::B
                    && c != BidiClass::S
                    && c != BidiClass::BN
                {
                    break;
                }
                i += 1;
            }

            // Find preceding strong type (or sos)
            let prev_strong = if run_start > 0 {
                classes[indices[run_start - 1]].to_strong_for_neutral()
            } else {
                seq.sos
            };

            // Find following strong type (or eos)
            let next_strong = if i < indices.len() {
                classes[indices[i]].to_strong_for_neutral()
            } else {
                seq.eos
            };

            // N1: If surrounding strong types match, use that type
            // N2: Otherwise use embedding direction
            let resolved = if prev_strong == next_strong {
                prev_strong
            } else {
                embedding_dir
            };

            // Apply to all neutrals in this run
            for j in run_start..i {
                classes[indices[j]] = resolved;
            }
        } else {
            i += 1;
        }
    }
}

/// I1-I2: Resolve implicit embedding levels.
fn resolve_implicit(levels: &mut [u8], classes: &[BidiClass], _paragraph_level: u8) {
    for i in 0..levels.len() {
        let level = levels[i];
        let class = classes[i];

        if level.is_multiple_of(2) {
            // I1: Even level
            match class {
                BidiClass::R => levels[i] = level + 1,
                BidiClass::AN | BidiClass::EN => levels[i] = level + 2,
                _ => {}
            }
        } else {
            // I2: Odd level
            match class {
                BidiClass::L | BidiClass::AN | BidiClass::EN => levels[i] = level + 1,
                _ => {}
            }
        }
    }
}

/// L1: Reset whitespace and separator levels at line end.
fn resolve_whitespace(levels: &mut [u8], original_classes: &[BidiClass], paragraph_level: u8) {
    // Scan from end backward, resetting WS/isolate/format types to paragraph level
    let mut reset = true; // Start true because we're at line end
    for i in (0..levels.len()).rev() {
        let class = original_classes[i];
        match class {
            BidiClass::B | BidiClass::S => {
                levels[i] = paragraph_level;
                reset = true;
            }
            BidiClass::WS | BidiClass::LRI | BidiClass::RLI | BidiClass::FSI | BidiClass::PDI => {
                if reset {
                    levels[i] = paragraph_level;
                }
            }
            BidiClass::BN
            | BidiClass::LRE
            | BidiClass::RLE
            | BidiClass::LRO
            | BidiClass::RLO
            | BidiClass::PDF => {
                // X9-removed chars: if adjacent to reset chars, also reset
                if reset {
                    levels[i] = paragraph_level;
                }
            }
            _ => {
                reset = false;
            }
        }
    }
}

/// Compute the least greater odd level.
fn least_odd_greater_than(level: u8) -> u8 {
    (level + 1) | 1
}

/// Compute the least greater even level.
fn least_even_greater_than(level: u8) -> u8 {
    (level + 2) & !1
}

#[cfg(test)]
#[path = "resolver_test.rs"]
mod tests;
