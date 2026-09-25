//! `function_epoch` bump attribution: how often, and why, the obarray's
//! function epoch moves. Every bump makes every armed speculated call site
//! re-validate its binding on its next call, so the rate is a direct cost
//! input for call speculation.
//!
//! The per-reason counters are thread-local `Cell`s (one increment per
//! function-cell write — cold next to calls) and always on; the per-symbol
//! histogram only counts under `NEOVM_JIT_COMPILE_STATS`.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;
use strum::{EnumCount, IntoEnumIterator};

use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::FunctionEpochBump;

const BUMP_REASONS: usize = FunctionEpochBump::COUNT;

/// What a speculated bytecode call site found when it re-validated its
/// binding after `function_epoch` moved (the slow half of
/// `neovm_jit_call_spec`; the armed fast path is never counted).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
pub(crate) enum SpecRevalidation {
    /// The binding still holds the compile-time callee: the site re-armed
    /// (a bump for an unrelated symbol cost it one binding lookup).
    #[strum(serialize = "spec-rearm")]
    Rearmed,
    /// The binding changed: the site takes the strict call.
    #[strum(serialize = "spec-rebind")]
    BindingChanged,
}

const SPEC_KINDS: usize = SpecRevalidation::COUNT;

thread_local! {
    /// Bumps per [`FunctionEpochBump`] reason.
    static FN_EPOCH_BUMPS: [Cell<u64>; BUMP_REASONS] =
        const { [const { Cell::new(0) }; BUMP_REASONS] };
    /// Function-cell writes that stored the value already there (no bump).
    static FN_CELL_UNCHANGED: Cell<u64> = const { Cell::new(0) };
    /// Redefinitions per symbol (only under `NEOVM_JIT_COMPILE_STATS`).
    static FN_REDEFINED: RefCell<FxHashMap<SymId, u64>> = RefCell::new(FxHashMap::default());
    /// Speculated-call-site re-validations per [`SpecRevalidation`] outcome.
    static SPEC_REVALIDATIONS: [Cell<u64>; SPEC_KINDS] =
        const { [const { Cell::new(0) }; SPEC_KINDS] };
    /// Compiled leaves evicted because a callee they inlined was redefined.
    static INLINE_EVICTED_LEAVES: Cell<u64> = const { Cell::new(0) };
}

/// Count one speculated-call-site re-validation (the slow half only).
#[cold]
#[inline(never)]
pub(crate) fn note_spec_revalidation(kind: SpecRevalidation) {
    SPEC_REVALIDATIONS.with(|c| {
        let c = &c[kind as usize];
        c.set(c.get() + 1);
    });
}

/// Count leaves evicted for inlining a redefined callee.
#[inline]
pub(crate) fn note_inline_evictions(n: usize) {
    INLINE_EVICTED_LEAVES.with(|c| c.set(c.get() + n as u64));
}

/// Count one `function_epoch` bump. `sym` is the redefined symbol for a
/// cell write, `None` for a bump without one (subr rewrite, overrides).
#[inline]
pub(crate) fn note_function_epoch_bump(why: FunctionEpochBump, sym: Option<SymId>) {
    FN_EPOCH_BUMPS.with(|c| {
        let c = &c[why as usize];
        c.set(c.get() + 1);
    });
    if let Some(sym) = sym
        && super::summary_enabled()
    {
        note_redefined_symbol(sym);
    }
}

#[cold]
#[inline(never)]
fn note_redefined_symbol(sym: SymId) {
    FN_REDEFINED.with(|m| *m.borrow_mut().entry(sym).or_insert(0) += 1);
}

/// Count one function-cell write that changed nothing (no bump).
#[inline]
pub(crate) fn note_function_cell_unchanged() {
    FN_CELL_UNCHANGED.with(|c| c.set(c.get() + 1));
}

/// A copy of this thread's epoch counters (the report's data).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EpochCounters {
    /// Indexed by `FunctionEpochBump as usize`.
    pub(crate) bumps: [u64; BUMP_REASONS],
    pub(crate) unchanged_writes: u64,
    /// Indexed by `SpecRevalidation as usize`.
    pub(crate) spec: [u64; SPEC_KINDS],
    pub(crate) inline_evicted_leaves: u64,
}

impl EpochCounters {
    /// This thread's counters now.
    pub(crate) fn snapshot() -> Self {
        let mut bumps = [0u64; BUMP_REASONS];
        FN_EPOCH_BUMPS.with(|c| {
            for (slot, cell) in bumps.iter_mut().zip(c.iter()) {
                *slot = cell.get();
            }
        });
        let mut spec = [0u64; SPEC_KINDS];
        SPEC_REVALIDATIONS.with(|c| {
            for (slot, cell) in spec.iter_mut().zip(c.iter()) {
                *slot = cell.get();
            }
        });
        EpochCounters {
            bumps,
            unchanged_writes: FN_CELL_UNCHANGED.with(Cell::get),
            spec,
            inline_evicted_leaves: INLINE_EVICTED_LEAVES.with(Cell::get),
        }
    }

    /// Counts accumulated since `base` (an earlier snapshot).
    pub(crate) fn since(&self, base: &EpochCounters) -> EpochCounters {
        let mut bumps = [0u64; BUMP_REASONS];
        for (i, slot) in bumps.iter_mut().enumerate() {
            *slot = self.bumps[i].saturating_sub(base.bumps[i]);
        }
        let mut spec = [0u64; SPEC_KINDS];
        for (i, slot) in spec.iter_mut().enumerate() {
            *slot = self.spec[i].saturating_sub(base.spec[i]);
        }
        EpochCounters {
            bumps,
            unchanged_writes: self.unchanged_writes.saturating_sub(base.unchanged_writes),
            spec,
            inline_evicted_leaves: self
                .inline_evicted_leaves
                .saturating_sub(base.inline_evicted_leaves),
        }
    }

    /// Re-validations with one outcome.
    pub(crate) fn spec_for(&self, kind: SpecRevalidation) -> u64 {
        self.spec[kind as usize]
    }

    /// Bumps for one reason.
    pub(crate) fn bumps_for(&self, why: FunctionEpochBump) -> u64 {
        self.bumps[why as usize]
    }

    /// Total bumps over every reason.
    pub(crate) fn total(&self) -> u64 {
        self.bumps.iter().sum()
    }

    /// `total=N fset=N defalias=N ... unchanged-writes=N`.
    pub(crate) fn render(&self) -> String {
        let mut out = format!("total={}", self.total());
        for why in FunctionEpochBump::iter() {
            let name: &'static str = why.into();
            out.push_str(&format!(" {name}={}", self.bumps_for(why)));
        }
        out.push_str(&format!(" unchanged-writes={}", self.unchanged_writes));
        out.push_str(&format!(
            " inline-evicted-leaves={}",
            self.inline_evicted_leaves
        ));
        for kind in SpecRevalidation::iter() {
            let name: &'static str = kind.into();
            out.push_str(&format!(" {name}={}", self.spec_for(kind)));
        }
        out
    }

    /// The nonzero reasons only (`total=N defalias=N ...`), for the compact
    /// since-command-loop section.
    pub(crate) fn render_nonzero(&self) -> String {
        let mut out = format!("total={}", self.total());
        for why in FunctionEpochBump::iter() {
            let n = self.bumps_for(why);
            if n > 0 {
                let name: &'static str = why.into();
                out.push_str(&format!(" {name}={n}"));
            }
        }
        if self.unchanged_writes > 0 {
            out.push_str(&format!(" unchanged-writes={}", self.unchanged_writes));
        }
        if self.inline_evicted_leaves > 0 {
            out.push_str(&format!(
                " inline-evicted-leaves={}",
                self.inline_evicted_leaves
            ));
        }
        for kind in SpecRevalidation::iter() {
            let n = self.spec_for(kind);
            if n > 0 {
                let name: &'static str = kind.into();
                out.push_str(&format!(" {name}={n}"));
            }
        }
        out
    }
}

/// The `n` most-redefined symbols, `name=count` most frequent first (only
/// populated under `NEOVM_JIT_COMPILE_STATS`).
pub(crate) fn top_redefined(n: usize) -> String {
    FN_REDEFINED.with(|m| {
        let m = m.borrow();
        let mut v: Vec<(SymId, u64)> = m.iter().map(|(&s, &c)| (s, c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.0.cmp(&b.0.0)));
        v.iter()
            .take(n)
            .map(|&(sym, count)| format!("{}={count}", symbol_label(sym)))
            .collect::<Vec<_>>()
            .join(",")
    })
}

/// A symbol's name for a report line (see [`report_token`]).
pub(crate) fn symbol_label(sym: SymId) -> String {
    report_token(
        crate::emacs_core::intern::resolve_sym_lisp_string(sym)
            .as_utf8_str()
            .unwrap_or("<non-utf8>"),
    )
}

/// A Lisp name made safe as one token of a report line: whitespace, control
/// characters and `,`/`=` (the line's separators) become `_`.
pub(crate) fn report_token(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_whitespace() || c.is_control() || c == ',' || c == '=' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Test-only: zero this thread's epoch counters.
#[cfg(test)]
pub(crate) fn reset_epoch_counters_for_test() {
    FN_EPOCH_BUMPS.with(|c| c.iter().for_each(|cell| cell.set(0)));
    FN_CELL_UNCHANGED.with(|c| c.set(0));
    FN_REDEFINED.with(|m| m.borrow_mut().clear());
    SPEC_REVALIDATIONS.with(|c| c.iter().for_each(|cell| cell.set(0)));
    INLINE_EVICTED_LEAVES.with(|c| c.set(0));
}
