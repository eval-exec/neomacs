//! The one classification of an interpreted form's head, shared by the serial
//! interpreter and (later) E4's parallel executor.
//!
//! `FormHead::classify` decides, from a symbol and its function cell alone,
//! what dispatch does with that cell.  The serial interpreter caches the
//! answer per function epoch and matches it once; Tier-I consumes the same
//! answer; E4 will match the same `EvalDispatch` in its owner-side interpreter
//! and VM.  There is no second classification table.
//!
//! # GC-inert by construction
//!
//! A cached classification may not become a GC root.  `FormHead` therefore
//! carries only `Copy` discriminants, a code pointer and arities, plus the
//! function cell itself, which the obarray keeps alive for as long as the
//! cache slot's epoch holds (the same rule the cache has always used).  A
//! closure's slots are read at dispatch from the already-rooted function
//! value, never captured here, so `CallTarget::InterpretedClosure` is a
//! marker in the shared type; E4 reads the slots when it matches.
//!
//! # E4 admission is consumer-side
//!
//! The serial classifier never consults the P3 class.  The parallel consumer
//! resolves the E1 catalog entry, authority and `MAY_SIGNAL` flag from its own
//! dense admission table keyed by the primitive identity (`PrimitiveCall`),
//! per the E4 design (§7) and the peer decisions 1 and 5.  `Atom` and
//! `Symbol` are E4's variants: the serial `eval_sub` keeps its own symbol and
//! atom path untouched, and the serial dispatcher folds `literal_head` into
//! its literal test before matching `dispatch`.

use super::*;

/// What evaluating one form does, decided from its surface and (for a cons
/// form) its head symbol's function cell.
#[allow(dead_code)] // E4 consumes the full axis; the serial path uses a subset.
#[derive(Clone, Copy)]
pub(crate) enum EvalDispatch {
    /// A non-cons, non-symbol self-evaluating object (GNU eval.c:2564-2565).
    Atom,
    /// A bare symbol: an effective variable access (E4's §4 key rule).
    Symbol(SymbolAccess),
    /// An evaluator-internal literal head (`lambda`, `byte-code-literal`).
    Literal(LiteralForm),
    /// A public special form: the evaluator handler, run UNEVALLED.
    Special(SpecialFormKind),
    /// A function call, classified by the callable in the head's cell.
    Call(CallTarget),
    /// Anything the cached classification cannot decide: the full resolution
    /// (GNU eval.c:2604-2766) runs unchanged.
    Escape(EscapeReason),
}

/// The variable-access kind of a symbol form.  E4's effective-value API
/// distinguishes a Plain read from a write; the serial interpreter decides by
/// call site and never branches on this.
#[allow(dead_code)] // E4 variant axis; serial dispatch is call-site driven.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolAccess {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LiteralForm {
    Lambda,
    ByteCodeLiteral,
}

/// The evaluator handler for a public special form.  Runs UNEVALLED, exactly
/// as `SpecialFormHandler` does today.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SpecialFormKind(pub(super) SpecialFormHandler);

/// The callable in the head's cell, in the only three shapes the cached
/// classification can decide without the full resolution.
#[derive(Clone, Copy)]
pub(crate) enum CallTarget {
    /// A fixed-arity builtin subr with a native function (GNU `XSUBR`).
    Primitive(PrimitiveCall),
    /// An interpreted closure: a `Lambda` veclike, never a macro and never a
    /// cons (`setcar` can change a cons; a veclike's type cannot change).
    InterpretedClosure,
    /// A byte-code object; `get_bytecode_data` still runs at dispatch, where
    /// a dump stub is materialized (GNU materializes it per call, not per
    /// classification).
    Bytecode,
}

/// The serial-visible identity of a primitive.  E4's admission table is keyed
/// by this identity; the authority, E1 entry and `MAY_SIGNAL` flag live in the
/// consumer's generated table, not in the per-form classification (zero
/// serial cost, E4 rev 2 §7).
#[derive(Clone, Copy)]
pub(crate) struct PrimitiveCall {
    pub(crate) function: crate::tagged::header::SubrFn,
    pub(crate) min_args: u16,
    pub(crate) max_args: Option<u16>,
}

/// Why the cached classification declined to decide a cell.  The serial
/// dispatcher treats every reason identically (the full resolution); E4 uses
/// the reason to pick its slow-path route without re-classifying.
#[allow(dead_code)] // E4's slow classifier fills the reasons the serial cache cannot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EscapeReason {
    /// The symbol has no function cell (GNU void-function).
    NoFunctionCell,
    /// The cell is another symbol's UNEVALLED subr: an aliased special form.
    AliasedSpecialForm,
    /// A symbol cell, an autoload, or another indirection to resolve.
    AutoloadOrAlias,
    /// A macro object or a cons-shaped `(macro . fn)`.
    Macro,
    /// A cons-shaped callable (`lambda`-literal head).
    ConsCallable,
    /// A `ContextCallable` subr, which needs the evaluator's callable path.
    ContextCallable,
    /// Compiler function overrides are active; resolution must re-read them.
    Overridden,
    /// A builtin subr entry with no native function.
    NotCallable,
    /// Any other cell shape.
    Other,
}

/// The head's answers: whether it is an evaluator-internal literal head, what
/// its function cell holds, and the one shared dispatch decision.
#[derive(Clone, Copy)]
pub(crate) struct FormHead {
    /// True for `lambda` / `byte-code-literal`, which the dispatcher answers
    /// before resolving anything.  The serial dispatcher tests this before
    /// matching `dispatch`, exactly as it did before the extraction.
    pub(crate) literal_head: bool,
    /// The symbol's function cell, or `None` if it had none.
    pub(crate) func: Option<Value>,
    /// The cell's dispatch class.  `Literal` is never produced here: the
    /// literal test is a separate fact about the symbol, not about the cell.
    pub(crate) dispatch: EvalDispatch,
}

impl FormHead {
    pub(crate) const EMPTY: Self = Self {
        literal_head: false,
        func: None,
        dispatch: EvalDispatch::Escape(EscapeReason::NoFunctionCell),
    };

    /// Classify SYM_ID's function cell FUNC.  Only the shapes whose dispatch
    /// needs nothing but the cell get a fast class; everything else is
    /// `Escape` and takes the full resolution unchanged.
    pub(crate) fn classify(sym_id: SymId, func: Option<Value>) -> Self {
        let literal_head = sym_id == lambda_symbol() || sym_id == byte_code_literal_symbol();
        let dispatch = match func {
            None => EvalDispatch::Escape(EscapeReason::NoFunctionCell),
            Some(func) => match subr_call_entry_from_value(func) {
                Some((target, entry)) => match entry.dispatch_kind {
                    SubrDispatchKind::SpecialForm if target == sym_id => {
                        match evaluator_handler(target) {
                            Some(EvaluatorHandler::SpecialForm(handler)) => {
                                EvalDispatch::Special(SpecialFormKind(handler))
                            }
                            _ => EvalDispatch::Escape(EscapeReason::Other),
                        }
                    }
                    SubrDispatchKind::SpecialForm => {
                        EvalDispatch::Escape(EscapeReason::AliasedSpecialForm)
                    }
                    SubrDispatchKind::Builtin => match entry.function {
                        Some(function) => {
                            EvalDispatch::Call(CallTarget::Primitive(PrimitiveCall {
                                function,
                                min_args: entry.min_args,
                                max_args: entry.max_args,
                            }))
                        }
                        None => EvalDispatch::Escape(EscapeReason::NotCallable),
                    },
                    SubrDispatchKind::ContextCallable => {
                        EvalDispatch::Escape(EscapeReason::ContextCallable)
                    }
                },
                None => match func.veclike_type() {
                    Some(VecLikeType::ByteCode) => EvalDispatch::Call(CallTarget::Bytecode),
                    Some(VecLikeType::Lambda) => EvalDispatch::Call(CallTarget::InterpretedClosure),
                    _ => EvalDispatch::Escape(EscapeReason::Other),
                },
            },
        };
        Self {
            literal_head,
            func,
            dispatch,
        }
    }
}
