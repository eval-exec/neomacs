//! Scan a folded two-character literal by its ASCII terminal character.
//!
//! GNU `search.c:search_buffer` sends trivial regexps to its literal search,
//! whose Boyer-Moore path can skip repeated non-ASCII prefix characters. Our
//! equivalent narrow path keeps the matcher's character predicate and scans
//! the terminal ASCII sources with memchr instead of trying every prefix.
//!
//! | Knob | Default | Meaning |
//! |---|---|---|
//! | `NEOVM_REGEX_SUFFIX_LITERAL` | off (`=on` enables) | Derive a suffix scanner when compiling a folded, multibyte two-character literal with a non-ASCII prefix and ASCII suffix; forward multibyte searches spanning at least 256 bytes use it while the case translation keeps ASCII and non-ASCII separate. |

use super::{
    CASE_TRANSLATION_UNFILLED, CaseTranslation, CharTableWalk, CompiledPattern, MatchRegisters,
    RegexOp, SparseAsciiFastmap, char_table_folds_into_ascii, emacs_char, match_exactn_char_at,
    re_prev_char_start,
};

/// Immutable literal bytes plus lazily derived ASCII source bytes.
///
/// Threading assumption: this belongs to one evaluator's `Rc<CompiledPattern>`,
/// like its `CaseTranslation`. The `Cell` is deliberately not `Sync`;
/// mutators must not share a compiled pattern without runtime synchronization.
/// There is no global or thread-local Lisp-state cache. Table mutations are
/// checked through the global char-table write tick at every search. Proof
/// and source derivation peek at unfilled byte slots without filling them;
/// only actual Exactn matching freezes encountered byte translations.
#[derive(Clone)]
pub(super) struct SuffixLiteral {
    literal: [u8; emacs_char::MAX_MULTIBYTE_LENGTH + 1],
    len: usize,
    memo: std::cell::Cell<Option<SuffixMemo>>,
}

/// Sources and their ASCII-separation proof at one char-table write tick.
/// This contains only integer/byte data and lives in its owner's non-Sync
/// Cell; it is never published to another mutator or shared globally.
#[derive(Clone, Copy)]
struct SuffixMemo {
    tick: u64,
    sources: Option<SparseAsciiFastmap>,
}

/// Distinguish an ineligible table from a completed unsuccessful search.
pub(super) enum SuffixSearch {
    Unavailable,
    Finished(Option<(usize, MatchRegisters)>),
}

/// Read once per process; only regexp compilation consults this knob.
/// Threading assumption: the immutable bool is published by `OnceLock` and
/// contains no Lisp state, so every mutator may read it concurrently.
pub(super) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = super::regex_knob_on(std::env::var("NEOVM_REGEX_SUFFIX_LITERAL").ok().as_deref());
        tracing::debug!(target: "neovm::regex", on, "NEOVM_REGEX_SUFFIX_LITERAL");
        on
    })
}

/// Admit only an entire non-ASCII + ASCII literal. Two characters bound
/// candidate rewind and verification work independently of pattern length;
/// a longer prefix against suffix-dense text could otherwise cost O(n*m).
/// `on` is explicit so tests force this path without changing process state.
#[cold]
#[inline(never)]
pub(super) fn derive(pattern: &CompiledPattern, on: bool) -> Option<SuffixLiteral> {
    if !on || !pattern.multibyte || pattern.translate.is_none() || pattern.re_nsub != 0 {
        return None;
    }
    let mut literal = [0; emacs_char::MAX_MULTIBYTE_LENGTH + 1];
    let mut len = 0usize;
    let mut first_run_len = 0usize;
    let mut pc = 0usize;
    loop {
        match RegexOp::from_byte(*pattern.buffer.get(pc)?)? {
            RegexOp::Exactn => {
                let count = *pattern.buffer.get(pc + 1)? as usize;
                let bytes = pattern.buffer.get(pc + 2..pc + 2 + count)?;
                if count == 0 || len + count > literal.len() {
                    return None;
                }
                if len == 0 {
                    first_run_len = count;
                }
                // Compiler runs end at character boundaries. Fail closed for
                // a hand-built run that splits a multibyte character.
                let mut at = 0;
                while at < count {
                    let (_, advance) = emacs_char::string_char(&bytes[at..]);
                    if advance == 0 || advance > count - at {
                        return None;
                    }
                    at += advance;
                }
                literal[len..len + count].copy_from_slice(bytes);
                len += count;
                pc += 2 + count;
            }
            RegexOp::Succeed | RegexOp::PosixEnd if pc + 1 == pattern.buffer.len() => break,
            _ => return None,
        }
    }
    if len == 0 {
        return None;
    }
    let (first, first_len) = emacs_char::string_char(&literal[..len]);
    if first < 0x80
        || first_len > first_run_len
        || first_len + 1 != len
        || literal[first_len] >= 0x80
    {
        return None;
    }
    Some(SuffixLiteral {
        literal,
        len,
        memo: std::cell::Cell::new(None),
    })
}

impl SuffixLiteral {
    #[inline]
    pub(super) fn search(
        &self,
        pattern: &CompiledPattern,
        text: &[u8],
        start: usize,
        stop: usize,
    ) -> SuffixSearch {
        let Some(table) = pattern.translate.as_ref() else {
            return SuffixSearch::Unavailable;
        };
        if start > stop || stop > text.len() {
            return SuffixSearch::Unavailable;
        }
        let tick = crate::emacs_core::chartable::char_table_write_tick();
        let sources = match self.memo.get() {
            Some(memo) if memo.tick == tick => memo.sources,
            _ => self.refresh_sources(table, tick),
        };
        let Some(sources) = sources else {
            return SuffixSearch::Unavailable;
        };
        let mut next = start;
        while next < stop {
            let window = &text[next..stop];
            let found = match sources {
                SparseAsciiFastmap::One(b0) => memchr::memchr(b0, window),
                SparseAsciiFastmap::Two(b0, b1) => memchr::memchr2(b0, b1, window),
                SparseAsciiFastmap::Three(b0, b1, b2) => memchr::memchr3(b0, b1, b2, window),
            };
            let Some(relative) = found else {
                return SuffixSearch::Finished(None);
            };
            let suffix_at = next + relative;
            next = suffix_at + 1;
            // The first canonical character is non-ASCII. Under AsciiOnly
            // its source must be non-ASCII too: suffix-dense ASCII stretches
            // can therefore reject each hit without decoding or translating.
            if suffix_at == 0 || text[suffix_at - 1] < 0x80 {
                continue;
            }
            let Some(candidate) = re_prev_char_start(text, suffix_at, true) else {
                continue;
            };
            if candidate < start {
                continue;
            }
            let Some((pattern_advance, text_advance)) = match_exactn_char_at(
                &self.literal[..self.len],
                0,
                true,
                true,
                &pattern.translate,
                text,
                candidate,
                stop,
            ) else {
                continue;
            };
            if candidate + text_advance != suffix_at {
                continue;
            }
            // Keep terminal verification on the Exactn predicate too. Every
            // accepted source is ASCII and consumes one byte, but this avoids
            // a second implementation of the translation's match semantics.
            let Some((last_pattern, last_text)) = match_exactn_char_at(
                &self.literal[..self.len],
                pattern_advance,
                true,
                true,
                &pattern.translate,
                text,
                suffix_at,
                stop,
            ) else {
                continue;
            };
            if pattern_advance + last_pattern != self.len {
                continue;
            }
            let mut regs = MatchRegisters::new(1);
            regs.start[0] = candidate as i64;
            regs.end[0] = (suffix_at + last_text) as i64;
            return SuffixSearch::Finished(Some((candidate, regs)));
        }
        SuffixSearch::Finished(None)
    }

    /// A failed suffix scan must not freeze byte translations it never
    /// matches. The ordinary short-search path has not encountered those
    /// bytes either, and a later table edit must still affect them.
    #[cold]
    #[inline(never)]
    fn refresh_sources(&self, table: &CaseTranslation, tick: u64) -> Option<SparseAsciiFastmap> {
        let sources = self.prove_sources(table, tick);
        self.memo.set(Some(SuffixMemo { tick, sources }));
        sources
    }

    #[cold]
    #[inline(never)]
    fn prove_sources(&self, table: &CaseTranslation, tick: u64) -> Option<SparseAsciiFastmap> {
        if let Some(char_table) = table.table {
            // Read an existing whole-table proof, but do not publish a new
            // one into the ordinary scan's cache. Doing so could make its
            // next short search eagerly fill every ASCII byte memo slot.
            let folds = char_table_folds_into_ascii(&char_table, tick, CharTableWalk::OnlyIfWalked)
                .unwrap_or_else(|| {
                    crate::emacs_core::chartable::char_table_may_hold_value_from(
                        &char_table,
                        0x80,
                        &mut |value| {
                            value
                                .as_fixnum()
                                .is_some_and(|code| (0..0x80).contains(&code))
                        },
                    )
                });
            if folds {
                return None;
            }
        }
        // Frozen Latin-1 translations remain the matcher's predicate even
        // when a table edit removes that translation from the live table.
        if table.byte[0x80..].iter().any(|slot| {
            let translated = slot.get();
            translated != CASE_TRANSLATION_UNFILLED && translated < 0x80
        }) {
            return None;
        }
        let suffix = self.literal[self.len - 1] as u32;
        let mut accepted = [false; 256];
        for byte in 0..0x80u8 {
            let translated = peek_byte(table, byte);
            if translated >= 0x80 {
                return None;
            }
            accepted[byte as usize] = translated == suffix;
        }
        super::sparse_ascii_fastmap(&accepted)
    }
}

/// Read the translation Exactn would use, preserving an unfilled byte slot.
#[inline]
fn peek_byte(table: &CaseTranslation, byte: u8) -> u32 {
    let cached = table.byte[byte as usize].get();
    if cached != CASE_TRANSLATION_UNFILLED {
        return cached;
    }
    match table.table {
        Some(char_table) => {
            crate::emacs_core::chartable::translate_char(&char_table, byte as i64) as u32
        }
        None => CaseTranslation::canonicalize_char(byte as u32),
    }
}
