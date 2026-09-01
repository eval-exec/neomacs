//! Native Lisp declarations and dispatch metadata owned by GNU `src/eval.c`'s mirror.

use std::sync::OnceLock;

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

/// Evaluator-owned dispatch attached to a Lisp-visible subroutine declaration.
///
/// Keeping the handler beside the [`SubrSpec`] makes this the only table that
/// maps a Lisp name to evaluator behavior. Dispatch is exhaustive over these
/// enums rather than repeating the names in a second routing table.
#[derive(Clone, Copy)]
pub(super) enum EvaluatorHandler {
    SpecialForm(SpecialFormHandler),
    Callable(CallableHandler),
}

#[derive(Clone, Copy)]
pub(super) enum SpecialFormHandler {
    Quote,
    Function,
    Let,
    LetStar,
    Setq,
    If,
    And,
    Or,
    Cond,
    While,
    Progn,
    Prog1,
    Defvar,
    Defconst,
    Catch,
    UnwindProtect,
    ConditionCase,
    Interactive,
    SaveExcursion,
    SaveRestriction,
    SaveCurrentBuffer,
}

#[derive(Clone, Copy)]
pub(super) enum CallableHandler {
    Throw,
}

#[derive(Clone, Copy)]
struct EvaluatorSubr {
    spec: SubrSpec,
    handler: EvaluatorHandler,
}

impl EvaluatorSubr {
    const fn special(name: &'static str, arity: SubrArity, handler: SpecialFormHandler) -> Self {
        Self {
            spec: SubrSpec::evaluator(name, arity, SubrDispatchKind::SpecialForm),
            handler: EvaluatorHandler::SpecialForm(handler),
        }
    }

    const fn callable(name: &'static str, arity: SubrArity, handler: CallableHandler) -> Self {
        Self {
            spec: SubrSpec::evaluator(name, arity, SubrDispatchKind::ContextCallable),
            handler: EvaluatorHandler::Callable(handler),
        }
    }
}

/// Evaluator-owned callable objects installed in Lisp function cells.
const EVALUATOR_SUBRS: &[EvaluatorSubr] = &[
    EvaluatorSubr::special("quote", SubrArity::new(1, None), SpecialFormHandler::Quote),
    EvaluatorSubr::special(
        "function",
        SubrArity::new(1, None),
        SpecialFormHandler::Function,
    ),
    EvaluatorSubr::special("let", SubrArity::new(1, None), SpecialFormHandler::Let),
    EvaluatorSubr::special("let*", SubrArity::new(1, None), SpecialFormHandler::LetStar),
    EvaluatorSubr::special("setq", SubrArity::new(0, None), SpecialFormHandler::Setq),
    EvaluatorSubr::special("if", SubrArity::new(2, None), SpecialFormHandler::If),
    EvaluatorSubr::special("and", SubrArity::new(0, None), SpecialFormHandler::And),
    EvaluatorSubr::special("or", SubrArity::new(0, None), SpecialFormHandler::Or),
    EvaluatorSubr::special("cond", SubrArity::new(0, None), SpecialFormHandler::Cond),
    EvaluatorSubr::special("while", SubrArity::new(1, None), SpecialFormHandler::While),
    EvaluatorSubr::special("progn", SubrArity::new(0, None), SpecialFormHandler::Progn),
    EvaluatorSubr::special("prog1", SubrArity::new(1, None), SpecialFormHandler::Prog1),
    EvaluatorSubr::special(
        "defvar",
        SubrArity::new(1, None),
        SpecialFormHandler::Defvar,
    ),
    EvaluatorSubr::special(
        "defconst",
        SubrArity::new(2, None),
        SpecialFormHandler::Defconst,
    ),
    EvaluatorSubr::special("catch", SubrArity::new(1, None), SpecialFormHandler::Catch),
    EvaluatorSubr::special(
        "unwind-protect",
        SubrArity::new(1, None),
        SpecialFormHandler::UnwindProtect,
    ),
    EvaluatorSubr::special(
        "condition-case",
        SubrArity::new(2, None),
        SpecialFormHandler::ConditionCase,
    ),
    EvaluatorSubr::special(
        "interactive",
        SubrArity::new(0, None),
        SpecialFormHandler::Interactive,
    ),
    EvaluatorSubr::special(
        "save-excursion",
        SubrArity::new(0, None),
        SpecialFormHandler::SaveExcursion,
    ),
    EvaluatorSubr::special(
        "save-restriction",
        SubrArity::new(0, None),
        SpecialFormHandler::SaveRestriction,
    ),
    EvaluatorSubr::special(
        "save-current-buffer",
        SubrArity::new(0, None),
        SpecialFormHandler::SaveCurrentBuffer,
    ),
    EvaluatorSubr::callable("throw", SubrArity::new(2, Some(2)), CallableHandler::Throw),
];

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "default-toplevel-value",
        NativeFn::ContextVec(default_toplevel_value),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "set-default-toplevel-value",
        NativeFn::ContextVec(set_default_toplevel_value),
        SubrArity::new(2, Some(2)),
    ),
}

fn evaluator_subr(name: &str) -> Option<EvaluatorSubr> {
    EVALUATOR_SUBRS
        .iter()
        .copied()
        .find(|declaration| declaration.spec.name() == name)
}

static EVALUATOR_HANDLERS: OnceLock<Vec<Option<EvaluatorHandler>>> = OnceLock::new();

pub(super) fn evaluator_handler(sym_id: SymId) -> Option<EvaluatorHandler> {
    EVALUATOR_HANDLERS
        .get_or_init(|| {
            let max_id = EVALUATOR_SUBRS
                .iter()
                .map(|declaration| intern(declaration.spec.name()).0 as usize)
                .max()
                .unwrap_or(0);
            let mut handlers = vec![None; max_id + 1];
            for declaration in EVALUATOR_SUBRS {
                handlers[intern(declaration.spec.name()).0 as usize] = Some(declaration.handler);
            }
            handlers
        })
        .get(sym_id.0 as usize)
        .copied()
        .flatten()
}

pub(crate) fn evaluator_dispatch_kind(name: &str) -> Option<SubrDispatchKind> {
    evaluator_subr(name).map(|declaration| declaration.spec.dispatch_kind())
}

/// Materialize evaluator-handled special forms and callables at their original
/// late startup position.
pub(crate) fn register_public_subrs(ctx: &mut Context) {
    for declaration in EVALUATOR_SUBRS {
        ctx.register_subr(declaration.spec);
    }
}

impl Context {
    /// Register one authoritative native Lisp declaration.
    ///
    /// This is the only path that installs native metadata, so call sites must
    /// construct a typed [`SubrSpec`] rather than passing loose parallel fields.
    pub(crate) fn register_subr(&mut self, spec: SubrSpec) {
        crate::emacs_core::subr::record_no_eval_policy(spec.name(), spec.no_eval_policy());
        let arity = spec.arity();
        self.install_subr(
            spec.name(),
            spec.function(),
            arity,
            spec.dispatch_kind(),
            spec.interactive_spec(),
        );

        if spec.command_default() == crate::emacs_core::subr::CommandDefault::Disabled {
            self.obarray
                .put_property(spec.name(), "disabled", Value::T)
                .expect("freshly registered subr must have a valid property list");
        }
    }

    pub(crate) fn register_subrs(&mut self, specs: &[SubrSpec]) {
        for &spec in specs {
            self.register_subr(spec);
        }
    }

    fn install_subr(
        &mut self,
        name: &str,
        function: Option<SubrFn>,
        arity: SubrArity,
        dispatch_kind: SubrDispatchKind,
        interactive_spec: Option<crate::emacs_core::interactive::BuiltinInteractiveSpec>,
    ) {
        let sym_id = intern(name);
        let name_id = symbol_name_id(sym_id);

        register_global_subr_entry(
            sym_id,
            SubrEntry {
                function,
                min_args: arity.min(),
                max_args: arity.max(),
                dispatch_kind,
                name_id,
                interactive_spec,
            },
        );

        self.obarray.intern(name);
        // `init_builtins` runs both on a fresh evaluator and again after
        // restoring a pdump image. On the pdump path, GNU-loaded Lisp
        // definitions may already shadow a primitive with the same name
        // (e.g. `switch-to-buffer`, `display-buffer`, `delete-window` --
        // see `rust_subrs_shadowed_by_lisp_test.rs` for the reviewed list).
        // Refresh stale subr cells, but do not clobber an existing non-subr
        // function cell that the dumped runtime already established.
        let should_install_public_subr =
            self.obarray
                .symbol_function_id(sym_id)
                .is_none_or(|existing| {
                    matches!(
                        existing.kind(),
                        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr)
                    )
                });
        if should_install_public_subr {
            self.obarray
                .set_symbol_function(name, Value::subr_from_sym_id(sym_id));
        }
        // The static subr entry above was rewritten IN PLACE even when the
        // cell write was skipped — keep function_epoch a complete change
        // signal (JIT call-speculation validity depends on it).
        self.obarray.bump_function_epoch();
    }
}
