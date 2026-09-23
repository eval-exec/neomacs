//! Per-symbol mark bits for one collection cycle.
//!
//! GNU marks a symbol by setting the mark bit in its `struct Lisp_Symbol` and
//! moves on; whether the symbol is interned is a sweep-time question.  The
//! marker used to ask `is_canonical_id` for every symbol reference it met so
//! that only uninterned symbols entered a hash set — an epoch-checked
//! thread-local lookup per visit, which cost more than the marking itself
//! (24 M calls in a 20-keystroke rust-lsp run).  A bit per symbol id makes
//! the mark a shift and an or, and keeps the same answer for the consumers
//! that care (weak hash tables, the dump-partition verifier): a canonical
//! symbol is always live, any other symbol is live when its bit is set.

use crate::emacs_core::intern::SymId;

/// One mark bit per symbol id, grown on demand, cleared at each cycle start.
#[derive(Debug, Default)]
pub(crate) struct SymbolMarkBits {
    words: Vec<u64>,
}

impl SymbolMarkBits {
    #[inline]
    fn coordinates(id: SymId) -> (usize, u64) {
        let index = id.0 as usize;
        (index / 64, 1u64 << (index % 64))
    }

    /// Mark `id` for this cycle.
    #[inline]
    pub(crate) fn insert(&mut self, id: SymId) {
        let (word, bit) = Self::coordinates(id);
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= bit;
    }

    /// Whether `id` was marked this cycle.
    #[inline]
    pub(crate) fn contains(&self, id: SymId) -> bool {
        let (word, bit) = Self::coordinates(id);
        self.words.get(word).is_some_and(|w| w & bit != 0)
    }

    /// Forget every mark, keeping the allocation for the next cycle.
    pub(crate) fn clear(&mut self) {
        self.words.fill(0);
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }
}

#[cfg(test)]
mod tests;
