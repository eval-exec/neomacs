//! The per-Context lexical-environment assq cache.
//!
//! GNU `eval_sub` walks `Vinternal_interpreter_environment` with `Fassq` on
//! every symbol reference; interpreted code pays that walk repeatedly for the
//! same (environment, symbol) pair.  This cache stores the binding cell, or a
//! negative answer, in a direct-mapped table.
//!
//! The two constants `nil` and `t` are a large share of interpreted reads.
//! GNU consults the environment for them too -- a crafted environment binds
//! them, as in `(eval 'nil '((nil . 42)))` -> 42 -- so they are cached like
//! any other symbol, but in slots no dynamic symbol can evict: the last two
//! entries are theirs and a dynamic bucket that hashes onto them folds back
//! into the dynamic range.  The DEFSYM const prefix gives fixed ids to the
//! most common dynamic names, so the old low-bit index could put a hot loop
//! variable in nil's slot: the regex-search row regressed 0.8% when
//! `matches` did exactly that.
//!
//! One slot computation per lookup.  The old shape classified the symbol
//! (constant vs dynamic) inside `find`, then classified it again inside
//! `push`, and remapped the dynamic bucket with a compare+cmov; on the
//! magit-status row that cost +14 instructions on every one of the cache's
//! ~36K calls.  Both call sites now share a single index: one constant range
//! check, one masked hash, one fold, and the fill reuses the result instead
//! of recomputing it.  The interpreter's lookup is out of line so
//! `eval_symbol_by_id` does not carry either half in its own body, and the
//! table keeps its 16-entry shape so `Context`'s pinned layout does not
//! move.

use std::cell::Cell;

use super::{Context, lexical_symbols};
use crate::emacs_core::intern::{NIL_SYM_ID, SymId, T_SYM_ID};
use crate::emacs_core::value::{Value, lexenv_assq};

/// Fourteen dynamic slots followed by the two constant slots.  The table is
/// still sixteen entries, so the field keeps its place in `Context`; a
/// dynamic bucket that hashes onto the constants folds back into 0..14.
const LEXENV_ASSQ_CACHE_CAPACITY: usize = 16;
const LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS: usize = LEXENV_ASSQ_CACHE_CAPACITY - LexenvConstant::COUNT;

#[derive(Clone, Copy, Debug)]
struct LexenvAssqCacheEntry {
    lexenv_bits: usize,
    symbol: SymId,
    /// The `(symbol . value)` cell this environment binds the symbol to, or
    /// `nil` for "this environment binds it nowhere".  A binding cell is
    /// always a cons, so `nil` cannot be confused for one.
    cell: Value,
}

/// The two Lisp constants whose `SymId` is a fixed seed.  A crafted lexical
/// environment can bind them, so they cannot bypass the assq cache; they do
/// get dedicated cache slots, out of reach of the dynamic symbol population.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LexenvConstant {
    Nil,
    T,
}

impl LexenvConstant {
    /// Number of leading cache entries owned by constants.
    const COUNT: usize = 2;

    #[inline]
    fn from_sym_id(sym_id: SymId) -> Option<Self> {
        // The constant ids are the first seeds.  One unsigned range check is
        // the whole dynamic-symbol rejection: the fast path runs once per
        // interpreted symbol read, so classification stays one compare.
        if sym_id.0 >= Self::COUNT as u32 {
            return None;
        }
        if sym_id == NIL_SYM_ID {
            Some(Self::Nil)
        } else if sym_id == T_SYM_ID {
            Some(Self::T)
        } else {
            None
        }
    }

    #[inline]
    const fn slot(self) -> usize {
        match self {
            Self::Nil => LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS,
            Self::T => LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS + 1,
        }
    }

    #[inline]
    const fn symbol(self) -> SymId {
        match self {
            Self::Nil => NIL_SYM_ID,
            Self::T => T_SYM_ID,
        }
    }
}

// The constant slots are the table's last two entries, the dynamic slots are
// the range below them, and the seed ids the range check assumes are the two
// constants.
const _: () = {
    assert!(LexenvConstant::COUNT == 2);
    assert!(LexenvConstant::Nil.slot() >= LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS);
    assert!(LexenvConstant::T.slot() >= LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS);
    assert!(LexenvConstant::Nil.slot() < LEXENV_ASSQ_CACHE_CAPACITY);
    assert!(LexenvConstant::T.slot() < LEXENV_ASSQ_CACHE_CAPACITY);
    assert!(LexenvConstant::Nil.slot() != LexenvConstant::T.slot());
    assert!(LexenvConstant::T.slot() + 1 == LEXENV_ASSQ_CACHE_CAPACITY);
    assert!(NIL_SYM_ID.0 < LexenvConstant::COUNT as u32);
    assert!(T_SYM_ID.0 < LexenvConstant::COUNT as u32);
    assert!(LEXENV_ASSQ_CACHE_CAPACITY & (LEXENV_ASSQ_CACHE_CAPACITY - 1) == 0);
    assert!(LEXENV_ASSQ_CACHE_CAPACITY == LexenvConstant::COUNT + LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS);
};

/// Cache index for one (environment, symbol) pair, computed once per lookup.
///
/// A constant owns one of the table's last two cells; a dynamic symbol's hash
/// bucket folds onto the fourteen cells below them, so no dynamic symbol can
/// evict a constant's cached answer.  Same rotate/xor/multiply mix as before:
/// one mask, one fold, no second classification and no recomputation in the
/// fill.
#[inline]
fn lexenv_assq_slot(lexenv_bits: usize, sym_id: SymId) -> usize {
    if let Some(constant) = LexenvConstant::from_sym_id(sym_id) {
        return constant.slot();
    }
    let mixed = lexenv_bits.rotate_left(7) ^ (sym_id.0 as usize).wrapping_mul(0x9E37_79B1);
    let bucket = mixed & (LEXENV_ASSQ_CACHE_CAPACITY - 1);
    if bucket < LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS {
        bucket
    } else {
        bucket - LEXENV_ASSQ_CACHE_DYNAMIC_SLOTS
    }
}

pub(crate) struct LexenvAssqCache {
    entries: [Cell<Option<LexenvAssqCacheEntry>>; LEXENV_ASSQ_CACHE_CAPACITY],
}

impl Default for LexenvAssqCache {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| Cell::new(None)),
        }
    }
}

impl LexenvAssqCache {
    #[inline]
    fn find_at(&self, index: usize, lexenv_bits: usize, sym_id: SymId) -> Option<Value> {
        let entry = self.entries[index].get()?;
        (entry.lexenv_bits == lexenv_bits && entry.symbol == sym_id).then_some(entry.cell)
    }

    #[inline]
    fn push_at(&self, index: usize, entry: LexenvAssqCacheEntry) {
        self.entries[index].set(Some(entry));
    }

    #[inline]
    pub(super) fn clear(&self) {
        for entry in &self.entries {
            entry.set(None);
        }
    }
}

impl Context {
    pub(crate) fn lexenv_assq_cached_in(&self, lexenv: Value, sym_id: SymId) -> Option<Value> {
        let lexenv_bits = lexenv.bits();
        let index = lexenv_assq_slot(lexenv_bits, sym_id);
        if let Some(cell) = self.lexenv_assq_cache.find_at(index, lexenv_bits, sym_id) {
            // `nil` is not a binding cell -- `lexenv_assq` only ever returns a
            // cons -- so it is free to record the other answer: this
            // environment has no binding for this symbol.  Absence is the
            // common answer and used to be the one nobody remembered.  Every
            // reference to a dynamically-bound variable in interpreted code
            // walks the environment to its end and finds nothing, and then
            // the next reference to the same variable walks it again; that
            // walk was 130M of the 208M a magit-status run spent resolving
            // symbols.
            return (!cell.is_nil()).then_some(cell);
        }
        self.lexenv_assq_cached_fill(lexenv, lexenv_bits, sym_id, index)
    }

    /// Walk the environment once and record the answer in the slot the caller
    /// already computed.  Kept out of line: the walk is the long tail, and
    /// neither lookup entry point should carry it.
    #[inline(never)]
    fn lexenv_assq_cached_fill(
        &self,
        lexenv: Value,
        lexenv_bits: usize,
        sym_id: SymId,
        index: usize,
    ) -> Option<Value> {
        let cell = if self.symbols_with_pos_enabled {
            lexical_symbols::assq(lexenv, sym_id)
        } else {
            lexenv_assq(lexenv, sym_id)
        };
        self.lexenv_assq_cache.push_at(
            index,
            LexenvAssqCacheEntry {
                lexenv_bits,
                symbol: sym_id,
                cell: cell.unwrap_or(Value::NIL),
            },
        );
        cell
    }

    /// The interpreter's symbol read path.
    ///
    /// Out of line: `eval_symbol_by_id` is instruction-size gated, and the
    /// constant-slot handling that used to sit in its body is one range check
    /// here instead of an inline branch plus a second classification in the
    /// generic cache.
    #[inline(never)]
    pub(crate) fn lexenv_lookup_cached_in(&self, lexenv: Value, sym_id: SymId) -> Option<Value> {
        let lexenv_bits = lexenv.bits();
        let index = lexenv_assq_slot(lexenv_bits, sym_id);
        if let Some(cell) = self.lexenv_assq_cache.find_at(index, lexenv_bits, sym_id) {
            return if cell.is_nil() {
                None
            } else {
                Some(cell.cons_cdr())
            };
        }
        self.lexenv_assq_cached_fill(lexenv, lexenv_bits, sym_id, index)
            .map(|cell| cell.cons_cdr())
    }
}

#[cfg(test)]
#[path = "tests/lexenv_cache_partition_test.rs"]
mod tests;
