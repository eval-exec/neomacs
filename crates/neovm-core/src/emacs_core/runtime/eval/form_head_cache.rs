//! What an interpreted cons form's head symbol resolves to, remembered for as
//! long as the obarray's function epoch stands still.

use super::*;

/// What dispatch does with a head whose function cell is stable for this
/// epoch, decided once when the cache slot is filled.
#[derive(Clone, Copy)]
pub(super) enum HeadClass {
    /// The full resolution: no cell, an alias, an autoload, a macro, a cons,
    /// a `ContextCallable` subr, an aliased special form, anything else.
    Slow,
    /// The cell is this symbol's own UNEVALLED subr, whose evaluator handler
    /// is this special form.
    SpecialForm(SpecialFormHandler),
    /// A builtin subr with a native function: the SubrObj's own fields, as
    /// GNU reads `XSUBR (fun)`.
    Subr {
        function: crate::tagged::header::SubrFn,
        min_args: u16,
        max_args: Option<u16>,
    },
    /// A byte-code object (`get_bytecode_data` still runs at dispatch: it is
    /// where a dump stub is materialized).
    ByteCode,
    /// An interpreted closure: a `Lambda` veclike, never a macro and never a
    /// cons (`setcar` can change a cons; a veclike's type cannot change).
    Lambda,
}

/// The head's answers: whether it is an evaluator-internal literal head,
/// what its function cell holds, and what dispatch does with that cell.
#[derive(Clone, Copy)]
pub(super) struct FormHead {
    /// True for `lambda` / `byte-code-literal` / `byte-code`, which the
    /// dispatcher answers before resolving anything.
    pub(super) literal_head: bool,
    /// The symbol's function cell, or `None` if it had none.
    pub(super) func: Option<Value>,
    pub(super) class: HeadClass,
}

impl FormHead {
    const EMPTY: Self = Self {
        literal_head: false,
        func: None,
        class: HeadClass::Slow,
    };

    /// Classify SYM_ID's function cell FUNC.  Only the shapes whose dispatch
    /// needs nothing but the cell get a fast class; everything else is
    /// `Slow` and takes the full resolution unchanged.
    pub(super) fn classify(sym_id: SymId, func: Option<Value>) -> Self {
        let literal_head = sym_id == lambda_symbol()
            || sym_id == byte_code_literal_symbol()
            || sym_id == byte_code_symbol();
        let class = match func {
            None => HeadClass::Slow,
            Some(func) => match subr_call_entry_from_value(func) {
                Some((target, entry)) => match entry.dispatch_kind {
                    SubrDispatchKind::SpecialForm if target == sym_id => {
                        match evaluator_handler(target) {
                            Some(EvaluatorHandler::SpecialForm(handler)) => {
                                HeadClass::SpecialForm(handler)
                            }
                            _ => HeadClass::Slow,
                        }
                    }
                    SubrDispatchKind::Builtin => match entry.function {
                        Some(function) => HeadClass::Subr {
                            function,
                            min_args: entry.min_args,
                            max_args: entry.max_args,
                        },
                        None => HeadClass::Slow,
                    },
                    _ => HeadClass::Slow,
                },
                None => match func.veclike_type() {
                    Some(VecLikeType::ByteCode) => HeadClass::ByteCode,
                    Some(VecLikeType::Lambda) => HeadClass::Lambda,
                    _ => HeadClass::Slow,
                },
            },
        };
        Self {
            literal_head,
            func,
            class,
        }
    }
}

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
