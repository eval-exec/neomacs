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
//! any other symbol, but in slots no dynamic symbol can evict.  The DEFSYM
//! const prefix gives fixed ids to the most common dynamic names, so the old
//! low-bit index could put a hot loop variable in nil's slot: the
//! regex-search row regressed 0.8% when `matches` did exactly that.

use std::cell::Cell;

use super::{Context, lexical_symbols};
use crate::emacs_core::intern::{NIL_SYM_ID, SymId, T_SYM_ID};
use crate::emacs_core::value::{Value, lexenv_assq};

const LEXENV_ASSQ_CACHE_CAPACITY: usize = 16;

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
            Self::Nil => 0,
            Self::T => 1,
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

// The constant slots are dense and immediately precede every dynamic slot.
const _: () = {
    assert!(LexenvConstant::Nil.slot() < LexenvConstant::COUNT);
    assert!(LexenvConstant::T.slot() < LexenvConstant::COUNT);
    assert!(LexenvConstant::Nil.slot() != LexenvConstant::T.slot());
    assert!(LexenvConstant::T.slot() + 1 == LexenvConstant::COUNT);
};

/// Cache index owned by an ordinary symbol.  The private field is only built
/// through [`Self::from_mixed`], which folds the hash away from the constant
/// slots, so no dynamic symbol can evict a [`LexenvConstant`] entry.
#[derive(Clone, Copy)]
struct DynamicLexenvAssqSlot(usize);

impl DynamicLexenvAssqSlot {
    #[inline]
    fn from_mixed(mixed: usize) -> Self {
        let index = mixed & (LEXENV_ASSQ_CACHE_CAPACITY - 1);
        Self(if index < LexenvConstant::COUNT {
            index + LexenvConstant::COUNT
        } else {
            index
        })
    }

    #[inline]
    const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy)]
enum LexenvAssqCacheSlot {
    Constant(LexenvConstant),
    Dynamic(DynamicLexenvAssqSlot),
}

impl LexenvAssqCacheSlot {
    #[inline]
    fn of(lexenv_bits: usize, sym_id: SymId) -> Self {
        if let Some(constant) = LexenvConstant::from_sym_id(sym_id) {
            return Self::Constant(constant);
        }
        let mixed = lexenv_bits.rotate_left(7) ^ (sym_id.0 as usize).wrapping_mul(0x9E37_79B1);
        Self::Dynamic(DynamicLexenvAssqSlot::from_mixed(mixed))
    }

    #[inline]
    const fn index(self) -> usize {
        match self {
            Self::Constant(constant) => constant.slot(),
            Self::Dynamic(slot) => slot.index(),
        }
    }
}

/// Answer of the constant fast path in
/// [`Context::lexenv_lookup_cached_in`].
#[derive(Clone, Copy)]
enum LexenvConstantLookup {
    /// This environment binds the constant nowhere: read its dynamic value.
    Absent,
    /// The environment's binding cell, projected to its value.
    Bound(Value),
    /// No cached answer for this environment: run the assq walk.
    CacheMiss,
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
    fn find(&self, lexenv_bits: usize, sym_id: SymId) -> Option<Value> {
        let entry = self.entries[LexenvAssqCacheSlot::of(lexenv_bits, sym_id).index()].get()?;
        (entry.lexenv_bits == lexenv_bits && entry.symbol == sym_id).then_some(entry.cell)
    }

    /// Cached answer for a constant symbol without the generic-assq call.
    /// The slot belongs to `constant` alone, but the entry tag is compared in
    /// full: the cache is cleared on sweep, not per environment.
    #[inline]
    fn constant_lookup(
        &self,
        lexenv_bits: usize,
        constant: LexenvConstant,
    ) -> LexenvConstantLookup {
        let Some(entry) = self.entries[constant.slot()].get() else {
            return LexenvConstantLookup::CacheMiss;
        };
        if entry.lexenv_bits != lexenv_bits || entry.symbol != constant.symbol() {
            return LexenvConstantLookup::CacheMiss;
        }
        if entry.cell.is_nil() {
            LexenvConstantLookup::Absent
        } else {
            LexenvConstantLookup::Bound(entry.cell.cons_cdr())
        }
    }

    #[inline]
    fn push(&self, entry: LexenvAssqCacheEntry) {
        let index = LexenvAssqCacheSlot::of(entry.lexenv_bits, entry.symbol).index();
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
        let cache = &self.lexenv_assq_cache;
        if let Some(cell) = cache.find(lexenv_bits, sym_id) {
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

        let cell = if self.symbols_with_pos_enabled {
            lexical_symbols::assq(lexenv, sym_id)
        } else {
            lexenv_assq(lexenv, sym_id)
        };
        cache.push(LexenvAssqCacheEntry {
            lexenv_bits,
            symbol: sym_id,
            cell: cell.unwrap_or(Value::NIL),
        });
        cell
    }

    /// The interpreter's symbol read path.  Constants use their dedicated
    /// slots and a cached negative answer needs no assq walk; any other
    /// outcome keeps the single generic definition above.
    #[inline]
    pub(crate) fn lexenv_lookup_cached_in(&self, lexenv: Value, sym_id: SymId) -> Option<Value> {
        let lexenv_bits = lexenv.bits();
        if let Some(constant) = LexenvConstant::from_sym_id(sym_id) {
            match self
                .lexenv_assq_cache
                .constant_lookup(lexenv_bits, constant)
            {
                LexenvConstantLookup::Bound(value) => return Some(value),
                LexenvConstantLookup::Absent => return None,
                LexenvConstantLookup::CacheMiss => {}
            }
        }
        self.lexenv_assq_cached_in(lexenv, sym_id)
            .map(|cell| cell.cons_cdr())
    }
}

#[cfg(test)]
#[path = "tests/lexenv_cache_partition_test.rs"]
mod tests;
