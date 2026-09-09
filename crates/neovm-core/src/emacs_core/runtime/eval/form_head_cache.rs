//! What an interpreted cons form's head symbol resolves to, remembered for as
//! long as the obarray's function epoch stands still.

use super::*;

/// What the head symbol of an interpreted cons form resolves to, remembered
/// for as long as the obarray's function epoch stands still.
///
/// Every interpreted form asks the same three questions of its head before it
/// can dispatch: is it one of the evaluator-internal literal heads, what is in
/// its function cell, and is that cell a subr.  All three depend only on the
/// symbol and the function epoch -- never on the form -- so a form evaluated a
/// second time re-derives an answer that cannot have changed.  Measured on
/// magit-status, the same cons is evaluated **32.5 times** on average
/// (org-journal-open: 61.4), so almost all of that work is repeat work.
///
/// The epoch is the same guard `fset`, `defalias` and advice already bump, and
/// the one the JIT's speculated call sites validate against.
#[derive(Clone, Copy)]
pub(crate) struct FormHeadCacheEntry {
    pub(crate) epoch: u64,
    pub(crate) sym: SymId,
    /// True for `lambda` / `byte-code-literal` / `byte-code`, which the
    /// dispatcher answers before resolving anything.
    pub(crate) literal_head: bool,
    /// The symbol's function cell, or `None` if it had none.
    pub(crate) func: Option<Value>,
    /// What that cell is, when it is a subr.
    pub(crate) subr: Option<(SymId, SubrEntry)>,
}

const FORM_HEAD_CACHE_CAPACITY: usize = 512;

pub(crate) struct FormHeadCache {
    entries: [Cell<Option<FormHeadCacheEntry>>; FORM_HEAD_CACHE_CAPACITY],
}

impl Default for FormHeadCache {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| Cell::new(None)),
        }
    }
}

impl FormHeadCache {
    #[inline]
    fn slot(sym_id: SymId) -> usize {
        (sym_id.0 as usize).wrapping_mul(0x9E37_79B1) >> 13 & (FORM_HEAD_CACHE_CAPACITY - 1)
    }

    #[inline]
    pub(crate) fn find(&self, sym_id: SymId, epoch: u64) -> Option<FormHeadCacheEntry> {
        let entry = self.entries[Self::slot(sym_id)].get()?;
        (entry.sym == sym_id && entry.epoch == epoch).then_some(entry)
    }

    #[inline]
    pub(crate) fn push(&self, entry: FormHeadCacheEntry) {
        self.entries[Self::slot(entry.sym)].set(Some(entry));
    }

    pub(crate) fn clear(&self) {
        for entry in &self.entries {
            entry.set(None);
        }
    }
}
