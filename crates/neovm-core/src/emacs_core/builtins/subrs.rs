//! Typed declarations for the legacy built-in startup surface.
//!
//! This is a compatibility manifest, not an implementation module. As a GNU
//! subsystem gains its own `register_subrs`, move its declarations beside that
//! implementation and leave only the sequencing call here.

use super::*;
use crate::emacs_core::buffer_ops::{
    builtin_buffer_list, builtin_other_buffer, builtin_replace_region_contents,
    builtin_set_buffer_multibyte,
};
use crate::emacs_core::display_entries::{
    builtin_old_selected_frame, builtin_query_fontset, builtin_reconsider_frame_fonts,
    builtin_tool_bar_pixel_width, builtin_xw_display_color_p_ctx,
};
use crate::emacs_core::frame::{builtin_next_frame, builtin_previous_frame};
use crate::emacs_core::gnutls_entries as gnutls;
use crate::emacs_core::kill_buffer::builtin_kill_buffer;
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
    crate::emacs_core::w32::subrs::SUBRS,
    #[cfg(neomacs_have_lcms2)]
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
    #[cfg(neomacs_have_dbus)]
    crate::emacs_core::dbusbind::SUBRS,
    crate::emacs_core::sqlite::SUBRS,
    crate::emacs_core::font::SUBRS,
    crate::emacs_core::neo::effects::SUBRS,
    crate::emacs_core::neo::modifiers::SUBRS,
    crate::emacs_core::neo::chrome::SUBRS,
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
        ctx.register_subr(crate::emacs_core::eval::FBOUNDP_SUBR);
        EvaluatorCompatibility(std::marker::PhantomData)
    }
}

impl EvaluatorCompatibility<NeedsEventProperties> {
    fn initialize_event_properties(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> EvaluatorCompatibility<NeedsPublicEval> {
        // SAFETY: compatibility setup only initializes event symbol properties;
        // it does not call Lisp or mutate canonical function/debugger metadata.
        unsafe {
            ctx.obarray_view_mut()
                .with_legacy_value_write(symbols::init_event_symbol_properties);
        }
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
/// Cons NAME onto `charset-list', as GNU's charset writers do.
///
/// GNU keeps every charset and alias ever defined in `Vcharset_list', newest
/// first; `describe-character-set', `list-charset-chars' and anything else
/// enumerating charsets reads it.
fn push_onto_charset_list(
    ctx: &mut crate::emacs_core::eval::Context,
    name: crate::emacs_core::value::Value,
) {
    use crate::emacs_core::value::Value;
    let current = ctx
        .builtin_var_value_by_name("charset-list")
        .unwrap_or(Value::NIL);
    // Unconditionally, as GNU does: defining the same alias twice leaves two
    // entries in its list, and nothing in a normal session defines a charset
    // twice.
    ctx.store_variable_by_name("charset-list", Value::cons(name, current));
}

pub(crate) fn register_subrs(ctx: &mut crate::emacs_core::eval::Context) {
    #[cfg(windows)]
    crate::emacs_core::w32::subrs::register_subrs(ctx);
    lcms::register_subrs(ctx);
    let evaluator_compatibility = EvaluatorCompatibility::begin()
        .register_data(ctx)
        .register_ordinary_eval(ctx)
        .register_fboundp(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "getenv-internal",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_getenv_internal),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-window-configuration-change-hook",
        NativeFn::ContextVec(
            crate::emacs_core::window_hooks::builtin_run_window_configuration_change_hook
        ),
        SubrArity::new(0, Some(1)),
    )));
    // GNU emacs.c sequences composite.c before window.c/xdisp.c.
    crate::emacs_core::composite::register_subrs(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "run-window-scroll-functions",
        NativeFn::ContextVec(crate::emacs_core::window_hooks::builtin_run_window_scroll_functions),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-buffer-create",
        NativeFn::ContextVec(|ctx, args| builtin_get_buffer_create(ctx, args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-buffer",
        NativeFn::ContextVec(|ctx, args| builtin_get_buffer(&mut ctx.edit.buffers, args)),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_terminal_shader_video_xwidget(ctx);
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-indirect-buffer",
            NativeFn::ContextVec(|ctx, args| builtin_make_indirect_buffer(ctx, args)),
            SubrArity::new(2, Some(4)),
        ))
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "bMake indirect buffer (to buffer): \nBName of indirect buffer: ",
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "find-buffer",
        NativeFn::ContextVec(|ctx, args| builtin_find_buffer(ctx, args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-live-p",
        NativeFn::ContextSlice(|ctx, args| builtin_buffer_live_p(&mut ctx.edit.buffers, args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-file-buffer",
        NativeFn::ContextVec(|ctx, args| builtin_get_file_buffer(ctx, args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "kill-buffer",
            NativeFn::ContextVec(builtin_kill_buffer),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("bKill buffer: "),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "set-buffer",
        |ctx, arg| builtin_set_buffer_1(ctx, arg),
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "current-buffer",
        |ctx| builtin_current_buffer_0(&mut ctx.edit.buffers)
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-name",
        NativeFn::ContextSlice(|ctx, args| builtin_buffer_name(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-file-name",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_file_name(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-base-buffer",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_base_buffer(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-last-name",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_last_name(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-string",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_string(ctx, args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-line-statistics",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_line_statistics(
            &mut ctx.edit.buffers,
            args
        )),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_buffer_text_pixel_size(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "buffer-substring",
        |ctx, start, end| builtin_buffer_substring_2(ctx, start, end),
        FixedMin2::Two,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "point",
        |ctx| builtin_point_0(&mut ctx.edit.buffers)
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "point-min",
        |ctx| builtin_point_min_0(&mut ctx.edit.buffers)
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "point-max",
        |ctx| builtin_point_max_0(&mut ctx.edit.buffers)
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
            "goto-char",
            |ctx, arg| builtin_goto_char_1(ctx, arg),
            FixedMin1::One
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                goto_char_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "field-beginning",
        NativeFn::ContextVec(|ctx, args| builtin_field_beginning(ctx, args)),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "field-end",
        NativeFn::ContextVec(|ctx, args| builtin_field_end(ctx, args)),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "field-string",
        NativeFn::ContextVec(|ctx, args| builtin_field_string(ctx, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "field-string-no-properties",
        NativeFn::ContextVec(|ctx, args| builtin_field_string_no_properties(ctx, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "constrain-to-field",
        NativeFn::ContextVec(|ctx, args| builtin_constrain_to_field(ctx, args)),
        SubrArity::new(2, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "insert",
        NativeFn::ContextVec(|ctx, args| builtin_insert(ctx, args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "insert-and-inherit",
            NativeFn::ContextVec(|ctx, args| builtin_insert_and_inherit(ctx, args)),
            SubrArity::new(0, None),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "insert-before-markers-and-inherit",
            NativeFn::ContextVec(|ctx, args| builtin_insert_before_markers_and_inherit(ctx, args)),
            SubrArity::new(0, None),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "insert-buffer-substring",
            NativeFn::ContextVec(|ctx, args| builtin_insert_buffer_substring(ctx, args)),
            SubrArity::new(1, Some(3)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "insert-char",
            NativeFn::ContextVec(|ctx, args| builtin_insert_char(ctx, args)),
            SubrArity::new(1, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                insert_char_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "insert-byte",
        NativeFn::ContextVec(|ctx, args| builtin_insert_byte(ctx, args)),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "replace-region-contents",
        NativeFn::ContextVec(builtin_replace_region_contents),
        SubrArity::new(3, Some(6)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-buffer-multibyte",
            NativeFn::ContextVec(builtin_set_buffer_multibyte),
            SubrArity::new(1, Some(1)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "kill-all-local-variables",
            NativeFn::ContextVec(|ctx, args| builtin_kill_all_local_variables(ctx, args)),
            SubrArity::new(0, Some(1)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-swap-text",
        NativeFn::ContextVec(builtin_buffer_swap_text),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
            "delete-region",
            crate::emacs_core::editfns::builtin_delete_region_2,
            FixedMin2::Two,
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-and-extract-region",
        NativeFn::ContextVec(crate::emacs_core::editfns::builtin_delete_and_extract_region),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "subst-char-in-region",
        NativeFn::ContextVec(builtin_subst_char_in_region),
        SubrArity::new(4, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-field",
        NativeFn::ContextVec(|ctx, args| builtin_delete_field(ctx, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-all-overlays",
        NativeFn::ContextVec(|ctx, args| builtin_delete_all_overlays(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "erase-buffer",
            NativeFn::ContextVec(crate::emacs_core::editfns::builtin_erase_buffer),
            SubrArity::new(0, Some(0)),
        ))
        .disabled_command()
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("*")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "buffer-enable-undo",
            NativeFn::ContextVec(|ctx, args| builtin_buffer_enable_undo(
                &mut ctx.edit.buffers,
                args
            )),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-size",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_size(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
            "narrow-to-region",
            |ctx, start, end| builtin_narrow_to_region_2(ctx, start, end),
            FixedMin2::Two,
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed0("widen", |ctx| builtin_widen_0(ctx)))
            .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "buffer-modified-p",
        |ctx, arg| builtin_buffer_modified_p_1(&mut ctx.edit.buffers, arg),
        FixedMin1::Zero
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-buffer-modified-p",
        NativeFn::ContextVec(|ctx, args| builtin_set_buffer_modified_p(ctx, args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-modified-tick",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_modified_tick(&mut ctx.edit.buffers, args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-chars-modified-tick",
        NativeFn::ContextVec(|ctx, args| builtin_buffer_chars_modified_tick(
            &mut ctx.edit.buffers,
            args
        )),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-list",
        NativeFn::ContextVec(builtin_buffer_list),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "other-buffer",
        NativeFn::ContextVec(builtin_other_buffer),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "generate-new-buffer-name",
        NativeFn::ContextVec(|ctx, args| builtin_generate_new_buffer_name(ctx, args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "char-after",
        |ctx, arg| builtin_char_after_1(&mut ctx.edit.buffers, arg),
        FixedMin1::Zero
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "char-before",
        |ctx, arg| builtin_char_before_1(&mut ctx.edit.buffers, arg),
        FixedMin1::Zero
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "byte-to-position",
        |ctx, arg| builtin_byte_to_position_1(&mut ctx.edit.buffers, arg),
        FixedMin1::One
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "position-bytes",
        |ctx, arg| builtin_position_bytes_1(&mut ctx.edit.buffers, arg),
        FixedMin1::One
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-byte",
        NativeFn::ContextVec(|ctx, args| builtin_get_byte(ctx, args)),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "command-modes",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_command_modes),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "find-charset-region",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_find_charset_region),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "charset-after",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_charset_after),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_format_mode_line(ctx);
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "new-fontset",
            NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_new_fontset),
            SubrArity::new(2, Some(2)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-fontset-font",
            NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_set_fontset_font),
            SubrArity::new(3, Some(5)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "insert-file-contents",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_insert_file_contents),
        SubrArity::new(1, Some(5)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "write-region",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_write_region),
            SubrArity::new(3, Some(7)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "r\nFWrite region to file: \ni\ni\ni\np",
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-completion",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_name_completion),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-visited-file-modtime",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_visited_file_modtime),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "print--preprocess",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_print_preprocess),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "format-network-address",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_format_network_address),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "network-interface-list",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_network_interface_list),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "network-interface-info",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_network_interface_info),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "signal-names",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_signal_names),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "accept-process-output",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_accept_process_output),
        SubrArity::new(0, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "list-system-processes",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_list_system_processes),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "num-processors",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_num_processors),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_process),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-network-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_network_process),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-open-tls-stream",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_neomacs_open_tls_stream),
        SubrArity::new(4, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-tls-available-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::tls::builtin_neomacs_tls_available_p(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-pipe-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_pipe_process),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-boot",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_boot),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-serial-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_make_serial_process),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "serial-process-configure",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_serial_process_configure),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "call-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_call_process),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "call-process-region",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_call_process_region),
        SubrArity::new(3, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "continue-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_continue_process),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "delete-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_delete_process),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                delete_process_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "interrupt-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_interrupt_process),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "kill-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_kill_process),
            SubrArity::new(0, Some(2)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                kill_process_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "quit-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_quit_process),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "signal-process",
            NativeFn::ContextVec(crate::emacs_core::process::builtin_signal_process),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                signal_process_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "stop-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_stop_process),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_get_process),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-buffer-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_get_buffer_process),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-attributes",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_attributes),
        SubrArity::new(1, Some(1)),
    )));
    // No `start-process' / `start-file-process' /
    // `start-process-shell-command' / `start-file-process-shell-command':
    // GNU has no C DEFUN for any of them.  All four are Lisp over
    // `make-process' -- lisp/subr.el:3466, lisp/simple.el:5249,
    // lisp/subr.el:5063 and lisp/subr.el:5076 -- and `loadup.el' preloads
    // both files, so a Rust subr here could only ever answer in unit tests.
    // DIVERGENCES.md 149.

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "processp",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_processp),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-id",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_id),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-command",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_command),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-contact",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_contact),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-filter",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_filter),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-filter",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_filter),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-sentinel",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_sentinel),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-sentinel",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_sentinel),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-coding-system",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_coding_system),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-datagram-address",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_datagram_address),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-buffer",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_buffer),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-thread",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_thread),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-window-size",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_window_size),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-tty-name",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_tty_name),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-plist",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_plist),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-plist",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_plist),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-mark",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_mark),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-type",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_type),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-thread",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_thread),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-running-child-p",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_running_child_p),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-send-region",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_region),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-send-eof",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_eof),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-send-string",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_send_string),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-status",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_status),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-exit-status",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_exit_status),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-list",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_list),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-name",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_name),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-buffer",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_buffer),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sleep-for",
        NativeFn::ContextVec(crate::emacs_core::timer::builtin_sleep_for),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "modify-syntax-entry",
            NativeFn::ContextVec(crate::emacs_core::syntax::builtin_modify_syntax_entry),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "cSet syntax for character: \nsSet syntax for %s to: ",
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "syntax-table",
        crate::emacs_core::syntax::builtin_syntax_table_0,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "set-syntax-table",
        crate::emacs_core::syntax::builtin_set_syntax_table_1,
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "char-syntax",
        crate::emacs_core::syntax::builtin_char_syntax_1,
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "matching-paren",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_matching_paren),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "forward-comment",
        NativeFn::ContextVec(crate::emacs_core::syntax_entries::builtin_forward_comment),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "backward-prefix-chars",
        NativeFn::ContextSlice(crate::emacs_core::syntax_entries::builtin_backward_prefix_chars),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "forward-word",
            NativeFn::ContextVec(crate::emacs_core::syntax_entries::builtin_forward_word),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "scan-lists",
        crate::emacs_core::syntax::builtin_scan_lists_3,
        FixedMin3::Three,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "scan-sexps",
        crate::emacs_core::syntax::builtin_scan_sexps_2,
        FixedMin2::Two,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed6(
        "parse-partial-sexp",
        crate::emacs_core::syntax::builtin_parse_partial_sexp_6,
        FixedMin6::Two,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "skip-syntax-forward",
        crate::emacs_core::syntax::builtin_skip_syntax_forward_2,
        FixedMin2::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "skip-syntax-backward",
        crate::emacs_core::syntax::builtin_skip_syntax_backward_2,
        FixedMin2::One,
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "start-kbd-macro",
            NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_start_kbd_macro),
            SubrArity::new(1, Some(2)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("P")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "end-kbd-macro",
            NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_end_kbd_macro),
            SubrArity::new(0, Some(2)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "call-last-kbd-macro",
            NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_call_last_kbd_macro),
            SubrArity::new(0, Some(2)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "execute-kbd-macro",
        NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_execute_kbd_macro),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "store-kbd-macro-event",
        NativeFn::ContextVec(crate::emacs_core::kmacro::builtin_store_kbd_macro_event),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed5(
        "put-text-property",
        crate::emacs_core::textprop::builtin_put_text_property_5,
        FixedMin5::Four,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "get-text-property",
        crate::emacs_core::textprop::builtin_get_text_property_3,
        FixedMin3::Two,
    )));
    ctx.register_subr(
        const {
            crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
                "get-char-property",
                crate::emacs_core::textprop_entries::builtin_get_char_property_3,
                FixedMin3::Two,
            ))
            .leaf(&leaves::GET_CHAR_PROPERTY)
        },
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "add-face-text-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_add_face_text_property),
        SubrArity::new(3, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
        "add-text-properties",
        crate::emacs_core::textprop::builtin_add_text_properties_4,
        FixedMin4::Three,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-text-properties",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_set_text_properties),
        SubrArity::new(3, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "remove-text-properties",
        NativeFn::ContextSlice(crate::emacs_core::textprop::builtin_remove_text_properties),
        SubrArity::new(3, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "text-properties-at",
        crate::emacs_core::textprop::builtin_text_properties_at_2,
        FixedMin2::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-display-property",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_get_display_property),
        SubrArity::new(2, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "next-property-change",
        NativeFn::ContextVec(crate::emacs_core::textprop::builtin_next_property_change),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "text-property-any",
        NativeFn::ContextSlice(crate::emacs_core::textprop::builtin_text_property_any),
        SubrArity::new(4, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed5(
        "text-property-not-all",
        crate::emacs_core::textprop::builtin_text_property_not_all_5,
        FixedMin5::Four,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "next-overlay-change",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_next_overlay_change(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "previous-overlay-change",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::buffer::builtin_previous_overlay_change(&mut ctx.edit.buffers, args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-overlay",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_make_overlay(
            &mut ctx.edit.buffers,
            args
        )),
        SubrArity::new(2, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-overlay",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_delete_overlay(ctx, args)
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-put",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_overlay_put(ctx, args)),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-get",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_overlay_get(ctx, args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlays-at",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_overlays_at(ctx, args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlays-in",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_overlays_in(
            &mut ctx.edit.buffers,
            args
        )),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "move-overlay",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_move_overlay(
            ctx, args
        )),
        SubrArity::new(3, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-start",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_overlay_start(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-end",
        NativeFn::ContextVec(|ctx, args| crate::emacs_core::buffer::builtin_overlay_end(
            &mut ctx.edit.buffers,
            args
        )),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-buffer",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_overlay_buffer(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-properties",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_overlay_properties(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlayp",
        NativeFn::ContextSlice(|ctx, args| crate::emacs_core::buffer::builtin_overlayp(ctx, args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "bobp",
        |ctx: &mut crate::emacs_core::eval::Context| {
            crate::emacs_core::navigation::builtin_bobp_0(&ctx.edit.buffers)
        },
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "eobp",
        |ctx: &mut crate::emacs_core::eval::Context| {
            crate::emacs_core::navigation::builtin_eobp_0(&ctx.edit.buffers)
        },
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "bolp",
        |ctx: &mut crate::emacs_core::eval::Context| {
            crate::emacs_core::navigation::builtin_bolp_0(&ctx.edit.buffers)
        },
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "eolp",
        |ctx: &mut crate::emacs_core::eval::Context| {
            crate::emacs_core::navigation::builtin_eolp_0(&ctx.edit.buffers)
        },
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "line-end-position",
        crate::emacs_core::navigation::builtin_line_end_position_1,
        FixedMin1::Zero,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "line-number-at-pos",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::navigation::builtin_line_number_at_pos(&ctx.edit.buffers, args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
            "forward-line",
            crate::emacs_core::navigation::builtin_forward_line_1,
            FixedMin1::Zero,
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
            "beginning-of-line",
            crate::emacs_core::navigation::builtin_beginning_of_line_1,
            FixedMin1::Zero,
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
            "end-of-line",
            crate::emacs_core::navigation::builtin_end_of_line_1,
            FixedMin1::Zero,
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
            "forward-char",
            crate::emacs_core::navigation::builtin_forward_char_1,
            FixedMin1::Zero,
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "backward-char",
            NativeFn::ContextVec(crate::emacs_core::navigation::builtin_backward_char),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("^p")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "skip-chars-forward",
        |ctx: &mut crate::emacs_core::eval::Context, string, lim| {
            crate::emacs_core::navigation::builtin_skip_chars_forward_2(
                &mut ctx.edit.buffers,
                string,
                lim,
            )
        },
        FixedMin2::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed2(
        "skip-chars-backward",
        |ctx: &mut crate::emacs_core::eval::Context, string, lim| {
            crate::emacs_core::navigation::builtin_skip_chars_backward_2(
                &mut ctx.edit.buffers,
                string,
                lim,
            )
        },
        FixedMin2::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mark-marker",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_mark_marker(&mut ctx.edit.buffers, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "region-beginning",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::navigation::builtin_region_beginning(&ctx.edit.buffers, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "region-end",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::navigation::builtin_region_end(&ctx.edit.buffers, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    // `symbol-file' is not here: it is a `defun' at lisp/subr.el:3351 that
    // walks `load-history' (DIVERGENCES.md 152).
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "downcase-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_downcase_region),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "upcase-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_upcase_region),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "capitalize-region",
            NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_capitalize_region),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "downcase-word",
            NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_downcase_word),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "upcase-word",
            NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_upcase_word),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "capitalize-word",
            NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_capitalize_word),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p")),
    );
    crate::emacs_core::indent::register_subrs(ctx);
    crate::emacs_core::display_subrs::register_selected_window(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-window-configuration",
        NativeFn::ContextVec(crate::emacs_core::window_hooks::builtin_current_window_configuration),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-window-configuration",
        NativeFn::ContextVec(crate::emacs_core::window_hooks::builtin_set_window_configuration),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "old-selected-frame",
            NativeFn::ContextVec(builtin_old_selected_frame),
            SubrArity::new(0, Some(0)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    );
    crate::emacs_core::display_subrs::register_selected_frame(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mouse-pixel-position",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_mouse_pixel_position),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mouse-position",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_mouse_position),
        SubrArity::new(0, Some(0)),
    )));
    crate::emacs_core::display_subrs::register_next_frame(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-id",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_id),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-root-frame",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_root_frame),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_x_open_connection(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "call-interactively",
        NativeFn::ContextSlice(crate::emacs_core::interactive::builtin_call_interactively),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "commandp",
        NativeFn::ContextSlice(crate::emacs_core::interactive::builtin_commandp_interactive),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "command-remapping",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_command_remapping),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "self-insert-command",
            NativeFn::ContextVec(crate::emacs_core::interactive::builtin_self_insert_command),
            SubrArity::new(1, Some(2)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                self_insert_command_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "key-binding",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_key_binding),
        SubrArity::new(1, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "where-is-internal",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_where_is_internal),
        SubrArity::new(1, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "this-command-keys",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_this_command_keys),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-from-minibuffer",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_from_minibuffer),
        SubrArity::new(1, Some(7)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-string",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_string),
        SubrArity::new(1, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "completing-read",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_completing_read),
        SubrArity::new(2, Some(8)),
    )));
    // `read-number' is not here: it is a `defun' at lisp/subr.el:3725 over
    // `read-from-minibuffer', and GNU's "n" interactive code letter reaches
    // it through the function cell (src/callint.c:645) (DIVERGENCES.md 152).
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-buffer",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_buffer),
        SubrArity::new(1, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-command",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_command),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-variable",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_variable),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "try-completion",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_try_completion),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "all-completions",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_all_completions),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "test-completion",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_test_completion),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "completion--flex-cost-gotoh",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_flex_cost_gotoh),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "input-pending-p",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_input_pending_p),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "discard-input",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_discard_input),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-input-mode",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_current_input_mode),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-input-mode",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_set_input_mode),
        SubrArity::new(3, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-input-interrupt-mode",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_set_input_interrupt_mode),
        SubrArity::new(1, Some(1)),
    )));
    // Keyboard audit Finding 16: register insert-special-event
    // (mirrors GNU `Finsert_special_event` at
    // `src/keyboard.c:12060`). Routes to the same unread queue
    // helper as `unread-command-events`, since neomacs treats
    // every Lisp-side event push the same way.
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "insert-special-event",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_insert_special_event),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-key-sequence",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_read_key_sequence),
        SubrArity::new(1, Some(6)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-key-sequence-vector",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_read_key_sequence_vector),
        SubrArity::new(1, Some(6)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "minibufferp",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibufferp_ctx),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "minibuffer-contents",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_contents_ctx),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "minibuffer-contents-no-properties",
        NativeFn::ContextVec(
            crate::emacs_core::minibuffer::builtin_minibuffer_contents_no_properties_ctx,
        ),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "minibuffer-depth",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_depth_ctx),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "exit-recursive-edit",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_exit_recursive_edit),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "abort-recursive-edit",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_abort_recursive_edit),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "undo-boundary",
            NativeFn::ContextSlice(crate::emacs_core::undo::builtin_undo_boundary),
            SubrArity::new(0, Some(0)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed3(
        "set-marker",
        |ctx: &mut crate::emacs_core::eval::Context, marker, position, buffer| {
            crate::emacs_core::marker::builtin_set_marker_in_buffers(
                &mut ctx.edit.buffers,
                &[marker, position, buffer],
            )
        },
        FixedMin3::Two,
    )));
    // No `move-marker' here: GNU has no DEFUN of that name.  It is
    // `(defalias 'move-marker #'set-marker)' at lisp/subr.el:2280, so the
    // function cell holds the SYMBOL `set-marker' (DIVERGENCES.md 148).
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "marker-position",
        |ctx: &mut crate::emacs_core::eval::Context, marker| {
            crate::emacs_core::marker::builtin_marker_position_1(&ctx.edit.buffers, marker)
        },
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "marker-buffer",
        NativeFn::ContextSlice(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_marker_buffer_in_buffers(&ctx.edit.buffers, args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copy-marker",
        NativeFn::ContextSlice(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_copy_marker_in_buffers(&mut ctx.edit.buffers, args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "point-marker",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_point_marker_in_buffers(&mut ctx.edit.buffers, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "point-min-marker",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_point_min_marker_in_buffers(
                &mut ctx.edit.buffers,
                args,
            )
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "point-max-marker",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_point_max_marker_in_buffers(
                &mut ctx.edit.buffers,
                args,
            )
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab_entries::builtin_current_case_table),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "standard-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab_entries::builtin_standard_case_table),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab_entries::builtin_set_case_table),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-category",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_define_category),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "category-docstring",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_category_docstring),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "modify-category-entry",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_modify_category_entry),
        SubrArity::new(2, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-category-set",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_char_category_set),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_category_table),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_set_category_table),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "map-char-table",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_map_char_table),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "json-parse-buffer",
        NativeFn::ContextVec(crate::emacs_core::json::builtin_json_parse_buffer),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "json-insert",
        NativeFn::ContextVec(crate::emacs_core::json::builtin_json_insert),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-char-exclusive",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_read_char_exclusive),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "insert-before-markers",
        NativeFn::ContextVec(|ctx, args| builtin_insert_before_markers(ctx, args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "delete-char",
            NativeFn::ContextSlice(crate::emacs_core::editfns::builtin_delete_char),
            SubrArity::new(1, Some(2)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("p\nP")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "following-char",
        crate::emacs_core::editfns::builtin_following_char_0,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "preceding-char",
        NativeFn::ContextVec(|eval, args| {
            crate::emacs_core::editfns::builtin_preceding_char(eval, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    crate::emacs_core::display_subrs::register_face_font(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "access-file",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_access_file),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "expand-file-name",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_expand_file_name),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-file-internal",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_delete_file_internal),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "rename-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_rename_file),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "fRename file: \nGRename %s to file: \np",
            ),
        ),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "copy-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_copy_file),
            SubrArity::new(2, Some(6)),
        ))
        .requires_eval_state()
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "fCopy file: \nGCopy %s to file: \np\nP",
            ),
        ),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "add-name-to-file",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_add_name_to_file),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "fAdd name to file: \nGName to add to %s: \np",
            ),
        ),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "make-symbolic-link",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_make_symbolic_link),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "FMake symbolic link to file: \nGMake symbolic link to file %s: \np",
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "directory-files",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_directory_files),
        SubrArity::new(1, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-attributes",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_attributes),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-exists-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_exists_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-readable-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_readable_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-writable-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_writable_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-acl",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_acl),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-executable-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_executable_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-locked-p",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_file_locked_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-selinux-context",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_selinux_context),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-system-info",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_system_info),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-directory-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_directory_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-regular-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_regular_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-symlink-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_symlink_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-modes",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_modes),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "set-file-modes",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_modes),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                set_file_modes_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-file-times",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_times),
        SubrArity::new(1, Some(3)),
    )));
    // No `macrop' here: GNU has no DEFUN of that name.  It is a `defun' at
    // lisp/subr.el:4793 over `indirect-function', which IS in C
    // (src/data.c:2557) -- DIVERGENCES.md 148.
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-inherit-coding-system-flag",
        NativeFn::ContextVec(
            crate::emacs_core::process::builtin_set_process_inherit_coding_system_flag,
        ),
        SubrArity::new(2, Some(2)),
    )));
    crate::emacs_core::display_subrs::register_frame_parameter(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-char",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_read_char),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "minibuffer-innermost-command-loop-p",
            NativeFn::ContextVec(
                crate::emacs_core::minibuffer::builtin_minibuffer_innermost_command_loop_p_ctx,
            ),
            SubrArity::new(0, Some(1)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "recursive-edit",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_recursive_edit),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "find-coding-systems-region-internal",
            NativeFn::ContextVec(
                crate::emacs_core::coding::builtin_find_coding_systems_region_internal,
            ),
            SubrArity::new(2, Some(3)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-event",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_read_event),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "integer-or-marker-p",
        NativeFn::ContextVec(|_ctx, args| builtin_integer_or_marker_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "number-or-marker-p",
        NativeFn::ContextVec(|_ctx, args| builtin_number_or_marker_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "vector-or-char-table-p",
        NativeFn::ContextVec(|_ctx, args| builtin_vector_or_char_table_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "marker-insertion-type",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::marker::builtin_marker_insertion_type(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-marker",
        NativeFn::ContextSlice(|_ctx, args| crate::emacs_core::marker::builtin_make_marker(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_p(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-category-set",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_make_category_set(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "function-equal",
        NativeFn::ContextVec(|_ctx, args| builtin_function_equal(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "module-function-p",
        NativeFn::ContextVec(|_ctx, args| builtin_module_function_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "user-ptrp",
        NativeFn::ContextVec(|_ctx, args| builtin_user_ptrp(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "length<",
        NativeFn::ContextVec(|_ctx, args| builtin_length_lt(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "length=",
        NativeFn::ContextVec(|_ctx, args| builtin_length_eq(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "length>",
        NativeFn::ContextVec(|_ctx, args| builtin_length_gt(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "substring-no-properties",
        NativeFn::ContextVec(|_ctx, args| builtin_substring_no_properties(args)),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "atan",
        NativeFn::ContextVec(|_ctx, args| builtin_atan(args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "log",
        NativeFn::ContextVec(|_ctx, args| builtin_log(args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "expt",
        NativeFn::ContextVec(|_ctx, args| builtin_expt(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "random",
        NativeFn::ContextVec(|_ctx, args| builtin_random(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "isnan",
        NativeFn::ContextVec(|_ctx, args| builtin_isnan(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string",
        NativeFn::ContextSlice(|_ctx, args| builtin_string_slice(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "identity",
        NativeFn::ContextVec(|_ctx, args| builtin_identity(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ngettext",
        NativeFn::ContextVec(|_ctx, args| builtin_ngettext(args)),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "secure-hash-algorithms",
        NativeFn::ContextVec(|_ctx, args| builtin_secure_hash_algorithms(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "prefix-numeric-value",
        NativeFn::ContextVec(|_ctx, args| builtin_prefix_numeric_value(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "propertize",
        NativeFn::ContextVec(|_ctx, args| builtin_propertize(args)),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "capitalize",
        NativeFn::ContextVec(crate::emacs_core::casefiddle_entries::builtin_capitalize_in_state),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "charsetp",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_charsetp_in_state),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "charset-plist",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_charset_plist_in_state),
        SubrArity::new(1, Some(1)),
    )));
    // A coding system's memoized `:ascii-compatible-p`/`:category` are
    // computed from its charsets (`compute_coding_ascii_compat`,
    // `compute_coding_category`), so a charset defined, redefined or
    // aliased after the memo filled would leave it stale: the two charset
    // writers drop the coding-system memo too.
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-charset-internal",
        NativeFn::ContextVec(|ctx, args| {
            ctx.coding_systems().invalidate_lookup_cache();
            let name = args.first().copied();
            let defined =
                crate::emacs_core::charset::builtin_define_charset_internal_in_state(ctx, args)?;
            // GNU `Fdefine_charset_internal' ends by consing the name onto
            // `Vcharset_list' (charset.c), newest first. Ours never did, so
            // `charset-list' stayed nil while GNU's held 203 charsets --
            // anything enumerating the defined charsets saw none.
            if let Some(name) = name {
                push_onto_charset_list(ctx, name);
            }
            Ok(defined)
        }),
        SubrArity::new(17, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "define-charset-alias",
        NativeFn::ContextVec(|ctx, args| {
            ctx.coding_systems().invalidate_lookup_cache();
            let alias = args.first().copied();
            let defined =
                crate::emacs_core::charset::builtin_define_charset_alias_in_state(ctx, args)?;
            // An alias joins `charset-list' too, though not the priority list.
            if let Some(alias) = alias {
                push_onto_charset_list(ctx, alias);
            }
            Ok(defined)
        }),
        SubrArity::new(2, Some(2)),
    )));
    crate::emacs_core::display_subrs::register_internal_lisp_face_p(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-to-syntax",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_string_to_syntax(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "syntax-class-to-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_syntax_class_to_char(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copy-syntax-table",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_copy_syntax_table_in_state),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "syntax-table-p",
        NativeFn::ContextVec(crate::emacs_core::syntax::builtin_syntax_table_p_in_state),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "standard-syntax-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::syntax::builtin_standard_syntax_table(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-time",
        NativeFn::ContextVec(crate::emacs_core::timefns::builtin_current_time_in_context),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-cpu-time",
        NativeFn::ContextVec(|_ctx, args| builtin_current_cpu_time(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-internal-run-time",
        NativeFn::ContextVec(|_ctx, args| builtin_get_internal_run_time(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "float-time",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_float_time(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "flush-standard-output",
        NativeFn::ContextVec(|_ctx, args| builtin_flush_standard_output(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-directory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_directory),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-nondirectory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_nondirectory),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-as-directory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_as_directory),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "directory-file-name",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_directory_file_name),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-concat",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_file_name_concat(args)
        }),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-absolute-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_file_name_absolute_p(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "directory-name-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_directory_name_p(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "substitute-in-file-name",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_substitute_in_file_name),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-file-acl",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_acl),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-file-selinux-context",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_set_file_selinux_context),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "visited-file-modtime",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_visited_file_modtime),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-temp-name",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::fileio::builtin_make_temp_name(ctx, args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "next-read-file-uses-dialog-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_next_read_file_uses_dialog_p(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unhandled-file-name-directory",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_unhandled_file_name_directory_eval),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-truename-buffer",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_get_truename_buffer(ctx, args)
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "single-key-description",
        NativeFn::ContextVec(|ctx, args| builtin_single_key_description(args, ctx.symbol_mode())),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "key-description",
        NativeFn::ContextVec(|ctx, args| builtin_key_description(args, ctx.symbol_mode())),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "event-convert-list",
        NativeFn::ContextVec(|ctx, args| builtin_event_convert_list(args, ctx.symbol_mode())),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "text-char-description",
        NativeFn::ContextVec(|_ctx, args| builtin_text_char_description(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-binary-mode",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::process::builtin_set_binary_mode(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "group-name",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_group_name(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "group-gid",
        NativeFn::ContextVec(
            |_ctx, args| crate::emacs_core::identity_commands::builtin_group_gid(args)
        ),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "group-real-gid",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::identity_commands::builtin_group_real_gid(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "load-average",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_load_average(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "logcount",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::editfns::builtin_logcount(args)),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_set_frame_size_and_position_pixelwise(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "mouse-position-in-root-frame",
        NativeFn::ContextVec(
            crate::emacs_core::display_entries::builtin_mouse_position_in_root_frame
        ),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-load-color-file",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_x_load_color_file(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "display--line-is-continued-p",
        NativeFn::ContextVec(|_ctx, args| builtin_display_line_is_continued_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "display--update-for-mouse-movement",
        NativeFn::ContextVec(
            crate::emacs_core::display_entries::builtin_display_update_for_mouse_movement
        ),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "do-auto-save",
            NativeFn::ContextVec(crate::emacs_core::fileio::builtin_do_auto_save),
            SubrArity::new(0, Some(2)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "describe-buffer-bindings",
            NativeFn::ContextVec(crate::emacs_core::keymap::builtin_describe_buffer_bindings),
            SubrArity::new(1, Some(3)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "face-attributes-as-vector",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_face_attributes_as_vector(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_font_get_system_font(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fontset-font",
        NativeFn::ContextVec(
            |_ctx, args| crate::emacs_core::display_entries::builtin_fontset_font(args)
        ),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fontset-info",
        NativeFn::ContextVec(|_ctx, args| builtin_fontset_info(args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fontset-list",
        NativeFn::ContextVec(|_ctx, args| builtin_fontset_list(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame--set-was-invisible",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_set_was_invisible),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-after-make-frame",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_after_make_frame),
        SubrArity::new(2, Some(2)),
    )));
    crate::emacs_core::display_subrs::register_frame_ancestor_p(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-font-cache",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_font_cache),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-fringe-width",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_fringe_width(args)),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_frame_internal_border_width(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-pointer-visible-p",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_pointer_visible_p(args)),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_frame_right_divider_width(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-scroll-bar-height",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_scroll_bar_height),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-scroll-bar-width",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_frame_scroll_bar_width),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_frame_window_state_change(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "garbage-collect-heapsize",
        NativeFn::ContextVec(|_ctx, args| builtin_garbage_collect_heapsize(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-unicode-property-internal",
        NativeFn::ContextVec(
            crate::emacs_core::chartable::builtin_get_unicode_property_internal_in_state,
        ),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-available-p",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_available_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-asynchronous-parameters",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_asynchronous_parameters),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-bye",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_bye),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-ciphers",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_ciphers(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-deinit",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_deinit),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-digests",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_digests(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-error-fatalp",
        NativeFn::ContextVec(gnutls::builtin_gnutls_error_fatalp_with_ctx),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-error-string",
        NativeFn::ContextVec(gnutls::builtin_gnutls_error_string_with_ctx),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-errorp",
        NativeFn::ContextVec(|ctx, args| gnutls::builtin_gnutls_errorp(args, ctx.symbol_mode())),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-format-certificate",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_format_certificate(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-get-initstage",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_get_initstage),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-hash-digest",
        NativeFn::ContextVec(|ctx, args| {
            gnutls::builtin_gnutls_hash_digest(args, ctx.symbol_mode())
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-hash-mac",
        NativeFn::ContextVec(|ctx, args| gnutls::builtin_gnutls_hash_mac(args, ctx.symbol_mode())),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-macs",
        NativeFn::ContextVec(|_ctx, args| gnutls::builtin_gnutls_macs(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-peer-status",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_gnutls_peer_status),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-peer-status-warning-describe",
        NativeFn::ContextVec(|ctx, args| {
            gnutls::builtin_gnutls_peer_status_warning_describe(args, ctx.symbol_mode())
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-symmetric-decrypt",
        NativeFn::ContextVec(|ctx, args| {
            gnutls::builtin_gnutls_symmetric_decrypt(args, ctx.symbol_mode())
        }),
        SubrArity::new(4, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "gnutls-symmetric-encrypt",
        NativeFn::ContextVec(|ctx, args| {
            gnutls::builtin_gnutls_symmetric_encrypt(args, ctx.symbol_mode())
        }),
        SubrArity::new(4, Some(5)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "handle-save-session",
            NativeFn::ContextVec(|_ctx, args| builtin_handle_save_session(args)),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("e")),
    );
    crate::emacs_core::display_subrs::register_handle_switch_frame(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "help--describe-vector",
        NativeFn::ContextVec(crate::emacs_core::keymap::builtin_help_describe_vector),
        SubrArity::new(7, Some(7)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "init-image-library",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::display_entries::builtin_init_image_library(args, ctx.symbol_mode())
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal--obarray-buckets",
        NativeFn::ContextVec(|_ctx, args| builtin_internal_obarray_buckets(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal--set-buffer-modified-tick",
        NativeFn::ContextVec(|ctx, args| builtin_internal_set_buffer_modified_tick(
            &mut ctx.edit.buffers,
            args
        )),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-set-lisp-face-attribute-from-resource",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_internal_set_lisp_face_attribute_from_resource),
        SubrArity::new(3, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-stack-stats",
        NativeFn::ContextVec(|_ctx, args| builtin_internal_stack_stats(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-subr-documentation",
        NativeFn::ContextVec(|_ctx, args| builtin_internal_subr_documentation(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "decode-coding-region",
            NativeFn::ContextVec(crate::encoding::builtin_decode_coding_region),
            SubrArity::new(3, Some(4)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r\nzCoding system: "),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "dump-emacs-portable",
        NativeFn::ContextVec(crate::emacs_core::system_entries::builtin_dump_emacs_portable),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "dump-emacs-portable--sort-predicate",
        NativeFn::ContextVec(|_ctx, args| builtin_dump_emacs_portable_sort_predicate(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "dump-emacs-portable--sort-predicate-copied",
        NativeFn::ContextVec(|_ctx, args| builtin_dump_emacs_portable_sort_predicate_copied(args)),
        SubrArity::new(2, Some(2)),
    )));
    // `emacs-repository-get-version' (lisp/version.el:183) and
    // `emacs-repository-get-branch' (:231) are not here.  They were
    // registered as "gap-fill stubs for loadup.el", but loadup loads
    // version.el at :128 and only calls them at :429 (DIVERGENCES.md 152).
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "encode-coding-region",
            NativeFn::ContextVec(crate::encoding::builtin_encode_coding_region),
            SubrArity::new(3, Some(4)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("r\nzCoding system: "),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "iso-charset",
        NativeFn::ContextVec(|_ctx, args| builtin_iso_charset(args)),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "keymap--get-keyelt",
        NativeFn::ContextVec(crate::emacs_core::keymap::builtin_keymap_get_keyelt),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "lower-frame",
            NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_lower_frame),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "lread--substitute-object-in-subtree",
        NativeFn::ContextVec(|_ctx, args| builtin_lread_substitute_object_in_subtree(args)),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "malloc-info",
            NativeFn::ContextVec(|_ctx, args| builtin_malloc_info(args)),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "malloc-trim",
            NativeFn::ContextVec(|_ctx, args| builtin_malloc_trim(args)),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-byte-code",
        NativeFn::ContextVec(|_ctx, args| builtin_make_byte_code(args)),
        SubrArity::new(4, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-char",
        NativeFn::ContextVec(charset::builtin_make_char_in_state),
        SubrArity::new(1, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-closure",
        NativeFn::ContextSlice(|_ctx, args| builtin_make_closure(args)),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "marker-last-position",
        NativeFn::ContextVec(|_ctx, args| builtin_marker_last_position(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-interpreted-closure",
        NativeFn::ContextVec(|_ctx, args| builtin_make_interpreted_closure(args)),
        SubrArity::new(3, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-record",
        NativeFn::ContextVec(|_ctx, args| builtin_make_record(args)),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-temp-file-internal",
        NativeFn::ContextVec(crate::emacs_core::system_entries::builtin_make_temp_file_internal),
        SubrArity::new(4, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "memory-info",
        NativeFn::ContextVec(|_ctx, args| builtin_memory_info(args)),
        SubrArity::new(0, Some(0)),
    )));
    // `memory-limit' is not here: it is a `defun' at lisp/subr.el:3574 over
    // `process-attributes', which IS registered (src/process.c)
    // (DIVERGENCES.md 152).
    crate::emacs_core::display_subrs::register_make_frame_invisible(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "menu-bar-menu-at-x-y",
        NativeFn::ContextVec(crate::emacs_core::keymap::builtin_menu_bar_menu_at_x_y),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "menu-or-popup-active-p",
        NativeFn::ContextVec(|_ctx, args| builtin_menu_or_popup_active_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "module-load",
        NativeFn::ContextVec(crate::emacs_core::system_entries::builtin_module_load),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "newline-cache-check",
        NativeFn::ContextVec(|_ctx, args| builtin_newline_cache_check(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "native-comp-available-p",
        NativeFn::ContextVec(|_ctx, args| builtin_native_comp_available_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "obarray-clear",
        NativeFn::ContextVec(|_ctx, args| builtin_obarray_clear(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "obarray-make",
        NativeFn::ContextVec(|_ctx, args| builtin_obarray_make(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "open-dribble-file",
            NativeFn::ContextVec(crate::emacs_core::system_entries::builtin_open_dribble_file),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String("FOpen dribble file: "),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "open-font",
        NativeFn::ContextVec(|ctx, args| builtin_open_font(args, ctx.symbol_mode())),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "optimize-char-table",
        NativeFn::ContextVec(|ctx, args| builtin_optimize_char_table(args, ctx.symbol_mode())),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-lists",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_overlay_lists(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "overlay-recenter",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_overlay_recenter(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "pdumper-stats",
        NativeFn::ContextVec(|_ctx, args| builtin_pdumper_stats(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "play-sound-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::sound::builtin_play_sound_internal(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "put-unicode-property-internal",
        NativeFn::ContextVec(
            crate::emacs_core::chartable::builtin_put_unicode_property_internal_in_state,
        ),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "query-fontset",
        NativeFn::ContextVec(|ctx, args| builtin_query_fontset(ctx, args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "raise-frame",
            NativeFn::ContextVec(|_ctx, args| builtin_raise_frame(args)),
            SubrArity::new(0, Some(1)),
        ))
        .placeholder(NoEvalPlaceholder::Nil)
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-positioning-symbols",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::minibuffer::builtin_read_impl(ctx, args, true)
        }),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "re--describe-compiled",
        NativeFn::ContextVec(|_ctx, args| builtin_re_describe_compiled(args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "recent-auto-save-p",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_recent_auto_save_p(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(0, Some(0)),
    )));
    crate::emacs_core::display_subrs::register_neomacs_frame_snapshot(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "record",
        NativeFn::ContextVec(|_ctx, args| builtin_record(args)),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "reconsider-frame-fonts",
        NativeFn::ContextVec(builtin_reconsider_frame_fonts),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "redirect-debugging-output",
            NativeFn::ContextVec(
                crate::emacs_core::system_entries::builtin_redirect_debugging_output
            ),
            SubrArity::new(1, Some(2)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "FDebug output file: \nP",
            ),
        ),
    );
    crate::emacs_core::display_subrs::register_redirect_frame_focus(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "remove-pos-from-symbol",
        NativeFn::ContextVec(|_ctx, args| builtin_remove_pos_from_symbol(args)),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_resize_mini_window_internal(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "restore-buffer-modified-p",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::buffer::builtin_restore_buffer_modified_p(ctx, args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-buffer-auto-saved",
        NativeFn::ContextVec(
            |ctx, args| crate::emacs_core::buffer::builtin_set_buffer_auto_saved(
                &mut ctx.edit.buffers,
                args
            )
        ),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-charset-plist",
        NativeFn::ContextVec(|ctx, args| builtin_set_charset_plist(args, ctx.symbol_mode())),
        SubrArity::new(2, Some(2)),
    )));
    crate::emacs_core::display_subrs::register_set_frame_window_state_change(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-mouse-pixel-position",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_set_mouse_pixel_position),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-mouse-position",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_set_mouse_position),
        SubrArity::new(3, Some(3)),
    )));
    crate::emacs_core::display_subrs::register_set_window_new_normal(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sort-charsets",
        NativeFn::ContextVec(|_ctx, args| builtin_sort_charsets(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "split-char",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::charset::builtin_split_char(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-distance",
        NativeFn::ContextVec(|_ctx, args| builtin_string_distance(args)),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "subr-native-lambda-list",
        NativeFn::ContextVec(|_ctx, args| builtin_subr_native_lambda_list(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "subr-type",
        NativeFn::ContextVec(|_ctx, args| builtin_subr_type(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "suspend-emacs",
            NativeFn::ContextVec(|_ctx, args| builtin_suspend_emacs(args)),
            SubrArity::new(0, Some(1)),
        ))
        .placeholder(NoEvalPlaceholder::Nil)
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tool-bar-get-system-style",
        NativeFn::ContextVec(|_ctx, args| builtin_tool_bar_get_system_style(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tool-bar-pixel-width",
        NativeFn::ContextVec(builtin_tool_bar_pixel_width),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "translate-region-internal",
            NativeFn::ContextVec(crate::emacs_core::editfns::builtin_translate_region_internal),
            SubrArity::new(3, Some(3)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tty--output-buffer-size",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_output_buffer_size(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tty--set-output-buffer-size",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_set_output_buffer_size(args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tty-display-pixel-height",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_tty_display_pixel_height),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tty-display-pixel-width",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_tty_display_pixel_width),
        SubrArity::new(0, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_tty_frame_at(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tty-frame-restack",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_frame_restack(args)),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "tty-suppress-bold-inverse-default-colors",
        NativeFn::ContextVec(|_ctx, args| builtin_tty_suppress_bold_inverse_default_colors(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unencodable-char-position",
        NativeFn::ContextVec(crate::emacs_core::coding::builtin_unencodable_char_position),
        SubrArity::new(3, Some(5)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unicode-property-table-internal",
        NativeFn::ContextVec(
            crate::emacs_core::chartable_entries::builtin_unicode_property_table_internal
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unify-charset",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_unify_charset_in_state),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "unix-sync",
            NativeFn::ContextVec(|_ctx, args| builtin_unix_sync(args)),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-begin-drag",
        NativeFn::ContextVec(|_ctx, args| builtin_x_begin_drag(args)),
        SubrArity::new(1, Some(6)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-double-buffered-p",
        NativeFn::ContextVec(|_ctx, args| builtin_x_double_buffered_p(args)),
        SubrArity::new(0, Some(1)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "x-menu-bar-open-internal",
            NativeFn::ContextVec(|_ctx, args| builtin_x_menu_bar_open_internal(args)),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("i")),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "xw-color-defined-p",
            NativeFn::ContextVec(|ctx, args| {
                crate::emacs_core::xfaces::builtin_xw_color_defined_p_ctx(ctx, args)
            }),
            SubrArity::new(1, Some(2)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    );
    // `color-defined-p' is lisp/faces.el:1923, not a DEFUN (DIVERGENCES.md
    // 154).  Its body dispatches on `display-graphic-p' to `xw-color-defined-p'
    // -- registered immediately above, and a C DEFUN in GNU -- or to
    // `tty-color-translate'.  Registering the graphical arm under the generic
    // name skipped that dispatch.
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "xw-color-values",
            NativeFn::ContextVec(|ctx, args| {
                crate::emacs_core::xfaces::builtin_xw_color_values_ctx(ctx, args)
            }),
            SubrArity::new(1, Some(2)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    );
    // `color-values' is lisp/faces.el:1940, not a DEFUN (DIVERGENCES.md 154),
    // and dispatches the same way: `xw-color-values' (above, a C DEFUN) or
    // `tty-color-values'.
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "xw-display-color-p",
            NativeFn::ContextVec(|ctx, args| builtin_xw_display_color_p_ctx(ctx, args)),
            SubrArity::new(0, Some(1)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    );
    std::cfg_select! {
        any(target_os = "linux", target_os = "macos", target_os = "windows") => {
            file_notify::register_subrs(ctx);
        }
        _ => {}
    }
    crate::emacs_core::dbusbind::register_subrs(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "lock-buffer",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_lock_buffer),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "lock-file",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_lock_file),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "lossage-size",
            NativeFn::ContextVec(|_ctx, args| builtin_lossage_size(args)),
            SubrArity::new(0, Some(1)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                lossage_size_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unlock-buffer",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_unlock_buffer),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unlock-file",
        NativeFn::ContextVec(crate::emacs_core::filelock::builtin_unlock_file),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_window_bottom_divider_width(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-available-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_available_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-compiled-query-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_compiled_query_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-library-abi-version",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_library_abi_version(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_node_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-node-parser",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_node_parser(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-parser-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_parser_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-language",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_language(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-p",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "treesit-query-source",
        NativeFn::ContextVec(|_ctx, args| builtin_treesit_query_source(args)),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::sqlite::register_subrs(ctx);
    // GNU emacs.c sequences font.c immediately after sqlite.c.
    crate::emacs_core::display_subrs::register_font(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fillarray",
        NativeFn::ContextVec(|_ctx, args| builtin_fillarray(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-test",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_test(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-size",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_size(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-rehash-size",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_rehash_size(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-rehash-threshold",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_rehash_threshold(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-weakness",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_hash_table_weakness(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copy-hash-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_copy_hash_table(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sxhash-eq",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::hashtab::builtin_sxhash_eq_with_mode(args, ctx.symbol_mode())
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sxhash-eql",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::hashtab::builtin_sxhash_eql_with_mode(args, ctx.symbol_mode())
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sxhash-equal",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::hashtab::builtin_sxhash_equal_with_mode(args, ctx.symbol_mode())
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sxhash-equal-including-properties",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::hashtab::builtin_sxhash_equal_including_properties_with_mode(
                args,
                ctx.symbol_mode(),
            )
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal--hash-table-buckets",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_internal_hash_table_buckets(
                args,
                crate::emacs_core::print::print_value,
            )
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal--hash-table-histogram",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_internal_hash_table_histogram(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal--hash-table-index-size",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::hashtab::builtin_internal_hash_table_index_size(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-frame-geometry",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_frame_geometry(args)),
        SubrArity::new(0, None),
    )));
    crate::emacs_core::display_subrs::register_neomacs_frame_edges(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_mouse_absolute_pixel_position(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-set-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_set_mouse_absolute_pixel_position(args)),
        SubrArity::new(0, None),
    )));
    crate::emacs_core::display_subrs::register_effects_modifiers_chrome(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-display-monitor-attributes-list",
        NativeFn::ContextVec(
            crate::emacs_core::display_entries::builtin_neomacs_display_monitor_attributes_list
        ),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "neomacs-core-backend",
        NativeFn::ContextVec(|_ctx, args| builtin_neomacs_core_backend(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "debugger-trap",
        NativeFn::ContextVec(|_ctx, args| builtin_debugger_trap(args)),
        SubrArity::new(0, Some(0)),
    )));
    crate::emacs_core::display_subrs::register_window_discard_buffer_from_window(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame-windows-min-size",
        NativeFn::ContextVec(|_ctx, args| builtin_frame_windows_min_size(args)),
        SubrArity::new(4, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "remember-mouse-glyph",
        NativeFn::ContextVec(crate::emacs_core::display_entries::builtin_remember_mouse_glyph),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "obarrayp",
        NativeFn::ContextVec(|_ctx, args| builtin_obarrayp(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ntake",
        NativeFn::ContextVec(|_ctx, args| builtin_ntake(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "default-file-modes",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_default_file_modes(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-default-file-modes",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fileio::builtin_set_default_file_modes(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-configuration-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_hooks::builtin_window_configuration_p(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-configuration-frame",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_hooks::builtin_window_configuration_frame(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "window-configuration-equal-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::window_hooks::builtin_window_configuration_equal_p(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-input-meta-mode",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::keyboard::builtin_set_input_meta_mode(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-output-flow-control",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::keyboard::builtin_set_output_flow_control(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-quit-char",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_set_quit_char),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "top-level",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_top_level),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "documentation-stringp",
        NativeFn::ContextVec(|_ctx, args| builtin_documentation_stringp(args)),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_window_text_pixel_size(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "delete-directory-internal",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_delete_directory_internal),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-directory-internal",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_make_directory_internal),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "directory-files-and-attributes",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_directory_files_and_attributes),
        SubrArity::new(1, Some(6)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "find-file-name-handler",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_find_file_name_handler),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-all-completions",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_name_all_completions),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-accessible-directory-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_accessible_directory_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-name-case-insensitive-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_name_case_insensitive_p),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-newer-than-file-p",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_file_newer_than_file_p),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "verify-visited-file-modtime",
        NativeFn::ContextVec(crate::emacs_core::fileio::builtin_verify_visited_file_modtime),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-default-interrupt-process",
        NativeFn::ContextVec(
            crate::emacs_core::process::builtin_internal_default_interrupt_process,
        ),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-default-process-filter",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_process_filter),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-default-process-sentinel",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_process_sentinel),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-default-signal-process",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_internal_default_signal_process),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "network-lookup-address-info",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_network_lookup_address_info),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-network-process-option",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_network_process_option),
        SubrArity::new(3, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-query-on-exit-flag",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_process_query_on_exit_flag),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-query-on-exit-flag",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_query_on_exit_flag),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "process-inherit-coding-system-flag",
        NativeFn::ContextVec(
            crate::emacs_core::process::builtin_process_inherit_coding_system_flag,
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-coding-system",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_coding_system),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-process-datagram-address",
        NativeFn::ContextVec(crate::emacs_core::process::builtin_set_process_datagram_address),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
        "remove-list-of-text-properties",
        crate::emacs_core::textprop::builtin_remove_list_of_text_properties_4,
        FixedMin4::Three,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-char-property-and-overlay",
        NativeFn::ContextVec(
            crate::emacs_core::textprop_entries::builtin_get_char_property_and_overlay
        ),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
        "next-single-property-change",
        crate::emacs_core::textprop::builtin_next_single_property_change_4,
        FixedMin4::Two,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed4(
        "previous-single-property-change",
        crate::emacs_core::textprop::builtin_previous_single_property_change_4,
        FixedMin4::Two,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "line-beginning-position",
        crate::emacs_core::navigation::builtin_line_beginning_position_1,
        FixedMin1::Zero,
    )));
    crate::emacs_core::display_subrs::register_active_minibuffer_window(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "minor-mode-key-binding",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_minor_mode_key_binding),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "this-command-keys-vector",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_this_command_keys_vector),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "this-single-command-keys",
            NativeFn::ContextVec(crate::emacs_core::interactive::builtin_this_single_command_keys),
            SubrArity::new(0, Some(0)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "this-single-command-raw-keys",
            NativeFn::ContextVec(
                crate::emacs_core::interactive::builtin_this_single_command_raw_keys,
            ),
            SubrArity::new(0, Some(0)),
        ))
        .placeholder(NoEvalPlaceholder::Nil),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "clear-this-command-keys",
        NativeFn::ContextVec(crate::emacs_core::interactive::builtin_clear_this_command_keys),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "waiting-for-user-input-p",
        NativeFn::ContextVec(crate::emacs_core::keyboard::builtin_waiting_for_user_input_p_ctx),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "minibuffer-prompt",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_prompt_ctx),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "minibuffer-prompt-end",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_minibuffer_prompt_end_ctx),
            SubrArity::new(0, Some(0)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "innermost-minibuffer-p",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_innermost_minibuffer_p_ctx),
            SubrArity::new(0, Some(1)),
        ))
        .requires_eval_state(),
    );
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "abort-minibuffers",
            NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_abort_minibuffers_ctx),
            SubrArity::new(0, Some(0)),
        ))
        .interactive(crate::emacs_core::interactive::BuiltinInteractiveSpec::String("")),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-marker-insertion-type",
        NativeFn::ContextVec(|ctx: &mut crate::emacs_core::eval::Context, args| {
            crate::emacs_core::marker::builtin_set_marker_insertion_type_in_buffers(
                &mut ctx.edit.buffers,
                args,
            )
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-standard-case-table",
        NativeFn::ContextVec(crate::emacs_core::casetab_entries::builtin_set_standard_case_table),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-unused-category",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_get_unused_category),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "standard-category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_standard_category_table),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "upcase-initials-region",
            NativeFn::ContextVec(
                crate::emacs_core::casefiddle_entries::builtin_upcase_initials_region
            ),
            SubrArity::new(2, Some(3)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::Form(
                region_noncontiguous_interactive_spec,
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "buffer-substring-no-properties",
        NativeFn::ContextVec(|eval, args| {
            crate::emacs_core::editfns::builtin_buffer_substring_no_properties(eval, args)
        }),
        SubrArity::new(2, Some(2)),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "subrp",
        NativeFn::ContextSlice(|_ctx, args| crate::emacs_core::builtins_extra::builtin_subrp(args)),
        SubrArity::new(1, Some(1)),
    )));

    // -----------------------------------------------------------------------
    // Additional native subr declarations.
    // -----------------------------------------------------------------------

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "*",
        NativeFn::ContextSlice(|_ctx, args| builtin_mul(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "/",
        NativeFn::ContextSlice(|_ctx, args| builtin_div(args)),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "abs",
        NativeFn::ContextSlice(|_ctx, args| builtin_abs(args)),
        SubrArity::new(1, Some(1)),
    )));

    // -- Logical / bitwise --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "logand",
        NativeFn::ContextSlice(|_ctx, args| builtin_logand_slice(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "logior",
        NativeFn::ContextSlice(|_ctx, args| builtin_logior_slice(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "logxor",
        NativeFn::ContextSlice(|_ctx, args| builtin_logxor_slice(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "lognot",
        NativeFn::ContextVec(|_ctx, args| builtin_lognot(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ash",
        NativeFn::ContextSlice(|_ctx, args| builtin_ash_slice(args)),
        SubrArity::new(2, Some(2)),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "characterp",
        NativeFn::ContextVec(|_ctx, args| builtin_characterp(args)),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-p",
        NativeFn::ContextVec(|_ctx, args| builtin_hash_table_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bufferp",
        NativeFn::ContextVec(|_ctx, args| builtin_bufferp(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "sequencep",
        NativeFn::ContextVec(|_ctx, args| builtin_sequencep(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "arrayp",
        NativeFn::ContextSlice(|_ctx, args| builtin_arrayp(args)),
        SubrArity::new(1, Some(1)),
    )));
    // `ignore' is not here: it is a `defun' at lisp/subr.el:501, and the
    // byte compiler names it itself -- `(byte-defop-compiler-1 ignore)',
    // lisp/emacs-lisp/bytecomp.el:4429 -- so a compiled `(ignore X)' emits
    // Bconstant nil and never reads the cell (DIVERGENCES.md 152).
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "cl-type-of",
        NativeFn::ContextVec(|_ctx, args| builtin_cl_type_of(&args)),
        SubrArity::new(1, Some(1)),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "reverse",
        NativeFn::ContextVec(|_ctx, args| builtin_reverse(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copy-alist",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_copy_alist(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-list",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_make_list(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "safe-length",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_safe_length(args)),
        SubrArity::new(1, Some(1)),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "substring",
        NativeFn::ContextSlice(|_ctx, args| builtin_substring_slice(args)),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "concat",
        NativeFn::ContextSlice(|_ctx, args| builtin_concat_slice(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unibyte-string",
        NativeFn::ContextVec(|_ctx, args| builtin_unibyte_string(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "number-to-string",
        NativeFn::ContextVec(|ctx, args| builtin_number_to_string(ctx, args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-to-string",
        NativeFn::ContextVec(|_ctx, args| builtin_char_to_string(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-to-char",
        NativeFn::ContextVec(|_ctx, args| builtin_string_to_char(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "clear-string",
        NativeFn::ContextVec(|_ctx, args| builtin_clear_string(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-make-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_string_make_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-make-unibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_string_make_unibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-to-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_string_to_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-to-unibyte",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_string_to_unibyte(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-as-unibyte",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_string_as_unibyte(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "string-as-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_string_as_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "unibyte-char-to-multibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_unibyte_char_to_multibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "multibyte-char-to-unibyte",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::misc::builtin_multibyte_char_to_unibyte(args)
        }),
        SubrArity::new(1, Some(1)),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "vconcat",
        NativeFn::ContextSlice(|_ctx, args| builtin_vconcat_slice(args)),
        SubrArity::new(0, None),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "clrhash",
        NativeFn::ContextVec(|_ctx, args| builtin_clrhash(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "hash-table-count",
        NativeFn::ContextVec(|_ctx, args| builtin_hash_table_count(args)),
        SubrArity::new(1, Some(1)),
    )));

    // -- Float / math / conversion --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "float",
        NativeFn::ContextSlice(|_ctx, args| builtin_float(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copysign",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_copysign(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frexp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_frexp(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ldexp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_ldexp(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "logb",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_logb(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fceiling",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_fceiling(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ffloor",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_ffloor(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "fround",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_fround(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ftruncate",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::floatfns::builtin_ftruncate(args)),
        SubrArity::new(1, Some(1)),
    )));

    // -- Misc pure --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bitmap-spec-p",
        NativeFn::ContextVec(|_ctx, args| builtin_bitmap_spec_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "byte-to-string",
        NativeFn::ContextVec(|_ctx, args| builtin_byte_to_string(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "clear-buffer-auto-save-failure",
        NativeFn::ContextVec(builtin_clear_buffer_auto_save_failure),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "locale-info",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::misc::builtin_locale_info(args)),
        SubrArity::new(1, Some(1)),
    )));
    // -- Subr introspection --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "subr-name",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::subr_info::builtin_subr_name(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "native-comp-function-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::subr_info::builtin_native_comp_function_p(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "interpreted-function-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::subr_info::builtin_interpreted_function_p(args)
        }),
        SubrArity::new(1, Some(1)),
    )));

    // -- Character encoding --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-width",
        NativeFn::ContextVec(|ctx, args| crate::encoding::builtin_char_width_in_context(ctx, args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "string-bytes",
        crate::encoding::builtin_string_bytes_1,
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "multibyte-string-p",
        crate::encoding::builtin_multibyte_string_p_1,
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "encode-coding-string",
        NativeFn::ContextVec(crate::encoding::builtin_encode_coding_string_in_context),
        SubrArity::new(2, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "decode-coding-string",
        NativeFn::ContextVec(crate::encoding::builtin_decode_coding_string_in_context),
        SubrArity::new(2, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "char-or-string-p",
        crate::encoding::builtin_char_or_string_p_1,
        FixedMin1::One,
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "max-char",
        NativeFn::ContextVec(|_ctx, args| crate::encoding::builtin_max_char(args)),
        SubrArity::new(0, Some(1)),
    )));

    // -- Search --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "regexp-quote",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::search::builtin_regexp_quote(args)),
        SubrArity::new(1, Some(1)),
    )));

    // -- File I/O --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "file-attributes-lessp",
        NativeFn::ContextVec(crate::emacs_core::dired::builtin_file_attributes_lessp),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "system-users",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::dired::builtin_system_users(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "system-groups",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::dired::builtin_system_groups(args)),
        SubrArity::new(0, Some(0)),
    )));

    // -- User / editfns --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "user-uid",
        NativeFn::ContextVec(
            |_ctx, args| crate::emacs_core::identity_commands::builtin_user_uid(args)
        ),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "user-real-uid",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::identity_commands::builtin_user_real_uid(args)
        }),
        SubrArity::new(0, Some(0)),
    )));

    // -- Time/date --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "time-add",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_add(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "time-subtract",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_subtract(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "time-less-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_less_p(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "time-equal-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_time_equal_p(args)),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-time-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::timefns::builtin_current_time_string(args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "current-time-zone",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::timefns::builtin_current_time_zone(args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "encode-time",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_encode_time(args)),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "decode-time",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::timefns::builtin_decode_time(args)),
        SubrArity::new(0, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "time-convert",
        NativeFn::ContextVec(crate::emacs_core::timefns::builtin_time_convert_in_context),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-time-zone-rule",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::timefns::builtin_set_time_zone_rule(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "format-time-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::format_time_entries::builtin_format_time_string(args)
        }),
        SubrArity::new(1, Some(3)),
    )));

    // -- Case/char --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "upcase-initials",
        NativeFn::ContextVec(
            crate::emacs_core::casefiddle_entries::builtin_upcase_initials_in_state
        ),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-resolve-modifiers",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::casefiddle::builtin_char_resolve_modifiers(args)
        }),
        SubrArity::new(1, Some(1)),
    )));

    // -- Font/face --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-lisp-face-attribute-values",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_lisp_face_attribute_values(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_internal_lisp_face_equal_p(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "face-attribute-relative-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_face_attribute_relative_p(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    crate::emacs_core::display_subrs::register_merge_face_attribute(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "color-supported-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_color_supported_p(args)
        }),
        SubrArity::new(1, Some(3)),
    )));
    crate::emacs_core::display_subrs::register_color_distance(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "color-values-from-color-spec",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_color_values_from_color_spec(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-face-x-get-resource",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_face_x_get_resource(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-set-font-selection-order",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_set_font_selection_order(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-set-alternative-font-family-alist",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_set_alternative_font_family_alist(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "internal-set-alternative-font-registry-alist",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xfaces::builtin_internal_set_alternative_font_registry_alist(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_internal_copy_lisp_face(ctx);

    // -- Case table --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "case-table-p",
        NativeFn::ContextVec(crate::emacs_core::casetab::builtin_case_table_p_in_state),
        SubrArity::new(1, Some(1)),
    )));

    // -- Category --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "category-table-p",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_category_table_p_in_state),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "copy-category-table",
        NativeFn::ContextVec(crate::emacs_core::category::builtin_copy_category_table_in_state),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-category-table",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_make_category_table(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "category-set-mnemonics",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::category::builtin_category_set_mnemonics(args)
        }),
        SubrArity::new(1, Some(1)),
    )));

    // -- Char-table / bool-vector --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-table-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::chartable::builtin_char_table_p(args)),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-char-table-range",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_set_char_table_range_in_state),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-table-range",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_char_table_range_in_state),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-table-parent",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_parent(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-char-table-parent",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_set_char_table_parent(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-table-extra-slot",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_extra_slot(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-char-table-extra-slot",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_set_char_table_extra_slot(args)
        }),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-table-subtype",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_char_table_subtype(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::chartable::builtin_bool_vector(args)),
        SubrArity::new(0, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-count-population",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_count_population(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-count-consecutive",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_count_consecutive(args)
        }),
        SubrArity::new(3, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-intersection",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_intersection(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-not",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_not(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-set-difference",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_set_difference(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-union",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_union(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-exclusive-or",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_exclusive_or(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bool-vector-subsetp",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::chartable::builtin_bool_vector_subsetp(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "make-char-table",
        NativeFn::ContextVec(crate::emacs_core::chartable::builtin_make_char_table),
        SubrArity::new(1, Some(2)),
    )));

    // -- Charset --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "charset-priority-list",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_charset_priority_list(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-charset-priority",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_set_charset_priority_in_state),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "char-charset",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_char_charset_in_state),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "charset-id-internal",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_charset_id_internal_in_state),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "declare-equiv-charset",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_declare_equiv_charset_in_state),
        SubrArity::new(4, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "find-charset-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_find_charset_string(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "decode-big5-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_decode_big5_char(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "decode-char",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_decode_char_in_state),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "decode-sjis-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_decode_sjis_char(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "encode-big5-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_encode_big5_char(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "encode-char",
        NativeFn::ContextVec(crate::emacs_core::charset::builtin_encode_char_in_state),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "encode-sjis-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_encode_sjis_char(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-unused-iso-final-char",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_get_unused_iso_final_char(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "clear-charset-maps",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::charset::builtin_clear_charset_maps(args)
        }),
        SubrArity::new(0, Some(0)),
    )));

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "set-text-conversion-style",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::coding::builtin_set_text_conversion_style(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "text-quoting-style",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::coding::builtin_text_quoting_style(ctx, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    // `set-buffer-file-coding-system' is not here: it is a `defun' at
    // lisp/international/mule.el:1302 that merges coding systems, sets
    // `buffer-file-coding-system-explicit' and marks the buffer modified
    // (DIVERGENCES.md 152).

    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "yes-or-no-p",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_yes_or_no_p),
        SubrArity::new(1, Some(1)),
    )));

    // -- Dispnew --
    crate::emacs_core::display_subrs::register_redraw_display(ctx);
    ctx.register_subr(
        crate::emacs_core::subr::world_subr!(SubrSpec::new(
            "open-termscript",
            NativeFn::ContextVec(|_ctx, args| {
                crate::emacs_core::dispnew::pure::builtin_open_termscript(args)
            }),
            SubrArity::new(1, Some(1)),
        ))
        .interactive(
            crate::emacs_core::interactive::BuiltinInteractiveSpec::String(
                "FOpen termscript file: ",
            ),
        ),
    );
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "ding",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::dispnew::pure::builtin_ding(args)),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "frame--z-order-lessp",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::dispnew::pure::builtin_frame_z_order_lessp(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    crate::emacs_core::display_subrs::register_force_window_update(ctx);

    // -- Display/terminal --
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-export-frames",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_export_frames(args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-backspace-delete-keys-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_backspace_delete_keys_p(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    #[cfg(not(target_os = "macos"))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-change-window-property",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_change_window_property(args)
        }),
        SubrArity::new(2, Some(7)),
    )));
    crate::emacs_core::display_subrs::register_x_focus_frame(ctx);
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-get-local-selection",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_local_selection(args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-get-modifier-masks",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_modifier_masks(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-get-selection-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_selection_internal(args)
        }),
        SubrArity::new(2, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-display-list",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_display_list(args)),
        SubrArity::new(0, Some(0)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-disown-selection-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_disown_selection_internal(args)
        }),
        SubrArity::new(1, Some(3)),
    )));
    #[cfg(not(target_os = "macos"))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-delete-window-property",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_delete_window_property(args)
        }),
        SubrArity::new(1, Some(3)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-frame-edges",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_frame_edges(args)),
        SubrArity::new(0, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-frame-geometry",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_frame_geometry(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-frame-list-z-order",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_frame_list_z_order(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-frame-restack",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_frame_restack(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-family-fonts",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_family_fonts(args)),
        SubrArity::new(0, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-get-atom-name",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_get_atom_name(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_mouse_absolute_pixel_position(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-own-selection-internal",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_own_selection_internal(args)
        }),
        SubrArity::new(2, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-parse-geometry",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_parse_geometry(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_x_popup_dialog(ctx);
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-register-dnd-atom",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_register_dnd_atom(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-selection-exists-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_selection_exists_p(args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-selection-owner-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_selection_owner_p(args)
        }),
        SubrArity::new(0, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-hide-tip",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::display::builtin_x_hide_tip_eval(ctx, args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-internal-focus-input-context",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_internal_focus_input_context(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-send-client-message",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_send_client_message(args)
        }),
        SubrArity::new(6, Some(6)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-show-tip",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::display::builtin_x_show_tip_eval(ctx, args)
        }),
        SubrArity::new(1, Some(6)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-set-mouse-absolute-pixel-position",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_set_mouse_absolute_pixel_position(args)
        }),
        SubrArity::new(2, Some(2)),
    )));
    #[cfg(not(target_os = "macos"))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-synchronize",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::display::builtin_x_synchronize(args)),
        SubrArity::new(1, Some(2)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-translate-coordinates",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_translate_coordinates(args)
        }),
        SubrArity::new(1, Some(6)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-uses-old-gtk-dialog",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_uses_old_gtk_dialog(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    #[cfg(not(target_os = "macos"))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-window-property",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_window_property(args)
        }),
        SubrArity::new(1, Some(6)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-window-property-attributes",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_window_property_attributes(args)
        }),
        SubrArity::new(1, Some(3)),
    )));
    #[cfg(not(any(windows, target_os = "macos")))]
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "x-wm-set-size-hint",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::display::builtin_x_wm_set_size_hint(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "terminal-list",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::terminal::pure::builtin_terminal_list(args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    crate::emacs_core::display_subrs::register_delete_terminal(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "imagep",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::image::builtin_imagep(args)),
        SubrArity::new(1, Some(1)),
    )));
    crate::emacs_core::display_subrs::register_image_transforms_p(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "move-point-visually",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_move_point_visually(args)
        }),
        SubrArity::new(1, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "lookup-image-map",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::xdisp::builtin_lookup_image_map(args)),
        SubrArity::new(3, Some(3)),
    )));
    crate::emacs_core::display_subrs::register_current_bidi_paragraph_direction(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bidi-resolved-levels",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_bidi_resolved_levels(args)
        }),
        SubrArity::new(0, Some(1)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "bidi-find-overridden-directionality",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_bidi_find_overridden_directionality(args)
        }),
        SubrArity::new(3, Some(4)),
    )));
    crate::emacs_core::display_subrs::register_move_to_window_line(ctx);
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "long-line-optimizations-p",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::xdisp::builtin_long_line_optimizations_p(args)
        }),
        SubrArity::new(0, Some(0)),
    )));

    // -- XML/decompress --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "libxml-parse-html-region",
        NativeFn::ContextVec(crate::emacs_core::xml::builtin_libxml_parse_html_region),
        SubrArity::new(0, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "libxml-parse-xml-region",
        NativeFn::ContextVec(crate::emacs_core::xml::builtin_libxml_parse_xml_region),
        SubrArity::new(0, Some(4)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "libxml-available-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::xml::builtin_libxml_available_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "zlib-available-p",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::zlib::builtin_zlib_available_p(args)),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "zlib-decompress-region",
        NativeFn::ContextVec(crate::emacs_core::zlib::builtin_zlib_decompress_region),
        SubrArity::new(2, Some(3)),
    )));

    // -- Native compilation compatibility --

    // -- DBus --
    //
    // Registered from `emacs_core::dbusbind` (GNU `syms_of_dbusbind`). The six
    // primitives exist only under `cfg(neomacs_have_dbus)`, matching
    // `#ifdef HAVE_DBUS`.

    // -- JSON --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "json-serialize",
        NativeFn::ContextVec(crate::emacs_core::json::builtin_json_serialize_in_state),
        SubrArity::new(1, None),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "json-parse-string",
        NativeFn::ContextVec(crate::emacs_core::json::builtin_json_parse_string_in_state),
        SubrArity::new(1, None),
    )));

    // -- Marker --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "markerp",
        NativeFn::ContextVec(|_ctx, args| crate::emacs_core::marker::builtin_markerp(args)),
        SubrArity::new(1, Some(1)),
    )));

    // -- Lread --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "get-load-suffixes",
        NativeFn::ContextVec(|ctx, args| {
            crate::emacs_core::lread::builtin_get_load_suffixes(ctx.variables(), args)
        }),
        SubrArity::new(0, Some(0)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-coding-system",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_coding_system),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "read-non-nil-coding-system",
        NativeFn::ContextVec(crate::emacs_core::minibuffer::builtin_read_non_nil_coding_system),
        SubrArity::new(1, Some(1)),
    )));

    // -- Base64/hash --
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "base64-encode-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_base64_encode_string(args)
        }),
        SubrArity::new(1, Some(2)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "base64-decode-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_base64_decode_string(args)
        }),
        SubrArity::new(1, Some(3)),
    )));
    ctx.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::new(
        "base64url-encode-string",
        NativeFn::ContextVec(|_ctx, args| {
            crate::emacs_core::fns::builtin_base64url_encode_string(args)
        }),
        SubrArity::new(1, Some(2)),
    )));

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
