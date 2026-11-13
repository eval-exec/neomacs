//! Native Lisp declarations and dispatch metadata owned by GNU `src/eval.c`'s mirror.

use crate::emacs_core::eval::subr_abi::SubrValueExt;
use std::sync::OnceLock;

use super::*;
use crate::emacs_core::builtins::*;
use crate::emacs_core::subr::{FixedMin1, FixedMin2, FixedMin3, FixedMin4};
#[doc(hidden)]
pub use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    #[doc(hidden)]
    pub const fn special(
        name: &'static str,
        arity: SubrArity,
        handler: SpecialFormHandler,
    ) -> Self {
        Self {
            spec: crate::emacs_core::subr::WorldSubr::new(SubrSpec::evaluator(
                name,
                arity,
                SubrDispatchKind::SpecialForm,
            ))
            .serial(),
            handler: EvaluatorHandler::SpecialForm(handler),
        }
    }

    #[doc(hidden)]
    pub const fn callable(name: &'static str, arity: SubrArity, handler: CallableHandler) -> Self {
        Self {
            spec: crate::emacs_core::subr::WorldSubr::new(SubrSpec::evaluator(
                name,
                arity,
                SubrDispatchKind::ContextCallable,
            ))
            .serial(),
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

/// `fboundp`'s C declaration. It moves beside its evaluator implementation
/// with the C14 registrar split (H3a); core's compatibility sequence registers
/// this same spec between the ordinary-eval and public-eval batches.
#[doc(hidden)]
pub const FBOUNDP_SUBR: SubrSpec = crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
    "fboundp",
    builtin_fboundp_1,
    FixedMin1::One
));

crate::emacs_core::subr::define_subrs! {
    // Diagnostics-only VM-profiler control subrs (feature `vm-profile`).
    #[cfg(feature = "vm-profile")]
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "neovm--vm-profile-reset",
            NativeFn::ContextVec(vm_profile_reset),
            SubrArity::new(0, Some(0)),
        )),
    #[cfg(feature = "vm-profile")]
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "neovm--vm-profile-dump",
            NativeFn::ContextVec(vm_profile_dump),
            SubrArity::new(0, Some(1)),
        )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neovm--internal-panic",
        NativeFn::ContextVec(neovm_internal_panic),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "apply",
        NativeFn::ContextSlice(builtin_apply_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "funcall",
        NativeFn::ContextSlice(builtin_funcall_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "funcall-interactively",
        NativeFn::ContextSlice(builtin_funcall_interactively_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "funcall-with-delayed-message",
        NativeFn::ContextVec(builtin_funcall_with_delayed_message),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "defalias",
        NativeFn::ContextVec(builtin_defalias),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "provide",
        NativeFn::ContextVec(builtin_provide),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "require",
        NativeFn::ContextVec(builtin_require),
        SubrArity::new(1, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mapcan",
        NativeFn::ContextVec(builtin_mapcan),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "mapcar",
        builtin_mapcar_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "mapc",
        builtin_mapc_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mapconcat",
        NativeFn::ContextVec(builtin_mapconcat),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sort",
        NativeFn::ContextSlice(builtin_sort_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
            "functionp",
            builtin_functionp_1,
            FixedMin1::One
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "defvaralias",
            NativeFn::ContextVec(builtin_defvaralias),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state(),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
                "boundp",
                builtin_boundp_1,
                FixedMin1::One
            ))
            .leaf(&leaves::BOUNDP)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "default-toplevel-value",
        NativeFn::ContextVec(default_toplevel_value),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-default-toplevel-value",
        NativeFn::ContextVec(set_default_toplevel_value),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-make-var-non-special",
        NativeFn::ContextVec(builtin_internal_make_var_non_special),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "indirect-variable",
            NativeFn::ContextVec(builtin_indirect_variable),
            SubrArity::new(1, Some(1)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "handler-bind-1",
            NativeFn::ContextVec(builtin_handler_bind_1),
            SubrArity::new(1, None),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "symbol-value",
        builtin_symbol_value_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "symbol-function",
        builtin_symbol_function_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "set",
        builtin_set_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fset",
        NativeFn::ContextVec(builtin_fset),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "makunbound",
        NativeFn::ContextVec(builtin_makunbound),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fmakunbound",
        NativeFn::ContextVec(builtin_fmakunbound),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "macroexpand",
            NativeFn::ContextVec(builtin_macroexpand),
            SubrArity::new(1, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "get",
        builtin_get_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "put",
        builtin_put_3,
        FixedMin3::Three
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "setplist",
            NativeFn::ContextVec(builtin_setplist),
            SubrArity::new(2, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "symbol-plist",
        NativeFn::ContextVec(builtin_symbol_plist_fn),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "indirect-function",
        builtin_indirect_function_2,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "signal",
        NativeFn::ContextVec(crate::emacs_core::eval::signal_entry::builtin_signal),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "special-variable-p",
        builtin_special_variable_p_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "intern",
        NativeFn::ContextVec(builtin_intern_fn),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "intern-soft",
        builtin_intern_soft_2,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-hook-with-args",
        NativeFn::ContextSlice(builtin_run_hook_with_args),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-hook-with-args-until-success",
        NativeFn::ContextSlice(builtin_run_hook_with_args_until_success),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-hook-with-args-until-failure",
        NativeFn::ContextVec(builtin_run_hook_with_args_until_failure),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-hook-wrapped",
        NativeFn::ContextSlice(builtin_run_hook_wrapped),
        SubrArity::new(2, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "featurep",
        NativeFn::ContextVec(builtin_featurep),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "garbage-collect",
            NativeFn::ContextVec(builtin_garbage_collect),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("")),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "eval",
        builtin_eval_2,
        FixedMin2::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "barf-if-buffer-read-only",
        NativeFn::ContextVec(builtin_barf_if_buffer_read_only),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bury-buffer-internal",
        NativeFn::ContextVec(builtin_bury_buffer_internal),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "rename-buffer",
            NativeFn::ContextVec(builtin_rename_buffer),
            SubrArity::new(1, Some(2)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
                rename_buffer_interactive_spec,
            ),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "base64-encode-region",
            NativeFn::ContextVec(crate::emacs_core::fns::builtin_base64_encode_region),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("r")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "base64-decode-region",
            NativeFn::ContextVec(crate::emacs_core::fns::builtin_base64_decode_region),
            SubrArity::new(2, Some(4)),
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("r")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "base64url-encode-region",
            NativeFn::ContextVec(crate::emacs_core::fns::builtin_base64url_encode_region),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("r")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "md5",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_md5),
        SubrArity::new(1, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "secure-hash",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_secure_hash),
        SubrArity::new(2, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-hash",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_buffer_hash),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "compare-buffer-substrings",
        NativeFn::ContextVec(builtin_compare_buffer_substrings),
        SubrArity::new(6, Some(6)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal--labeled-narrow-to-region",
            NativeFn::ContextVec(builtin_internal_labeled_narrow_to_region),
            SubrArity::new(3, Some(3)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal--labeled-widen",
            NativeFn::ContextVec(builtin_internal_labeled_widen),
            SubrArity::new(1, Some(1)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
                "buffer-local-value",
                crate::emacs_core::eval::builtin_buffer_local_value_2,
                FixedMin2::Two,
            ))
            .leaf(&leaves::BUFFER_LOCAL_VALUE)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "local-variable-if-set-p",
        NativeFn::ContextVec(builtin_local_variable_if_set_p),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "variable-binding-locus",
        NativeFn::ContextVec(builtin_variable_binding_locus),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "interactive-form",
        NativeFn::ContextVec(builtin_interactive_form),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
            "search-forward",
            builtin_search_forward_4,
            FixedMin4::One
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("MSearch: ")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "search-backward",
            NativeFn::ContextVec(builtin_search_backward),
            SubrArity::new(1, Some(4)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("MSearch backward: "),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
            "re-search-forward",
            builtin_re_search_forward_4,
            FixedMin4::One,
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("sRE search: "),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "re-search-backward",
            NativeFn::ContextVec(builtin_re_search_backward),
            SubrArity::new(1, Some(4)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("sRE search backward: "),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "looking-at",
        builtin_looking_at_2,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "posix-looking-at",
        NativeFn::ContextVec(builtin_posix_looking_at),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-match",
        NativeFn::ContextSlice(builtin_string_match_slice),
        SubrArity::new(2, Some(4)),
    )),
    // `string-match-p' is NOT here: GNU DEFUNs `string-match'
    // (src/search.c:442) and writes `string-match-p' as a `defsubst' over
    // it (lisp/subr.el:5941), so a compiled caller INLINES
    // `(string-match REGEXP STRING START t)' and never reads the cell
    // (DIVERGENCES.md 152).
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "posix-string-match",
        NativeFn::ContextVec(builtin_posix_string_match),
        SubrArity::new(2, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "match-beginning",
        builtin_match_beginning_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "match-end",
        builtin_match_end_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "match-data",
        builtin_match_data_3,
        FixedMin3::Zero,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "match-data--translate",
            NativeFn::ContextVec(builtin_match_data_translate),
            SubrArity::new(1, Some(1)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "set-match-data",
        builtin_set_match_data_2,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "replace-match",
        NativeFn::ContextVec(builtin_replace_match),
        SubrArity::new(1, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-keymap",
        NativeFn::ContextVec(builtin_make_keymap),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-sparse-keymap",
        NativeFn::ContextVec(builtin_make_sparse_keymap),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copy-keymap",
        NativeFn::ContextVec(builtin_copy_keymap),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-key",
        NativeFn::ContextVec(builtin_define_key),
        SubrArity::new(3, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "lookup-key",
        NativeFn::ContextVec(builtin_lookup_key),
        SubrArity::new(2, Some(3)),
    )),
    // `global-set-key' (lisp/subr.el:1545) and `local-set-key' (:1569) are
    // NOT here: GNU has no C version of either.  Both are Lisp over
    // `define-key' + `current-global-map' / `current-local-map', which ARE
    // registered just above (DIVERGENCES.md 152).
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "use-local-map",
        NativeFn::ContextVec(builtin_use_local_map),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "use-global-map",
        NativeFn::ContextVec(builtin_use_global_map),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-local-map",
        NativeFn::ContextVec(builtin_current_local_map),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-global-map",
        NativeFn::ContextVec(builtin_current_global_map),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-active-maps",
        NativeFn::ContextVec(builtin_current_active_maps),
        SubrArity::new(0, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-minor-mode-maps",
        NativeFn::ContextVec(builtin_current_minor_mode_maps),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "keymap-parent",
        NativeFn::ContextVec(builtin_keymap_parent),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-keymap-parent",
        NativeFn::ContextVec(builtin_set_keymap_parent),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "keymapp",
        NativeFn::ContextVec(builtin_keymapp),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "accessible-keymaps",
        NativeFn::ContextVec(builtin_accessible_keymaps),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "map-keymap",
            NativeFn::ContextVec(builtin_map_keymap),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "map-keymap-internal",
            NativeFn::ContextVec(builtin_map_keymap_internal),
            SubrArity::new(2, Some(2)),
        ))
        .requires_eval_state(),
    // Timer functions (run-at-time, run-with-timer, run-with-idle-timer,
    // cancel-timer, timerp, timer-activate) are NOT C primitives in GNU
    // Emacs — they're defined in timer.el as Elisp functions.
    // The C layer only provides timer-check (in keyboard.rs) which reads
    // timer-list / timer-idle-list and calls timer-event-handler.
    // Registering them as Rust builtins would shadow the Elisp definitions
    // and create an incompatible parallel timer system.
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "add-variable-watcher",
            NativeFn::ContextVec(crate::emacs_core::advice::builtin_add_variable_watcher),
            SubrArity::new(2, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "remove-variable-watcher",
            NativeFn::ContextVec(crate::emacs_core::advice::builtin_remove_variable_watcher),
            SubrArity::new(2, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "get-variable-watchers",
            NativeFn::ContextVec(crate::emacs_core::advice::builtin_get_variable_watchers),
            SubrArity::new(1, Some(1)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "get-pos-property",
        builtin_get_pos_property_3,
        FixedMin3::Two,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
        "next-single-char-property-change",
        builtin_next_single_char_property_change_4,
        FixedMin4::Two,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "previous-single-char-property-change",
        NativeFn::ContextVec(builtin_previous_single_char_property_change),
        SubrArity::new(2, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "next-char-property-change",
        NativeFn::ContextVec(builtin_next_char_property_change),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "previous-property-change",
        NativeFn::ContextVec(builtin_previous_property_change),
        SubrArity::new(1, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "previous-char-property-change",
        NativeFn::ContextVec(builtin_previous_char_property_change),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "pos-bol",
        builtin_pos_bol_1,
        FixedMin1::Zero,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "pos-eol",
        builtin_pos_eol_1,
        FixedMin1::Zero,
    )),
    // `transient-mark-mode' the FUNCTION is not here: it is a
    // `define-minor-mode' at lisp/simple.el:7614.  Only the VARIABLE is C
    // (DEFVAR_LISP, src/buffer.c:5835), and that stays (DIVERGENCES.md 152).
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-local-variable",
            NativeFn::ContextSlice(crate::emacs_core::custom::builtin_make_local_variable),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                "vMake Local Variable: ",
            ),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "local-variable-p",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_local_variable_p),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-local-variables",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_buffer_local_variables),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "kill-local-variable",
            NativeFn::ContextVec(crate::emacs_core::custom::builtin_kill_local_variable),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                "vKill Local Variable: ",
            ),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "autoload",
        NativeFn::ContextVec(crate::emacs_core::autoload::builtin_autoload),
        SubrArity::new(2, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "autoload-do-load",
        crate::emacs_core::autoload::builtin_autoload_do_load_3,
        FixedMin3::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-idle-time",
        NativeFn::ContextVec(builtin_current_idle_time),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "format",
        NativeFn::ContextSlice(builtin_format_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "format-message",
        NativeFn::ContextSlice(builtin_format_message_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "message-box",
            NativeFn::ContextVec(builtin_message_box),
            SubrArity::new(1, None),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "message-or-box",
            NativeFn::ContextVec(builtin_message_or_box),
            SubrArity::new(1, None),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-message",
        NativeFn::ContextSlice(builtin_current_message),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-from-string",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_from_string),
        SubrArity::new(1, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "recent-keys",
        NativeFn::ContextVec(builtin_recent_keys),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "princ",
            NativeFn::ContextVec(builtin_princ),
            SubrArity::new(1, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "prin1",
            NativeFn::ContextVec(builtin_prin1),
            SubrArity::new(1, Some(3)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "prin1-to-string",
            NativeFn::ContextVec(builtin_prin1_to_string),
            SubrArity::new(1, Some(3)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "print",
            NativeFn::ContextVec(builtin_print),
            SubrArity::new(1, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "terpri",
            NativeFn::ContextVec(builtin_terpri),
            SubrArity::new(0, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "write-char",
            NativeFn::ContextVec(builtin_write_char),
            SubrArity::new(1, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "backtrace--locals",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_locals),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "backtrace-debug",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_debug),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "backtrace-eval",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_eval),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "backtrace-frame--internal",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_frame_internal),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "recursion-depth",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_recursion_depth),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "kill-emacs",
            NativeFn::ContextVec(builtin_kill_emacs),
            SubrArity::new(0, Some(2)),
        ))
        .requires_eval_state()
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("P")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-thread",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_make_thread),
        SubrArity::new(1, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-join",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_join),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-yield",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_yield),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-name",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_name),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-live-p",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_live_p),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "threadp",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_threadp),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-signal",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_signal),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-thread",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_current_thread),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "all-threads",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_all_threads),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-last-error",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_last_error),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-mutex",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_make_mutex),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mutex-name",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutex_name),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mutex-lock",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutex_lock),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mutex-unlock",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutex_unlock),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mutexp",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutexp),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-condition-variable",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_make_condition_variable),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "condition-variable-p",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_variable_p),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "condition-name",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_name),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "condition-mutex",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_mutex),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "condition-wait",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_wait),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "condition-notify",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_notify),
        SubrArity::new(1, Some(2)),
    )),
    // No `undo' and no `buffer-disable-undo' subr: GNU has neither in C.
    // `syms_of_undo' (src/undo.c:423-490) registers only `&Sundo_boundary'
    // (:435); `undo' is (defun undo (&optional arg) ...) at
    // lisp/simple.el:3466 and `buffer-disable-undo' is
    // (defun buffer-disable-undo (&optional buffer) ...) at
    // lisp/simple.el:3591.  Its partner `buffer-enable-undo' IS in C
    // (src/buffer.c:1829) and is registered above -- the pair is asymmetric
    // in GNU, and copying that asymmetry is the point.  DIVERGENCES.md 150.
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "maphash",
        NativeFn::ContextVec(crate::emacs_core::builtins::maphash::builtin_maphash),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mapatoms",
        NativeFn::ContextVec(crate::emacs_core::builtins::symbols::builtin_mapatoms),
        SubrArity::new(1, Some(2)),
    )),
    // GNU `Sunintern` is `2, 2, 0`: the OBARRAY argument is mandatory (it may
    // be nil to default to `obarray`, but it must be supplied).
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unintern",
        NativeFn::ContextVec(crate::emacs_core::builtins::symbols::builtin_unintern),
        SubrArity::new(2, Some(2)),
    )),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
                "assoc",
                builtin_assoc_3,
                FixedMin3::Two
            ))
            .requires_eval_state()
            .leaf(&leaves::ASSOC)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "plist-member",
            NativeFn::ContextVec(builtin_plist_member),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "documentation",
        NativeFn::ContextVec(crate::emacs_core::doc::builtin_documentation),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "documentation-property",
        NativeFn::ContextVec(crate::emacs_core::doc::builtin_documentation_property),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "eval-buffer",
            NativeFn::ContextVec(crate::emacs_core::lread::builtin_eval_buffer),
            SubrArity::new(0, Some(5)),
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "eval-region",
            NativeFn::ContextVec(crate::emacs_core::lread::builtin_eval_region),
            SubrArity::new(2, Some(4)),
        ))
        .interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("r")),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "error-message-string",
        NativeFn::ContextVec(crate::emacs_core::print::builtin_error_message_string),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "char-equal",
        builtin_char_equal_2,
        FixedMin2::Two,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "posix-search-forward",
            NativeFn::ContextVec(crate::emacs_core::builtins::search::builtin_posix_search_forward),
            SubrArity::new(1, Some(4)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String("sPosix search: "),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "posix-search-backward",
            NativeFn::ContextVec(
                crate::emacs_core::builtins::search::builtin_posix_search_backward,
            ),
            SubrArity::new(1, Some(4)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                "sPosix search backward: ",
            ),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-hooks",
        NativeFn::ContextVec(run_hooks_traced),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "load",
        NativeFn::ContextVec(load_traced),
        SubrArity::new(1, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "message",
            NativeFn::ContextSlice(message_traced),
            SubrArity::new(1, None),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-aliases",
        NativeFn::ContextVec(coding_system_aliases),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-plist",
        NativeFn::ContextVec(coding_system_plist),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-put",
        NativeFn::ContextVec(coding_system_put),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-base",
        NativeFn::ContextVec(coding_system_base),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-eol-type",
        NativeFn::ContextSlice(coding_system_eol_type),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "detect-coding-string",
        NativeFn::ContextVec(detect_coding_string),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "detect-coding-region",
        NativeFn::ContextVec(detect_coding_region),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "keyboard-coding-system",
        NativeFn::ContextVec(keyboard_coding_system),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "terminal-coding-system",
        NativeFn::ContextVec(terminal_coding_system),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-priority-list",
        NativeFn::ContextVec(coding_system_priority_list),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "symbol-with-pos-p",
        builtin_symbol_with_pos_p_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "symbol-with-pos-pos",
        builtin_symbol_with_pos_pos_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "sqrt",
        builtin_sqrt_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "sin",
        builtin_sin_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "cos",
        builtin_cos_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "tan",
        builtin_tan_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "asin",
        builtin_asin_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "acos",
        builtin_acos_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "exp",
        builtin_exp_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-string",
        NativeFn::ContextVec(crate::emacs_core::builtins::strings::builtin_make_string_in_context),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-width",
        NativeFn::ContextVec(builtin_string_width),
        SubrArity::new(1, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete",
        NativeFn::ContextVec(builtin_delete_with_ctx),
        SubrArity::new(2, Some(2)),
    )),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
                "delq",
                builtin_delq_2,
                FixedMin2::Two
            ))
            .leaf(&leaves::DELQ)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "elt",
        NativeFn::ContextVec(builtin_elt_in_state),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "memql",
        builtin_memql_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "nconc",
        NativeFn::ContextSlice(builtin_nconc_slice),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "daemonp",
        NativeFn::ContextVec(builtin_daemonp),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "daemon-initialized",
        NativeFn::ContextVec(builtin_daemon_initialized),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "force-mode-line-update",
        NativeFn::ContextVec(builtin_force_mode_line_update),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "invocation-directory",
        NativeFn::ContextVec(builtin_invocation_directory),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "invocation-name",
        NativeFn::ContextVec(builtin_invocation_name),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-fringe-bitmap",
        NativeFn::ContextVec(builtin_define_fringe_bitmap),
        SubrArity::new(2, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "destroy-fringe-bitmap",
        NativeFn::ContextVec(builtin_destroy_fringe_bitmap),
        SubrArity::new(1, Some(1)),
    )),
    // `make-auto-save-file-name' is not here: it is a `defun' at
    // lisp/files.el:7699 over `auto-save-file-name-transforms'.  GNU's C
    // side only READS the buffer field (src/fileio.c:6406)
    // (DIVERGENCES.md 152).
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "external-debugging-output",
        NativeFn::ContextVec(builtin_external_debugging_output),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "describe-vector",
        NativeFn::ContextVec(builtin_describe_vector),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "gap-position",
            NativeFn::ContextVec(builtin_gap_position),
            SubrArity::new(0, Some(0)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "gap-size",
            NativeFn::ContextVec(builtin_gap_size),
            SubrArity::new(0, Some(0)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "garbage-collect-maybe",
        NativeFn::ContextVec(builtin_garbage_collect_maybe),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal--track-mouse",
        NativeFn::ContextVec(builtin_internal_track_mouse),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-complete-buffer",
        NativeFn::ContextVec(builtin_internal_complete_buffer),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-describe-syntax-value",
        NativeFn::ContextVec(builtin_internal_describe_syntax_value),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-event-symbol-parse-modifiers",
        NativeFn::ContextVec(builtin_internal_event_symbol_parse_modifiers),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-handle-focus-in",
        NativeFn::ContextVec(builtin_internal_handle_focus_in),
        SubrArity::new(1, Some(1)),
    )),
    // byte-code: mirrors GNU Emacs Fbyte_code (src/bytecode.c).
    // Receives pre-evaluated args (bytestr, vector, maxdepth), decodes
    // the GNU bytecodes, and executes them via the bytecode VM.
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "byte-code",
        NativeFn::ContextVec(crate::emacs_core::builtins::builtin_byte_code),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "find-operation-coding-system",
            NativeFn::ContextVec(builtin_find_operation_coding_system),
            SubrArity::new(1, None),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "keymap-prompt",
        NativeFn::ContextVec(builtin_keymap_prompt),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-finalizer",
        NativeFn::ContextVec(builtin_make_finalizer),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "map-charset-chars",
        NativeFn::ContextVec(builtin_map_charset_chars),
        SubrArity::new(2, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "mapbacktrace",
            NativeFn::ContextVec(crate::emacs_core::misc::builtin_mapbacktrace),
            SubrArity::new(1, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "object-intervals",
        NativeFn::ContextVec(builtin_object_intervals),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "position-symbol",
        NativeFn::ContextVec(builtin_position_symbol),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-cpu-log",
        NativeFn::ContextVec(builtin_profiler_cpu_log),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-cpu-running-p",
        NativeFn::ContextVec(builtin_profiler_cpu_running_p),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-cpu-start",
        NativeFn::ContextVec(builtin_profiler_cpu_start),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-cpu-stop",
        NativeFn::ContextVec(builtin_profiler_cpu_stop),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-memory-log",
        NativeFn::ContextVec(builtin_profiler_memory_log),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-memory-running-p",
        NativeFn::ContextVec(builtin_profiler_memory_running_p),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-memory-start",
        NativeFn::ContextVec(builtin_profiler_memory_start),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "profiler-memory-stop",
        NativeFn::ContextVec(builtin_profiler_memory_stop),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "redisplay",
        NativeFn::ContextVec(builtin_redisplay),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "recordp",
        builtin_recordp_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set--this-command-keys",
        NativeFn::ContextVec(builtin_set_this_command_keys),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-buffer-major-mode",
        NativeFn::ContextVec(builtin_set_buffer_major_mode),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-buffer-redisplay",
        NativeFn::ContextVec(builtin_set_buffer_redisplay),
        SubrArity::new(4, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-fringe-bitmap-face",
        NativeFn::ContextVec(builtin_set_fringe_bitmap_face),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-minibuffer-window",
        NativeFn::ContextVec(builtin_set_minibuffer_window),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "thread--blocker",
            NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_blocker),
            SubrArity::new(1, Some(1)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "transpose-regions",
            NativeFn::ContextVec(builtin_transpose_regions),
            SubrArity::new(4, Some(5)),
        ))
        .requires_eval_state()
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(
                transpose_regions_interactive_spec,
            ),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "value<",
        NativeFn::ContextVec(builtin_value_lt),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-induce-sparse-tree",
        NativeFn::ContextVec(builtin_treesit_induce_sparse_tree),
        SubrArity::new(2, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-language-abi-version",
        NativeFn::ContextVec(builtin_treesit_language_abi_version),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-language-available-p",
        NativeFn::ContextVec(builtin_treesit_language_available_p),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-check",
        NativeFn::ContextVec(builtin_treesit_node_check),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-child",
        NativeFn::ContextVec(builtin_treesit_node_child),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-child-by-field-name",
        NativeFn::ContextVec(builtin_treesit_node_child_by_field_name),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-child-count",
        NativeFn::ContextVec(builtin_treesit_node_child_count),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-descendant-for-range",
        NativeFn::ContextVec(builtin_treesit_node_descendant_for_range),
        SubrArity::new(3, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-end",
        NativeFn::ContextVec(builtin_treesit_node_end),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-eq",
        NativeFn::ContextVec(builtin_treesit_node_eq),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-field-name-for-child",
        NativeFn::ContextVec(builtin_treesit_node_field_name_for_child),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-first-child-for-pos",
        NativeFn::ContextVec(builtin_treesit_node_first_child_for_pos),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-match-p",
        NativeFn::ContextVec(builtin_treesit_node_match_p),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-next-sibling",
        NativeFn::ContextVec(builtin_treesit_node_next_sibling),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-parent",
        NativeFn::ContextVec(builtin_treesit_node_parent),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-prev-sibling",
        NativeFn::ContextVec(builtin_treesit_node_prev_sibling),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-start",
        NativeFn::ContextVec(builtin_treesit_node_start),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-string",
        NativeFn::ContextVec(builtin_treesit_node_string),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-type",
        NativeFn::ContextVec(builtin_treesit_node_type),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-add-notifier",
        NativeFn::ContextVec(builtin_treesit_parser_add_notifier),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-buffer",
        NativeFn::ContextVec(builtin_treesit_parser_buffer),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-create",
        NativeFn::ContextVec(builtin_treesit_parser_create),
        SubrArity::new(1, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-delete",
        NativeFn::ContextVec(builtin_treesit_parser_delete),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-included-ranges",
        NativeFn::ContextVec(builtin_treesit_parser_included_ranges),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-language",
        NativeFn::ContextVec(builtin_treesit_parser_language),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-list",
        NativeFn::ContextVec(builtin_treesit_parser_list),
        SubrArity::new(0, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-notifiers",
        NativeFn::ContextVec(builtin_treesit_parser_notifiers),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-remove-notifier",
        NativeFn::ContextVec(builtin_treesit_parser_remove_notifier),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-root-node",
        NativeFn::ContextVec(builtin_treesit_parser_root_node),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-set-included-ranges",
        NativeFn::ContextVec(builtin_treesit_parser_set_included_ranges),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-tag",
        NativeFn::ContextVec(builtin_treesit_parser_tag),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-pattern-expand",
        NativeFn::ContextVec(builtin_treesit_pattern_expand_in_state),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-capture",
        NativeFn::ContextVec(builtin_treesit_query_capture),
        SubrArity::new(2, Some(6)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-compile",
        NativeFn::ContextVec(builtin_treesit_query_compile),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-expand",
        NativeFn::ContextVec(builtin_treesit_query_expand_in_state),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-search-forward",
        NativeFn::ContextVec(builtin_treesit_search_forward),
        SubrArity::new(2, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-search-subtree",
        NativeFn::ContextVec(builtin_treesit_search_subtree),
        SubrArity::new(2, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-subtree-stat",
        NativeFn::ContextVec(builtin_treesit_subtree_stat),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-grammar-location",
        NativeFn::ContextVec(builtin_treesit_grammar_location),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-tracking-line-column-p",
        NativeFn::ContextVec(builtin_treesit_tracking_line_column_p),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-tracking-line-column-p",
        NativeFn::ContextVec(builtin_treesit_parser_tracking_line_column_p),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-eagerly-compiled-p",
        NativeFn::ContextVec(builtin_treesit_query_eagerly_compiled_p),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-embed-level",
        NativeFn::ContextVec(builtin_treesit_parser_embed_level),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-set-embed-level",
        NativeFn::ContextVec(builtin_treesit_parser_set_embed_level),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parse-string",
        NativeFn::ContextVec(builtin_treesit_parse_string),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-changed-regions",
        NativeFn::ContextVec(builtin_treesit_parser_changed_regions),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit--linecol-at",
        NativeFn::ContextVec(builtin_treesit_linecol_at),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit--linecol-cache-set",
        NativeFn::ContextVec(builtin_treesit_linecol_cache_set),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit--linecol-cache",
        NativeFn::ContextVec(builtin_treesit_linecol_cache),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-hash-table-test",
        NativeFn::ContextVec(builtin_define_hash_table_test_in_state),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-clipboard-set",
        NativeFn::ContextVec(builtin_neomacs_clipboard_set),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-clipboard-get",
        NativeFn::ContextVec(builtin_neomacs_clipboard_get),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-primary-selection-set",
        NativeFn::ContextVec(builtin_neomacs_primary_selection_set),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-primary-selection-get",
        NativeFn::ContextVec(builtin_neomacs_primary_selection_get),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-primary-selection-owner",
        NativeFn::ContextVec(builtin_neomacs_primary_selection_owner),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_buffer_text_backend),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-default-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_default_buffer_text_backend),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-set-default-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_set_default_buffer_text_backend),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-set-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_set_buffer_text_backend),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-local-toplevel-value",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_buffer_local_toplevel_value),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-buffer-local-toplevel-value",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_set_buffer_local_toplevel_value),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-delete-indirect-variable",
        NativeFn::ContextVec(builtin_internal_delete_indirect_variable),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-buffer-disposition",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_buffer_disposition),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "thread-set-buffer-disposition",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_set_buffer_disposition),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "cancel-kbd-macro-events",
        NativeFn::ContextVec(builtin_cancel_kbd_macro_events),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "internal--define-uninitialized-variable",
            NativeFn::ContextVec(crate::emacs_core::builtins::symbols::builtin_internal_define_uninitialized_variable),
            SubrArity::new(1, Some(2)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-variable-buffer-local",
            NativeFn::ContextVec(crate::emacs_core::custom::builtin_make_variable_buffer_local),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(
                "vMake Variable Buffer Local: ",
            ),
        ),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "backtrace--frames-from-thread",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_frames_from_thread),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("take", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_take(args)), SubrArity::new(2, Some(2)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "assoc-string",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_assoc_string_in_state),
        SubrArity::new(2, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("string-search", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_string_search(args)), SubrArity::new(2, Some(3)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "bare-symbol",
        crate::emacs_core::builtins_extra::builtin_bare_symbol_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("bare-symbol-p", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_bare_symbol_p(args)), SubrArity::new(1, Some(1)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("byteorder", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_byteorder(args)), SubrArity::new(0, Some(0)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("car-less-than-car", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_car_less_than_car(args)), SubrArity::new(2, Some(2)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("proper-list-p", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_proper_list_p(args)), SubrArity::new(1, Some(1)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("byte-code-function-p", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_byte_code_function_p(args)), SubrArity::new(1, Some(1)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "closurep",
        crate::emacs_core::builtins_extra::builtin_closurep_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("natnump", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_natnump(args)), SubrArity::new(1, Some(1)))),
    // GNU defines `fixnump` and `bignump` in `lisp/subr.el` (not in C),
    // so they must come from the loaded Lisp source — registering Rust
    // subrs here would shadow the elisp definitions and make
    // `(subrp (symbol-function 'fixnump))` return t instead of nil.
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "user-login-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_user_login_name),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "user-real-login-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_user_real_login_name),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "user-full-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_user_full_name),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "system-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_system_name),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("emacs-pid", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_emacs_pid(args)), SubrArity::new(0, Some(0)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("memory-use-counts", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_memory_use_counts(args)), SubrArity::new(0, Some(0)))),
    crate::emacs_core::subr::world_subr!(SubrSpec::new("neomacs--heap-layout-stats", NativeFn::ContextVec(|_eval, args| crate::emacs_core::builtins_extra::builtin_neomacs_heap_layout_stats(args)), SubrArity::new(0, Some(0)))),
    // -- Arithmetic --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "+",
        NativeFn::ContextSlice(crate::emacs_core::builtins::arithmetic::builtin_add_slice),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "-",
        NativeFn::ContextSlice(crate::emacs_core::builtins::arithmetic::builtin_sub_slice),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "%",
        builtin_percent,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "mod",
        builtin_mod,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "1+",
        builtin_add1_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "1-",
        builtin_sub1_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "max",
        NativeFn::ContextSlice(builtin_max_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "min",
        NativeFn::ContextSlice(builtin_min_slice),
        SubrArity::new(1, None),
    )),
    // -- Numeric comparisons --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "=",
        NativeFn::ContextSlice(builtin_num_eq_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "<",
        NativeFn::ContextSlice(builtin_num_lt_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "<=",
        NativeFn::ContextSlice(builtin_num_le_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        ">",
        NativeFn::ContextSlice(builtin_num_gt_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        ">=",
        NativeFn::ContextSlice(builtin_num_ge_slice),
        SubrArity::new(1, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "/=",
        builtin_num_ne_2,
        FixedMin2::Two
    )),
    // -- Type predicates --
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "null",
        builtin_null_1,
        FixedMin1::One
    )),
    // No `not': GNU has no DEFUN of that name.  `(defalias 'not #'null)'
    // (lisp/subr.el:71) puts the SYMBOL `null' in the cell, and a compiled
    // caller emits the Bnot opcode instead -- DIVERGENCES.md 148.
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "atom",
        builtin_atom_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "consp",
        builtin_consp_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "listp",
        builtin_listp_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "nlistp",
        builtin_nlistp_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "symbolp",
        builtin_symbolp_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "numberp",
        builtin_numberp_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "integerp",
        builtin_integerp_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "floatp",
        builtin_floatp_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "stringp",
        builtin_stringp_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "vectorp",
        builtin_vectorp_1,
        FixedMin1::One,
    )),
    // No `booleanp', `integer-or-null-p', `string-or-null-p',
    // `list-of-strings-p' or `char-uppercase-p': GNU has no DEFUN for any of
    // the six type predicates.  They are `defun's at lisp/subr.el:4762-4812
    // and lisp/simple.el:6683 over primitives that ARE in C
    // (`stringp', `integerp', `indirect-function',
    // `get-char-code-property') -- DIVERGENCES.md 148.
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
                "keywordp",
                builtin_keywordp_1,
                FixedMin1::One
            ))
            .leaf(&leaves::KEYWORDP)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "type-of",
        crate::emacs_core::builtins::types::builtin_type_of_with_ctx_1,
        FixedMin1::One,
    )),
    // -- Equality --
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "eq",
        builtin_eq_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "eql",
        builtin_eql_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "equal",
        builtin_equal_2,
        FixedMin2::Two
    )),
    // -- Cons / List --
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "cons",
        builtin_cons_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "car",
        builtin_car_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "cdr",
        builtin_cdr_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "car-safe",
        builtin_car_safe_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "cdr-safe",
        builtin_cdr_safe_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "setcar",
        builtin_setcar_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "setcdr",
        builtin_setcdr_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "list",
        NativeFn::ContextSlice(builtin_list_slice),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "length",
        builtin_length_1,
        FixedMin1::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "nth",
        builtin_nth_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "nthcdr",
        builtin_nthcdr_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "append",
        NativeFn::ContextSlice(builtin_append_slice),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "nreverse",
        builtin_nreverse_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "member",
        builtin_member_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "memq",
        builtin_memq_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "assq",
        builtin_assq_2,
        FixedMin2::Two
    )),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
                "copy-sequence",
                builtin_copy_sequence_1,
                FixedMin1::One
            ))
            .leaf(&leaves::COPY_SEQUENCE)
        },
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
                "plist-get",
                builtin_plist_get_3,
                FixedMin3::Two
            ))
            .leaf(&leaves::PLIST_GET)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "plist-put",
        NativeFn::ContextVec(builtin_plist_put_with_ctx),
        SubrArity::new(3, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "rassoc",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_rassoc_with_ctx),
        SubrArity::new(2, Some(2)),
    )),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
                "rassq",
                crate::emacs_core::misc::builtin_rassq_2,
                FixedMin2::Two,
            ))
            .leaf(&leaves::RASSQ)
        },
    // -- String --
    // GNU DEFUNs `string-equal' and `string-lessp' (src/fns.c) and nothing
    // else here: `string=', `string<' and `string>' are `defalias'es at
    // lisp/subr.el:2277-2279, so their cells hold the TARGET SYMBOL, and a
    // compiled caller emits Bstringeqlsign / Bstringlss instead
    // (DIVERGENCES.md 148).  `string-greaterp' is itself Lisp
    // (lisp/subr.el:6283) with a `compiler-macro' (:6287-6290) that swaps
    // its arguments into `string-lessp', so it is not registered either
    // (DIVERGENCES.md 152).
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "string-equal",
        builtin_string_equal_2,
        FixedMin2::Two,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "string-lessp",
        builtin_string_lessp_2,
        FixedMin2::Two,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "string-to-number",
        builtin_string_to_number,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "upcase",
        builtin_upcase_in_state_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "downcase",
        builtin_downcase_in_state_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "compare-strings",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_compare_strings_in_state),
        SubrArity::new(6, Some(7)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-version-lessp",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_string_version_lessp),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-collate-lessp",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_string_collate_lessp),
        SubrArity::new(2, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-collate-equalp",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_string_collate_equalp),
        SubrArity::new(2, Some(4)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "equal-including-properties",
        crate::emacs_core::fns::builtin_equal_including_properties_2,
        FixedMin2::Two,
    )),
    // -- Vector --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-vector",
        NativeFn::ContextVec(crate::emacs_core::builtins::collections::builtin_make_vector_in_context),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "vector",
        NativeFn::ContextSlice(builtin_vector_slice),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "aref",
        builtin_aref_2,
        FixedMin2::Two
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "aset",
        NativeFn::ContextSlice(builtin_aset_args_in_state),
        SubrArity::new(3, Some(3)),
    )),
    // -- Hash table --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-hash-table",
        NativeFn::ContextSlice(builtin_make_hash_table_in_state),
        SubrArity::new(0, None),
    )),
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
                "gethash",
                builtin_gethash_3,
                FixedMin3::Two
            ))
            .leaf(&leaves::GETHASH)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "puthash",
        builtin_puthash_3,
        FixedMin3::Three,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "remhash",
        builtin_remhash_2,
        FixedMin2::Two,
    )),
    // GNU's rounding functions are fixed 1..2-argument subrs: a missing
    // divisor arrives as nil, which `rounding_driver` treats as omitted, as
    // GNU does. (A `Many` subr materialized an argument vector per call.)
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "truncate",
        builtin_truncate_2,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "floor",
        builtin_floor_2,
        FixedMin2::One
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "ceiling",
        builtin_ceiling_2,
        FixedMin2::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "round",
        builtin_round_2,
        FixedMin2::One
    )),
    // -- Symbol --
    const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
                "symbol-name",
                builtin_symbol_name_1,
                FixedMin1::One
            ))
            .leaf(&leaves::SYMBOL_NAME)
        },
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "make-symbol",
        builtin_make_symbol_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "clear-face-cache",
        NativeFn::ContextVec(builtin_clear_face_cache),
        SubrArity::new(0, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "combine-after-change-execute",
        NativeFn::ContextVec(builtin_combine_after_change_execute),
        SubrArity::new(0, Some(0)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "command-error-default-function",
        NativeFn::ContextVec(crate::emacs_core::eval::builtin_command_error_default_function),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "subr-arity",
        crate::emacs_core::subr_info::builtin_subr_arity_1,
        FixedMin1::One,
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "func-arity",
        NativeFn::ContextVec(builtin_func_arity),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-bool-vector",
        NativeFn::ContextVec(crate::emacs_core::builtins::collections::builtin_make_bool_vector_in_context),
        SubrArity::new(2, Some(2)),
    )),
    // -- Coding system (eval-dependent via coding_systems field) --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "coding-system-p",
        NativeFn::ContextVec(coding_system_p),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "check-coding-system",
        NativeFn::ContextVec(check_coding_system),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "check-coding-systems-region",
        NativeFn::ContextVec(check_coding_systems_region),
        SubrArity::new(3, Some(3)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "define-coding-system-internal",
            NativeFn::ContextVec(define_coding_system_internal),
            SubrArity::new(13, None),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-coding-system-alias",
        NativeFn::ContextVec(define_coding_system_alias),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-coding-system-priority",
        NativeFn::ContextVec(set_coding_system_priority),
        SubrArity::new(0, None),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-keyboard-coding-system-internal",
        NativeFn::ContextVec(set_keyboard_coding_system_internal),
        SubrArity::new(1, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-safe-terminal-coding-system-internal",
        NativeFn::ContextVec(set_safe_terminal_coding_system_internal),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-terminal-coding-system-internal",
        NativeFn::ContextVec(set_terminal_coding_system_internal),
        SubrArity::new(1, Some(2)),
    )),
    // -- CCL (eval-dependent) --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ccl-program-p",
        NativeFn::ContextVec(builtin_ccl_program_p),
        SubrArity::new(1, Some(1)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ccl-execute",
        NativeFn::ContextVec(builtin_ccl_execute),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ccl-execute-on-string",
        NativeFn::ContextVec(builtin_ccl_execute_on_string),
        SubrArity::new(3, Some(5)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "register-ccl-program",
        NativeFn::ContextVec(builtin_register_ccl_program),
        SubrArity::new(2, Some(2)),
    )),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "register-code-conversion-map",
        NativeFn::ContextVec(builtin_register_code_conversion_map),
        SubrArity::new(2, Some(2)),
    )),
    // -- Eval builtins (eval-dependent) --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "defconst-1",
            NativeFn::ContextVec(builtin_defconst_1),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "defvar-1",
            NativeFn::ContextVec(builtin_defvar_1),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state(),
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "locate-file-internal",
        NativeFn::ContextVec(crate::emacs_core::lread::builtin_locate_file_internal),
        SubrArity::new(2, Some(4)),
    )),
    // -- Documentation/help --
    crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "Snarf-documentation",
        NativeFn::ContextVec(crate::emacs_core::doc::builtin_snarf_documentation),
        SubrArity::new(1, Some(1)),
    )),
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
                .map(|declaration| intern(declaration.spec.name()).to_raw() as usize)
                .max()
                .unwrap_or(0);
            let mut handlers = vec![None; max_id + 1];
            for declaration in EVALUATOR_SUBRS {
                handlers[intern(declaration.spec.name()).to_raw() as usize] =
                    Some(declaration.handler);
            }
            handlers
        })
        .get(sym_id.to_raw() as usize)
        .copied()
        .flatten()
}

#[doc(hidden)]
pub fn evaluator_dispatch_kind(name: &str) -> Option<SubrDispatchKind> {
    evaluator_subr(name).map(|declaration| declaration.spec.dispatch_kind())
}

/// Materialize evaluator-handled special forms and callables at their original
/// late startup position.
#[doc(hidden)]
pub fn register_public_subrs(ctx: &mut Context) {
    for declaration in EVALUATOR_SUBRS {
        ctx.register_subr(declaration.spec);
    }
}

impl Context {
    /// Register one authoritative native Lisp declaration.
    ///
    /// This is the only path that installs native metadata, so call sites must
    /// construct a typed [`SubrSpec`] rather than passing loose parallel fields.
    #[doc(hidden)]
    pub fn register_subr(&mut self, spec: SubrSpec) {
        crate::emacs_core::subr::record_no_eval_policy(spec.name(), spec.no_eval_policy());
        let arity = spec.arity();
        #[cfg(any(test, feature = "test-support"))]
        crate::emacs_core::subr::record_registered_subr(spec.name(), arity);
        self.install_subr(
            spec.name(),
            spec.function(),
            arity,
            spec.dispatch_kind(),
            spec.interactive_spec(),
        );
        // The leaf comes from the same declaration as the subr object, so the
        // two cannot disagree. Registration runs before any JIT compile on
        // both the fresh and the pdump path; a leaf that CHANGES under an
        // already-registered builtin (only a test re-registers one) must not
        // stay baked into compiled code.
        let change =
            crate::emacs_core::subr::leaf::record_subr_leaf(intern(spec.name()), spec.leaf_spec());
        if change == crate::emacs_core::subr::leaf::LeafChange::Replaced {
            #[cfg(feature = "jit")]
            crate::emacs_core::jit::cache::clear();
        }

        if spec.command_default() == crate::emacs_core::subr::CommandDefault::Disabled {
            self.obarray
                .put_property(spec.name(), "disabled", Value::T)
                .expect("freshly registered subr must have a valid property list");
        }
    }

    #[doc(hidden)]
    pub fn register_subrs(&mut self, specs: &[SubrSpec]) {
        for &spec in specs {
            self.register_subr(spec);
        }
    }

    #[doc(hidden)]
    pub fn install_subr(
        &mut self,
        name: &str,
        function: Option<SubrFn>,
        arity: SubrArity,
        dispatch_kind: SubrDispatchKind,
        interactive_spec: Option<
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec,
        >,
    ) {
        let sym_id = intern(name);
        #[cfg(debug_assertions)]
        super::green::debug_assert_native_tables_open(name);

        register_global_subr_entry(
            sym_id,
            SubrEntry {
                function,
                min_args: arity.min(),
                max_args: arity.max(),
                dispatch_kind,
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
                .set_symbol_function(name, <Value as SubrValueExt>::subr_from_sym_id(sym_id));
        }
        // The static subr entry above was rewritten IN PLACE even when the
        // cell write was skipped — keep function_epoch a complete change
        // signal (JIT call-speculation validity depends on it).
        self.obarray
            .invalidate_all_function_bindings(neovm_value::symbol::FunctionEpochBump::SubrRewrite);
    }
}
