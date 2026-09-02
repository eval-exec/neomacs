//! Typed declarations for the legacy built-in startup surface.
//!
//! This is a compatibility manifest, not an implementation module. As a GNU
//! subsystem gains its own `register_subrs`, move its declarations beside that
//! implementation and leave only the sequencing call here.

use super::*;
#[cfg(test)]
use crate::emacs_core::subr::SubrBatch;

/// Localized declaration batches, in their current startup order.
///
/// Startup registrars and architecture checks consume the same compiled
/// `SubrBatch` values; this catalog records their reviewed relative order. The
/// legacy declarations below remain the explicit compatibility exception until
/// their owning subsystems adopt a batch.
#[cfg(test)]
const LOCALIZED_SUBR_CATALOG: &[SubrBatch] = &[
    #[cfg(windows)]
    crate::emacs_core::w32::SUBRS,
    lcms::SUBRS,
    crate::emacs_core::data::SUBRS,
    crate::emacs_core::eval::SUBRS,
    crate::emacs_core::composite::SUBRS,
    crate::emacs_core::neo::terminal::SUBRS,
    crate::emacs_core::shader_surface::SUBRS,
    crate::emacs_core::video::SUBRS,
    crate::emacs_core::xwidget::SUBRS,
    crate::emacs_core::indent::SUBRS,
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    file_notify::SUBRS,
    crate::emacs_core::sqlite::SUBRS,
    crate::emacs_core::font::SUBRS,
    crate::emacs_core::neo::effects::SUBRS,
];

#[cfg(test)]
pub(crate) const fn localized_subr_catalog() -> &'static [SubrBatch] {
    LOCALIZED_SUBR_CATALOG
}

// These states encode the reviewed Neomacs compatibility order around the
// declarations localized from data/eval. They are deliberately not presented
// as GNU startup phases; the GNU source audit shows a different grouping.
struct NeedsData;
struct NeedsOrdinaryEval;
struct NeedsFboundp;
struct NeedsEventProperties;
struct NeedsPublicEval;
struct EvaluatorCompatibilityComplete;

#[must_use = "the evaluator compatibility sequence must be completed"]
struct EvaluatorCompatibility<State>(std::marker::PhantomData<State>);

const FBOUNDP_SUBR: SubrSpec = SubrSpec::fixed1("fboundp", builtin_fboundp_1, FixedMin1::One);

impl EvaluatorCompatibility<NeedsData> {
    const fn begin() -> Self {
        Self(std::marker::PhantomData)
    }

    fn register_data(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> EvaluatorCompatibility<NeedsOrdinaryEval> {
        crate::emacs_core::data::register_subrs(ctx);
        EvaluatorCompatibility(std::marker::PhantomData)
    }
}

impl EvaluatorCompatibility<NeedsOrdinaryEval> {
    fn register_ordinary_eval(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> EvaluatorCompatibility<NeedsFboundp> {
        crate::emacs_core::eval::register_subrs(ctx);
        EvaluatorCompatibility(std::marker::PhantomData)
    }
}

impl EvaluatorCompatibility<NeedsFboundp> {
    fn register_fboundp(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> EvaluatorCompatibility<NeedsEventProperties> {
        ctx.register_subr(FBOUNDP_SUBR);
        EvaluatorCompatibility(std::marker::PhantomData)
    }
}

impl EvaluatorCompatibility<NeedsEventProperties> {
    fn initialize_event_properties(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> EvaluatorCompatibility<NeedsPublicEval> {
        symbols::init_event_symbol_properties(&mut ctx.obarray);
        EvaluatorCompatibility(std::marker::PhantomData)
    }
}

impl EvaluatorCompatibility<NeedsPublicEval> {
    fn register_public_eval(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> EvaluatorCompatibility<EvaluatorCompatibilityComplete> {
        crate::emacs_core::eval::register_public_subrs(ctx);
        EvaluatorCompatibility(std::marker::PhantomData)
    }
}

impl EvaluatorCompatibility<EvaluatorCompatibilityComplete> {
    const fn finish(self) {}
}

/// Register the Rust-backed Elisp startup surface.
///
/// Subsystem-owned registrars are called explicitly at their GNU-compatible
/// initialization point. The remaining declarations are typed compatibility
/// entries awaiting localization; every path installs a [`SubrSpec`] in the
/// same static registry.
pub(crate) fn register_subrs(ctx: &mut crate::emacs_core::eval::Context) {
    use crate::emacs_core::value::*;

    #[cfg(windows)]
    crate::emacs_core::w32::register_subrs(ctx);
    lcms::register_subrs(ctx);
    // Diagnostics-only VM-profiler control subrs (feature `vm-profile`).
    #[cfg(feature = "vm-profile")]
    {
        ctx.register_subr(SubrSpec::new(
            "neovm--vm-profile-reset",
            NativeFn::ContextVec(vm_profile_reset),
            SubrArity::new(0, Some(0)),
        ));
        ctx.register_subr(SubrSpec::new(
            "neovm--vm-profile-dump",
            NativeFn::ContextVec(vm_profile_dump),
            SubrArity::new(0, Some(1)),
        ));
    }
    ctx.register_subr(SubrSpec::new(
        "neovm--internal-panic",
        NativeFn::ContextVec(neovm_internal_panic),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "apply",
        NativeFn::ContextSlice(builtin_apply_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "funcall",
        NativeFn::ContextSlice(builtin_funcall_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "funcall-interactively",
        NativeFn::ContextSlice(builtin_funcall_interactively_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "funcall-with-delayed-message",
        NativeFn::ContextVec(builtin_funcall_with_delayed_message),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "defalias",
        NativeFn::ContextVec(builtin_defalias),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "provide",
        NativeFn::ContextVec(builtin_provide),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "require",
        NativeFn::ContextVec(builtin_require),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mapcan",
        NativeFn::ContextVec(builtin_mapcan),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::fixed2("mapcar", builtin_mapcar_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("mapc", builtin_mapc_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "mapconcat",
        NativeFn::ContextVec(builtin_mapconcat),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sort",
        NativeFn::ContextSlice(builtin_sort_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(
        SubrSpec::fixed1("functionp", builtin_functionp_1, FixedMin1::One).requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "defvaralias",
            NativeFn::ContextVec(builtin_defvaralias),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::fixed1("boundp", builtin_boundp_1, FixedMin1::One));
    let evaluator_compatibility = EvaluatorCompatibility::begin()
        .register_data(ctx)
        .register_ordinary_eval(ctx)
        .register_fboundp(ctx);
    ctx.register_subr(SubrSpec::new(
        "internal-make-var-non-special",
        NativeFn::ContextVec(builtin_internal_make_var_non_special),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "indirect-variable",
            NativeFn::ContextVec(builtin_indirect_variable),
            SubrArity::new(1, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "handler-bind-1",
            NativeFn::ContextVec(builtin_handler_bind_1),
            SubrArity::new(1, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::fixed1(
        "symbol-value",
        builtin_symbol_value_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "symbol-function",
        builtin_symbol_function_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed2("set", builtin_set_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "fset",
        NativeFn::ContextVec(builtin_fset),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "makunbound",
        NativeFn::ContextVec(builtin_makunbound),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "fmakunbound",
        NativeFn::ContextVec(builtin_fmakunbound),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "macroexpand",
            NativeFn::ContextVec(builtin_macroexpand),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::fixed2("get", builtin_get_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed3("put", builtin_put_3, FixedMin3::Three));
    ctx.register_subr(
        SubrSpec::new(
            "setplist",
            NativeFn::ContextVec(builtin_setplist),
            SubrArity::new(2, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "symbol-plist",
        NativeFn::ContextVec(builtin_symbol_plist_fn),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "indirect-function",
        NativeFn::ContextVec(builtin_indirect_function),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "signal",
        NativeFn::ContextVec(crate::emacs_core::errors::builtin_signal),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "getenv-internal",
        NativeFn::ContextVec(crate::emacs_core::environment::builtin_getenv_internal),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "special-variable-p",
        builtin_special_variable_p_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::new(
        "intern",
        NativeFn::ContextVec(builtin_intern_fn),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "intern-soft",
        NativeFn::ContextVec(builtin_intern_soft),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "run-hook-with-args",
        NativeFn::ContextVec(builtin_run_hook_with_args),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "run-hook-with-args-until-success",
        NativeFn::ContextVec(builtin_run_hook_with_args_until_success),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "run-hook-with-args-until-failure",
        NativeFn::ContextVec(builtin_run_hook_with_args_until_failure),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "run-hook-wrapped",
        NativeFn::ContextVec(builtin_run_hook_wrapped),
        SubrArity::new(2, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "run-window-configuration-change-hook",
        NativeFn::ContextVec(hooks::builtin_run_window_configuration_change_hook),
        SubrArity::new(0, Some(1)),
    ));
    // GNU emacs.c sequences composite.c before window.c/xdisp.c.
    crate::emacs_core::composite::register_subrs(ctx);
    ctx.register_subr(SubrSpec::new(
        "run-window-scroll-functions",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_run_window_scroll_functions),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "featurep",
        NativeFn::ContextVec(builtin_featurep),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "garbage-collect",
            NativeFn::ContextVec(builtin_garbage_collect),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::fixed2("eval", builtin_eval_2, FixedMin2::One));
    ctx.register_subr(SubrSpec::new(
        "get-buffer-create",
        NativeFn::ContextVec(builtin_get_buffer_create),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-buffer",
        NativeFn::ContextVec(builtin_get_buffer),
        SubrArity::new(1, Some(1)),
    ));
    crate::emacs_core::neo::terminal::register_subrs(ctx);
    crate::emacs_core::shader_surface::register_subrs(ctx);
    crate::emacs_core::video::register_subrs(ctx);
    crate::emacs_core::xwidget::register_subrs(ctx);
    ctx.register_subr(
        SubrSpec::new(
            "make-indirect-buffer",
            NativeFn::ContextVec(builtin_make_indirect_buffer),
            SubrArity::new(2, Some(4)),
        )
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "bMake indirect buffer (to buffer): \nBName of indirect buffer: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "find-buffer",
        NativeFn::ContextVec(builtin_find_buffer),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-live-p",
        NativeFn::ContextVec(builtin_buffer_live_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "barf-if-buffer-read-only",
        NativeFn::ContextVec(builtin_barf_if_buffer_read_only),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bury-buffer-internal",
        NativeFn::ContextVec(builtin_bury_buffer_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-file-buffer",
        NativeFn::ContextVec(builtin_get_file_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "kill-buffer",
            NativeFn::ContextVec(builtin_kill_buffer),
            SubrArity::new(0, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("bKill buffer: "),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "set-buffer",
        NativeFn::ContextVec(builtin_set_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-buffer",
        NativeFn::ContextVec(builtin_current_buffer),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-name",
        NativeFn::ContextVec(builtin_buffer_name),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-file-name",
        NativeFn::ContextVec(builtin_buffer_file_name),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-base-buffer",
        NativeFn::ContextVec(builtin_buffer_base_buffer),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-last-name",
        NativeFn::ContextVec(builtin_buffer_last_name),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "rename-buffer",
            NativeFn::ContextVec(builtin_rename_buffer),
            SubrArity::new(1, Some(2)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                rename_buffer_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "buffer-string",
        NativeFn::ContextVec(builtin_buffer_string),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-line-statistics",
        NativeFn::ContextVec(builtin_buffer_line_statistics),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-text-pixel-size",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_buffer_text_pixel_size),
        SubrArity::new(0, Some(4)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "base64-encode-region",
            NativeFn::ContextVec(crate::emacs_core::fns::builtin_base64_encode_region),
            SubrArity::new(2, Some(3)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "base64-decode-region",
            NativeFn::ContextVec(crate::emacs_core::fns::builtin_base64_decode_region),
            SubrArity::new(2, Some(4)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "base64url-encode-region",
            NativeFn::ContextVec(crate::emacs_core::fns::builtin_base64url_encode_region),
            SubrArity::new(2, Some(3)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(SubrSpec::new(
        "md5",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_md5),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "secure-hash",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_secure_hash),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-hash",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_buffer_hash),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-substring",
        NativeFn::ContextVec(builtin_buffer_substring),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "compare-buffer-substrings",
        NativeFn::ContextVec(builtin_compare_buffer_substrings),
        SubrArity::new(6, Some(6)),
    ));
    ctx.register_subr(SubrSpec::fixed0("point", builtin_point_0));
    ctx.register_subr(SubrSpec::fixed0("point-min", builtin_point_min_0));
    ctx.register_subr(SubrSpec::fixed0("point-max", builtin_point_max_0));
    ctx.register_subr(
        SubrSpec::fixed1("goto-char", builtin_goto_char_1, FixedMin1::One).interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                goto_char_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "field-beginning",
        NativeFn::ContextVec(builtin_field_beginning),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "field-end",
        NativeFn::ContextVec(builtin_field_end),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "field-string",
        NativeFn::ContextVec(builtin_field_string),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "field-string-no-properties",
        NativeFn::ContextVec(builtin_field_string_no_properties),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "constrain-to-field",
        NativeFn::ContextVec(builtin_constrain_to_field),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "insert",
        NativeFn::ContextVec(builtin_insert),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "insert-and-inherit",
            NativeFn::ContextVec(builtin_insert_and_inherit),
            SubrArity::new(0, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "insert-before-markers-and-inherit",
            NativeFn::ContextVec(builtin_insert_before_markers_and_inherit),
            SubrArity::new(0, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "insert-buffer-substring",
            NativeFn::ContextVec(builtin_insert_buffer_substring),
            SubrArity::new(1, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "insert-char",
            NativeFn::ContextVec(builtin_insert_char),
            SubrArity::new(1, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                insert_char_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "insert-byte",
        NativeFn::ContextVec(builtin_insert_byte),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "replace-region-contents",
        NativeFn::ContextVec(builtin_replace_region_contents),
        SubrArity::new(3, Some(6)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "set-buffer-multibyte",
            NativeFn::ContextVec(builtin_set_buffer_multibyte),
            SubrArity::new(1, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "kill-all-local-variables",
            NativeFn::ContextVec(builtin_kill_all_local_variables),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "buffer-swap-text",
        NativeFn::ContextVec(builtin_buffer_swap_text),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "delete-region",
            NativeFn::ContextVec(crate::emacs_core::editfns::builtin_delete_region),
            SubrArity::new(2, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(SubrSpec::new(
        "delete-and-extract-region",
        NativeFn::ContextVec(crate::emacs_core::editfns::builtin_delete_and_extract_region),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "subst-char-in-region",
        NativeFn::ContextVec(builtin_subst_char_in_region),
        SubrArity::new(4, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-field",
        NativeFn::ContextVec(builtin_delete_field),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-all-overlays",
        NativeFn::ContextVec(builtin_delete_all_overlays),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "erase-buffer",
            NativeFn::ContextVec(crate::emacs_core::editfns::builtin_erase_buffer),
            SubrArity::new(0, Some(0)),
        )
        .disabled_command()
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("*")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "buffer-enable-undo",
            NativeFn::ContextVec(builtin_buffer_enable_undo),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "buffer-size",
        NativeFn::ContextVec(builtin_buffer_size),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "narrow-to-region",
            NativeFn::ContextVec(builtin_narrow_to_region),
            SubrArity::new(2, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "widen",
            NativeFn::ContextVec(builtin_widen),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "internal--labeled-narrow-to-region",
            NativeFn::ContextVec(builtin_internal_labeled_narrow_to_region),
            SubrArity::new(3, Some(3)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(
        SubrSpec::new(
            "internal--labeled-widen",
            NativeFn::ContextVec(builtin_internal_labeled_widen),
            SubrArity::new(1, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(SubrSpec::new(
        "buffer-modified-p",
        NativeFn::ContextVec(builtin_buffer_modified_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-buffer-modified-p",
        NativeFn::ContextVec(builtin_set_buffer_modified_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-modified-tick",
        NativeFn::ContextVec(builtin_buffer_modified_tick),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-chars-modified-tick",
        NativeFn::ContextVec(builtin_buffer_chars_modified_tick),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-list",
        NativeFn::ContextVec(builtin_buffer_list),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "other-buffer",
        NativeFn::ContextVec(builtin_other_buffer),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "generate-new-buffer-name",
        NativeFn::ContextVec(builtin_generate_new_buffer_name),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-after",
        NativeFn::ContextVec(builtin_char_after),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-before",
        NativeFn::ContextVec(builtin_char_before),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "byte-to-position",
        NativeFn::ContextVec(builtin_byte_to_position),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "position-bytes",
        NativeFn::ContextVec(builtin_position_bytes),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-byte",
        NativeFn::ContextVec(builtin_get_byte),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-local-value",
        NativeFn::ContextVec(builtin_buffer_local_value),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "local-variable-if-set-p",
        NativeFn::ContextVec(builtin_local_variable_if_set_p),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "variable-binding-locus",
        NativeFn::ContextVec(builtin_variable_binding_locus),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "interactive-form",
        NativeFn::ContextVec(builtin_interactive_form),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "command-modes",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_command_modes),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "search-forward",
            NativeFn::ContextVec(builtin_search_forward),
            SubrArity::new(1, Some(4)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("MSearch: ")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "search-backward",
            NativeFn::ContextVec(builtin_search_backward),
            SubrArity::new(1, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("MSearch backward: "),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "re-search-forward",
            NativeFn::ContextVec(builtin_re_search_forward),
            SubrArity::new(1, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("sRE search: "),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "re-search-backward",
            NativeFn::ContextVec(builtin_re_search_backward),
            SubrArity::new(1, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("sRE search backward: "),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "looking-at",
        NativeFn::ContextVec(builtin_looking_at),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "posix-looking-at",
        NativeFn::ContextVec(builtin_posix_looking_at),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-match",
        NativeFn::ContextSlice(builtin_string_match_slice),
        SubrArity::new(2, Some(4)),
    ));
    // `string-match-p' is NOT here: GNU DEFUNs `string-match'
    // (src/search.c:442) and writes `string-match-p' as a `defsubst' over
    // it (lisp/subr.el:5941), so a compiled caller INLINES
    // `(string-match REGEXP STRING START t)' and never reads the cell
    // (DIVERGENCES.md 152).
    ctx.register_subr(SubrSpec::new(
        "posix-string-match",
        NativeFn::ContextVec(builtin_posix_string_match),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "match-beginning",
        NativeFn::ContextVec(builtin_match_beginning),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "match-end",
        NativeFn::ContextVec(builtin_match_end),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "match-data",
        NativeFn::ContextVec(builtin_match_data),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "match-data--translate",
            NativeFn::ContextVec(builtin_match_data_translate),
            SubrArity::new(1, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "set-match-data",
        NativeFn::ContextVec(builtin_set_match_data),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "replace-match",
        NativeFn::ContextVec(builtin_replace_match),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "find-charset-region",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_find_charset_region),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "charset-after",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_charset_after),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "format-mode-line",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_format_mode_line_ctx),
        SubrArity::new(1, Some(4)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-line-height",
            NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_window_line_height),
            SubrArity::new(0, Some(2)),
        )
        .placeholder(NoEvalPlaceholder::WindowLineHeight),
    );
    ctx.register_subr(SubrSpec::new(
        "posn-at-point",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_posn_at_point),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "posn-at-x-y",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_posn_at_x_y),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "coordinates-in-window-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_coordinates_in_window_p),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tool-bar-height",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_tool_bar_height_ctx),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tab-bar-height",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_tab_bar_height_ctx),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "new-fontset",
            NativeFn::ContextVec(builtin_new_fontset),
            SubrArity::new(2, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "set-fontset-font",
            NativeFn::ContextVec(builtin_set_fontset_font),
            SubrArity::new(3, Some(5)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "insert-file-contents",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_insert_file_contents),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "write-region",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_write_region),
            SubrArity::new(3, Some(7)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "r\nFWrite region to file: \ni\ni\ni\np",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "file-name-completion",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_name_completion),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-visited-file-modtime",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_visited_file_modtime),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-keymap",
        NativeFn::ContextVec(builtin_make_keymap),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-sparse-keymap",
        NativeFn::ContextVec(builtin_make_sparse_keymap),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copy-keymap",
        NativeFn::ContextVec(builtin_copy_keymap),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "define-key",
        NativeFn::ContextVec(builtin_define_key),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "lookup-key",
        NativeFn::ContextVec(builtin_lookup_key),
        SubrArity::new(2, Some(3)),
    ));
    // `global-set-key' (lisp/subr.el:1545) and `local-set-key' (:1569) are
    // NOT here: GNU has no C version of either.  Both are Lisp over
    // `define-key' + `current-global-map' / `current-local-map', which ARE
    // registered just above (DIVERGENCES.md 152).
    ctx.register_subr(SubrSpec::new(
        "use-local-map",
        NativeFn::ContextVec(builtin_use_local_map),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "use-global-map",
        NativeFn::ContextVec(builtin_use_global_map),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-local-map",
        NativeFn::ContextVec(builtin_current_local_map),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-global-map",
        NativeFn::ContextVec(builtin_current_global_map),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-active-maps",
        NativeFn::ContextVec(builtin_current_active_maps),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-minor-mode-maps",
        NativeFn::ContextVec(builtin_current_minor_mode_maps),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "keymap-parent",
        NativeFn::ContextVec(builtin_keymap_parent),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-keymap-parent",
        NativeFn::ContextVec(builtin_set_keymap_parent),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "keymapp",
        NativeFn::ContextVec(builtin_keymapp),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "accessible-keymaps",
        NativeFn::ContextVec(builtin_accessible_keymaps),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "map-keymap",
            NativeFn::ContextVec(builtin_map_keymap),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "map-keymap-internal",
            NativeFn::ContextVec(builtin_map_keymap_internal),
            SubrArity::new(2, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "print--preprocess",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_print_preprocess),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "format-network-address",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_format_network_address),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "network-interface-list",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_network_interface_list),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "network-interface-info",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_network_interface_info),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "signal-names",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_signal_names),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "accept-process-output",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_accept_process_output),
        SubrArity::new(0, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "list-system-processes",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_list_system_processes),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "num-processors",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_num_processors),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_process),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-network-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_network_process),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-open-tls-stream",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_neomacs_open_tls_stream),
        SubrArity::new(4, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-tls-available-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::tls::builtin_neomacs_tls_available_p(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-pipe-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_pipe_process),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-boot",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_boot),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-serial-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_serial_process),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "serial-process-configure",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_serial_process_configure),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "call-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_call_process),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "call-process-region",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_call_process_region),
        SubrArity::new(3, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "continue-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_continue_process),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "delete-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_delete_process),
            SubrArity::new(0, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                delete_process_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "interrupt-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_interrupt_process),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "kill-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_kill_process),
            SubrArity::new(0, Some(2)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                kill_process_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "quit-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_quit_process),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "signal-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_signal_process),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                signal_process_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "stop-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_stop_process),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_get_process),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-buffer-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_get_buffer_process),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-attributes",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_attributes),
        SubrArity::new(1, Some(1)),
    ));
    // No `start-process' / `start-file-process' /
    // `start-process-shell-command' / `start-file-process-shell-command':
    // GNU has no C DEFUN for any of them.  All four are Lisp over
    // `make-process' -- lisp/subr.el:3466, lisp/simple.el:5249,
    // lisp/subr.el:5063 and lisp/subr.el:5076 -- and `loadup.el' preloads
    // both files, so a Rust subr here could only ever answer in unit tests.
    // DIVERGENCES.md 149.

    ctx.register_subr(SubrSpec::new(
        "processp",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_processp),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-id",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_id),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-command",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_command),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-contact",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_contact),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-filter",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_filter),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-filter",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_filter),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-sentinel",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_sentinel),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-sentinel",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_sentinel),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-coding-system",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_coding_system),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-datagram-address",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_datagram_address),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-buffer",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_buffer),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-thread",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_thread),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-window-size",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_window_size),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-tty-name",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_tty_name),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-plist",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_plist),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-plist",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_plist),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-mark",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_mark),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-type",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_type),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-thread",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_thread),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-running-child-p",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_running_child_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-send-region",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_region),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-send-eof",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_eof),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-send-string",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_string),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-status",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_status),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-exit-status",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_exit_status),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-list",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_list),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-name",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_name),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-buffer",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sleep-for",
        NativeFn::ContextVec(crate::emacs_core::timer::builtin_sleep_for),
        SubrArity::new(1, Some(2)),
    ));
    // Timer functions (run-at-time, run-with-timer, run-with-idle-timer,
    // cancel-timer, timerp, timer-activate) are NOT C primitives in GNU
    // Emacs — they're defined in timer.el as Elisp functions.
    // The C layer only provides timer-check (in keyboard.rs) which reads
    // timer-list / timer-idle-list and calls timer-event-handler.
    // Registering them as Rust builtins would shadow the Elisp definitions
    // and create an incompatible parallel timer system.
    ctx.register_subr(
        SubrSpec::new(
            "add-variable-watcher",
            NativeFn::ContextVec(crate::emacs_core::advice::builtin_add_variable_watcher),
            SubrArity::new(2, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "remove-variable-watcher",
            NativeFn::ContextVec(crate::emacs_core::advice::builtin_remove_variable_watcher),
            SubrArity::new(2, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "get-variable-watchers",
            NativeFn::ContextVec(crate::emacs_core::advice::builtin_get_variable_watchers),
            SubrArity::new(1, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "modify-syntax-entry",
            NativeFn::ContextVec(crate::emacs_core::syntax::builtin_modify_syntax_entry),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "cSet syntax for character: \nsSet syntax for %s to: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "syntax-table",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_syntax_table),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-syntax-table",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_set_syntax_table),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-syntax",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_char_syntax),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "matching-paren",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_matching_paren),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "forward-comment",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_forward_comment),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "backward-prefix-chars",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_backward_prefix_chars),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "forward-word",
            NativeFn::ContextVec(crate::emacs_core::syntax::builtin_forward_word),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(SubrSpec::new(
        "scan-lists",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_scan_lists),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "scan-sexps",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_scan_sexps),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "parse-partial-sexp",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_parse_partial_sexp),
        SubrArity::new(2, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "skip-syntax-forward",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_skip_syntax_forward),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "skip-syntax-backward",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_skip_syntax_backward),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "start-kbd-macro",
            NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_start_kbd_macro),
            SubrArity::new(1, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("P")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "end-kbd-macro",
            NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_end_kbd_macro),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "call-last-kbd-macro",
            NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_call_last_kbd_macro),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(SubrSpec::new(
        "execute-kbd-macro",
        NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_execute_kbd_macro),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "store-kbd-macro-event",
        NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_store_kbd_macro_event),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "put-text-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_put_text_property),
        SubrArity::new(4, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-text-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_get_text_property),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-char-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_get_char_property),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-pos-property",
        NativeFn::ContextVec(builtin_get_pos_property),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "add-face-text-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_add_face_text_property),
        SubrArity::new(3, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "add-text-properties",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_add_text_properties),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-text-properties",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_set_text_properties),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "remove-text-properties",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_remove_text_properties),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "text-properties-at",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_text_properties_at),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-display-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_get_display_property),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "next-single-char-property-change",
        NativeFn::ContextVec(builtin_next_single_char_property_change),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "previous-single-char-property-change",
        NativeFn::ContextVec(builtin_previous_single_char_property_change),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "next-property-change",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_next_property_change),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "next-char-property-change",
        NativeFn::ContextVec(builtin_next_char_property_change),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "previous-property-change",
        NativeFn::ContextVec(builtin_previous_property_change),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "previous-char-property-change",
        NativeFn::ContextVec(builtin_previous_char_property_change),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "text-property-any",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_text_property_any),
        SubrArity::new(4, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "text-property-not-all",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_text_property_not_all),
        SubrArity::new(4, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "next-overlay-change",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_next_overlay_change),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "previous-overlay-change",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_previous_overlay_change),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-overlay",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_make_overlay),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-overlay",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_delete_overlay),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-put",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_put),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-get",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_get),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlays-at",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlays_at),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlays-in",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlays_in),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "move-overlay",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_move_overlay),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-start",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_start),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-end",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_end),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-buffer",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-properties",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_properties),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlayp",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlayp),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bobp",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_bobp),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "eobp",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_eobp),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bolp",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_bolp),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "eolp",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_eolp),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "pos-bol",
        NativeFn::ContextVec(builtin_pos_bol),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "line-end-position",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_line_end_position),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "pos-eol",
        NativeFn::ContextVec(builtin_pos_eol),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "line-number-at-pos",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_line_number_at_pos),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "forward-line",
            NativeFn::ContextVec(crate::emacs_core::navigation::builtin_forward_line),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "beginning-of-line",
            NativeFn::ContextVec(crate::emacs_core::navigation::builtin_beginning_of_line),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "end-of-line",
            NativeFn::ContextVec(crate::emacs_core::navigation::builtin_end_of_line),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "forward-char",
            NativeFn::ContextVec(crate::emacs_core::navigation::builtin_forward_char),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "backward-char",
            NativeFn::ContextVec(crate::emacs_core::navigation::builtin_backward_char),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(SubrSpec::new(
        "skip-chars-forward",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_skip_chars_forward),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "skip-chars-backward",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_skip_chars_backward),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mark-marker",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_mark_marker),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "region-beginning",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_region_beginning),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "region-end",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_region_end),
        SubrArity::new(0, Some(0)),
    ));
    // `transient-mark-mode' the FUNCTION is not here: it is a
    // `define-minor-mode' at lisp/simple.el:7614.  Only the VARIABLE is C
    // (DEFVAR_LISP, src/buffer.c:5835), and that stays (DIVERGENCES.md 152).
    ctx.register_subr(
        SubrSpec::new(
            "make-local-variable",
            NativeFn::ContextVec(crate::emacs_core::custom::builtin_make_local_variable),
            SubrArity::new(1, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "vMake Local Variable: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "local-variable-p",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_local_variable_p),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-local-variables",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_buffer_local_variables),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "kill-local-variable",
            NativeFn::ContextVec(crate::emacs_core::custom::builtin_kill_local_variable),
            SubrArity::new(1, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "vKill Local Variable: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "autoload",
        NativeFn::ContextVec(crate::emacs_core::autoload::builtin_autoload),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::fixed3(
        "autoload-do-load",
        crate::emacs_core::autoload::builtin_autoload_do_load_3,
        FixedMin3::One,
    ));
    // `symbol-file' is not here: it is a `defun' at lisp/subr.el:3351 that
    // walks `load-history' (DIVERGENCES.md 152).
    ctx.register_subr(
        SubrSpec::new(
            "downcase-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_downcase_region),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "upcase-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_upcase_region),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "capitalize-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_capitalize_region),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "downcase-word",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_downcase_word),
            SubrArity::new(1, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "upcase-word",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_upcase_word),
            SubrArity::new(1, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "capitalize-word",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_capitalize_word),
            SubrArity::new(1, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    crate::emacs_core::indent::register_subrs(ctx);
    ctx.register_subr(SubrSpec::new(
        "selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_selected_window),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "old-selected-window",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_old_selected_window),
            SubrArity::new(0, Some(0)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "minibuffer-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_minibuffer_window),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-parameter",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_parameter),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-parameter",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_parameter),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-parameters",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_parameters),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-parent",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_parent),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-top-child",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_top_child),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-left-child",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_left_child),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "window-next-sibling",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_next_sibling),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "window-prev-sibling",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_prev_sibling),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "window-normal-size",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_normal_size),
            SubrArity::new(0, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "window-display-table",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_display_table),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-cursor-type",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_cursor_type),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-buffer",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_buffer),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-start",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_start),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-end",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_end),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-point",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_point),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-use-time",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_use_time),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-bump-use-time",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_bump_use_time),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "window-old-point",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_point),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-old-buffer",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_old_buffer),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-prev-buffers",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_prev_buffers),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-next-buffers",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_next_buffers),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-left-column",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_left_column),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-top-line",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_top_line),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-pixel-left",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_pixel_left),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-pixel-top",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_pixel_top),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-hscroll",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_hscroll),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-vscroll",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_vscroll),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-margins",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_margins),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-fringes",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_fringes),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-scroll-bars",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_scroll_bars),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-pixel-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_pixel_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-pixel-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_pixel_width),
        SubrArity::new(0, Some(1)),
    ));
    // `window-edges' (lisp/window.el:3839), `window-pixel-edges' (:3922) and
    // `window-absolute-pixel-edges' (:3937) are Lisp and only Lisp: GNU has no
    // DEFUN for any of the three (DIVERGENCES.md 154).  `window-edges' is
    // written over the C primitives registered around here --
    // `window-pixel-left', `window-pixel-top', `window-pixel-width',
    // `window-pixel-height', `window-left-column', `window-top-line',
    // `window-total-width', `window-total-height', `window-body-width',
    // `window-body-height' -- and the other two are one-line wrappers over
    // `window-edges' itself, not over any primitive.
    ctx.register_subr(SubrSpec::new(
        "window-body-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_body_height),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-body-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_body_width),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-text-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_text_height),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-text-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_text_width),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-total-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_total_height),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-total-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_total_width),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-list",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_list),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-list-1",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_list_1),
            SubrArity::new(0, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "get-buffer-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_get_buffer_window),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-dedicated-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_dedicated_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-minibuffer-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_minibuffer_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-at",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_at),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "window-live-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_live_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-start",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_start),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-hscroll",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_hscroll),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-margins",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_margins),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-fringes",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_fringes),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-vscroll",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_vscroll),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-point",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_point),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "split-window-internal",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_split_window_internal),
            SubrArity::new(4, Some(5)),
        )
        .requires_eval_state(),
    );
    // `delete-window' (lisp/window.el:4318), `delete-other-windows' (:4453)
    // and `fit-window-to-buffer' (:10307) are Lisp and only Lisp
    // (DIVERGENCES.md 154).  The C primitives they are written over --
    // `delete-window-internal' and `delete-other-windows-internal'
    // (src/window.c) -- are registered below and stay.
    ctx.register_subr(SubrSpec::new(
        "select-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_select_window),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "scroll-up",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_up),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^P")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "scroll-down",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_down),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^P")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "scroll-left",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_left),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^P\np")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "scroll-right",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_scroll_right),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^P\np")),
    );
    ctx.register_subr(SubrSpec::new(
        "window-resize-apply",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_resize_apply),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "recenter",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_recenter),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("P\np")),
    );
    ctx.register_subr(SubrSpec::new(
        "next-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_next_window),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "previous-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_previous_window),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-buffer",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_buffer),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs--record-window-navigation-intent",
        NativeFn::ContextVec(
            crate::emacs_core::window_cmds::builtin_neomacs_record_window_navigation_intent,
        ),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs--record-frame-navigation-intent",
        NativeFn::ContextVec(
            crate::emacs_core::window_cmds::builtin_neomacs_record_frame_navigation_intent,
        ),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-window-configuration",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_current_window_configuration),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-configuration",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_configuration),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "old-selected-frame",
            NativeFn::ContextVec(builtin_old_selected_frame),
            SubrArity::new(0, Some(0)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(SubrSpec::new(
        "selected-frame",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_selected_frame),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mouse-pixel-position",
        NativeFn::ContextVec(builtin_mouse_pixel_position),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mouse-position",
        NativeFn::ContextVec(builtin_mouse_position),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "next-frame",
            NativeFn::ContextVec(builtin_next_frame),
            SubrArity::new(0, Some(2)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(
        SubrSpec::new(
            "previous-frame",
            NativeFn::ContextVec(builtin_previous_frame),
            SubrArity::new(0, Some(2)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(
        SubrSpec::new(
            "select-frame",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_select_frame),
            SubrArity::new(1, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("e")),
    );
    ctx.register_subr(SubrSpec::new(
        "last-nonminibuffer-frame",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_selected_frame),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "visible-frame-list",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_visible_frame_list),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-list",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_list),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-create-frame",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_x_create_frame),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "make-frame-visible",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_make_frame_visible),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    // `make-frame' is lisp/frame.el:1019, not a DEFUN (DIVERGENCES.md 154).
    // It funcalls `frame-creation-function', which on a text terminal reaches
    // `make-terminal-frame' -- that one IS a C DEFUN (src/frame.c) and stays.
    ctx.register_subr(
        SubrSpec::new(
            "iconify-frame",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_iconify_frame),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "delete-frame",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_delete_frame),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "frame-char-height",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_char_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-char-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_char_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-native-height",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_native_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-native-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_native_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-text-cols",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_cols),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-text-height",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-text-lines",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_lines),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-text-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_text_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-total-cols",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_total_cols),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-total-lines",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_total_lines),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-position",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_position),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-parameters",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_parameters),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "set-frame-height",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_height),
            SubrArity::new(2, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                set_frame_height_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "set-frame-width",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_width),
            SubrArity::new(2, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                set_frame_width_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "set-frame-size",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_size),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-frame-position",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_position),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-visible-p",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_visible_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-live-p",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_live_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-initial-p",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_frame_initial_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-first-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_first_window),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-root-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_root_window),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "windowp",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_windowp),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-valid-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_valid_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "framep",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_framep),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-frame",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_frame),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-id",
        NativeFn::ContextVec(builtin_frame_id),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-root-frame",
        NativeFn::ContextVec(builtin_frame_root_frame),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-open-connection",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_open_connection),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-get-resource",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_get_resource),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-list-fonts",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_list_fonts),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-system",
            NativeFn::ContextVec(crate::emacs_core::display::builtin_window_system),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "current-idle-time",
        NativeFn::ContextVec(builtin_current_idle_time),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-server-version",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_version),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-server-input-extension-version",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_input_extension_version),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-server-vendor",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_vendor),
        SubrArity::new(0, Some(1)),
    ));
    // NO `display-color-cells' here.  It is lisp/frame.el:2966 and NOT a
    // DEFUN, so registering it was a shadow like the seventeen
    // DIVERGENCES.md 154 deleted; it was the eighteenth, held back because our
    // `(load "faces")' reached it before `frame.el' defined it.  The cause was
    // a `background-mode' frame parameter Rust seeded before loadup, which GNU
    // computes after it (DIVERGENCES.md 157).  With the seeding gone the
    // caller is gone, and the two C names its Lisp body dispatches to --
    // `x-display-color-cells' (src/xfns.c:5714) and `tty-display-color-cells'
    // (src/term.c:2226) -- are registered right where they always were.
    ctx.register_subr(SubrSpec::new(
        "x-display-mm-height",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_mm_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-mm-width",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_mm_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-planes",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_planes),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-screens",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_screens),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-close-connection",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_close_connection),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "call-interactively",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_call_interactively),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "commandp",
        NativeFn::ContextSlice(crate::emacs_core::interactive::builtin_commandp_interactive),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "command-remapping",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_command_remapping),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "self-insert-command",
            NativeFn::ContextVec(crate::emacs_core::interactive::builtin_self_insert_command),
            SubrArity::new(1, Some(2)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                self_insert_command_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "key-binding",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_key_binding),
        SubrArity::new(1, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "where-is-internal",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_where_is_internal),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "this-command-keys",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_this_command_keys),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "format",
        NativeFn::ContextSlice(builtin_format_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "format-message",
        NativeFn::ContextSlice(builtin_format_message_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "message-box",
            NativeFn::ContextVec(builtin_message_box),
            SubrArity::new(1, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "message-or-box",
            NativeFn::ContextVec(builtin_message_or_box),
            SubrArity::new(1, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "current-message",
        NativeFn::ContextVec(builtin_current_message),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-from-string",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_from_string),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-from-minibuffer",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_from_minibuffer),
        SubrArity::new(1, Some(7)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-string",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_string),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "completing-read",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_completing_read),
        SubrArity::new(2, Some(8)),
    ));
    // `read-number' is not here: it is a `defun' at lisp/subr.el:3725 over
    // `read-from-minibuffer', and GNU's "n" interactive code letter reaches
    // it through the function cell (src/callint.c:645) (DIVERGENCES.md 152).
    ctx.register_subr(SubrSpec::new(
        "read-buffer",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_buffer),
        SubrArity::new(1, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-command",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_command),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-variable",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_variable),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "try-completion",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_try_completion),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "all-completions",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_all_completions),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "test-completion",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_test_completion),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "completion--flex-cost-gotoh",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_flex_cost_gotoh),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "input-pending-p",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_input_pending_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "discard-input",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_discard_input),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-input-mode",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_current_input_mode),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-input-mode",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_set_input_mode),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-input-interrupt-mode",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_set_input_interrupt_mode),
        SubrArity::new(1, Some(1)),
    ));
    // Keyboard audit Finding 16: register insert-special-event
    // (mirrors GNU `Finsert_special_event` at
    // `src/keyboard.c:12060`). Routes to the same unread queue
    // helper as `unread-command-events`, since neomacs treats
    // every Lisp-side event push the same way.
    ctx.register_subr(SubrSpec::new(
        "insert-special-event",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_insert_special_event),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-key-sequence",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_key_sequence),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-key-sequence-vector",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_key_sequence_vector),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "recent-keys",
        NativeFn::ContextVec(builtin_recent_keys),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minibufferp",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibufferp_ctx),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minibuffer-contents",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_contents_ctx),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minibuffer-contents-no-properties",
        NativeFn::ContextVec(
            crate::emacs_core::minibuffer::builtin_minibuffer_contents_no_properties_ctx,
        ),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minibuffer-depth",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_depth_ctx),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "princ",
            NativeFn::ContextVec(builtin_princ),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "prin1",
            NativeFn::ContextVec(builtin_prin1),
            SubrArity::new(1, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "prin1-to-string",
            NativeFn::ContextVec(builtin_prin1_to_string),
            SubrArity::new(1, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "print",
            NativeFn::ContextVec(builtin_print),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "terpri",
            NativeFn::ContextVec(builtin_terpri),
            SubrArity::new(0, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "write-char",
            NativeFn::ContextVec(builtin_write_char),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "backtrace--locals",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_locals),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "backtrace-debug",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_debug),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "backtrace-eval",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_eval),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "backtrace-frame--internal",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_frame_internal),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "recursion-depth",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_recursion_depth),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "kill-emacs",
            NativeFn::ContextVec(builtin_kill_emacs),
            SubrArity::new(0, Some(2)),
        )
        .requires_eval_state()
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("P")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "exit-recursive-edit",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_exit_recursive_edit),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "abort-recursive-edit",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_abort_recursive_edit),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "make-thread",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_make_thread),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-join",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_join),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-yield",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_yield),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-name",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_name),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-live-p",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_live_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "threadp",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_threadp),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-signal",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_signal),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-thread",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_current_thread),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "all-threads",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_all_threads),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-last-error",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_last_error),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-mutex",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_make_mutex),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mutex-name",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutex_name),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mutex-lock",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutex_lock),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mutex-unlock",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutex_unlock),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mutexp",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_mutexp),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-condition-variable",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_make_condition_variable),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "condition-variable-p",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_variable_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "condition-name",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_name),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "condition-mutex",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_mutex),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "condition-wait",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_wait),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "condition-notify",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_condition_notify),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "undo-boundary",
            NativeFn::ContextVec(crate::emacs_core::undo::builtin_undo_boundary),
            SubrArity::new(0, Some(0)),
        )
        .requires_eval_state(),
    );
    // No `undo' and no `buffer-disable-undo' subr: GNU has neither in C.
    // `syms_of_undo' (src/undo.c:423-490) registers only `&Sundo_boundary'
    // (:435); `undo' is (defun undo (&optional arg) ...) at
    // lisp/simple.el:3466 and `buffer-disable-undo' is
    // (defun buffer-disable-undo (&optional buffer) ...) at
    // lisp/simple.el:3591.  Its partner `buffer-enable-undo' IS in C
    // (src/buffer.c:1829) and is registered above -- the pair is asymmetric
    // in GNU, and copying that asymmetry is the point.  DIVERGENCES.md 150.
    ctx.register_subr(SubrSpec::new(
        "maphash",
        NativeFn::ContextVec(crate::emacs_core::hashtab::builtin_maphash),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mapatoms",
        NativeFn::ContextVec(crate::emacs_core::hashtab::builtin_mapatoms),
        SubrArity::new(1, Some(2)),
    ));
    // GNU `Sunintern` is `2, 2, 0`: the OBARRAY argument is mandatory (it may
    // be nil to default to `obarray`, but it must be supplied).
    ctx.register_subr(SubrSpec::new(
        "unintern",
        NativeFn::ContextVec(crate::emacs_core::hashtab::builtin_unintern),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-marker",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_set_marker),
        SubrArity::new(2, Some(3)),
    ));
    // No `move-marker' here: GNU has no DEFUN of that name.  It is
    // `(defalias 'move-marker #'set-marker)' at lisp/subr.el:2280, so the
    // function cell holds the SYMBOL `set-marker' (DIVERGENCES.md 148).
    ctx.register_subr(SubrSpec::new(
        "marker-position",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_marker_position),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "marker-buffer",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_marker_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copy-marker",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_copy_marker),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "point-marker",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_point_marker),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "point-min-marker",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_point_min_marker),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "point-max-marker",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_point_max_marker),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab::builtin_current_case_table),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "standard-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab::builtin_standard_case_table),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab::builtin_set_case_table),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "define-category",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_define_category),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "category-docstring",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_category_docstring),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "modify-category-entry",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_modify_category_entry),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-category-set",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_char_category_set),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_category_table),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_set_category_table),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "map-char-table",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_map_char_table),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "assoc",
            NativeFn::ContextVec(builtin_assoc),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "plist-member",
            NativeFn::ContextVec(builtin_plist_member),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "json-parse-buffer",
        NativeFn::ContextVec(crate::emacs_core::json::builtin_json_parse_buffer),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "json-insert",
        NativeFn::ContextVec(crate::emacs_core::json::builtin_json_insert),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "documentation",
        NativeFn::ContextVec(crate::emacs_core::doc::builtin_documentation),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "documentation-property",
        NativeFn::ContextVec(crate::emacs_core::doc::builtin_documentation_property),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "eval-buffer",
            NativeFn::ContextVec(crate::emacs_core::lread::builtin_eval_buffer),
            SubrArity::new(0, Some(5)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "eval-region",
            NativeFn::ContextVec(crate::emacs_core::lread::builtin_eval_region),
            SubrArity::new(2, Some(4)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(SubrSpec::new(
        "read-char-exclusive",
        NativeFn::ContextVec(crate::emacs_core::lread::builtin_read_char_exclusive),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "insert-before-markers",
        NativeFn::ContextVec(builtin_insert_before_markers),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "delete-char",
            NativeFn::ContextVec(crate::emacs_core::editfns::builtin_delete_char),
            SubrArity::new(1, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p\nP")),
    );
    ctx.register_subr(SubrSpec::fixed0(
        "following-char",
        crate::emacs_core::editfns::builtin_following_char_0,
    ));
    ctx.register_subr(SubrSpec::new(
        "preceding-char",
        NativeFn::ContextVec(|eval, args| {
            crate::emacs_core::editfns::builtin_preceding_char(eval, args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "face-font",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_face_font),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "access-file",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_access_file),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "expand-file-name",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_expand_file_name),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-file-internal",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_delete_file_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "rename-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_rename_file),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "fRename file: \nGRename %s to file: \np",
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "copy-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_copy_file),
            SubrArity::new(2, Some(6)),
        )
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "fCopy file: \nGCopy %s to file: \np\nP",
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "add-name-to-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_add_name_to_file),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "fAdd name to file: \nGName to add to %s: \np",
            ),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "make-symbolic-link",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_make_symbolic_link),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "FMake symbolic link to file: \nGMake symbolic link to file %s: \np",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "directory-files",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_directory_files),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-attributes",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_attributes),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-exists-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_exists_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-readable-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_readable_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-writable-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_writable_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-acl",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_acl),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-executable-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_executable_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-locked-p",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_file_locked_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-selinux-context",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_selinux_context),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-system-info",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_system_info),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-directory-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_directory_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-regular-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_regular_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-symlink-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_symlink_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-modes",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_modes),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "set-file-modes",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_modes),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                set_file_modes_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "set-file-times",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_times),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "error-message-string",
        NativeFn::ContextVec(crate::emacs_core::errors::builtin_error_message_string),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-equal",
        NativeFn::ContextVec(builtin_char_equal),
        SubrArity::new(2, Some(2)),
    ));
    // No `macrop' here: GNU has no DEFUN of that name.  It is a `defun' at
    // lisp/subr.el:4793 over `indirect-function', which IS in C
    // (src/data.c:2557) -- DIVERGENCES.md 148.
    ctx.register_subr(SubrSpec::new(
        "set-process-inherit-coding-system-flag",
        NativeFn::ContextVec(
            crate::emacs_core::process::builtin_set_process_inherit_coding_system_flag,
        ),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-parameter",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_parameter),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "send-string-to-terminal",
        NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_send_string_to_terminal),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-show-cursor",
        NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_internal_show_cursor),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-show-cursor-p",
        NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_internal_show_cursor_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "redraw-frame",
        NativeFn::ContextVec(crate::emacs_core::dispnew::pure::builtin_redraw_frame),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "display-supports-face-attributes-p",
        NativeFn::ContextVec(
            crate::emacs_core::display::builtin_display_supports_face_attributes_p,
        ),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "terminal-name",
            NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_name),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "terminal-live-p",
            NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_live_p),
            SubrArity::new(1, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "terminal-parameter",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_parameter),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "terminal-parameters",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_terminal_parameters),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-terminal-parameter",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_set_terminal_parameter),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-type",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_type),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-top-frame",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_top_frame),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-display-color-p",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_display_color_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-display-color-cells",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_display_color_cells),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-no-underline",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_tty_no_underline),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "controlling-tty-p",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_controlling_tty_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "suspend-tty",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_suspend_tty),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "resume-tty",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_resume_tty),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-terminal",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_frame_terminal),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-monitor-attributes-list",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_monitor_attributes_list),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-char",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_read_char),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "minibuffer-innermost-command-loop-p",
            NativeFn::ContextVec(
                crate::emacs_core::minibuffer::builtin_minibuffer_innermost_command_loop_p_ctx,
            ),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "recursive-edit",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_recursive_edit),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "find-coding-systems-region-internal",
            NativeFn::ContextVec(
                crate::emacs_core::coding::builtin_find_coding_systems_region_internal,
            ),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "posix-search-forward",
            NativeFn::ContextVec(crate::emacs_core::builtins::search::builtin_posix_search_forward),
            SubrArity::new(1, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("sPosix search: "),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "posix-search-backward",
            NativeFn::ContextVec(
                crate::emacs_core::builtins::search::builtin_posix_search_backward,
            ),
            SubrArity::new(1, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "sPosix search backward: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "read-event",
        NativeFn::ContextVec(crate::emacs_core::lread::builtin_read_event),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "run-hooks",
        NativeFn::ContextVec(run_hooks_traced),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "load",
        NativeFn::ContextVec(load_traced),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "message",
            NativeFn::ContextVec(message_traced),
            SubrArity::new(1, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "coding-system-aliases",
        NativeFn::ContextVec(coding_system_aliases),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "coding-system-plist",
        NativeFn::ContextVec(coding_system_plist),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "coding-system-put",
        NativeFn::ContextVec(coding_system_put),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "coding-system-base",
        NativeFn::ContextVec(coding_system_base),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "coding-system-eol-type",
        NativeFn::ContextVec(coding_system_eol_type),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "detect-coding-string",
        NativeFn::ContextVec(detect_coding_string),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "detect-coding-region",
        NativeFn::ContextVec(detect_coding_region),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "keyboard-coding-system",
        NativeFn::ContextVec(keyboard_coding_system),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "terminal-coding-system",
        NativeFn::ContextVec(terminal_coding_system),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "coding-system-priority-list",
        NativeFn::ContextVec(coding_system_priority_list),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "integer-or-marker-p",
        NativeFn::ContextVec(|_ctx, args| builtin_integer_or_marker_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "number-or-marker-p",
        NativeFn::ContextVec(|_ctx, args| builtin_number_or_marker_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "vector-or-char-table-p",
        NativeFn::ContextVec(|_ctx, args| builtin_vector_or_char_table_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "marker-insertion-type",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::marker::builtin_marker_insertion_type(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-marker",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::marker::builtin_make_marker(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-category-set",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_make_category_set(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "function-equal",
        NativeFn::ContextVec(|_ctx, args| builtin_function_equal(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "module-function-p",
        NativeFn::ContextVec(|_ctx, args| builtin_module_function_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "user-ptrp",
        NativeFn::ContextVec(|_ctx, args| builtin_user_ptrp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "symbol-with-pos-p",
        builtin_symbol_with_pos_p_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "symbol-with-pos-pos",
        builtin_symbol_with_pos_pos_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::new(
        "length<",
        NativeFn::ContextVec(|_ctx, args| builtin_length_lt(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "length=",
        NativeFn::ContextVec(|_ctx, args| builtin_length_eq(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "length>",
        NativeFn::ContextVec(|_ctx, args| builtin_length_gt(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "substring-no-properties",
        NativeFn::ContextVec(|_ctx, args| builtin_substring_no_properties(args)),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sqrt",
        NativeFn::ContextVec(|_ctx, args| builtin_sqrt(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sin",
        NativeFn::ContextVec(|_ctx, args| builtin_sin(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "cos",
        NativeFn::ContextVec(|_ctx, args| builtin_cos(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tan",
        NativeFn::ContextVec(|_ctx, args| builtin_tan(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "asin",
        NativeFn::ContextVec(|_ctx, args| builtin_asin(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "acos",
        NativeFn::ContextVec(|_ctx, args| builtin_acos(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "atan",
        NativeFn::ContextVec(|_ctx, args| builtin_atan(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "exp",
        NativeFn::ContextVec(|_ctx, args| builtin_exp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "log",
        NativeFn::ContextVec(|_ctx, args| builtin_log(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "expt",
        NativeFn::ContextVec(|_ctx, args| builtin_expt(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "random",
        NativeFn::ContextVec(|_ctx, args| builtin_random(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "isnan",
        NativeFn::ContextVec(|_ctx, args| builtin_isnan(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-string",
        NativeFn::ContextVec(|_ctx, args| builtin_make_string(args)),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string",
        NativeFn::ContextSlice(|_ctx, args| builtin_string_slice(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-width",
        NativeFn::ContextVec(builtin_string_width),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete",
        NativeFn::ContextVec(builtin_delete_with_ctx),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::fixed2("delq", builtin_delq_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "elt",
        NativeFn::ContextVec(|_ctx, args| builtin_elt(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::fixed2("memql", builtin_memql_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "nconc",
        NativeFn::ContextSlice(builtin_nconc_slice),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "identity",
        NativeFn::ContextVec(|_ctx, args| builtin_identity(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ngettext",
        NativeFn::ContextVec(|_ctx, args| builtin_ngettext(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "secure-hash-algorithms",
        NativeFn::ContextVec(|_ctx, args| builtin_secure_hash_algorithms(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "prefix-numeric-value",
        NativeFn::ContextVec(|_ctx, args| builtin_prefix_numeric_value(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "propertize",
        NativeFn::ContextVec(|_ctx, args| builtin_propertize(args)),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "capitalize",
        NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_capitalize_in_state),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "charsetp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_charsetp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "charset-plist",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_charset_plist(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "define-charset-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_define_charset_internal(args)
        }),
        SubrArity::new(17, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "define-charset-alias",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_define_charset_alias(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-lisp-face-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_lisp_face_p),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-make-lisp-face",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_make_lisp_face),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-set-lisp-face-attribute",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-to-syntax",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_string_to_syntax(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "syntax-class-to-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_syntax_class_to_char(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copy-syntax-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_copy_syntax_table(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "syntax-table-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::syntax::builtin_syntax_table_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "standard-syntax-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_standard_syntax_table(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-time",
        NativeFn::ContextVec(crate::emacs_core::timefns::builtin_current_time_in_context),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-cpu-time",
        NativeFn::ContextVec(|_ctx, args| builtin_current_cpu_time(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-internal-run-time",
        NativeFn::ContextVec(|_ctx, args| builtin_get_internal_run_time(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "float-time",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_float_time(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "daemonp",
        NativeFn::ContextVec(|_ctx, args| builtin_daemonp(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "daemon-initialized",
        NativeFn::ContextVec(|_ctx, args| builtin_daemon_initialized(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "flush-standard-output",
        NativeFn::ContextVec(|_ctx, args| builtin_flush_standard_output(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "force-mode-line-update",
        NativeFn::ContextVec(builtin_force_mode_line_update),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "invocation-directory",
        NativeFn::ContextVec(builtin_invocation_directory),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "invocation-name",
        NativeFn::ContextVec(builtin_invocation_name),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-directory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_directory),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-nondirectory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_nondirectory),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-as-directory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_as_directory),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "directory-file-name",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_directory_file_name),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-concat",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_file_name_concat(args)
        }),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-absolute-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_file_name_absolute_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "directory-name-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_directory_name_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "substitute-in-file-name",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_substitute_in_file_name),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-file-acl",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::fileio::builtin_set_file_acl(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-file-selinux-context",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_selinux_context),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "visited-file-modtime",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_visited_file_modtime),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-temp-name",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::fileio::builtin_make_temp_name(ctx, args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "next-read-file-uses-dialog-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_next_read_file_uses_dialog_p(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "unhandled-file-name-directory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_unhandled_file_name_directory_eval),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-truename-buffer",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_get_truename_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "single-key-description",
        NativeFn::ContextVec(|_ctx, args| builtin_single_key_description(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "key-description",
        NativeFn::ContextVec(|_ctx, args| builtin_key_description(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "event-convert-list",
        NativeFn::ContextVec(|_ctx, args| builtin_event_convert_list(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "text-char-description",
        NativeFn::ContextVec(|_ctx, args| builtin_text_char_description(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-binary-mode",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::fileio::builtin_set_binary_mode(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "group-name",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_group_name(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "group-gid",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_group_gid(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "group-real-gid",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_group_real_gid(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "load-average",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_load_average(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "logcount",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_logcount(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-frame-size-and-position-pixelwise",
        NativeFn::ContextVec(
            crate::emacs_core::frame::builtin_set_frame_size_and_position_pixelwise,
        ),
        SubrArity::new(5, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "mouse-position-in-root-frame",
        NativeFn::ContextVec(|_ctx, args| builtin_mouse_position_in_root_frame(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-load-color-file",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_x_load_color_file(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "define-fringe-bitmap",
        NativeFn::ContextVec(builtin_define_fringe_bitmap),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "destroy-fringe-bitmap",
        NativeFn::ContextVec(builtin_destroy_fringe_bitmap),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "display--line-is-continued-p",
        NativeFn::ContextVec(|_ctx, args| builtin_display_line_is_continued_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "display--update-for-mouse-movement",
        NativeFn::ContextVec(builtin_display_update_for_mouse_movement),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "do-auto-save",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_do_auto_save),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    // `make-auto-save-file-name' is not here: it is a `defun' at
    // lisp/files.el:7699 over `auto-save-file-name-transforms'.  GNU's C
    // side only READS the buffer field (src/fileio.c:6406)
    // (DIVERGENCES.md 152).
    ctx.register_subr(SubrSpec::new(
        "external-debugging-output",
        NativeFn::ContextVec(builtin_external_debugging_output),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "describe-buffer-bindings",
            NativeFn::ContextVec(keymaps::builtin_describe_buffer_bindings),
            SubrArity::new(1, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "describe-vector",
        NativeFn::ContextVec(builtin_describe_vector),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "face-attributes-as-vector",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_face_attributes_as_vector(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "font-get-system-font",
        NativeFn::ContextVec(|_ctx, args| builtin_font_get_system_font(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "font-get-system-normal-font",
        NativeFn::ContextVec(|_ctx, args| builtin_font_get_system_normal_font(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "fontset-font",
        NativeFn::ContextVec(|_ctx, args| builtin_fontset_font(args)),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "fontset-info",
        NativeFn::ContextVec(|_ctx, args| builtin_fontset_info(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "fontset-list",
        NativeFn::ContextVec(|_ctx, args| builtin_fontset_list(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame--set-was-invisible",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_set_was_invisible(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-after-make-frame",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_after_make_frame(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-ancestor-p",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_ancestor_p),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-bottom-divider-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_bottom_divider_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-child-frame-border-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_child_frame_border_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-focus",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_focus),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-font-cache",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_font_cache(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-fringe-width",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_fringe_width(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-internal-border-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_internal_border_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-or-buffer-changed-p",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_or_buffer_changed_p(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-parent",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_parent),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-pointer-visible-p",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_pointer_visible_p(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-right-divider-width",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_right_divider_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-scale-factor",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_scale_factor),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-scroll-bar-height",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_scroll_bar_height(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-scroll-bar-width",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_scroll_bar_width(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-window-state-change",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_frame_window_state_change),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "fringe-bitmaps-at-pos",
            NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_fringe_bitmaps_at_pos),
            SubrArity::new(0, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "gap-position",
            NativeFn::ContextVec(builtin_gap_position),
            SubrArity::new(0, Some(0)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "gap-size",
            NativeFn::ContextVec(builtin_gap_size),
            SubrArity::new(0, Some(0)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "garbage-collect-heapsize",
        NativeFn::ContextVec(|_ctx, args| builtin_garbage_collect_heapsize(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "garbage-collect-maybe",
        NativeFn::ContextVec(builtin_garbage_collect_maybe),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-unicode-property-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_get_unicode_property_internal(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-available-p",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_available_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-asynchronous-parameters",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_asynchronous_parameters),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-bye",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_bye),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-ciphers",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_ciphers(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-deinit",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_deinit),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-digests",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_digests(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-error-fatalp",
        NativeFn::ContextVec(gnutls::builtin_gnutls_error_fatalp_with_ctx),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-error-string",
        NativeFn::ContextVec(gnutls::builtin_gnutls_error_string_with_ctx),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-errorp",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_errorp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-format-certificate",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_format_certificate(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-get-initstage",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_get_initstage),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-hash-digest",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_hash_digest(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-hash-mac",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_hash_mac(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-macs",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_macs(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-peer-status",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_peer_status),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-peer-status-warning-describe",
        NativeFn::ContextVec(|_ctx, args| {
            gnutls::builtin_gnutls_peer_status_warning_describe(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-symmetric-decrypt",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_symmetric_decrypt(args)),
        SubrArity::new(4, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "gnutls-symmetric-encrypt",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_symmetric_encrypt(args)),
        SubrArity::new(4, Some(5)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "handle-save-session",
            NativeFn::ContextVec(|_ctx, args| builtin_handle_save_session(args)),
            SubrArity::new(1, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("e")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "handle-switch-frame",
            NativeFn::ContextVec(|_ctx, args| builtin_handle_switch_frame(args)),
            SubrArity::new(1, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^e")),
    );
    ctx.register_subr(SubrSpec::new(
        "help--describe-vector",
        NativeFn::ContextVec(keymaps::builtin_help_describe_vector),
        SubrArity::new(7, Some(7)),
    ));
    ctx.register_subr(SubrSpec::new(
        "init-image-library",
        NativeFn::ContextVec(|_ctx, args| builtin_init_image_library(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal--obarray-buckets",
        NativeFn::ContextVec(|_ctx, args| builtin_internal_obarray_buckets(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal--set-buffer-modified-tick",
        NativeFn::ContextVec(builtin_internal_set_buffer_modified_tick),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal--track-mouse",
        NativeFn::ContextVec(builtin_internal_track_mouse),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-complete-buffer",
        NativeFn::ContextVec(builtin_internal_complete_buffer),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-describe-syntax-value",
        NativeFn::ContextVec(builtin_internal_describe_syntax_value),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-event-symbol-parse-modifiers",
        NativeFn::ContextVec(builtin_internal_event_symbol_parse_modifiers),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-handle-focus-in",
        NativeFn::ContextVec(builtin_internal_handle_focus_in),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-set-lisp-face-attribute-from-resource",
        NativeFn::ContextVec(builtin_internal_set_lisp_face_attribute_from_resource),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-stack-stats",
        NativeFn::ContextVec(|_ctx, args| builtin_internal_stack_stats(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-subr-documentation",
        NativeFn::ContextVec(|_ctx, args| builtin_internal_subr_documentation(args)),
        SubrArity::new(1, Some(1)),
    ));
    // byte-code: mirrors GNU Emacs Fbyte_code (src/bytecode.c).
    // Receives pre-evaluated args (bytestr, vector, maxdepth), decodes
    // the GNU bytecodes, and executes them via the bytecode VM.
    ctx.register_subr(SubrSpec::new(
        "byte-code",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::builtins::expect_args("byte-code", &args, 3)?;
            let bytestr = args[0];
            let constants_vec = args[1];
            let maxdepth = args[2];

            use crate::emacs_core::bytecode::ByteCodeFunction;
            use crate::emacs_core::bytecode::decode::decode_gnu_bytecode_with_offset_map;
            use crate::emacs_core::value::LambdaParams;

            // Bytecode strings are unibyte and may contain non-UTF-8 bytes.
            let raw_bytes = if let Some(ls) = ctx.lisp_string(bytestr) {
                ls.as_bytes().to_vec()
            } else {
                return Err(crate::emacs_core::error::signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), bytestr],
                ));
            };

            let mut constants: Vec<Value> = match constants_vec.kind() {
                ValueKind::Veclike(VecLikeType::Vector) => {
                    constants_vec.as_vector_data().unwrap().clone()
                }
                _ => {
                    return Err(crate::emacs_core::error::signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("vectorp"), constants_vec],
                    ));
                }
            };

            for constant in &mut constants {
                *constant =
                    crate::emacs_core::builtins::try_convert_nested_compiled_literal(*constant);
            }

            let (ops, gnu_byte_offset_map) =
                decode_gnu_bytecode_with_offset_map(&raw_bytes, &mut constants).map_err(|e| {
                    crate::emacs_core::error::signal(
                        "error",
                        vec![Value::string(format!("bytecode decode error: {}", e))],
                    )
                })?;

            let max_stack = match maxdepth.kind() {
                ValueKind::Fixnum(n) => n as u16,
                _ => 16,
            };

            let bc = ByteCodeFunction {
                source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
                ops,
                // The instructions above came straight from the sealing decoder;
                // the stack proof is recomputed below once every shape field
                // (params/lexical/arglist/env/max_stack) is in place.
                ops_sealed: true,
                stack_verified: false,
                constants: constants.into(),
                max_stack,
                params: LambdaParams::simple(vec![]),
                arglist: Value::NIL,
                lexical: false,
                env: None,
                gnu_byte_offset_map: Some(gnu_byte_offset_map),
                gnu_bytecode_bytes: None,
                docstring: None,
                doc_form: None,
                interactive: None,
                closure_slot_count: 4,
                extra_slots: Vec::new(),
                #[cfg(feature = "jit")]
                runtime: Some(crate::emacs_core::jit::Runtime::new()),
                lazy_gnu_code: None,
            };

            ctx.refresh_features_from_variable();
            let mut vm = crate::emacs_core::bytecode::Vm::from_context(ctx);
            let result = vm.execute(&bc, vec![]);
            ctx.sync_features_variable();
            result
        }),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "decode-coding-region",
            NativeFn::ContextVec(crate::encoding::builtin_decode_coding_region),
            SubrArity::new(3, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r\nzCoding system: "),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "dump-emacs-portable",
        NativeFn::ContextVec(builtin_dump_emacs_portable),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "dump-emacs-portable--sort-predicate",
        NativeFn::ContextVec(|_ctx, args| builtin_dump_emacs_portable_sort_predicate(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "dump-emacs-portable--sort-predicate-copied",
        NativeFn::ContextVec(|_ctx, args| builtin_dump_emacs_portable_sort_predicate_copied(args)),
        SubrArity::new(2, Some(2)),
    ));
    // `emacs-repository-get-version' (lisp/version.el:183) and
    // `emacs-repository-get-branch' (:231) are not here.  They were
    // registered as "gap-fill stubs for loadup.el", but loadup loads
    // version.el at :128 and only calls them at :429 (DIVERGENCES.md 152).
    ctx.register_subr(
        SubrSpec::new(
            "encode-coding-region",
            NativeFn::ContextVec(crate::encoding::builtin_encode_coding_region),
            SubrArity::new(3, Some(4)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r\nzCoding system: "),
        ),
    );
    ctx.register_subr(
        SubrSpec::new(
            "find-operation-coding-system",
            NativeFn::ContextVec(builtin_find_operation_coding_system),
            SubrArity::new(1, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "iso-charset",
        NativeFn::ContextVec(|_ctx, args| builtin_iso_charset(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "keymap--get-keyelt",
        NativeFn::ContextVec(builtin_keymap_get_keyelt),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "keymap-prompt",
        NativeFn::ContextVec(builtin_keymap_prompt),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "lower-frame",
            NativeFn::ContextVec(|_ctx, args| builtin_lower_frame(args)),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "lread--substitute-object-in-subtree",
        NativeFn::ContextVec(|_ctx, args| builtin_lread_substitute_object_in_subtree(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "malloc-info",
            NativeFn::ContextVec(|_ctx, args| builtin_malloc_info(args)),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "malloc-trim",
            NativeFn::ContextVec(|_ctx, args| builtin_malloc_trim(args)),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "make-byte-code",
        NativeFn::ContextVec(|_ctx, args| builtin_make_byte_code(args)),
        SubrArity::new(4, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-char",
        NativeFn::ContextVec(|_ctx, args| charset::builtin_make_char(args)),
        SubrArity::new(1, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-closure",
        NativeFn::ContextVec(|_ctx, args| builtin_make_closure(args)),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-finalizer",
        NativeFn::ContextVec(builtin_make_finalizer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "marker-last-position",
        NativeFn::ContextVec(|_ctx, args| builtin_marker_last_position(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-interpreted-closure",
        NativeFn::ContextVec(|_ctx, args| builtin_make_interpreted_closure(args)),
        SubrArity::new(3, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-record",
        NativeFn::ContextVec(|_ctx, args| builtin_make_record(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-temp-file-internal",
        NativeFn::ContextVec(builtin_make_temp_file_internal),
        SubrArity::new(4, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "map-charset-chars",
        NativeFn::ContextVec(builtin_map_charset_chars),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "mapbacktrace",
            NativeFn::ContextVec(crate::emacs_core::misc::builtin_mapbacktrace),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "memory-info",
        NativeFn::ContextVec(|_ctx, args| builtin_memory_info(args)),
        SubrArity::new(0, Some(0)),
    ));
    // `memory-limit' is not here: it is a `defun' at lisp/subr.el:3574 over
    // `process-attributes', which IS registered (src/process.c)
    // (DIVERGENCES.md 152).
    ctx.register_subr(
        SubrSpec::new(
            "make-frame-invisible",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_make_frame_invisible),
            SubrArity::new(0, Some(2)),
        )
        .requires_eval_state()
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "menu-bar-menu-at-x-y",
        NativeFn::ContextVec(builtin_menu_bar_menu_at_x_y),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "menu-or-popup-active-p",
        NativeFn::ContextVec(|_ctx, args| builtin_menu_or_popup_active_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "module-load",
        NativeFn::ContextVec(builtin_module_load),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "newline-cache-check",
        NativeFn::ContextVec(|_ctx, args| builtin_newline_cache_check(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "native-comp-available-p",
        NativeFn::ContextVec(|_ctx, args| builtin_native_comp_available_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "obarray-clear",
        NativeFn::ContextVec(|_ctx, args| builtin_obarray_clear(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "obarray-make",
        NativeFn::ContextVec(|_ctx, args| builtin_obarray_make(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "object-intervals",
        NativeFn::ContextVec(builtin_object_intervals),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "open-dribble-file",
            NativeFn::ContextVec(builtin_open_dribble_file),
            SubrArity::new(1, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("FOpen dribble file: "),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "open-font",
        NativeFn::ContextVec(|_ctx, args| builtin_open_font(args)),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "optimize-char-table",
        NativeFn::ContextVec(|_ctx, args| builtin_optimize_char_table(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-lists",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_lists),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "overlay-recenter",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_overlay_recenter),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "pdumper-stats",
        NativeFn::ContextVec(|_ctx, args| builtin_pdumper_stats(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "play-sound-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::sound::builtin_play_sound_internal(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "position-symbol",
        NativeFn::ContextVec(builtin_position_symbol),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-cpu-log",
        NativeFn::ContextVec(builtin_profiler_cpu_log),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-cpu-running-p",
        NativeFn::ContextVec(builtin_profiler_cpu_running_p),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-cpu-start",
        NativeFn::ContextVec(builtin_profiler_cpu_start),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-cpu-stop",
        NativeFn::ContextVec(builtin_profiler_cpu_stop),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-memory-log",
        NativeFn::ContextVec(builtin_profiler_memory_log),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-memory-running-p",
        NativeFn::ContextVec(builtin_profiler_memory_running_p),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-memory-start",
        NativeFn::ContextVec(builtin_profiler_memory_start),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "profiler-memory-stop",
        NativeFn::ContextVec(builtin_profiler_memory_stop),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "put-unicode-property-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_put_unicode_property_internal(args)
        }),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "query-fontset",
        NativeFn::ContextVec(|_ctx, args| builtin_query_fontset(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "raise-frame",
            NativeFn::ContextVec(|_ctx, args| builtin_raise_frame(args)),
            SubrArity::new(0, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::Nil)
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "read-positioning-symbols",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::reader::builtin_read_impl(ctx, args, true)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "re--describe-compiled",
        NativeFn::ContextVec(|_ctx, args| builtin_re_describe_compiled(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "recent-auto-save-p",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_recent_auto_save_p),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "redisplay",
        NativeFn::ContextVec(builtin_redisplay),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs--frame-snapshot",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_neomacs_frame_snapshot),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs--write-frame-snapshot",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_neomacs_write_frame_snapshot),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs--debug-lose-device",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_neomacs_debug_lose_device),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "record",
        NativeFn::ContextVec(|_ctx, args| builtin_record(args)),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "recordp",
        builtin_recordp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::new(
        "reconsider-frame-fonts",
        NativeFn::ContextVec(builtin_reconsider_frame_fonts),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "redirect-debugging-output",
            NativeFn::ContextVec(builtin_redirect_debugging_output),
            SubrArity::new(1, Some(2)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "FDebug output file: \nP",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "redirect-frame-focus",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_redirect_frame_focus),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "remove-pos-from-symbol",
        NativeFn::ContextVec(|_ctx, args| builtin_remove_pos_from_symbol(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "resize-mini-window-internal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_resize_mini_window_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "restore-buffer-modified-p",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_restore_buffer_modified_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set--this-command-keys",
        NativeFn::ContextVec(builtin_set_this_command_keys),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-buffer-auto-saved",
        NativeFn::ContextVec(crate::emacs_core::buffer::builtin_set_buffer_auto_saved),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-buffer-major-mode",
        NativeFn::ContextVec(builtin_set_buffer_major_mode),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-buffer-redisplay",
        NativeFn::ContextVec(builtin_set_buffer_redisplay),
        SubrArity::new(4, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-charset-plist",
        NativeFn::ContextVec(|_ctx, args| builtin_set_charset_plist(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-frame-window-state-change",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_set_frame_window_state_change),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-fringe-bitmap-face",
        NativeFn::ContextVec(builtin_set_fringe_bitmap_face),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-minibuffer-window",
        NativeFn::ContextVec(builtin_set_minibuffer_window),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-mouse-pixel-position",
        NativeFn::ContextVec(builtin_set_mouse_pixel_position),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-mouse-position",
        NativeFn::ContextVec(builtin_set_mouse_position),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-new-normal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_new_normal),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-new-pixel",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_new_pixel),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-new-total",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_new_total),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sort-charsets",
        NativeFn::ContextVec(|_ctx, args| builtin_sort_charsets(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "split-char",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_split_char(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-distance",
        NativeFn::ContextVec(|_ctx, args| builtin_string_distance(args)),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "subr-native-lambda-list",
        NativeFn::ContextVec(|_ctx, args| builtin_subr_native_lambda_list(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "subr-type",
        NativeFn::ContextVec(|_ctx, args| builtin_subr_type(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "suspend-emacs",
            NativeFn::ContextVec(|_ctx, args| builtin_suspend_emacs(args)),
            SubrArity::new(0, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::Nil)
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "thread--blocker",
            NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_blocker),
            SubrArity::new(1, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(SubrSpec::new(
        "tool-bar-get-system-style",
        NativeFn::ContextVec(|_ctx, args| builtin_tool_bar_get_system_style(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tool-bar-pixel-width",
        NativeFn::ContextVec(|_ctx, args| builtin_tool_bar_pixel_width(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "translate-region-internal",
            NativeFn::ContextVec(crate::emacs_core::editfns::builtin_translate_region_internal),
            SubrArity::new(3, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "transpose-regions",
            NativeFn::ContextVec(builtin_transpose_regions),
            SubrArity::new(4, Some(5)),
        )
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                transpose_regions_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "tty--output-buffer-size",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_output_buffer_size(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty--set-output-buffer-size",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_set_output_buffer_size(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-display-pixel-height",
        NativeFn::ContextVec(builtin_tty_display_pixel_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-display-pixel-width",
        NativeFn::ContextVec(builtin_tty_display_pixel_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-frame-at",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_at),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-frame-edges",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_edges),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-frame-geometry",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_geometry),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-frame-list-z-order",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_tty_frame_list_z_order),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-frame-restack",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_frame_restack(args)),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "tty-suppress-bold-inverse-default-colors",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_suppress_bold_inverse_default_colors(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "unencodable-char-position",
        NativeFn::ContextVec(crate::emacs_core::coding::builtin_unencodable_char_position),
        SubrArity::new(3, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "unicode-property-table-internal",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_unicode_property_table_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "unify-charset",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_unify_charset(args)),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "unix-sync",
            NativeFn::ContextVec(|_ctx, args| builtin_unix_sync(args)),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "value<",
        NativeFn::ContextVec(builtin_value_lt),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-begin-drag",
        NativeFn::ContextVec(|_ctx, args| builtin_x_begin_drag(args)),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-double-buffered-p",
        NativeFn::ContextVec(|_ctx, args| builtin_x_double_buffered_p(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "x-menu-bar-open-internal",
            NativeFn::ContextVec(|_ctx, args| builtin_x_menu_bar_open_internal(args)),
            SubrArity::new(0, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("i")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "xw-color-defined-p",
            NativeFn::ContextVec(|ctx, args| {
                crate::emacs_core::xfaces::builtin_xw_color_defined_p_ctx(ctx, args)
            }),
            SubrArity::new(1, Some(2)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    // `color-defined-p' is lisp/faces.el:1923, not a DEFUN (DIVERGENCES.md
    // 154).  Its body dispatches on `display-graphic-p' to `xw-color-defined-p'
    // -- registered immediately above, and a C DEFUN in GNU -- or to
    // `tty-color-translate'.  Registering the graphical arm under the generic
    // name skipped that dispatch.
    ctx.register_subr(
        SubrSpec::new(
            "xw-color-values",
            NativeFn::ContextVec(|ctx, args| {
                crate::emacs_core::xfaces::builtin_xw_color_values_ctx(ctx, args)
            }),
            SubrArity::new(1, Some(2)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    // `color-values' is lisp/faces.el:1940, not a DEFUN (DIVERGENCES.md 154),
    // and dispatches the same way: `xw-color-values' (above, a C DEFUN) or
    // `tty-color-values'.
    ctx.register_subr(
        SubrSpec::new(
            "xw-display-color-p",
            NativeFn::ContextVec(|ctx, args| builtin_xw_display_color_p_ctx(ctx, args)),
            SubrArity::new(0, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    std::cfg_select! {
        any(target_os = "linux", target_os = "macos", target_os = "windows") => {
            file_notify::register_subrs(ctx);
        }
        _ => {}
    }
    ctx.register_subr(SubrSpec::new(
        "lock-buffer",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_lock_buffer),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "lock-file",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_lock_file),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "lossage-size",
            NativeFn::ContextVec(|_ctx, args| builtin_lossage_size(args)),
            SubrArity::new(0, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                lossage_size_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "unlock-buffer",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_unlock_buffer),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "unlock-file",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_unlock_file),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-bottom-divider-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_bottom_divider_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-lines-pixel-dimensions",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_lines_pixel_dimensions(args)
        }),
        SubrArity::new(0, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-new-normal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_new_normal),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-new-pixel",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_new_pixel),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-new-total",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_new_total),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-old-body-pixel-height",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::window_cmds::builtin_window_old_body_pixel_height(args)
            }),
            SubrArity::new(0, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::FixnumZero),
    );
    ctx.register_subr(SubrSpec::new(
        "window-old-body-pixel-width",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_old_body_pixel_width(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-old-pixel-height",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_old_pixel_height(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-old-pixel-width",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_old_pixel_width(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-right-divider-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_right_divider_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-scroll-bar-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_scroll_bar_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-scroll-bar-width",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_scroll_bar_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-available-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_available_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-compiled-query-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_compiled_query_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-induce-sparse-tree",
        NativeFn::ContextVec(builtin_treesit_induce_sparse_tree),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-language-abi-version",
        NativeFn::ContextVec(builtin_treesit_language_abi_version),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-language-available-p",
        NativeFn::ContextVec(builtin_treesit_language_available_p),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-library-abi-version",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_library_abi_version(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-check",
        NativeFn::ContextVec(builtin_treesit_node_check),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-child",
        NativeFn::ContextVec(builtin_treesit_node_child),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-child-by-field-name",
        NativeFn::ContextVec(builtin_treesit_node_child_by_field_name),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-child-count",
        NativeFn::ContextVec(builtin_treesit_node_child_count),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-descendant-for-range",
        NativeFn::ContextVec(builtin_treesit_node_descendant_for_range),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-end",
        NativeFn::ContextVec(builtin_treesit_node_end),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-eq",
        NativeFn::ContextVec(builtin_treesit_node_eq),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-field-name-for-child",
        NativeFn::ContextVec(builtin_treesit_node_field_name_for_child),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-first-child-for-pos",
        NativeFn::ContextVec(builtin_treesit_node_first_child_for_pos),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-match-p",
        NativeFn::ContextVec(builtin_treesit_node_match_p),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-next-sibling",
        NativeFn::ContextVec(builtin_treesit_node_next_sibling),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_node_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-parent",
        NativeFn::ContextVec(builtin_treesit_node_parent),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-parser",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_node_parser(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-prev-sibling",
        NativeFn::ContextVec(builtin_treesit_node_prev_sibling),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-start",
        NativeFn::ContextVec(builtin_treesit_node_start),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-string",
        NativeFn::ContextVec(builtin_treesit_node_string),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-node-type",
        NativeFn::ContextVec(builtin_treesit_node_type),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-add-notifier",
        NativeFn::ContextVec(builtin_treesit_parser_add_notifier),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-buffer",
        NativeFn::ContextVec(builtin_treesit_parser_buffer),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-create",
        NativeFn::ContextVec(builtin_treesit_parser_create),
        SubrArity::new(1, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-delete",
        NativeFn::ContextVec(builtin_treesit_parser_delete),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-included-ranges",
        NativeFn::ContextVec(builtin_treesit_parser_included_ranges),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-language",
        NativeFn::ContextVec(builtin_treesit_parser_language),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-list",
        NativeFn::ContextVec(builtin_treesit_parser_list),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-notifiers",
        NativeFn::ContextVec(builtin_treesit_parser_notifiers),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_parser_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-remove-notifier",
        NativeFn::ContextVec(builtin_treesit_parser_remove_notifier),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-root-node",
        NativeFn::ContextVec(builtin_treesit_parser_root_node),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-set-included-ranges",
        NativeFn::ContextVec(builtin_treesit_parser_set_included_ranges),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-tag",
        NativeFn::ContextVec(builtin_treesit_parser_tag),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-pattern-expand",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_pattern_expand(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-capture",
        NativeFn::ContextVec(builtin_treesit_query_capture),
        SubrArity::new(2, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-compile",
        NativeFn::ContextVec(builtin_treesit_query_compile),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-expand",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_expand(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-language",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_language(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-search-forward",
        NativeFn::ContextVec(builtin_treesit_search_forward),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-search-subtree",
        NativeFn::ContextVec(builtin_treesit_search_subtree),
        SubrArity::new(2, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-subtree-stat",
        NativeFn::ContextVec(builtin_treesit_subtree_stat),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-grammar-location",
        NativeFn::ContextVec(builtin_treesit_grammar_location),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-tracking-line-column-p",
        NativeFn::ContextVec(builtin_treesit_tracking_line_column_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-tracking-line-column-p",
        NativeFn::ContextVec(builtin_treesit_parser_tracking_line_column_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-eagerly-compiled-p",
        NativeFn::ContextVec(builtin_treesit_query_eagerly_compiled_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-query-source",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_source(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-embed-level",
        NativeFn::ContextVec(builtin_treesit_parser_embed_level),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-set-embed-level",
        NativeFn::ContextVec(builtin_treesit_parser_set_embed_level),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parse-string",
        NativeFn::ContextVec(builtin_treesit_parse_string),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit-parser-changed-regions",
        NativeFn::ContextVec(builtin_treesit_parser_changed_regions),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit--linecol-at",
        NativeFn::ContextVec(builtin_treesit_linecol_at),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit--linecol-cache-set",
        NativeFn::ContextVec(builtin_treesit_linecol_cache_set),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "treesit--linecol-cache",
        NativeFn::ContextVec(builtin_treesit_linecol_cache),
        SubrArity::new(0, Some(0)),
    ));
    crate::emacs_core::sqlite::register_subrs(ctx);
    // GNU emacs.c sequences font.c immediately after sqlite.c.
    crate::emacs_core::font::register_subrs(ctx);
    ctx.register_subr(SubrSpec::new(
        "fillarray",
        NativeFn::ContextVec(|_ctx, args| builtin_fillarray(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "define-hash-table-test",
        NativeFn::ContextVec(|_ctx, args| builtin_define_hash_table_test(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-test",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_test(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-size",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_size(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-rehash-size",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_rehash_size(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-rehash-threshold",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_rehash_threshold(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-weakness",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_weakness(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copy-hash-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_copy_hash_table(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sxhash-eq",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::hashtab::builtin_sxhash_eq(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sxhash-eql",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::hashtab::builtin_sxhash_eql(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sxhash-equal",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::hashtab::builtin_sxhash_equal(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sxhash-equal-including-properties",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_sxhash_equal_including_properties(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal--hash-table-buckets",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_internal_hash_table_buckets(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal--hash-table-histogram",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_internal_hash_table_histogram(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal--hash-table-index-size",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_internal_hash_table_index_size(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-frame-geometry",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_frame_geometry(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-frame-edges",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_neomacs_frame_edges),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_mouse_absolute_pixel_position(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-set-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_set_mouse_absolute_pixel_position(args)),
        SubrArity::new(0, None),
    ));
    crate::emacs_core::neo::effects::register_subrs(ctx);
    ctx.register_subr(SubrSpec::new(
        "neomacs-display-monitor-attributes-list",
        NativeFn::ContextVec(builtin_neomacs_display_monitor_attributes_list),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-clipboard-set",
        NativeFn::ContextVec(builtin_neomacs_clipboard_set),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-clipboard-get",
        NativeFn::ContextVec(builtin_neomacs_clipboard_get),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-primary-selection-set",
        NativeFn::ContextVec(builtin_neomacs_primary_selection_set),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-primary-selection-get",
        NativeFn::ContextVec(builtin_neomacs_primary_selection_get),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-core-backend",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_core_backend(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_buffer_text_backend),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-default-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_default_buffer_text_backend),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-set-default-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_set_default_buffer_text_backend),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "neomacs-set-buffer-text-backend",
        NativeFn::ContextVec(builtin_neomacs_set_buffer_text_backend),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "buffer-local-toplevel-value",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_buffer_local_toplevel_value),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-buffer-local-toplevel-value",
        NativeFn::ContextVec(crate::emacs_core::custom::builtin_set_buffer_local_toplevel_value),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "debugger-trap",
        NativeFn::ContextVec(|_ctx, args| builtin_debugger_trap(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-delete-indirect-variable",
        NativeFn::ContextVec(builtin_internal_delete_indirect_variable),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-buffer-disposition",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_buffer_disposition),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "thread-set-buffer-disposition",
        NativeFn::ContextVec(crate::emacs_core::threads::builtin_thread_set_buffer_disposition),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-discard-buffer-from-window",
        NativeFn::ContextVec(
            crate::emacs_core::window_cmds::builtin_window_discard_buffer_from_window,
        ),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-cursor-info",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_cursor_info),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "combine-windows",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_combine_windows),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "uncombine-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_uncombine_window),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-windows-min-size",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_windows_min_size(args)),
        SubrArity::new(4, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "remember-mouse-glyph",
        NativeFn::ContextVec(builtin_remember_mouse_glyph),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "obarrayp",
        NativeFn::ContextVec(|_ctx, args| builtin_obarrayp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ntake",
        NativeFn::ContextVec(|_ctx, args| builtin_ntake(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "default-file-modes",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_default_file_modes(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-default-file-modes",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_set_default_file_modes(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "cancel-kbd-macro-events",
        NativeFn::ContextVec(builtin_cancel_kbd_macro_events),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-configuration-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_configuration_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-configuration-frame",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_configuration_frame(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-configuration-equal-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_cmds::builtin_window_configuration_equal_p(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-input-meta-mode",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::reader::builtin_set_input_meta_mode(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-output-flow-control",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::reader::builtin_set_output_flow_control(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-quit-char",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_set_quit_char),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "top-level",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::minibuffer::builtin_top_level(args)
            }),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "documentation-stringp",
        NativeFn::ContextVec(|_ctx, args| builtin_documentation_stringp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "internal--define-uninitialized-variable",
            NativeFn::ContextVec(symbols::builtin_internal_define_uninitialized_variable),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "window-text-pixel-size",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_window_text_pixel_size_ctx),
        SubrArity::new(0, Some(7)),
    ));
    ctx.register_subr(SubrSpec::new(
        "pos-visible-in-window-p",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_pos_visible_in_window_p_ctx),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame--face-hash-table",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_frame_face_hash_table),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-directory-internal",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_delete_directory_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-directory-internal",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_make_directory_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "directory-files-and-attributes",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_directory_files_and_attributes),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "find-file-name-handler",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_find_file_name_handler),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-all-completions",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_name_all_completions),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-accessible-directory-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_accessible_directory_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-name-case-insensitive-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_case_insensitive_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "file-newer-than-file-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_newer_than_file_p),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "verify-visited-file-modtime",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_verify_visited_file_modtime),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-default-interrupt-process",
        NativeFn::ContextVec(
            crate::emacs_core::process::builtin_internal_default_interrupt_process,
        ),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-default-process-filter",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_process_filter),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-default-process-sentinel",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_process_sentinel),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-default-signal-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_signal_process),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "network-lookup-address-info",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_network_lookup_address_info),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-network-process-option",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_network_process_option),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-query-on-exit-flag",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_query_on_exit_flag),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-query-on-exit-flag",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_query_on_exit_flag),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "process-inherit-coding-system-flag",
        NativeFn::ContextVec(
            crate::emacs_core::process::builtin_process_inherit_coding_system_flag,
        ),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-coding-system",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_coding_system),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-process-datagram-address",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_datagram_address),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "remove-list-of-text-properties",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_remove_list_of_text_properties),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-char-property-and-overlay",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_get_char_property_and_overlay),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "next-single-property-change",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_next_single_property_change),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "previous-single-property-change",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_previous_single_property_change),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "line-beginning-position",
        NativeFn::ContextVec(crate::emacs_core::navigation::builtin_line_beginning_position),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "make-variable-buffer-local",
            NativeFn::ContextVec(crate::emacs_core::custom::builtin_make_variable_buffer_local),
            SubrArity::new(1, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "vMake Variable Buffer Local: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "active-minibuffer-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_active_minibuffer_window),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minibuffer-selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_minibuffer_selected_window),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-mode-line-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_mode_line_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-header-line-height",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_header_line_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "window-tab-line-height",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_tab_line_height),
            SubrArity::new(0, Some(1)),
        )
        .placeholder(NoEvalPlaceholder::FixnumZero),
    );
    ctx.register_subr(SubrSpec::new(
        "set-window-display-table",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_display_table),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-cursor-type",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_cursor_type),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-scroll-bars",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_scroll_bars),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-next-buffers",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_next_buffers),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-prev-buffers",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_prev_buffers),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-dedicated-p",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_dedicated_p),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-window-internal",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_delete_window_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "delete-other-windows-internal",
            NativeFn::ContextVec(
                crate::emacs_core::window_cmds::builtin_delete_other_windows_internal,
            ),
            SubrArity::new(0, Some(2)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "window-combination-limit",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_combination_limit),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-window-combination-limit",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_window_combination_limit),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "window-resize-apply-total",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_window_resize_apply_total),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "other-window-for-scrolling",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_other_window_for_scrolling),
        SubrArity::new(0, Some(0)),
    ));
    // `select-frame-set-input-focus' is lisp/frame.el:1262, not a DEFUN
    // (DIVERGENCES.md 154).  Its body is `select-frame' + `x-focus-frame' +
    // `raise-frame', all three C DEFUNs that stay registered.
    ctx.register_subr(SubrSpec::new(
        "modify-frame-parameters",
        NativeFn::ContextVec(crate::emacs_core::frame::builtin_modify_frame_parameters),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame-selected-window",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_selected_window),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "frame-old-selected-window",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_frame_old_selected_window),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "set-frame-selected-window",
            NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_set_frame_selected_window),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "x-display-pixel-width",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_pixel_width),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-pixel-height",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_pixel_height),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-server-max-request-size",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_server_max_request_size),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-grayscale-p",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_grayscale_p),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-backing-store",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_backing_store),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-color-cells",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_color_cells),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-save-under",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_save_under),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-set-last-user-time",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_set_last_user_time),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-visual-class",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_display_visual_class),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minor-mode-key-binding",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_minor_mode_key_binding),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "this-command-keys-vector",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_this_command_keys_vector),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "this-single-command-keys",
            NativeFn::ContextVec(crate::emacs_core::interactive::builtin_this_single_command_keys),
            SubrArity::new(0, Some(0)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(
        SubrSpec::new(
            "this-single-command-raw-keys",
            NativeFn::ContextVec(
                crate::emacs_core::interactive::builtin_this_single_command_raw_keys,
            ),
            SubrArity::new(0, Some(0)),
        )
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(SubrSpec::new(
        "clear-this-command-keys",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_clear_this_command_keys),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "waiting-for-user-input-p",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_waiting_for_user_input_p_ctx),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "minibuffer-prompt",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_prompt_ctx),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "minibuffer-prompt-end",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_prompt_end_ctx),
            SubrArity::new(0, Some(0)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "innermost-minibuffer-p",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_innermost_minibuffer_p_ctx),
            SubrArity::new(0, Some(1)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "backtrace--frames-from-thread",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_backtrace_frames_from_thread),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "abort-minibuffers",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_abort_minibuffers_ctx),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(SubrSpec::new(
        "set-marker-insertion-type",
        NativeFn::ContextVec(crate::emacs_core::marker::builtin_set_marker_insertion_type),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-standard-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab::builtin_set_standard_case_table),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-unused-category",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_get_unused_category),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "standard-category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_standard_category_table),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "upcase-initials-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_upcase_initials_region),
            SubrArity::new(2, Some(3)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "buffer-substring-no-properties",
        NativeFn::ContextVec(|eval, args| {
            crate::emacs_core::editfns::builtin_buffer_substring_no_properties(eval, args)
        }),
        SubrArity::new(2, Some(2)),
    ));

    // Pure builtins from builtins_extra (previously in old match dispatch).
    // These don't need &mut Context, so we wrap them.
    macro_rules! register_pure_subr {
        ($ctx:expr, $name:expr, $func:expr, $min:expr, $max:expr) => {
            $ctx.register_subr(SubrSpec::new(
                $name,
                NativeFn::ContextVec(|_eval, args| $func(args)),
                SubrArity::new($min, $max),
            ));
        };
    }
    register_pure_subr!(
        ctx,
        "take",
        crate::emacs_core::builtins_extra::builtin_take,
        2,
        Some(2)
    );
    register_pure_subr!(
        ctx,
        "assoc-string",
        crate::emacs_core::builtins_extra::builtin_assoc_string,
        2,
        Some(3)
    );
    register_pure_subr!(
        ctx,
        "string-search",
        crate::emacs_core::builtins_extra::builtin_string_search,
        2,
        Some(3)
    );
    ctx.register_subr(SubrSpec::fixed1(
        "bare-symbol",
        crate::emacs_core::builtins_extra::builtin_bare_symbol_1,
        FixedMin1::One,
    ));
    register_pure_subr!(
        ctx,
        "bare-symbol-p",
        crate::emacs_core::builtins_extra::builtin_bare_symbol_p,
        1,
        Some(1)
    );
    register_pure_subr!(
        ctx,
        "byteorder",
        crate::emacs_core::builtins_extra::builtin_byteorder,
        0,
        Some(0)
    );
    register_pure_subr!(
        ctx,
        "car-less-than-car",
        crate::emacs_core::builtins_extra::builtin_car_less_than_car,
        2,
        Some(2)
    );
    register_pure_subr!(
        ctx,
        "proper-list-p",
        crate::emacs_core::builtins_extra::builtin_proper_list_p,
        1,
        Some(1)
    );
    register_pure_subr!(
        ctx,
        "subrp",
        crate::emacs_core::builtins_extra::builtin_subrp,
        1,
        Some(1)
    );
    register_pure_subr!(
        ctx,
        "byte-code-function-p",
        crate::emacs_core::builtins_extra::builtin_byte_code_function_p,
        1,
        Some(1)
    );
    ctx.register_subr(SubrSpec::fixed1(
        "closurep",
        crate::emacs_core::builtins_extra::builtin_closurep_1,
        FixedMin1::One,
    ));
    register_pure_subr!(
        ctx,
        "natnump",
        crate::emacs_core::builtins_extra::builtin_natnump,
        1,
        Some(1)
    );
    // GNU defines `fixnump` and `bignump` in `lisp/subr.el` (not in C),
    // so they must come from the loaded Lisp source — registering Rust
    // subrs here would shadow the elisp definitions and make
    // `(subrp (symbol-function 'fixnump))` return t instead of nil.
    ctx.register_subr(SubrSpec::new(
        "user-login-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_user_login_name),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "user-real-login-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_user_real_login_name),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "user-full-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_user_full_name),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "system-name",
        NativeFn::ContextVec(crate::emacs_core::builtins_extra::builtin_system_name),
        SubrArity::new(0, Some(0)),
    ));
    register_pure_subr!(
        ctx,
        "emacs-pid",
        crate::emacs_core::builtins_extra::builtin_emacs_pid,
        0,
        Some(0)
    );
    register_pure_subr!(
        ctx,
        "memory-use-counts",
        crate::emacs_core::builtins_extra::builtin_memory_use_counts,
        0,
        Some(0)
    );
    register_pure_subr!(
        ctx,
        "neomacs--heap-layout-stats",
        crate::emacs_core::builtins_extra::builtin_neomacs_heap_layout_stats,
        0,
        Some(0)
    );

    // -----------------------------------------------------------------------
    // Additional native subr declarations.
    // -----------------------------------------------------------------------

    // -- Arithmetic --
    ctx.register_subr(SubrSpec::new(
        "+",
        NativeFn::ContextSlice(crate::emacs_core::builtins::arithmetic::builtin_add_slice),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "-",
        NativeFn::ContextSlice(crate::emacs_core::builtins::arithmetic::builtin_sub_slice),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "*",
        NativeFn::ContextVec(|_ctx, args| builtin_mul(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "/",
        NativeFn::ContextVec(|_ctx, args| builtin_div(args)),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::fixed2("%", builtin_percent, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("mod", builtin_mod, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed1("1+", builtin_add1_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1("1-", builtin_sub1_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::new(
        "max",
        NativeFn::ContextSlice(builtin_max_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "min",
        NativeFn::ContextSlice(builtin_min_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "abs",
        NativeFn::ContextVec(|_ctx, args| builtin_abs(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- Logical / bitwise --
    ctx.register_subr(SubrSpec::new(
        "logand",
        NativeFn::ContextSlice(|_ctx, args| builtin_logand_slice(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "logior",
        NativeFn::ContextSlice(|_ctx, args| builtin_logior_slice(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "logxor",
        NativeFn::ContextSlice(|_ctx, args| builtin_logxor_slice(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "lognot",
        NativeFn::ContextVec(|_ctx, args| builtin_lognot(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ash",
        NativeFn::ContextSlice(|_ctx, args| builtin_ash_slice(args)),
        SubrArity::new(2, Some(2)),
    ));

    // -- Numeric comparisons --
    ctx.register_subr(SubrSpec::new(
        "=",
        NativeFn::ContextSlice(builtin_num_eq_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "<",
        NativeFn::ContextSlice(builtin_num_lt_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "<=",
        NativeFn::ContextSlice(builtin_num_le_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        ">",
        NativeFn::ContextSlice(builtin_num_gt_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        ">=",
        NativeFn::ContextSlice(builtin_num_ge_slice),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::fixed2("/=", builtin_num_ne_2, FixedMin2::Two));

    // -- Type predicates --
    ctx.register_subr(SubrSpec::fixed1("null", builtin_null_1, FixedMin1::One));
    // No `not': GNU has no DEFUN of that name.  `(defalias 'not #'null)'
    // (lisp/subr.el:71) puts the SYMBOL `null' in the cell, and a compiled
    // caller emits the Bnot opcode instead -- DIVERGENCES.md 148.
    ctx.register_subr(SubrSpec::fixed1("atom", builtin_atom_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1("consp", builtin_consp_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1("listp", builtin_listp_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1("nlistp", builtin_nlistp_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1(
        "symbolp",
        builtin_symbolp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "numberp",
        builtin_numberp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "integerp",
        builtin_integerp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1("floatp", builtin_floatp_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1(
        "stringp",
        builtin_stringp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "vectorp",
        builtin_vectorp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::new(
        "characterp",
        NativeFn::ContextVec(|_ctx, args| builtin_characterp(args)),
        SubrArity::new(1, Some(2)),
    ));
    // No `booleanp', `integer-or-null-p', `string-or-null-p',
    // `list-of-strings-p' or `char-uppercase-p': GNU has no DEFUN for any of
    // the six type predicates.  They are `defun's at lisp/subr.el:4762-4812
    // and lisp/simple.el:6683 over primitives that ARE in C
    // (`stringp', `integerp', `indirect-function',
    // `get-char-code-property') -- DIVERGENCES.md 148.
    ctx.register_subr(SubrSpec::fixed1(
        "keywordp",
        builtin_keywordp_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-p",
        NativeFn::ContextVec(|_ctx, args| builtin_hash_table_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bufferp",
        NativeFn::ContextVec(|_ctx, args| builtin_bufferp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "type-of",
        NativeFn::ContextVec(crate::emacs_core::builtins::types::builtin_type_of_with_ctx),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "sequencep",
        NativeFn::ContextVec(|_ctx, args| builtin_sequencep(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "arrayp",
        NativeFn::ContextVec(|_ctx, args| builtin_arrayp(args)),
        SubrArity::new(1, Some(1)),
    ));
    // `ignore' is not here: it is a `defun' at lisp/subr.el:501, and the
    // byte compiler names it itself -- `(byte-defop-compiler-1 ignore)',
    // lisp/emacs-lisp/bytecomp.el:4429 -- so a compiled `(ignore X)' emits
    // Bconstant nil and never reads the cell (DIVERGENCES.md 152).
    ctx.register_subr(SubrSpec::new(
        "cl-type-of",
        NativeFn::ContextVec(|_ctx, args| builtin_cl_type_of(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- Equality --
    ctx.register_subr(SubrSpec::fixed2("eq", builtin_eq_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("eql", builtin_eql_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("equal", builtin_equal_2, FixedMin2::Two));

    // -- Cons / List --
    ctx.register_subr(SubrSpec::fixed2("cons", builtin_cons_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed1("car", builtin_car_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1("cdr", builtin_cdr_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed1(
        "car-safe",
        builtin_car_safe_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "cdr-safe",
        builtin_cdr_safe_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed2("setcar", builtin_setcar_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("setcdr", builtin_setcdr_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "list",
        NativeFn::ContextSlice(builtin_list_slice),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::fixed1("length", builtin_length_1, FixedMin1::One));
    ctx.register_subr(SubrSpec::fixed2("nth", builtin_nth_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("nthcdr", builtin_nthcdr_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "append",
        NativeFn::ContextSlice(builtin_append_slice),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "reverse",
        NativeFn::ContextVec(|_ctx, args| builtin_reverse(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "nreverse",
        builtin_nreverse_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed2("member", builtin_member_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("memq", builtin_memq_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::fixed2("assq", builtin_assq_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "copy-sequence",
        NativeFn::ContextVec(|_ctx, args| builtin_copy_sequence(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "plist-get",
        NativeFn::ContextSlice(builtin_plist_get_slice),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "plist-put",
        NativeFn::ContextVec(builtin_plist_put_with_ctx),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copy-alist",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_copy_alist(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "rassoc",
        NativeFn::ContextVec(crate::emacs_core::misc::builtin_rassoc_with_ctx),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::fixed2(
        "rassq",
        crate::emacs_core::misc::builtin_rassq_2,
        FixedMin2::Two,
    ));
    ctx.register_subr(SubrSpec::new(
        "make-list",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_make_list(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "safe-length",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_safe_length(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- String --
    // GNU DEFUNs `string-equal' and `string-lessp' (src/fns.c) and nothing
    // else here: `string=', `string<' and `string>' are `defalias'es at
    // lisp/subr.el:2277-2279, so their cells hold the TARGET SYMBOL, and a
    // compiled caller emits Bstringeqlsign / Bstringlss instead
    // (DIVERGENCES.md 148).  `string-greaterp' is itself Lisp
    // (lisp/subr.el:6283) with a `compiler-macro' (:6287-6290) that swaps
    // its arguments into `string-lessp', so it is not registered either
    // (DIVERGENCES.md 152).
    ctx.register_subr(SubrSpec::fixed2(
        "string-equal",
        builtin_string_equal_2,
        FixedMin2::Two,
    ));
    ctx.register_subr(SubrSpec::fixed2(
        "string-lessp",
        builtin_string_lessp_2,
        FixedMin2::Two,
    ));
    ctx.register_subr(SubrSpec::new(
        "substring",
        NativeFn::ContextSlice(|_ctx, args| builtin_substring_slice(args)),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "concat",
        NativeFn::ContextSlice(|_ctx, args| builtin_concat_slice(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "unibyte-string",
        NativeFn::ContextVec(|_ctx, args| builtin_unibyte_string(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::fixed2(
        "string-to-number",
        builtin_string_to_number,
        FixedMin2::One,
    ));
    ctx.register_subr(SubrSpec::new(
        "number-to-string",
        NativeFn::ContextVec(|ctx, args| builtin_number_to_string(ctx, args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "upcase",
        NativeFn::ContextVec(builtin_upcase_in_state),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "downcase",
        NativeFn::ContextVec(builtin_downcase_in_state),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-to-string",
        NativeFn::ContextVec(|_ctx, args| builtin_char_to_string(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-to-char",
        NativeFn::ContextVec(|_ctx, args| builtin_string_to_char(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "clear-string",
        NativeFn::ContextVec(|_ctx, args| builtin_clear_string(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "compare-strings",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::fns::builtin_compare_strings(args)),
        SubrArity::new(6, Some(7)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-version-lessp",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_string_version_lessp),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-collate-lessp",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_string_collate_lessp),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-collate-equalp",
        NativeFn::ContextVec(crate::emacs_core::fns::builtin_string_collate_equalp),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::fixed2(
        "equal-including-properties",
        crate::emacs_core::fns::builtin_equal_including_properties_2,
        FixedMin2::Two,
    ));
    ctx.register_subr(SubrSpec::new(
        "string-make-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_string_make_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-make-unibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_string_make_unibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-to-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_string_to_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-to-unibyte",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_string_to_unibyte(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-as-unibyte",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_string_as_unibyte(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-as-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_string_as_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "unibyte-char-to-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_unibyte_char_to_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "multibyte-char-to-unibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_multibyte_char_to_unibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    ));

    // -- Vector --
    ctx.register_subr(SubrSpec::new(
        "make-vector",
        NativeFn::ContextVec(|_ctx, args| builtin_make_vector(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "vector",
        NativeFn::ContextSlice(builtin_vector_slice),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::fixed2("aref", builtin_aref_2, FixedMin2::Two));
    ctx.register_subr(SubrSpec::new(
        "aset",
        NativeFn::ContextVec(|_ctx, args| builtin_aset(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "vconcat",
        NativeFn::ContextSlice(|_ctx, args| builtin_vconcat_slice(args)),
        SubrArity::new(0, None),
    ));

    // -- Hash table --
    ctx.register_subr(SubrSpec::new(
        "make-hash-table",
        NativeFn::ContextSlice(|_ctx, args| builtin_make_hash_table_slice(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::fixed3(
        "gethash",
        builtin_gethash_3,
        FixedMin3::Two,
    ));
    ctx.register_subr(SubrSpec::fixed3(
        "puthash",
        builtin_puthash_3,
        FixedMin3::Three,
    ));
    ctx.register_subr(SubrSpec::fixed2(
        "remhash",
        builtin_remhash_2,
        FixedMin2::Two,
    ));
    ctx.register_subr(SubrSpec::new(
        "clrhash",
        NativeFn::ContextVec(|_ctx, args| builtin_clrhash(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "hash-table-count",
        NativeFn::ContextVec(|_ctx, args| builtin_hash_table_count(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- Float / math / conversion --
    ctx.register_subr(SubrSpec::new(
        "float",
        NativeFn::ContextVec(|_ctx, args| builtin_float(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "truncate",
        NativeFn::ContextVec(|_ctx, args| builtin_truncate(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "floor",
        NativeFn::ContextVec(|_ctx, args| builtin_floor(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ceiling",
        NativeFn::ContextVec(|_ctx, args| builtin_ceiling(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "round",
        NativeFn::ContextVec(|_ctx, args| builtin_round(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copysign",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_copysign(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frexp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_frexp(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ldexp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_ldexp(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "logb",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_logb(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "fceiling",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_fceiling(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ffloor",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_ffloor(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "fround",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_fround(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ftruncate",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_ftruncate(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- Symbol --
    ctx.register_subr(SubrSpec::fixed1(
        "symbol-name",
        builtin_symbol_name_1,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed1(
        "make-symbol",
        builtin_make_symbol_1,
        FixedMin1::One,
    ));

    // -- Misc pure --
    ctx.register_subr(SubrSpec::new(
        "bitmap-spec-p",
        NativeFn::ContextVec(|_ctx, args| builtin_bitmap_spec_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "byte-to-string",
        NativeFn::ContextVec(|_ctx, args| builtin_byte_to_string(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "clear-buffer-auto-save-failure",
        NativeFn::ContextVec(|_ctx, args| builtin_clear_buffer_auto_save_failure(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "clear-face-cache",
        NativeFn::ContextVec(builtin_clear_face_cache),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "combine-after-change-execute",
        NativeFn::ContextVec(builtin_combine_after_change_execute),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "command-error-default-function",
        NativeFn::ContextVec(builtin_command_error_default_function),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "locale-info",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_locale_info(args)),
        SubrArity::new(1, Some(1)),
    ));
    // -- Subr introspection --
    ctx.register_subr(SubrSpec::new(
        "subr-name",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::subr_info::builtin_subr_name(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "subr-arity",
        NativeFn::ContextVec(crate::emacs_core::subr_info::builtin_subr_arity),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "native-comp-function-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::subr_info::builtin_native_comp_function_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "interpreted-function-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::subr_info::builtin_interpreted_function_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "func-arity",
        NativeFn::ContextVec(builtin_func_arity),
        SubrArity::new(1, Some(1)),
    ));

    // -- Character encoding --
    ctx.register_subr(SubrSpec::new(
        "char-width",
        NativeFn::ContextVec(|ctx, args| crate::encoding::builtin_char_width_in_context(ctx, args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "string-bytes",
        NativeFn::ContextVec(|_ctx, args| crate::encoding::builtin_string_bytes(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "multibyte-string-p",
        NativeFn::ContextVec(|_ctx, args| crate::encoding::builtin_multibyte_string_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "encode-coding-string",
        NativeFn::ContextVec(crate::encoding::builtin_encode_coding_string_in_context),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "decode-coding-string",
        NativeFn::ContextVec(crate::encoding::builtin_decode_coding_string_in_context),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-or-string-p",
        NativeFn::ContextVec(|_ctx, args| crate::encoding::builtin_char_or_string_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "max-char",
        NativeFn::ContextVec(|_ctx, args| crate::encoding::builtin_max_char(args)),
        SubrArity::new(0, Some(1)),
    ));

    // -- Search --
    ctx.register_subr(SubrSpec::new(
        "regexp-quote",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::search::builtin_regexp_quote(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- File I/O --
    ctx.register_subr(SubrSpec::new(
        "file-attributes-lessp",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_attributes_lessp),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "system-users",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::dired::builtin_system_users(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "system-groups",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::dired::builtin_system_groups(args)),
        SubrArity::new(0, Some(0)),
    ));

    // -- User / editfns --
    ctx.register_subr(SubrSpec::new(
        "user-uid",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_user_uid(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "user-real-uid",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_user_real_uid(args)),
        SubrArity::new(0, Some(0)),
    ));

    // -- Time/date --
    ctx.register_subr(SubrSpec::new(
        "time-add",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_add(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "time-subtract",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_subtract(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "time-less-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_less_p(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "time-equal-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_equal_p(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-time-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::timefns::builtin_current_time_string(args)
        }),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-time-zone",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::timefns::builtin_current_time_zone(args)
        }),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "encode-time",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_encode_time(args)),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "decode-time",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_decode_time(args)),
        SubrArity::new(0, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "time-convert",
        NativeFn::ContextVec(crate::emacs_core::timefns::builtin_time_convert_in_context),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-time-zone-rule",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::timefns::builtin_set_time_zone_rule(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "format-time-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::format::builtin_format_time_string(args)
        }),
        SubrArity::new(1, Some(3)),
    ));

    // -- Case/char --
    ctx.register_subr(SubrSpec::new(
        "upcase-initials",
        NativeFn::ContextVec(crate::emacs_core::casefiddle::builtin_upcase_initials_in_state),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-resolve-modifiers",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::casefiddle::builtin_char_resolve_modifiers(args)
        }),
        SubrArity::new(1, Some(1)),
    ));

    // -- Font/face --
    ctx.register_subr(SubrSpec::new(
        "internal-lisp-face-attribute-values",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_lisp_face_attribute_values(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-lisp-face-equal-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_lisp_face_equal_p),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-lisp-face-empty-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_lisp_face_empty_p),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "face-attribute-relative-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_face_attribute_relative_p(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "merge-face-attribute",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_merge_face_attribute_with_eval),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "color-gray-p",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_color_gray_p),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "color-supported-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_color_supported_p(args)
        }),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "color-distance",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_color_distance),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "color-values-from-color-spec",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_color_values_from_color_spec(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-face-x-get-resource",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_face_x_get_resource(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-set-font-selection-order",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_set_font_selection_order(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-set-alternative-font-family-alist",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_set_alternative_font_family_alist(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-set-alternative-font-registry-alist",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_set_alternative_font_registry_alist(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-copy-lisp-face",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_copy_lisp_face),
        SubrArity::new(4, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-get-lisp-face-attribute",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_get_lisp_face_attribute),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "internal-merge-in-global-face",
        NativeFn::ContextVec(crate::emacs_core::xfaces::builtin_internal_merge_in_global_face),
        SubrArity::new(2, Some(2)),
    ));

    // -- Case table --
    ctx.register_subr(SubrSpec::new(
        "case-table-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::casetab::builtin_case_table_p(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- Category --
    ctx.register_subr(SubrSpec::new(
        "category-table-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_category_table_p(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "copy-category-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_copy_category_table(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-category-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_make_category_table(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "category-set-mnemonics",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_category_set_mnemonics(args)
        }),
        SubrArity::new(1, Some(1)),
    ));

    // -- Char-table / bool-vector --
    ctx.register_subr(SubrSpec::new(
        "char-table-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::chartable::builtin_char_table_p(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-char-table-range",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::chartable::builtin_set_char_table_range(args, Some(&ctx.obarray))
        }),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-table-range",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_range(args, Some(&ctx.obarray))
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-table-parent",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_parent(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-char-table-parent",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_set_char_table_parent(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-table-extra-slot",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_extra_slot(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-char-table-extra-slot",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_set_char_table_extra_slot(args)
        }),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-table-subtype",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_subtype(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::chartable::builtin_bool_vector(args)),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-bool-vector",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_make_bool_vector(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-count-population",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_count_population(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-count-consecutive",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_count_consecutive(args)
        }),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-intersection",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_intersection(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-not",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_not(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-set-difference",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_set_difference(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-union",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_union(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-exclusive-or",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_exclusive_or(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bool-vector-subsetp",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_subsetp(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "make-char-table",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_make_char_table),
        SubrArity::new(1, Some(2)),
    ));

    // -- Charset --
    ctx.register_subr(SubrSpec::new(
        "charset-priority-list",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_charset_priority_list(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-charset-priority",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_set_charset_priority(args)
        }),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "char-charset",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_char_charset(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "charset-id-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_charset_id_internal(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "declare-equiv-charset",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_declare_equiv_charset(args)
        }),
        SubrArity::new(4, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "find-charset-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_find_charset_string(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "decode-big5-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_decode_big5_char(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "decode-char",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_decode_char(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "decode-sjis-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_decode_sjis_char(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "encode-big5-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_encode_big5_char(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "encode-char",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_encode_char(args)),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "encode-sjis-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_encode_sjis_char(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "get-unused-iso-final-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_get_unused_iso_final_char(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "clear-charset-maps",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_clear_charset_maps(args)
        }),
        SubrArity::new(0, Some(0)),
    ));

    // -- Coding system (eval-dependent via coding_systems field) --
    ctx.register_subr(SubrSpec::new(
        "coding-system-p",
        NativeFn::ContextVec(coding_system_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "check-coding-system",
        NativeFn::ContextVec(check_coding_system),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "check-coding-systems-region",
        NativeFn::ContextVec(check_coding_systems_region),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "define-coding-system-internal",
            NativeFn::ContextVec(define_coding_system_internal),
            SubrArity::new(13, None),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "define-coding-system-alias",
        NativeFn::ContextVec(define_coding_system_alias),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-coding-system-priority",
        NativeFn::ContextVec(set_coding_system_priority),
        SubrArity::new(0, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-keyboard-coding-system-internal",
        NativeFn::ContextVec(set_keyboard_coding_system_internal),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-safe-terminal-coding-system-internal",
        NativeFn::ContextVec(set_safe_terminal_coding_system_internal),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-terminal-coding-system-internal",
        NativeFn::ContextVec(set_terminal_coding_system_internal),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "set-text-conversion-style",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::coding::builtin_set_text_conversion_style(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "text-quoting-style",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::coding::builtin_text_quoting_style(ctx, args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    // `set-buffer-file-coding-system' is not here: it is a `defun' at
    // lisp/international/mule.el:1302 that merges coding systems, sets
    // `buffer-file-coding-system-explicit' and marks the buffer modified
    // (DIVERGENCES.md 152).

    // -- CCL (eval-dependent) --
    ctx.register_subr(SubrSpec::new(
        "ccl-program-p",
        NativeFn::ContextVec(builtin_ccl_program_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ccl-execute",
        NativeFn::ContextVec(builtin_ccl_execute),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "ccl-execute-on-string",
        NativeFn::ContextVec(builtin_ccl_execute_on_string),
        SubrArity::new(3, Some(5)),
    ));
    ctx.register_subr(SubrSpec::new(
        "register-ccl-program",
        NativeFn::ContextVec(builtin_register_ccl_program),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "register-code-conversion-map",
        NativeFn::ContextVec(builtin_register_code_conversion_map),
        SubrArity::new(2, Some(2)),
    ));

    // -- Eval builtins (eval-dependent) --
    ctx.register_subr(
        SubrSpec::new(
            "defconst-1",
            NativeFn::ContextVec(builtin_defconst_1),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "defvar-1",
            NativeFn::ContextVec(builtin_defvar_1),
            SubrArity::new(2, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "yes-or-no-p",
        NativeFn::ContextVec(crate::emacs_core::reader::builtin_yes_or_no_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "locate-file-internal",
        NativeFn::ContextVec(crate::emacs_core::lread::builtin_locate_file_internal),
        SubrArity::new(2, Some(4)),
    ));

    // -- Dispnew --
    ctx.register_subr(
        SubrSpec::new(
            "redraw-display",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::dispnew::pure::builtin_redraw_display(args)
            }),
            SubrArity::new(0, Some(0)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        SubrSpec::new(
            "open-termscript",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::dispnew::pure::builtin_open_termscript(args)
            }),
            SubrArity::new(1, Some(1)),
        )
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "FOpen termscript file: ",
            ),
        ),
    );
    ctx.register_subr(SubrSpec::new(
        "ding",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::dispnew::pure::builtin_ding(args)),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "frame--z-order-lessp",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::dispnew::pure::builtin_frame_z_order_lessp(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "force-window-update",
        NativeFn::ContextVec(crate::emacs_core::window_cmds::builtin_force_window_update),
        SubrArity::new(0, Some(1)),
    ));

    // -- Display/terminal --
    ctx.register_subr(SubrSpec::new(
        "x-export-frames",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_export_frames(args)
        }),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-backspace-delete-keys-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_backspace_delete_keys_p(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-change-window-property",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_change_window_property(args)
        }),
        SubrArity::new(2, Some(7)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-focus-frame",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_focus_frame),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-get-local-selection",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_local_selection(args)
        }),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-get-modifier-masks",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_modifier_masks(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-get-selection-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_selection_internal(args)
        }),
        SubrArity::new(2, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-display-list",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_display_list(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-disown-selection-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_disown_selection_internal(args)
        }),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-delete-window-property",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_delete_window_property(args)
        }),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-frame-edges",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_frame_edges(args)),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-frame-geometry",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_frame_geometry(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-frame-list-z-order",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_frame_list_z_order(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-frame-restack",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_frame_restack(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-family-fonts",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_family_fonts(args)),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-get-atom-name",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_atom_name(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_mouse_absolute_pixel_position(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-own-selection-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_own_selection_internal(args)
        }),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-parse-geometry",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_parse_geometry(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-popup-dialog",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_popup_dialog),
        SubrArity::new(2, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-popup-menu",
        NativeFn::ContextVec(crate::emacs_core::display::builtin_x_popup_menu),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-register-dnd-atom",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_register_dnd_atom(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-selection-exists-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_selection_exists_p(args)
        }),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-selection-owner-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_selection_owner_p(args)
        }),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-hide-tip",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_hide_tip(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-internal-focus-input-context",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_internal_focus_input_context(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-send-client-message",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_send_client_message(args)
        }),
        SubrArity::new(6, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-show-tip",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_show_tip(args)),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-set-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_set_mouse_absolute_pixel_position(args)
        }),
        SubrArity::new(2, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-synchronize",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_synchronize(args)),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-translate-coordinates",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_translate_coordinates(args)
        }),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-uses-old-gtk-dialog",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_uses_old_gtk_dialog(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-window-property",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_window_property(args)
        }),
        SubrArity::new(1, Some(6)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-window-property-attributes",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_window_property_attributes(args)
        }),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "x-wm-set-size-hint",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_wm_set_size_hint(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "terminal-list",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::terminal::pure::builtin_terminal_list(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "delete-terminal",
        NativeFn::ContextVec(crate::emacs_core::terminal::pure::builtin_delete_terminal),
        SubrArity::new(0, Some(2)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "make-terminal-frame",
            NativeFn::ContextVec(crate::emacs_core::frame::builtin_make_terminal_frame),
            SubrArity::new(1, Some(1)),
        )
        .requires_eval_state(),
    );

    // -- Image --
    ctx.register_subr(
        SubrSpec::new(
            "image-size",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_image_size_in_context),
            SubrArity::new(1, Some(3)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "image-mask-p",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_image_mask_p_in_context),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "image-flush",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_image_flush_in_context),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "clear-image-cache",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_clear_image_cache_in_context),
            SubrArity::new(0, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "image-cache-size",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_image_cache_size_in_context),
            SubrArity::new(0, Some(0)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "image-metadata",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_image_metadata_in_context),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(
        SubrSpec::new(
            "neomacs-image-extent",
            NativeFn::ContextVec(crate::emacs_core::image::builtin_neomacs_image_extent_in_context),
            SubrArity::new(1, Some(2)),
        )
        .requires_eval_state(),
    );
    ctx.register_subr(SubrSpec::new(
        "imagep",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::image::builtin_imagep(args)),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "image-transforms-p",
        NativeFn::ContextVec(crate::emacs_core::image::builtin_image_transforms_p),
        SubrArity::new(0, Some(1)),
    ));

    // -- Display engine (xdisp) --
    ctx.register_subr(SubrSpec::new(
        "invisible-p",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_invisible_p),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "line-pixel-height",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_line_pixel_height(args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "move-point-visually",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_move_point_visually(args)
        }),
        SubrArity::new(1, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "lookup-image-map",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::xdisp::builtin_lookup_image_map(args)),
        SubrArity::new(3, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "current-bidi-paragraph-direction",
        NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_current_bidi_paragraph_direction),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bidi-resolved-levels",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_bidi_resolved_levels(args)
        }),
        SubrArity::new(0, Some(1)),
    ));
    ctx.register_subr(SubrSpec::new(
        "bidi-find-overridden-directionality",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_bidi_find_overridden_directionality(args)
        }),
        SubrArity::new(3, Some(4)),
    ));
    ctx.register_subr(
        SubrSpec::new(
            "move-to-window-line",
            NativeFn::ContextVec(crate::emacs_core::xdisp::builtin_move_to_window_line),
            SubrArity::new(1, Some(1)),
        )
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("P")),
    );
    ctx.register_subr(SubrSpec::new(
        "long-line-optimizations-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_long_line_optimizations_p(args)
        }),
        SubrArity::new(0, Some(0)),
    ));

    // -- XML/decompress --
    ctx.register_subr(SubrSpec::new(
        "libxml-parse-html-region",
        NativeFn::ContextVec(crate::emacs_core::xml::builtin_libxml_parse_html_region),
        SubrArity::new(0, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "libxml-parse-xml-region",
        NativeFn::ContextVec(crate::emacs_core::xml::builtin_libxml_parse_xml_region),
        SubrArity::new(0, Some(4)),
    ));
    ctx.register_subr(SubrSpec::new(
        "libxml-available-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::xml::builtin_libxml_available_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "zlib-available-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::zlib::builtin_zlib_available_p(args)),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "zlib-decompress-region",
        NativeFn::ContextVec(crate::emacs_core::zlib::builtin_zlib_decompress_region),
        SubrArity::new(2, Some(3)),
    ));

    // -- Native compilation compatibility --

    // -- DBus --
    //
    // None.  GNU's six `dbusbind.c' subrs are inside `#ifdef HAVE_DBUS'
    // (src/dbusbind.c:21, syms_of_dbusbind at :2003-2010) and this build links
    // no libdbus.  Ledger 192 deleted the three that stood here: they held no
    // D-Bus code, and answered a hardcoded `2', a fabricated `":1.0"' unique
    // name and an invented `dbus-event' reply from "org.freedesktop.DBus".

    // -- Documentation/help --
    ctx.register_subr(SubrSpec::new(
        "Snarf-documentation",
        NativeFn::ContextVec(crate::emacs_core::doc::builtin_snarf_documentation),
        SubrArity::new(1, Some(1)),
    ));

    // -- JSON --
    ctx.register_subr(SubrSpec::new(
        "json-serialize",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::json::builtin_json_serialize(args)),
        SubrArity::new(1, None),
    ));
    ctx.register_subr(SubrSpec::new(
        "json-parse-string",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::json::builtin_json_parse_string(args)),
        SubrArity::new(1, None),
    ));

    // -- Marker --
    ctx.register_subr(SubrSpec::new(
        "markerp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::marker::builtin_markerp(args)),
        SubrArity::new(1, Some(1)),
    ));

    // -- Lread --
    ctx.register_subr(SubrSpec::new(
        "get-load-suffixes",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::lread::builtin_get_load_suffixes(&ctx.obarray, args)
        }),
        SubrArity::new(0, Some(0)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-coding-system",
        NativeFn::ContextVec(crate::emacs_core::lread::builtin_read_coding_system),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "read-non-nil-coding-system",
        NativeFn::ContextVec(crate::emacs_core::lread::builtin_read_non_nil_coding_system),
        SubrArity::new(1, Some(1)),
    ));

    // -- Base64/hash --
    ctx.register_subr(SubrSpec::new(
        "base64-encode-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_base64_encode_string(args)
        }),
        SubrArity::new(1, Some(2)),
    ));
    ctx.register_subr(SubrSpec::new(
        "base64-decode-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_base64_decode_string(args)
        }),
        SubrArity::new(1, Some(3)),
    ));
    ctx.register_subr(SubrSpec::new(
        "base64url-encode-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_base64url_encode_string(args)
        }),
        SubrArity::new(1, Some(2)),
    ));

    // -- Window builtins: `switch-to-buffer' (lisp/window.el:9558),
    // `display-buffer' (:8166) and `pop-to-buffer' (:9403) are Lisp and only
    // Lisp (DIVERGENCES.md 154).  The C primitives underneath them --
    // `set-window-buffer', `select-window', `set-buffer' -- stay registered.

    // -- Window tree / resize: `balance-windows' (lisp/window.el:6222),
    // `enlarge-window' (:3714), `shrink-window' (:3759) and `window-tree'
    // (:3999) are Lisp and only Lisp (DIVERGENCES.md 154).  They are written
    // over `window-resize-apply', `window-resize-apply-total' and
    // `frame-root-window', which are C DEFUNs and stay registered.

    evaluator_compatibility
        .initialize_event_properties(ctx)
        .register_public_eval(ctx)
        .finish();
}
