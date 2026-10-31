//! What an interpreted cons form's head symbol resolves to, remembered for as
//! long as the obarray's function epoch stands still.
//!
//! The classification itself (and the types it returns) lives in
//! [`super::dispatch_class`], the neutral module shared with E4's parallel
//! executor; this file is only the epoch cache.

use super::dispatch_class::FormHead;
use super::*;

/// One slot: the key beside its answer, so a probe compares the key in
/// place and copies only the answer out.
struct FormHeadSlot {
    epoch: Cell<u64>,
    sym: Cell<SymId>,
    head: Cell<FormHead>,
}

/// No function epoch is `u64::MAX`: the counter starts at zero and skips it.
const EMPTY_EPOCH: u64 = u64::MAX;

const FORM_HEAD_CACHE_CAPACITY: usize = 512;

/// What the head symbol of an interpreted cons form resolves to, remembered
/// for as long as the obarray's function epoch stands still.
///
/// Every interpreted form asks the same three questions of its head before it
/// can dispatch: is it one of the evaluator-internal literal heads, what is in
/// its function cell, and what kind of callable that cell is.  All three
/// depend only on the symbol and the function epoch -- never on the form --
/// so a form evaluated a second time re-derives an answer that cannot have
/// changed.  Measured on magit-status, the same cons is evaluated **32.5
/// times** on average (org-journal-open: 61.4), so almost all of that work is
/// repeat work.
///
/// The epoch is the same guard `fset`, `defalias` and advice already bump, and
/// the one the JIT's speculated call sites validate against.
pub(crate) struct FormHeadCache {
    slots: [FormHeadSlot; FORM_HEAD_CACHE_CAPACITY],
}

impl Default for FormHeadCache {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| FormHeadSlot {
                epoch: Cell::new(EMPTY_EPOCH),
                sym: Cell::new(SymId(0)),
                head: Cell::new(FormHead::EMPTY),
            }),
        }
    }
}

impl FormHeadCache {
    #[inline]
    fn slot(sym_id: SymId) -> usize {
        (sym_id.0 as usize).wrapping_mul(0x9E37_79B1) >> 13 & (FORM_HEAD_CACHE_CAPACITY - 1)
    }

    #[inline]
    pub(super) fn find(&self, sym_id: SymId, epoch: u64) -> Option<FormHead> {
        let slot = &self.slots[Self::slot(sym_id)];
        if slot.epoch.get() != epoch || slot.sym.get() != sym_id {
            return None;
        }
        Some(slot.head.get())
    }

    #[inline]
    pub(super) fn push(&self, sym_id: SymId, epoch: u64, head: FormHead) {
        let slot = &self.slots[Self::slot(sym_id)];
        slot.epoch.set(epoch);
        slot.sym.set(sym_id);
        slot.head.set(head);
    }

    pub(crate) fn clear(&self) {
        for slot in &self.slots {
            slot.epoch.set(EMPTY_EPOCH);
        }
    }
}
