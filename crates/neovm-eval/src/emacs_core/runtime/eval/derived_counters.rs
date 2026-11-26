//! GNU's `*-consed` `DEFVAR_INT` counters, derived at read time.
//!
//! GNU binds `cons-cells-consed`, `floats-consed`, `vector-cells-consed`,
//! `symbols-consed`, `string-chars-consed`, `intervals-consed` and
//! `strings-consed` to the same C `intmax_t` slots that `memory-use-counts`
//! reports (`src/alloc.c:7429-7448`), so a Lisp read sees the live allocation
//! counter. Neomacs keeps the counters in the heap's per-mutator accounting,
//! not in the symbol cells. Copying them into the cells on every allocation
//! would put a store on the inline cons/float paths and would need every
//! owner's counters summed; instead the cell is refreshed when the variable is
//! read, from the same exact snapshot `memory-use-counts` uses.
//!
//! The JIT refuses to inline these seven sites (`jit::compile::inline_vars`
//! classifies them `Plain`), so a compiled read takes the shim and reaches the
//! same hook. The hook sits inside `read_forwarded_cached`'s `Int` arm, so a
//! localized or plain read and every non-`Int` forwarder family pay nothing.
//! A write still lands in the forwarded cell (GNU's `setq` writes the C
//! slot); the next read re-derives the value, because the allocation
//! counters, not the cell, are the authority.

use super::Context;
use neovm_value::forward::LispInteger;
use neovm_value::intern::{SymId, intern};
use neovm_value::value::Value;
use std::sync::OnceLock;

/// Slot order is `MemoryUseCountSlot`'s discriminant order, which is also the
/// order of `(memory-use-counts)`.
const NAMES: [&str; 7] = [
    "cons-cells-consed",
    "floats-consed",
    "vector-cells-consed",
    "symbols-consed",
    "string-chars-consed",
    "intervals-consed",
    "strings-consed",
];

static IDS: OnceLock<[SymId; 7]> = OnceLock::new();

fn ids() -> &'static [SymId; 7] {
    IDS.get_or_init(|| {
        NAMES.map(|name| {
            // The boot defines all seven before any Lisp can read them; the
            // fallback only covers a pre-boot read.
            neovm_value::intern::lookup_interned(name).unwrap_or_else(|| intern(name))
        })
    })
}

/// The counter slot behind a `DEFVAR_INT` symbol, or `None` for every other
/// variable. Only forwarded reads call this, so the seven compares are not on
/// the ordinary symbol-read path.
#[inline]
pub(crate) fn slot(id: SymId) -> Option<usize> {
    ids().iter().position(|&candidate| candidate == id)
}

impl Context {
    /// The live value of one of the seven counters. Refreshes the forwarded
    /// cell so a later direct read (a `default-value` walk or a dumped read)
    /// sees the same number.
    #[inline]
    pub(crate) fn derived_memory_use_value(&self, slot: usize) -> Value {
        let value = Value::memory_use_counts_snapshot()[slot];
        if let Some(forwarder) = self.obarray.int_forwarder(ids()[slot]) {
            forwarder.set(LispInteger::from_i64(value));
        }
        Value::fixnum(value)
    }

    /// GNU `do_symval_forwarding`'s live answer for a derived counter symbol.
    #[inline]
    pub(crate) fn derived_memory_use_read(&self, id: SymId) -> Option<Value> {
        slot(id).map(|slot| self.derived_memory_use_value(slot))
    }
}
