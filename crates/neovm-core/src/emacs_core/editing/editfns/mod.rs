//! Editing-function builtins — point/mark queries, insertion, deletion,
//! substring extraction, and miscellaneous user/system info.
//!
//! Emacs Lisp uses **1-based character positions** while the internal
//! `Buffer` stores **0-based Emacs-byte positions**.  Every Lisp↔Buffer boundary
//! must convert:
//!
//! - Lisp char pos  →  byte pos:  `LispCharPos1::to_byte_pos`
//! - byte pos       →  Lisp char: `EmacsBytePos::to_lisp`

use super::error::{EvalResult, Flow, signal};
use super::eval::OverlayModificationHook;
use super::symbol::Obarray;
use super::value::*;
use crate::buffer::{
    Buffer, BufferManager, CharLen, EmacsByteLen, EmacsBytePos, EmacsByteRange, LispCharPos1,
    TextChange, TextEditRange, TextExtent,
};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::emacs_core::runtime_identity::{CredentialScope, process_group_id, process_user_id};
use crate::emacs_core::value::ValueKind;
use crate::heap_types::LispString;
#[cfg(unix)]
use std::ffi::CStr;
use strum::IntoStaticStr;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Extract an integer (or char-as-integer) from a Value, signalling
/// `wrong-type-argument` on type mismatch.
fn expect_integer(_name: &str, val: &Value) -> Result<i64, Flow> {
    val.as_int().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *val],
        )
    })
}

/// Convert a Lisp 1-based character position to a 0-based byte position,
/// clamping to the accessible region `[begv, zv]`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn lisp_pos_to_byte(
    buf: &crate::buffer::Buffer,
    lisp_pos: LispCharPos1,
) -> EmacsBytePos {
    buf.lisp_pos_to_accessible_emacs_byte_pos(lisp_pos)
}

/// Pre-interned symbols for the buffer-modification hot path: interning these
/// by NAME on every insert/change signal was a measured cost (~140M Ir on the
/// buffer benchmark). OnceLock is the established cached-SymId pattern.
macro_rules! editfns_cached_symbol {
    ($fn_name:ident, $name:literal) => {
        fn $fn_name() -> crate::emacs_core::intern::SymId {
            static S: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
                std::sync::OnceLock::new();
            *S.get_or_init(|| crate::emacs_core::intern::intern($name))
        }
    };
}
editfns_cached_symbol!(inhibit_read_only_symbol, "inhibit-read-only");
editfns_cached_symbol!(buffer_read_only_symbol, "buffer-read-only");
editfns_cached_symbol!(
    inhibit_modification_hooks_symbol,
    "inhibit-modification-hooks"
);
editfns_cached_symbol!(deactivate_mark_symbol, "deactivate-mark");
editfns_cached_symbol!(
    undo_auto_undoable_change_symbol,
    "undo-auto--undoable-change"
);
editfns_cached_symbol!(first_change_hook_symbol, "first-change-hook");
editfns_cached_symbol!(before_change_functions_symbol, "before-change-functions");
editfns_cached_symbol!(after_change_functions_symbol, "after-change-functions");
editfns_cached_symbol!(
    combine_after_change_calls_symbol,
    "combine-after-change-calls"
);

pub(crate) fn buffer_read_only_active_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buf: &Buffer,
) -> bool {
    let iro = inhibit_read_only_symbol();
    if let Some(value) = buf.get_buffer_local_by_sym_id_gated(iro, obarray.is_localized(iro))
        && value.is_truthy()
    {
        return false;
    }

    if obarray.symbol_value_id_or_nil(iro).is_truthy() {
        return false;
    }

    if buf.get_read_only() {
        return true;
    }

    let _ = dynamic;
    let bro = buffer_read_only_symbol();
    if let Some(value) = buf.get_buffer_local_by_sym_id_gated(bro, obarray.is_localized(bro)) {
        return value.is_truthy();
    }
    obarray.symbol_value_id_or_nil(bro).is_truthy()
}

pub(crate) fn ensure_current_buffer_writable_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &BufferManager,
) -> Result<(), Flow> {
    if let Some(buf) = buffers.current_buffer()
        && buffer_read_only_active_in_state(obarray, dynamic, buf)
    {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(buf.id)],
        ));
    }
    Ok(())
}

pub(crate) fn buffer_edit_range_for_byte_range_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<TextEditRange, Flow> {
    buffers
        .edit_range_for_buffer_emacs_byte_range(buffer_id, byte_range)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))
}

pub(crate) fn lisp_string_text_extent(text: &LispString) -> TextExtent {
    TextExtent::new(
        CharLen::new(text.schars()),
        EmacsByteLen::new(text.sbytes()),
    )
}

pub(crate) fn text_change_for_replacement_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
    new_extent: TextExtent,
) -> Result<TextChange, Flow> {
    let old_range = buffer_edit_range_for_byte_range_in_manager(buffers, buffer_id, byte_range)?;
    Ok(TextChange::new(old_range, new_extent))
}

pub(crate) fn text_change_for_empty_insertion_at_emacs_byte_pos(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_pos: EmacsBytePos,
    new_extent: TextExtent,
) -> Result<TextChange, Flow> {
    text_change_for_replacement_in_manager(
        buffers,
        buffer_id,
        EmacsByteRange::from_start_len(byte_pos, EmacsByteLen::ZERO),
        new_extent,
    )
}

pub(crate) fn text_change_for_lisp_string_replacement_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
    replacement: &LispString,
) -> Result<TextChange, Flow> {
    text_change_for_replacement_in_manager(
        buffers,
        buffer_id,
        byte_range,
        lisp_string_text_extent(replacement),
    )
}

pub(crate) fn text_change_for_deletion_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<TextChange, Flow> {
    let old_range = buffer_edit_range_for_byte_range_in_manager(buffers, buffer_id, byte_range)?;
    Ok(TextChange::deletion(old_range))
}

pub(crate) fn text_change_for_unchanged_extent_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<TextChange, Flow> {
    let old_range = buffer_edit_range_for_byte_range_in_manager(buffers, buffer_id, byte_range)?;
    Ok(TextChange::unchanged_extent(old_range))
}

// ---------------------------------------------------------------------------
// Buffer modification hooks — GNU Emacs signal_before_change / signal_after_change
// ---------------------------------------------------------------------------

/// Visible (buffer-local aware) value of a hook variable is non-nil.
fn hook_symbol_value_truthy(
    ctx: &crate::emacs_core::eval::Context,
    sym: crate::emacs_core::intern::SymId,
) -> bool {
    let sym = crate::emacs_core::hook_runtime::hook_symbol_by_id(ctx, sym);
    crate::emacs_core::hook_runtime::hook_value_by_id(ctx, sym).is_some_and(|v| v.is_truthy())
}

/// Check whether `inhibit-modification-hooks` is non-nil.
pub(crate) fn inhibit_modification_hooks(ctx: &crate::emacs_core::eval::Context) -> bool {
    let sym = crate::emacs_core::hook_runtime::hook_symbol_by_id(
        ctx,
        inhibit_modification_hooks_symbol(),
    );
    crate::emacs_core::hook_runtime::hook_value_by_id(ctx, sym).is_some_and(|v| v.is_truthy())
}

fn run_named_hook_reset_on_error(
    ctx: &mut crate::emacs_core::eval::Context,
    hook_name: crate::emacs_core::intern::SymId,
    hook_args: &[Value],
) -> Result<(), Flow> {
    let hook_sym = crate::emacs_core::hook_runtime::hook_symbol_by_id(ctx, hook_name);
    let hook_value =
        crate::emacs_core::hook_runtime::hook_value_by_id(ctx, hook_sym).unwrap_or(Value::NIL);
    if hook_value.is_nil() {
        return Ok(());
    }
    match crate::emacs_core::hook_runtime::run_hook_value(
        ctx, hook_sym, hook_value, hook_args, true,
    ) {
        Ok(_) => Ok(()),
        Err(flow) => {
            let _ = ctx.try_set_runtime_binding_by_id(hook_sym, Value::NIL);
            Err(flow)
        }
    }
}

fn run_named_hook_without_reset(
    ctx: &mut crate::emacs_core::eval::Context,
    hook_name: crate::emacs_core::intern::SymId,
    hook_args: &[Value],
) -> Result<(), Flow> {
    let hook_sym = crate::emacs_core::hook_runtime::hook_symbol_by_id(ctx, hook_name);
    let hook_value =
        crate::emacs_core::hook_runtime::hook_value_by_id(ctx, hook_sym).unwrap_or(Value::NIL);
    if hook_value.is_nil() {
        return Ok(());
    }
    let _ = crate::emacs_core::hook_runtime::run_hook_value(
        ctx, hook_sym, hook_value, hook_args, true,
    )?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BufferChangeKind {
    Characters,
    PropertiesOnly,
}

/// GNU `signal_before_change(beg, end)` plus an explicit distinction between
/// character input consumed by Tree-sitter and property-only modifications.
/// `byte_range` is 0-based Emacs bytes and is converted to 1-based character
/// positions for Lisp hooks.
fn signal_before_change_with_kind(
    ctx: &mut crate::emacs_core::eval::Context,
    byte_range: EmacsByteRange,
    kind: BufferChangeKind,
) -> Result<(), Flow> {
    // GNU `prepare_to_modify_buffer_1` (insdel.c): the *first* action is
    // `Fbarf_if_buffer_read_only`, which signals `buffer-read-only` when the
    // buffer's `read-only' flag is set (and `inhibit-read-only' is nil) BEFORE
    // running `verify_interval_modification' or `signal_before_change' (and
    // hence before `before-change-functions').  Performing the buffer-wide
    // read-only barf here -- the central modification chokepoint -- means a
    // rejected modification of a read-only buffer never fires
    // `before-change-functions', matching GNU.  Previously each insert/delete
    // primitive ran this function (and thus `before-change-functions') first
    // and only barfed afterwards, so a rejected insert double-counted the
    // hook.
    ensure_current_buffer_writable_in_state(&ctx.obarray, &[], &ctx.buffers)?;

    // GNU `prepare_to_modify_buffer` -> `verify_interval_modification`: enforce
    // the `read-only` text property before any modification. This is the central
    // modification chokepoint (every insert/delete/replace/case/abbrev/indent
    // primitive routes through `signal_before_text_change`), so checking here
    // matches GNU's single enforcement point. It runs regardless of
    // `inhibit-modification-hooks` (read-only is gated only by
    // `inhibit-read-only`). For an insertion (empty range) the stickiness of the
    // adjacent characters' `read-only` decides; for a range modification any
    // read-only interval in the range signals.
    if let Some(current_id) = ctx.buffers.current_buffer_id() {
        if byte_range.is_empty() {
            crate::emacs_core::textprop::verify_text_read_only_for_insert_in_state(
                &ctx.obarray,
                &ctx.buffers,
                current_id,
                byte_range.start(),
            )?;
        } else {
            crate::emacs_core::textprop::verify_text_read_only_emacs_byte_range_in_state(
                &ctx.obarray,
                &ctx.buffers,
                current_id,
                byte_range,
            )?;
        }
    }

    if let Some(current_id) = ctx.buffers.current_buffer_id() {
        let undo_enabled = ctx
            .buffers
            .get(current_id)
            .is_some_and(|buf| !buf.get_undo_list().is_t());
        let undoable_change = undo_auto_undoable_change_symbol();
        if undo_enabled && ctx.obarray.fboundp_id(undoable_change) {
            ctx.apply(Value::from_sym_id(undoable_change), vec![])?;
        }
    }

    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Ok(());
    };
    let beg = byte_range.start();
    let end = byte_range.end();

    if kind == BufferChangeKind::Characters
        && ctx.treesit.has_editable_tree(current_id)
        && let Some(buf) = ctx.buffers.get(current_id)
    {
        ctx.treesit
            .begin_buffer_edit(current_id, buf, EmacsByteRange::ordered(beg, end));
    }

    if inhibit_modification_hooks(ctx) {
        return Ok(());
    }

    // GNU `prepare_to_modify_buffer_1` locks a clean file-visiting base buffer
    // at this exact chokepoint, before first-change-hook and
    // before-change-functions.  Text edits already converge here, so the lock
    // transition remains complete without being duplicated across producers.
    super::filelock::lock_current_buffer_before_change(ctx)?;

    crate::emacs_core::textprop::prepare_interval_modification_for_change(
        ctx, current_id, beg, end,
    )?;

    // Quiet fast path: when nothing can run under the bind — no
    // first-change hook due, `before-change-functions` nil, no overlays —
    // the `inhibit-modification-hooks` binding is unobservable. GNU binds
    // unconditionally (insdel.c signal_before_change), but its bind is a C
    // specpdl push; ours was ~590 Ir of bind+unbind per modification.
    {
        let first_change_due = ctx
            .buffers
            .get(current_id)
            .is_some_and(|buf| buf.modified_state_value().is_nil())
            && hook_symbol_value_truthy(ctx, first_change_hook_symbol());
        if !first_change_due
            && !hook_symbol_value_truthy(ctx, before_change_functions_symbol())
            && !buffer_has_overlays(ctx, current_id)
        {
            ctx.last_overlay_modification_hooks = Vec::new();
            return Ok(());
        }
    }

    // Convert byte positions to 1-based character positions.
    let (lisp_beg, lisp_end) = {
        let Some(buf) = ctx.buffers.get(current_id) else {
            return Ok(());
        };
        let beg_char = buf
            .emacs_byte_pos_to_lisp_char_pos(byte_range.start())
            .as_i64();
        let end_char = buf
            .emacs_byte_pos_to_lisp_char_pos(byte_range.end())
            .as_i64();
        (beg_char, end_char)
    };

    let hook_args = vec![Value::fixnum(lisp_beg), Value::fixnum(lisp_end)];
    let run_first_change = ctx
        .buffers
        .get(current_id)
        .is_some_and(|buf| buf.modified_state_value().is_nil());
    let specpdl_count = ctx.specpdl.len();
    ctx.try_specbind_or_unwind_to(specpdl_count, inhibit_modification_hooks_symbol(), Value::T)?;
    let result = (|| -> Result<(), Flow> {
        if run_first_change {
            run_named_hook_without_reset(ctx, first_change_hook_symbol(), &[])?;
        }
        run_named_hook_reset_on_error(ctx, before_change_functions_symbol(), &hook_args)?;

        ctx.last_overlay_modification_hooks = collect_overlay_change_hooks(
            ctx,
            byte_range.start().get(),
            byte_range.end().get(),
            byte_range.is_empty(),
        );
        run_recorded_overlay_change_hooks(ctx, Value::NIL, lisp_beg, lisp_end, None)?;

        Ok(())
    })();
    ctx.unbind_to_with_result(specpdl_count, result.map(|()| Value::NIL))
        .map(|_| ())
}

pub(crate) fn signal_before_text_change(
    ctx: &mut crate::emacs_core::eval::Context,
    change: TextChange,
) -> Result<(), Flow> {
    signal_before_change_with_kind(
        ctx,
        change.before_byte_range(),
        BufferChangeKind::Characters,
    )?;
    deactivate_mark_after_preparing_change(ctx);
    Ok(())
}

/// The before-change signal for an insertion, which needs a POSITION and not a
/// measured change.
///
/// GNU's insertion core signals `prepare_to_modify_buffer (PT, PT, NULL)` --
/// `insert_from_string_1` (src/insdel.c:1043), `insert_from_buffer_1`
/// (:1287), `insert_1_both` (:906).  Both ends are `PT`, so the range is
/// empty: the text about to be inserted is deliberately not part of what the
/// hook is told about, which is exactly why GNU is free to read that text
/// afterwards (`copy_text` at :1053, `string_intervals` at :1093).
///
/// Taking `EmacsBytePos` rather than a `TextChange` is the load-bearing part.
/// A `TextChange` carries the NEW extent, so a caller had to measure -- and in
/// practice fully materialize -- the inserted text before it could signal.
/// This signature removes that obligation from the type, so the pre-hook
/// snapshot cannot be reintroduced by accident.  See DIVERGENCES.md 164.
pub(crate) fn signal_before_insertion_at_emacs_byte_pos(
    ctx: &mut crate::emacs_core::eval::Context,
    byte_pos: EmacsBytePos,
) -> Result<(), Flow> {
    signal_before_change_with_kind(
        ctx,
        EmacsByteRange::from_start_len(byte_pos, EmacsByteLen::ZERO),
        BufferChangeKind::Characters,
    )?;
    deactivate_mark_after_preparing_change(ctx);
    Ok(())
}

/// Run GNU's modification-hook protocol for a text-property-only change.
/// Tree-sitter consumes characters, not properties, so this deliberately does
/// not create an incremental parser edit.
pub(crate) fn signal_before_property_change(
    ctx: &mut crate::emacs_core::eval::Context,
    change: TextChange,
) -> Result<(), Flow> {
    signal_before_change_with_kind(
        ctx,
        change.before_byte_range(),
        BufferChangeKind::PropertiesOnly,
    )?;
    deactivate_mark_after_preparing_change(ctx);
    Ok(())
}

fn deactivate_mark_after_preparing_change(ctx: &mut crate::emacs_core::eval::Context) {
    // GNU `prepare_to_modify_buffer_1` (insdel.c) unconditionally runs
    // `Fset (Qdeactivate_mark, Qt)` after signaling before-change. Because
    // `deactivate-mark` is buffer-local-when-set, this creates a buffer-local
    // binding on the modified buffer (so it appears in buffer-local-variables).
    //
    // Every modification after the first in a command finds that binding
    // already `t`, and `Fset` of the same value onto it changes nothing unless a
    // variable watcher is trapped on the symbol. Skip exactly that no-op: the
    // general store walks the forwarding / localization / specpdl machinery and
    // was the single most expensive step of a text-property put.
    let sym = deactivate_mark_symbol();
    if deactivate_mark_set_is_noop(ctx, sym) {
        return;
    }
    let _ = ctx.try_set_runtime_binding_by_id(sym, Value::T);
}

/// True when `(set 'deactivate-mark t)` would change no binding and notify no
/// watcher: the symbol is untrapped and the binding the store would hit — the
/// current buffer's local one for a localized symbol, the value cell otherwise —
/// already holds `t`.
fn deactivate_mark_set_is_noop(
    ctx: &crate::emacs_core::eval::Context,
    sym: crate::emacs_core::intern::SymId,
) -> bool {
    use crate::emacs_core::symbol::{SymbolRedirect, SymbolTrappedWrite};
    let Some(symbol) = ctx.obarray.get_by_id(sym) else {
        return false;
    };
    if symbol.flags.trapped_write() != SymbolTrappedWrite::Untrapped {
        return false;
    }
    match symbol.redirect() {
        SymbolRedirect::Plainval => ctx.obarray.symbol_value_id(sym).copied() == Some(Value::T),
        SymbolRedirect::Localized => {
            ctx.buffers
                .current_buffer()
                .and_then(|buf| buf.get_buffer_local_binding_by_sym_id_gated(sym, true))
                .and_then(|binding| binding.as_value())
                == Some(Value::T)
        }
        _ => false,
    }
}

/// GNU `signal_after_change(beg, end, old_len)` — run `after-change-functions`
/// and overlay hooks after a buffer modification.
///
/// `byte_range` is a 0-based Emacs-byte range in the new buffer state.
/// `old_len` is the character length of the old text that was replaced.
pub(crate) fn signal_after_change(
    ctx: &mut crate::emacs_core::eval::Context,
    byte_range: EmacsByteRange,
    old_len: CharLen,
) -> Result<(), Flow> {
    signal_after_change_with_kind(ctx, byte_range, old_len, BufferChangeKind::Characters)
}

fn signal_after_change_with_kind(
    ctx: &mut crate::emacs_core::eval::Context,
    byte_range: EmacsByteRange,
    old_len: CharLen,
    kind: BufferChangeKind,
) -> Result<(), Flow> {
    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Ok(());
    };

    // GNU window positions are markers, so their Lisp-visible values already
    // reflect this edit before after-change hooks run.  Neomacs keeps typed
    // position caches beside the authoritative markers for layout; refresh all
    // derived caches at this common post-edit boundary, including when
    // modification hooks are inhibited.
    ctx.sync_window_positions(current_id);

    // GNU `adjust_overlays_for_delete_in_buffer` queries only the deletion
    // boundary after shifting the interval tree.  Do the category-resolving
    // evaporation pass at that same boundary; enumerating the whole buffer
    // here makes every localized deletion O(number of overlays).
    if old_len.get() > 0 && buffer_has_overlays(ctx, current_id) {
        evaporate_emptied_overlays_at(ctx, current_id, byte_range.start());
    }

    if kind == BufferChangeKind::Characters {
        finish_treesit_after_buffer_change(ctx, current_id, byte_range.start(), byte_range.end());
    }

    if inhibit_modification_hooks(ctx) {
        return Ok(());
    }

    // GNU `signal_after_change` (insdel.c:2390) defers `after-change-functions`
    // to `combine-after-change-execute` when:
    //   - `combine-after-change-calls` is non-nil,
    //   - `before-change-functions` is nil (or the syntax-ppss-flush-cache
    //     special case),
    //   - the current buffer has no overlays.
    // Mirrored here so wrappers like `combine-after-change-calls` coalesce
    // multiple edits into a single after-change call as in GNU Emacs.
    if combine_after_change_calls_active(ctx) && !buffer_has_overlays(ctx, current_id) {
        // If the pending deferred list belongs to a different buffer, GNU
        // flushes it via `Fcombine_after_change_execute` before recording
        // the new change.
        let needs_flush = !ctx.combine_after_change_list.is_empty()
            && ctx.combine_after_change_buffer != Some(current_id);
        if needs_flush {
            execute_combined_after_change(ctx)?;
        }

        if let Some(buf) = ctx.buffers.get(current_id) {
            let beg_char = buf.emacs_byte_pos_to_char_pos_clamped(byte_range.start());
            let end_char = buf.emacs_byte_pos_to_char_pos_clamped(byte_range.end());
            let beg_char = beg_char.get() as i64;
            let end_char = end_char.get() as i64;
            let charpos = beg_char + 1; // 1-based, like GNU's PT/charpos.
            let lenins = end_char - beg_char;
            let lendel = old_len.get() as i64;
            let z = buf.z_lisp_char_pos().as_i64(); // 1-based Z.
            let beg_field = charpos - 1; // charpos - BEG
            let end_field = z - (charpos - lendel + lenins);
            let change = lenins - lendel;
            ctx.combine_after_change_list
                .push((beg_field, end_field, change));
            ctx.combine_after_change_buffer = Some(current_id);
        }
        return Ok(());
    }

    // Not deferring: any pending coalesced changes must run first so their
    // hooks observe the buffer state from before this new edit's after-pass.
    if !ctx.combine_after_change_list.is_empty() {
        execute_combined_after_change(ctx)?;
    }

    // Quiet fast path (twin of the one in `signal_before_change_with_kind`):
    // with `after-change-functions` nil, no recorded or live overlay hooks,
    // no interval insert hooks, and no text-property intervals to report,
    // nothing can run under the bind.
    if !hook_symbol_value_truthy(ctx, after_change_functions_symbol())
        && ctx.last_overlay_modification_hooks.is_empty()
        && ctx.interval_insert_behind_hooks.is_nil()
        && ctx.interval_insert_in_front_hooks.is_nil()
        && !buffer_has_overlays(ctx, current_id)
        && ctx
            .buffers
            .get(current_id)
            .is_some_and(|buf| buf.text_props_is_empty())
    {
        return Ok(());
    }

    // Convert byte positions to 1-based character positions.
    let (lisp_beg, lisp_end, lisp_old_len) = {
        let Some(buf) = ctx.buffers.get(current_id) else {
            return Ok(());
        };
        let beg_char = buf
            .emacs_byte_pos_to_lisp_char_pos(byte_range.start())
            .as_i64();
        let end_char = buf
            .emacs_byte_pos_to_lisp_char_pos(byte_range.end())
            .as_i64();
        (beg_char, end_char, old_len.get() as i64)
    };

    let hook_args = vec![
        Value::fixnum(lisp_beg),
        Value::fixnum(lisp_end),
        Value::fixnum(lisp_old_len),
    ];
    let saved_interval_insert_behind_hooks = ctx.interval_insert_behind_hooks;
    let saved_interval_insert_in_front_hooks = ctx.interval_insert_in_front_hooks;

    let specpdl_count = ctx.specpdl.len();
    // The saved hook lists live only in the Rust locals above while
    // after-change-functions run arbitrary Lisp; if a hook clears the
    // context fields (their previous root), a GC frees the lists before
    // they are reinstated below. GcRoot entries unwind with the specpdl,
    // so unbind_to(specpdl_count) pops them with the specbind.
    ctx.push_specpdl_root(saved_interval_insert_behind_hooks);
    ctx.push_specpdl_root(saved_interval_insert_in_front_hooks);
    ctx.try_specbind_or_unwind_to(specpdl_count, inhibit_modification_hooks_symbol(), Value::T)?;
    let result = (|| -> Result<(), Flow> {
        run_named_hook_reset_on_error(ctx, after_change_functions_symbol(), &hook_args)?;

        ctx.interval_insert_behind_hooks = saved_interval_insert_behind_hooks;
        ctx.interval_insert_in_front_hooks = saved_interval_insert_in_front_hooks;

        // --- Run overlay hooks ---
        // insert-in-front-hooks: overlays whose start == beg
        // insert-behind-hooks:   overlays whose end == beg (before insertion point)
        // modification-hooks:    overlays covering [beg, end)
        run_overlay_after_change_hooks(
            ctx,
            byte_range.start().get(),
            byte_range.end().get(),
            lisp_beg,
            lisp_end,
            lisp_old_len,
        )?;

        if lisp_old_len == 0 {
            crate::emacs_core::textprop::report_interval_modification(ctx, lisp_beg, lisp_end)?;
        }

        Ok(())
    })();
    ctx.unbind_to_with_result(specpdl_count, result.map(|()| Value::NIL))
        .map(|_| ())
}

pub(crate) fn signal_after_text_change(
    ctx: &mut crate::emacs_core::eval::Context,
    change: TextChange,
) -> Result<(), Flow> {
    signal_after_change_with_kind(
        ctx,
        change.after_byte_range(),
        change.old_char_len(),
        BufferChangeKind::Characters,
    )
}

pub(crate) fn signal_after_property_change(
    ctx: &mut crate::emacs_core::eval::Context,
    change: TextChange,
) -> Result<(), Flow> {
    signal_after_change_with_kind(
        ctx,
        change.after_byte_range(),
        change.old_char_len(),
        BufferChangeKind::PropertiesOnly,
    )
}

/// Insert process or subsystem output into BUFFER-ID as one semantic edit.
///
/// This is the ownership boundary for non-Lisp producers that already hold a
/// decoded `LispString`: select the buffer whose hooks and read-only state
/// govern the edit, signal the paired change notifications, perform the raw
/// storage mutation, and restore the caller's current buffer on every exit.
///
/// `text` is read AFTER `signal_before_text_change`, i.e. after arbitrary Lisp.
/// That is sound only because every caller passes a borrow of a local it owns
/// outright -- a freshly decoded `run.text`, a `LispString::from_utf8` -- which
/// no hook can reach, let alone collect. DIVERGENCES.md 163 §10 named this a
/// latent trap because the signature does not say so and a caller passing
/// `value.as_lisp_string()?` would compile; 164 shows what saying so looks
/// like, at the `insert` door: carry the `Value`, root it, and take the borrow
/// past the safepoint (`PendingInsert` in `emacs_core/editing/buffer/mod.rs`). Anything
/// added here that wants to pass a heap borrow must do that instead.
pub(crate) fn insert_lisp_string_with_change_hooks_in_buffer(
    ctx: &mut crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
    text: &LispString,
) -> Result<(), Flow> {
    if text.is_empty() {
        return Ok(());
    }

    let saved_current = ctx.buffers.current_buffer_id();
    let result = (|| {
        ctx.set_current_buffer_unrecorded(buffer_id)?;
        let insert_pos = ctx
            .buffers
            .get(buffer_id)
            .map(|buffer| buffer.point_emacs_byte_pos())
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
        let change = text_change_for_empty_insertion_at_emacs_byte_pos(
            &ctx.buffers,
            buffer_id,
            insert_pos,
            lisp_string_text_extent(text),
        )?;

        signal_before_text_change(ctx, change)?;
        // A before-change function may select another buffer. The mutation and
        // after-change notification still belong to the declared edit target.
        ctx.set_current_buffer_unrecorded(buffer_id)?;
        ctx.buffers
            .insert_lisp_string_into_buffer(buffer_id, text)
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
        signal_after_text_change(ctx, change)
    })();

    if let Some(saved) = saved_current {
        ctx.restore_current_buffer_if_live(saved);
    }
    result
}

/// Delete overlays that a deletion left empty and whose `evaporate` property is
/// non-nil. Unlike the low-level direct-plist evaporation in
/// `OverlayList::adjust_for_delete_emacs_byte_range`, this resolves `evaporate`
/// through the overlay's `category` symbol (GNU `overlay-get` semantics), so a
/// category-inherited evaporate flag is honored. Mirrors GNU deleting empty
/// evaporate overlays during a buffer deletion.
fn evaporate_emptied_overlays_at(
    ctx: &mut crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
    position: EmacsBytePos,
) {
    let empty: Vec<Value> = {
        let Some(buf) = ctx.buffers.get(buffer_id) else {
            return;
        };
        buf.overlays
            .overlays_in_emacs_byte_range(EmacsByteRange::new(position, position))
            .into_iter()
            .filter(|overlay| {
                let start = buf.overlays.overlay_start_emacs_byte_pos(*overlay);
                start.is_some() && start == buf.overlays.overlay_end_emacs_byte_pos(*overlay)
            })
            .collect()
    };
    let to_delete: Vec<Value> = empty
        .into_iter()
        .filter(|overlay| {
            crate::emacs_core::textprop::lookup_overlay_property(
                &ctx.obarray,
                &ctx.buffers,
                *overlay,
                Value::symbol("evaporate"),
            )
            .is_truthy()
        })
        .collect();
    for overlay in to_delete {
        ctx.buffers.delete_buffer_overlay(buffer_id, overlay);
    }
}

fn finish_treesit_after_buffer_change(
    ctx: &mut crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
    beg: EmacsBytePos,
    end: EmacsBytePos,
) {
    ctx.treesit.note_buffer_change(buffer_id, beg);
    if ctx.treesit.has_pending_edit(buffer_id)
        && let Some(buf) = ctx.buffers.get(buffer_id)
    {
        ctx.treesit.finish_buffer_edit(buffer_id, buf, end);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum BeforeChangeSpecialFunction {
    SyntaxPpssFlushCache,
}

impl BeforeChangeSpecialFunction {
    fn is_lisp_value(self, value: Value) -> bool {
        value == Value::symbol(self.name())
    }

    fn name(self) -> &'static str {
        self.into()
    }
}

/// Mirrors GNU's deferral predicate for `signal_after_change`
/// (`insdel.c:2393`). True when `combine-after-change-calls` is non-nil and
/// `before-change-functions` is either nil or the well-known
/// `(t syntax-ppss-flush-cache)` special case.
fn combine_after_change_calls_active(ctx: &crate::emacs_core::eval::Context) -> bool {
    let combine_sym = crate::emacs_core::hook_runtime::hook_symbol_by_id(
        ctx,
        combine_after_change_calls_symbol(),
    );
    let combine_val =
        crate::emacs_core::hook_runtime::hook_value_by_id(ctx, combine_sym).unwrap_or(Value::NIL);
    if combine_val.is_nil() {
        return false;
    }

    let before_sym =
        crate::emacs_core::hook_runtime::hook_symbol_by_id(ctx, before_change_functions_symbol());
    let before_val =
        crate::emacs_core::hook_runtime::hook_value_by_id(ctx, before_sym).unwrap_or(Value::NIL);
    if before_val.is_nil() {
        return true;
    }

    // GNU permits the special case `(t syntax-ppss-flush-cache)` where the
    // buffer-local list is just the global trampoline plus the cache flush.
    if before_val.is_cons() {
        let head = before_val.cons_car();
        let tail = before_val.cons_cdr();
        if head.is_t() && tail.is_cons() {
            let second = tail.cons_car();
            let rest = tail.cons_cdr();
            if rest.is_nil()
                && second.is_symbol()
                && BeforeChangeSpecialFunction::SyntaxPpssFlushCache.is_lisp_value(second)
            {
                let default_val = ctx
                    .obarray
                    .default_value_id(before_sym)
                    .copied()
                    .unwrap_or(Value::NIL);
                return default_val.is_nil();
            }
        }
    }
    false
}

fn buffer_has_overlays(
    ctx: &crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
) -> bool {
    ctx.buffers
        .get(buffer_id)
        .is_some_and(|buf| !buf.overlays.is_empty())
}

/// GNU `Fcombine_after_change_execute` (insdel.c:2475). Merges the deferred
/// per-change records into a single (begpos, lendel, lenins) triple and
/// dispatches one `signal_after_change` call.
pub(crate) fn execute_combined_after_change(
    ctx: &mut crate::emacs_core::eval::Context,
) -> Result<(), Flow> {
    if ctx.combine_after_change_list.is_empty() {
        return Ok(());
    }

    let Some(target_id) = ctx.combine_after_change_buffer else {
        ctx.combine_after_change_list.clear();
        return Ok(());
    };

    if ctx.buffers.get(target_id).is_none() {
        ctx.combine_after_change_list.clear();
        ctx.combine_after_change_buffer = None;
        return Ok(());
    }

    // GNU temporarily switches to the recording buffer.
    let saved_buffer = ctx.buffers.current_buffer_id();
    if saved_buffer != Some(target_id) {
        let _ = ctx.set_current_buffer_unrecorded(target_id);
    }

    let (begpos, endpos, change_total, list_len) = {
        let buf = match ctx.buffers.get(target_id) {
            Some(b) => b,
            None => {
                ctx.combine_after_change_list.clear();
                ctx.combine_after_change_buffer = None;
                if let Some(prev) = saved_buffer {
                    let _ = ctx.set_current_buffer_unrecorded(prev);
                }
                return Ok(());
            }
        };
        let z = buf.z_lisp_char_pos().as_i64();
        let init = z - 1;
        let mut beg = init;
        let mut end = init;
        let mut change: i64 = 0;
        for (thisbeg, thisend, thischange) in &ctx.combine_after_change_list {
            change += *thischange;
            if *thisbeg < beg {
                beg = *thisbeg;
            }
            if *thisend < end {
                end = *thisend;
            }
        }
        let begpos = 1 + beg;
        let endpos = z - end;
        (begpos, endpos, change, ctx.combine_after_change_list.len())
    };

    ctx.combine_after_change_list.clear();
    ctx.combine_after_change_buffer = None;

    let _ = list_len;

    // Convert merged 1-based char range back into byte positions for our
    // signal_after_change typed byte range.
    let byte_range = {
        let buf = ctx.buffers.get(target_id).expect("target buffer");
        EmacsByteRange::new(
            buf.char_pos_to_emacs_byte_pos_clamped(LispCharPos1::new(begpos).to_char_pos()),
            buf.char_pos_to_emacs_byte_pos_clamped(LispCharPos1::new(endpos).to_char_pos()),
        )
    };
    let old_len = CharLen::new((endpos - begpos - change_total).max(0) as usize);

    let result = signal_after_change(ctx, byte_range, old_len);

    if let Some(prev) = saved_buffer
        && prev != target_id
    {
        let _ = ctx.set_current_buffer_unrecorded(prev);
    }
    result
}

/// GNU `report_overlay_modification` (buffer.c:4119) collection step.
/// Walks overlays touching the change region and returns (hook_function,
/// overlay) pairs in the order GNU records them: per-overlay
/// `insert-in-front-hooks` (insertions only), then `insert-behind-hooks`
/// (insertions only), then `modification-hooks`.
///
/// `insertion` mirrors GNU's local: true when this change is a pure
/// insertion (start == end before, or old_len == 0 after).
fn collect_overlay_change_hooks(
    ctx: &crate::emacs_core::eval::Context,
    beg: usize,
    end: usize,
    insertion: bool,
) -> Vec<OverlayModificationHook> {
    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Vec::new();
    };
    let Some(buf) = ctx.buffers.get(current_id) else {
        return Vec::new();
    };

    // GNU widens the search by one on each side for insertions so that
    // overlays whose endpoints touch the insertion point are included.
    let search_beg = if insertion && beg > 0 { beg - 1 } else { beg };
    let search_end = if insertion { end + 1 } else { end.max(beg) };
    let overlay_ids = buf
        .overlays
        .overlays_in_emacs_byte_range(EmacsByteRange::new(
            EmacsBytePos::new(search_beg),
            EmacsBytePos::new(search_end),
        ));
    let beg_pos = EmacsBytePos::new(beg);
    let end_pos = EmacsBytePos::new(end);

    let mut result = Vec::new();
    for ov_id in overlay_ids {
        let ov_start = match buf.overlays.overlay_start_emacs_byte_pos(ov_id) {
            Some(s) => s,
            None => continue,
        };
        let ov_end = match buf.overlays.overlay_end_emacs_byte_pos(ov_id) {
            Some(e) => e,
            None => continue,
        };

        // GNU `report_overlay_modification` reads these hook lists with
        // `Foverlay_get`, which resolves through `lookup_char_property` -- so a
        // `category' overlay property whose value is a symbol contributes the
        // symbol's `insert-in-front-hooks' / `insert-behind-hooks' /
        // `modification-hooks' property.  Use the same category-resolving
        // lookup here instead of a plain plist read.
        let overlay_hook = |prop: &str| -> Option<Value> {
            let value = crate::emacs_core::textprop::lookup_overlay_property(
                &ctx.obarray,
                &ctx.buffers,
                ov_id,
                Value::symbol(prop),
            );
            (!value.is_nil()).then_some(value)
        };

        if insertion
            && (beg_pos == ov_start || end_pos == ov_start)
            && let Some(hook_val) = overlay_hook("insert-in-front-hooks")
        {
            result.push(OverlayModificationHook {
                hook_list: hook_val,
                overlay: ov_id,
            });
        }
        if insertion
            && (beg_pos == ov_end || end_pos == ov_end)
            && let Some(hook_val) = overlay_hook("insert-behind-hooks")
        {
            result.push(OverlayModificationHook {
                hook_list: hook_val,
                overlay: ov_id,
            });
        }
        // GNU intersection test (open interval):
        //   end > obegin && begin < oend
        if end_pos > ov_start
            && beg_pos < ov_end
            && let Some(hook_val) = overlay_hook("modification-hooks")
        {
            result.push(OverlayModificationHook {
                hook_list: hook_val,
                overlay: ov_id,
            });
        }
    }
    result
}

/// Run overlay `insert-in-front-hooks`, `insert-behind-hooks`, and
/// `modification-hooks` after a change.  Mirrors GNU
/// `report_overlay_modification` (buffer.c:4119) for the AFTER phase.
fn run_overlay_after_change_hooks(
    ctx: &mut crate::emacs_core::eval::Context,
    beg: usize,
    end: usize,
    lisp_beg: i64,
    lisp_end: i64,
    lisp_old_len: i64,
) -> Result<(), Flow> {
    let _ = (beg, end);
    // GNU passes `t` for AFTER in the after-change phase and replays the
    // hook-list/overlay pairs recorded by the before-change scan.
    run_recorded_overlay_change_hooks(ctx, Value::T, lisp_beg, lisp_end, Some(lisp_old_len))
}

fn run_recorded_overlay_change_hooks(
    ctx: &mut crate::emacs_core::eval::Context,
    after_flag: Value,
    lisp_beg: i64,
    lisp_end: i64,
    lisp_old_len: Option<i64>,
) -> Result<(), Flow> {
    let hooks = ctx.last_overlay_modification_hooks.clone();
    if hooks.is_empty() {
        return Ok(());
    }
    let roots = ctx.save_specpdl_roots();
    for hook in &hooks {
        ctx.push_specpdl_root(hook.hook_list);
        ctx.push_specpdl_root(hook.overlay);
    }
    let apply_result = (|| -> Result<(), Flow> {
        for hook in &hooks {
            if !overlay_belongs_to_current_buffer(ctx, hook.overlay) {
                continue;
            }
            call_overlay_hook_list(
                ctx,
                hook.hook_list,
                hook.overlay,
                after_flag,
                lisp_beg,
                lisp_end,
                lisp_old_len,
            )?;
        }
        Ok(())
    })();
    ctx.restore_specpdl_roots(roots);
    apply_result
}

fn overlay_belongs_to_current_buffer(
    ctx: &crate::emacs_core::eval::Context,
    overlay: Value,
) -> bool {
    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return false;
    };
    overlay
        .as_overlay_data()
        .is_some_and(|data| data.buffer == Some(current_id))
}

fn call_overlay_hook_list(
    ctx: &mut crate::emacs_core::eval::Context,
    hook_list: Value,
    overlay: Value,
    after_flag: Value,
    lisp_beg: i64,
    lisp_end: i64,
    lisp_old_len: Option<i64>,
) -> Result<(), Flow> {
    // A hook can unlink its own chain from the overlay plist (the
    // one-shot-hook idiom: overlay-put with the fn deleted) and trigger GC,
    // freeing the conses this walk still reads. Root the moving cursor in a
    // single updatable slot: marking is transitive, so the remaining chain
    // survives exactly as GNU's conservatively-scanned tail local does.
    let root_scope = ctx.save_specpdl_roots();
    let cursor_slot = ctx.push_specpdl_root_slot(Value::NIL);
    let result = (|| -> Result<(), Flow> {
        let mut cursor = hook_list;
        while cursor.is_cons() {
            ctx.set_specpdl_root_slot(&cursor_slot, cursor);
            let func = cursor.cons_car();
            let mut args = vec![
                overlay,
                after_flag,
                Value::fixnum(lisp_beg),
                Value::fixnum(lisp_end),
            ];
            if let Some(old_len) = lisp_old_len {
                args.push(Value::fixnum(old_len));
            }
            ctx.apply(func, args)?;
            cursor = cursor.cons_cdr();
        }
        Ok(())
    })();
    ctx.restore_specpdl_roots(root_scope);
    result
}

fn expect_integer_or_marker_in_buffers(
    buffers: &BufferManager,
    value: &Value,
) -> Result<LispCharPos1, Flow> {
    Ok(LispCharPos1::new(
        super::position::fix_position_with_buffers(buffers, value)?,
    ))
}

pub(crate) fn current_buffer_accessible_char_region_in_buffers(
    buffers: &BufferManager,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<Option<EmacsByteRange>, Flow> {
    let Some(buf) = buffers.current_buffer() else {
        return Ok(None);
    };

    let start = expect_integer_or_marker_in_buffers(buffers, start_arg)?;
    let end = expect_integer_or_marker_in_buffers(buffers, end_arg)?;
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if start.as_i64() < point_min
        || start.as_i64() > point_max
        || end.as_i64() < point_min
        || end.as_i64() > point_max
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::make_buffer(buf.id), *start_arg, *end_arg],
        ));
    }

    let (from, to) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    Ok(Some(EmacsByteRange::new(
        buf.lisp_pos_to_accessible_emacs_byte_pos(from),
        buf.lisp_pos_to_accessible_emacs_byte_pos(to),
    )))
}

/// [`current_buffer_accessible_char_region_in_buffers`] that also reports the
/// region's CHARACTER count.
///
/// The resolver clamps and orders two character positions, so the count is
/// `to - from` -- arithmetic the caller would otherwise recover by walking
/// every byte of the copy.  GNU takes both lengths the same way
/// (`editfns.c:1608`).
pub(crate) fn current_buffer_accessible_char_region_with_chars(
    buffers: &BufferManager,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<Option<(EmacsByteRange, usize)>, Flow> {
    let Some(buf) = buffers.current_buffer() else {
        return Ok(None);
    };
    let start = expect_integer_or_marker_in_buffers(buffers, start_arg)?;
    let end = expect_integer_or_marker_in_buffers(buffers, end_arg)?;
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if start.as_i64() < point_min
        || start.as_i64() > point_max
        || end.as_i64() < point_min
        || end.as_i64() > point_max
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::make_buffer(buf.id), *start_arg, *end_arg],
        ));
    }
    let (from, to) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let chars = (to.as_i64() - from.as_i64()).max(0) as usize;
    Ok(Some((
        EmacsByteRange::new(
            buf.lisp_pos_to_accessible_emacs_byte_pos(from),
            buf.lisp_pos_to_accessible_emacs_byte_pos(to),
        ),
        chars,
    )))
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins (need &mut Context for buffer access)
// ---------------------------------------------------------------------------

/// Collect the insertable text from a mixed list of strings and characters.
///
/// Returns raw Emacs-internal-encoding bytes. String args contribute their
/// `LispString.as_bytes()` directly (promoted via overlong C0/C1 for
/// unibyte 0x80..0xFF bytes). Character args are encoded via
/// `emacs_char::char_string`. The caller is responsible for wrapping the
/// result into a `LispString` before handing it to buffer insertion.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn collect_insert_text(_name: &str, args: &[Value]) -> Result<Vec<u8>, Flow> {
    use crate::emacs_core::emacs_char;
    let mut bytes: Vec<u8> = Vec::new();
    for arg in args {
        match arg.kind() {
            ValueKind::String => {
                let ls = arg.as_lisp_string().ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("stringp"), *arg],
                    )
                })?;
                if ls.is_multibyte() {
                    bytes.extend_from_slice(ls.as_bytes());
                } else {
                    // Unibyte string: each byte is a raw byte value. Promote
                    // 0x80..0xFF to overlong C0/C1 Emacs encoding so the
                    // concatenated result is a well-formed multibyte byte
                    // stream.
                    for &b in ls.as_bytes() {
                        if b < 0x80 {
                            bytes.push(b);
                        } else {
                            bytes.push(0xC0 | ((b >> 6) & 0x01));
                            bytes.push(0x80 | (b & 0x3F));
                        }
                    }
                }
                continue;
            }
            ValueKind::Fixnum(_) => {
                let code = super::builtins::expect_character_code(arg)? as u32;
                if code > emacs_char::MAX_CHAR {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("characterp"), *arg],
                    ));
                }
                let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
                let len = emacs_char::char_string(code, &mut buf);
                bytes.extend_from_slice(&buf[..len]);
                continue;
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("char-or-string-p"), *arg],
                ));
            }
        }
    }
    Ok(bytes)
}

/// `(insert-before-markers &rest ARGS)` — insert at point, advancing ALL
/// markers at that position past the inserted text (regardless of their
/// InsertionType).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_insert_before_markers(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::builtins::builtin_insert_before_markers(ctx, args)
}

/// `(delete-char N &optional KILLFLAG)` — delete N characters forward.
pub(crate) fn builtin_delete_char(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("delete-char", &args, 1)?;
    expect_max_args("delete-char", &args, 2)?;
    let n = expect_integer("delete-char", &args[0])?;
    let killflag = args.get(1).is_some_and(|v| v.is_truthy());
    ensure_current_buffer_writable_in_state(&ctx.obarray, &[], &ctx.buffers)?;
    if n.unsigned_abs() < 2 {
        ctx.apply(Value::symbol("undo-auto-amalgamate"), vec![])?;
    }
    // GNU `Fdelete_char` (cmds.c:221) dispatches to `kill-forward-chars`
    // when KILLFLAG is non-nil, saving the deleted text in the kill ring.
    if killflag {
        return ctx.apply(Value::symbol("kill-forward-chars"), vec![args[0]]);
    }
    if let Some(current_id) = ctx.buffers.current_buffer_id() {
        let Some(byte_range) = ({
            let Some(buf) = ctx.buffers.get(current_id) else {
                return Ok(Value::NIL);
            };
            let accessible = buf.accessible_emacs_byte_region();
            let pt = buf.point_emacs_byte_pos();
            if n > 0 {
                // Delete N characters forward from point.
                let mut end = pt;
                for _ in 0..n {
                    if end >= accessible.end() {
                        return Err(signal(LispCondition::EndOfBuffer, vec![]));
                    }
                    match buf.char_after_emacs_byte_len(end) {
                        Some(char_len) => end = end.add_len(char_len),
                        None => {
                            return Err(signal(LispCondition::EndOfBuffer, vec![]));
                        }
                    }
                }
                Some(EmacsByteRange::new(pt, end))
            } else if n < 0 {
                // Delete |N| characters backward from point.
                let mut start = pt;
                for _ in 0..(-n) {
                    if start <= accessible.start() {
                        return Err(signal(LispCondition::BeginningOfBuffer, vec![]));
                    }
                    match buf.char_before_emacs_byte_len(start) {
                        Some(char_len) => start = start.saturating_sub_len(char_len),
                        None => {
                            return Err(signal(LispCondition::BeginningOfBuffer, vec![]));
                        }
                    }
                }
                Some(EmacsByteRange::new(start, pt))
            } else {
                None
            }
        }) else {
            return Ok(Value::NIL);
        };
        let start = byte_range.start().get();
        let end = byte_range.end().get();
        let delete_range =
            buffer_edit_range_for_byte_range_in_manager(&ctx.buffers, current_id, byte_range)?;
        crate::emacs_core::textprop::verify_text_read_only_in_state(
            &ctx.obarray,
            &ctx.buffers,
            current_id,
            start,
            end,
        )?;
        let change = TextChange::deletion(delete_range);
        signal_before_text_change(ctx, change)?;
        let _ = ctx
            .buffers
            .delete_buffer_measured_region(current_id, delete_range);
        signal_after_text_change(ctx, change)?;
    }
    Ok(Value::NIL)
}

/// `(delete-region START END)` — delete text in the accessible current buffer.
pub(crate) fn builtin_delete_region(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-region", &args, 2)?;
    let Some(byte_range) =
        current_buffer_accessible_char_region_in_buffers(&ctx.buffers, &args[0], &args[1])?
    else {
        return Ok(Value::NIL);
    };
    let start_byte = byte_range.start().get();
    let end_byte = byte_range.end().get();
    if start_byte == end_byte {
        return Ok(Value::NIL);
    }

    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };
    let read_only = ctx
        .buffers
        .get(current_id)
        .is_some_and(|buf| buffer_read_only_active_in_state(&ctx.obarray, &[], buf));
    if read_only {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }
    crate::emacs_core::textprop::verify_text_read_only_in_state(
        &ctx.obarray,
        &ctx.buffers,
        current_id,
        start_byte,
        end_byte,
    )?;

    let delete_range =
        buffer_edit_range_for_byte_range_in_manager(&ctx.buffers, current_id, byte_range)?;
    let change = TextChange::deletion(delete_range);
    signal_before_text_change(ctx, change)?;
    let _ = ctx
        .buffers
        .delete_buffer_measured_region(current_id, delete_range);
    signal_after_text_change(ctx, change)?;
    Ok(Value::NIL)
}

/// `(delete-and-extract-region START END)` — delete text and return it.
pub(crate) fn builtin_delete_and_extract_region(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-and-extract-region", &args, 2)?;
    let Some(byte_range) =
        current_buffer_accessible_char_region_in_buffers(&ctx.buffers, &args[0], &args[1])?
    else {
        return Ok(Value::string(""));
    };
    let start_byte = byte_range.start().get();
    let end_byte = byte_range.end().get();
    if start_byte == end_byte {
        return Ok(Value::string(""));
    }

    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Ok(Value::string(""));
    };
    {
        let Some(buf) = ctx.buffers.get(current_id) else {
            return Ok(Value::string(""));
        };
        if buffer_read_only_active_in_state(&ctx.obarray, &[], buf) {
            return Err(signal(
                LispCondition::BufferReadOnly,
                vec![Value::make_buffer(current_id)],
            ));
        }
    }
    crate::emacs_core::textprop::verify_text_read_only_in_state(
        &ctx.obarray,
        &ctx.buffers,
        current_id,
        start_byte,
        end_byte,
    )?;

    let delete_range =
        buffer_edit_range_for_byte_range_in_manager(&ctx.buffers, current_id, byte_range)?;
    let change = TextChange::deletion(delete_range);
    signal_before_text_change(ctx, change)?;
    // GNU `del_range_1 (from, to, true, true)`: the string is made by
    // `del_range_2` AFTER `prepare_to_modify_buffer` ran the before-change
    // hooks, and it is the same string `record_delete` gets -- one
    // `make_buffer_string_both`, not a substring plus a second copy for undo.
    let deleted = ctx
        .buffers
        .delete_and_extract_buffer_measured_region(current_id, delete_range)
        .map(Value::heap_string)
        .unwrap_or_else(|| Value::string(""));
    signal_after_text_change(ctx, change)?;
    Ok(deleted)
}

/// `(erase-buffer)` — delete all text and remove any narrowing restriction.
pub(crate) fn builtin_erase_buffer(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("erase-buffer", &args, 0)?;
    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };
    let byte_range = ctx
        .buffers
        .get(current_id)
        .map(|buf| buf.full_emacs_byte_range())
        .unwrap_or(EmacsByteRange::EMPTY);
    let delete_range =
        buffer_edit_range_for_byte_range_in_manager(&ctx.buffers, current_id, byte_range)?;
    let change = TextChange::deletion(delete_range);
    if !byte_range.is_empty() {
        signal_before_text_change(ctx, change)?;
    }
    erase_buffer_impl(&ctx.obarray, &[], &mut ctx.buffers, vec![])?;
    if !byte_range.is_empty() {
        signal_after_text_change(ctx, change)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn erase_buffer_impl(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("erase-buffer", &args, 0)?;
    let Some(current_id) = buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };

    let should_signal_read_only = buffers.get(current_id).is_some_and(|buf| {
        !buf.is_text_empty() && buffer_read_only_active_in_state(obarray, dynamic, buf)
    });
    if should_signal_read_only {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }

    let _ = buffers.clear_buffer_labeled_restrictions(current_id);
    let Some(byte_range) = buffers.full_buffer_emacs_byte_range(current_id) else {
        return Ok(Value::NIL);
    };
    let _ = buffers.restore_buffer_emacs_byte_restriction(current_id, byte_range);
    if !byte_range.is_empty() {
        let delete_range =
            buffer_edit_range_for_byte_range_in_manager(buffers, current_id, byte_range)?;
        let _ = buffers.delete_buffer_measured_region(current_id, delete_range);
    }
    let _ = buffers.goto_buffer_emacs_byte_pos(current_id, EmacsBytePos::new(0));
    Ok(Value::NIL)
}

/// `(buffer-substring-no-properties START END)` — same as buffer-substring
/// (text properties not yet implemented at the Lisp value level).
pub(crate) fn builtin_buffer_substring_no_properties(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-substring-no-properties", &args, 2)?;
    // No Lisp runs between resolving the region and copying it (the
    // fontify hook belongs to `buffer-substring`, not this one), so the
    // character count the resolver computed still describes the bytes.
    let Some((byte_range, chars)) =
        current_buffer_accessible_char_region_with_chars(&ctx.buffers, &args[0], &args[1])?
    else {
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };
    let Some(buf) = ctx.buffers.current_buffer() else {
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };
    let mut bytes = Vec::new();
    buf.copy_emacs_byte_range_to(byte_range, &mut bytes);
    Ok(Value::heap_string(if buf.get_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes_with_chars(bytes, chars)
    } else {
        crate::emacs_core::builtins::lisp_string_from_buffer_bytes(bytes, false)
    }))
}

/// `(following-char)` — return character after point (0 if at end).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_following_char(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("following-char", &args, 0)?;
    following_char_value(ctx)
}

pub(crate) fn builtin_following_char_0(ctx: &mut crate::emacs_core::eval::Context) -> EvalResult {
    following_char_value(ctx)
}

fn following_char_value(ctx: &crate::emacs_core::eval::Context) -> EvalResult {
    match ctx.buffers.current_buffer() {
        Some(buf) => {
            let accessible = buf.accessible_emacs_byte_region();
            let point = buf.point_emacs_byte_pos();
            match (point < accessible.end())
                .then(|| buf.char_code_after_emacs_byte_pos(point))
                .flatten()
            {
                Some(code) => Ok(Value::fixnum(code as i64)),
                None => Ok(Value::fixnum(0)),
            }
        }
        None => Ok(Value::fixnum(0)),
    }
}

/// `(preceding-char)` — return character before point (0 if at beginning).
pub(crate) fn builtin_preceding_char(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("preceding-char", &args, 0)?;
    match ctx.buffers.current_buffer() {
        Some(buf) => {
            let accessible = buf.accessible_emacs_byte_region();
            let point = buf.point_emacs_byte_pos();
            match (point > accessible.start())
                .then(|| buf.char_code_before_emacs_byte_pos(point))
                .flatten()
            {
                Some(code) => Ok(Value::fixnum(code as i64)),
                None => Ok(Value::fixnum(0)),
            }
        }
        None => Ok(Value::fixnum(0)),
    }
}

// ---------------------------------------------------------------------------
// Pure builtins (no evaluator needed)
// ---------------------------------------------------------------------------

/// `(user-uid)` — return effective user ID.
pub(crate) fn builtin_user_uid(args: Vec<Value>) -> EvalResult {
    expect_args("user-uid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_user_id(
        CredentialScope::Effective,
    ))))
}

/// `(file-user-uid)` — return the UID used for file ownership.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_file_user_uid(args: Vec<Value>) -> EvalResult {
    expect_args("file-user-uid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_user_id(
        CredentialScope::Effective,
    ))))
}

/// `(user-real-uid)` — return real user ID.
pub(crate) fn builtin_user_real_uid(args: Vec<Value>) -> EvalResult {
    expect_args("user-real-uid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_user_id(
        CredentialScope::Real,
    ))))
}

/// `(group-gid)` — return the effective group ID.
pub(crate) fn builtin_group_gid(args: Vec<Value>) -> EvalResult {
    expect_args("group-gid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_group_id(
        CredentialScope::Effective,
    ))))
}

/// `(file-group-gid)` — return the GID used for file ownership.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_file_group_gid(args: Vec<Value>) -> EvalResult {
    expect_args("file-group-gid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_group_id(
        CredentialScope::Effective,
    ))))
}

/// `(group-real-gid)` — return the real group ID.
pub(crate) fn builtin_group_real_gid(args: Vec<Value>) -> EvalResult {
    expect_args("group-real-gid", &args, 0)?;
    Ok(Value::fixnum(i64::from(process_group_id(
        CredentialScope::Real,
    ))))
}

/// `(group-name GID)` — return the group name for numeric GID.
pub(crate) fn builtin_group_name(args: Vec<Value>) -> EvalResult {
    expect_args("group-name", &args, 1)?;
    let gid = match args[0].kind() {
        ValueKind::Fixnum(n) => n,
        _ => {
            return Err(signal(
                "error",
                vec![Value::string("Invalid GID specification")],
            ));
        }
    };
    if gid < 0 || gid > u32::MAX as i64 {
        return Err(signal(
            "error",
            vec![Value::string("Invalid GID specification")],
        ));
    }
    let Some(name) = lookup_group_name(gid as u32) else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid GID specification")],
        ));
    };
    Ok(Value::string(name))
}

/// `(load-average &optional USE-FLOATS)` — return load averages.
///
/// With USE-FLOATS non-nil, returns 3 floats.
/// With USE-FLOATS nil/omitted, returns 3 integers scaled by 100.
pub(crate) fn builtin_load_average(args: Vec<Value>) -> EvalResult {
    expect_max_args("load-average", &args, 1)?;
    let use_floats = args.first().is_some_and(|value| value.is_truthy());
    let loads = read_load_average().unwrap_or([0.0, 0.0, 0.0]);
    if use_floats {
        Ok(Value::list(vec![
            Value::make_float(loads[0]),
            Value::make_float(loads[1]),
            Value::make_float(loads[2]),
        ]))
    } else {
        Ok(Value::list(vec![
            Value::fixnum((loads[0] * 100.0) as i64),
            Value::fixnum((loads[1] * 100.0) as i64),
            Value::fixnum((loads[2] * 100.0) as i64),
        ]))
    }
}

/// `(logcount INTEGER)` — return the number of 1 bits for nonnegative integers,
/// or the number of 0 bits in two's-complement form for negative integers.
pub(crate) fn builtin_logcount(args: Vec<Value>) -> EvalResult {
    expect_args("logcount", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Bignum) => {
            let n = args[0].as_bignum().expect("bignum kind");
            let bits = if *n < 0 {
                n.checked_count_zeros()
                    .expect("negative bignum has zero count")
            } else {
                n.checked_count_ones()
                    .expect("nonnegative bignum has finite one count")
            };
            Ok(Value::fixnum(i64::try_from(bits).unwrap_or(i64::MAX)))
        }
        ValueKind::Fixnum(n) => {
            let bits = if n >= 0 {
                (n as u64).count_ones() as i64
            } else {
                ((!n) as u64).count_ones() as i64
            };
            Ok(Value::fixnum(bits))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), args[0]],
        )),
    }
}

// ---------------------------------------------------------------------------
// OS lookup helpers
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn lookup_group_name(gid: u32) -> Option<String> {
    let group = unsafe { libc::getgrgid(gid as libc::gid_t) };
    if group.is_null() {
        return None;
    }
    let name_ptr = unsafe { (*group).gr_name };
    if name_ptr.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(name_ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(not(unix))]
fn lookup_group_name(_gid: u32) -> Option<String> {
    None
}

#[cfg(unix)]
fn read_load_average() -> Option<[f64; 3]> {
    let load = sysinfo::System::load_average();
    Some([load.one, load.five, load.fifteen])
}

#[cfg(not(unix))]
fn read_load_average() -> Option<[f64; 3]> {
    None
}
// ---------------------------------------------------------------------------
// translate-region-internal (mirrors GNU editfns.c:2506)
// ---------------------------------------------------------------------------

/// `(translate-region-internal START END TABLE)`
///
/// Translate every character between START and END through TABLE.
/// TABLE may be a string (Nth char in TABLE is the mapping for char N) or
/// a char-table whose `purpose` is `translation-table`.
///
/// Returns the number of characters changed.
///
/// Helper for `translate-region-internal`: scan a `(([FROM-CHAR ...] . TO) ...)`
/// alist looking for the first element whose FROM-CHAR vector matches the
/// character sequence at byte offset `p` in `source`. Returns
/// `(consumed_bytes, consumed_chars, TO)` on a successful match. Mirrors GNU
/// `check_translation` (editfns.c:2448).
fn check_translation(
    source: &[u8],
    p: usize,
    multibyte: bool,
    val: &Value,
) -> Option<(usize, usize, Value)> {
    use super::emacs_char::string_char_advance;

    // Cache decoded chars and their byte lengths.
    let mut buf_chars: Vec<i64> = Vec::with_capacity(8);
    let mut buf_lens: Vec<usize> = Vec::with_capacity(8);
    let mut scan = p;

    let mut cur = *val;
    while cur.is_cons() {
        let elt = cur.cons_car();
        cur = cur.cons_cdr();
        if !elt.is_cons() {
            continue;
        }
        let from_vec = elt.cons_car();
        let items = match from_vec.as_vector_data() {
            Some(v) => v,
            None => continue,
        };
        let need = items.len();
        // Decode enough chars from source.
        while buf_chars.len() < need {
            if scan >= source.len() {
                break;
            }
            let start = scan;
            let c = if multibyte {
                let mut q = scan;
                let c = string_char_advance(source, &mut q);
                scan = q;
                c as i64
            } else {
                let b = source[scan] as i64;
                scan += 1;
                b
            };
            buf_chars.push(c);
            buf_lens.push(scan - start);
        }
        if buf_chars.len() < need {
            continue;
        }
        let mut all_match = true;
        for (i, item) in items.iter().enumerate() {
            match item.as_fixnum() {
                Some(n) if n == buf_chars[i] => {}
                _ => {
                    all_match = false;
                    break;
                }
            }
        }
        if all_match {
            let consumed_bytes: usize = buf_lens[..need].iter().sum();
            return Some((consumed_bytes, need, elt.cons_cdr()));
        }
    }
    None
}

/// Encode the TO half of a `(([FROM ...] . TO) ...)` element as bytes for
/// the destination buffer's encoding. TO is either a character (fixnum) or a
/// vector of characters.
fn encode_translation_to(to: &Value, multibyte: bool) -> Vec<u8> {
    use super::emacs_char::{MAX_CHAR, MAX_MULTIBYTE_LENGTH, char_string};

    let mut bytes = Vec::new();
    if let Some(c) = to.as_fixnum() {
        if (0..=MAX_CHAR as i64).contains(&c) {
            if multibyte {
                let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
                let n = char_string(c as u32, &mut buf);
                bytes.extend_from_slice(&buf[..n]);
            } else {
                bytes.push((c & 0xff) as u8);
            }
        }
    } else if let Some(items) = to.as_vector_data() {
        for ch in items.iter() {
            if let Some(c) = ch.as_fixnum()
                && (0..=MAX_CHAR as i64).contains(&c)
            {
                if multibyte {
                    let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
                    let n = char_string(c as u32, &mut buf);
                    bytes.extend_from_slice(&buf[..n]);
                } else {
                    bytes.push((c & 0xff) as u8);
                }
            }
        }
    }
    bytes
}

/// Mirrors GNU `Ftranslate_region_internal` (editfns.c:2506) using a
/// whole-region read/translate/replace strategy (rather than GNU's
/// in-place gap mutation). The behaviour for simple char→char and
/// char→string/vector mappings matches GNU. The multi-character
/// `(([FROM-CHAR ...] . TO) ...)` form is currently treated as identity
/// (no lookahead) — this is a known pragmatic deviation, marked TODO.
pub(crate) fn builtin_translate_region_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    use super::chartable::{builtin_char_table_subtype, ct_lookup, is_char_table};
    use super::emacs_char::{
        MAX_CHAR, MAX_MULTIBYTE_LENGTH, byte8_to_char, char_string, chars_in_multibyte,
        string_char_advance,
    };

    expect_args("translate-region-internal", &args, 3)?;
    let table = &args[2];

    // ----- Validate TABLE ----------------------------------------------------
    let table_str = eval.lisp_string(*table);
    let is_str_table = table_str.is_some();
    let is_ct_table = is_char_table(table);
    if !is_str_table && !is_ct_table {
        return Err(signal(
            "error",
            vec![Value::string("Not a translation table")],
        ));
    }
    if is_ct_table {
        let purpose = builtin_char_table_subtype(vec![*table])?;
        let translation_sym = Value::symbol("translation-table");
        if !super::value::eq_value(&purpose, &translation_sym) {
            return Err(signal(
                "error",
                vec![Value::string("Not a translation table")],
            ));
        }
    }

    // ----- Resolve region in the current buffer ------------------------------
    let (buffer_id, byte_range) = super::fns::normalize_current_buffer_region_bounds_in_manager(
        &eval.buffers,
        &args[0],
        &args[1],
    )?;
    if byte_range.is_empty() {
        return Ok(Value::fixnum(0));
    }
    let multibyte = eval
        .buffers
        .get(buffer_id)
        .map(|b| b.get_multibyte())
        .unwrap_or(true);

    // Capture point and the region start as 1-based character positions so we
    // can reproduce GNU's per-character `replace_range` point relocation.  GNU
    // translates the region one character at a time, calling
    // `replace_range (pos, pos + len, ...)` for each grow/shrink; that path
    // relocates point with `adjust_point` only when the replaced character ends
    // at or before point (`from < PT || PT == to`).  Net effect: point's
    // *character* position is shifted by the cumulative character-count delta of
    // every replacement that ends at or before point, and char->char
    // replacements (which preserve the character count) never move point even
    // when the byte length grows.  Our whole-region replace strategy would
    // otherwise drag point to the end of the inserted replacement, so we track
    // the delta during the walk and restore point afterwards.
    let (point_char, region_start_char) = eval
        .buffers
        .get(buffer_id)
        .map(|b| {
            (
                b.point_lisp_char_pos().as_i64(),
                b.emacs_byte_pos_to_lisp_char_pos(byte_range.start())
                    .as_i64(),
            )
        })
        .unwrap_or((0, 0));

    // Read the whole region up front (whole-region replace strategy).
    let source =
        super::fns::read_buffer_region_bytes_in_manager(&eval.buffers, buffer_id, byte_range)?;

    // ----- String-table prep -------------------------------------------------
    let table_string_info: Option<(Vec<u8>, bool)> = table_str.map(|s| {
        let mut bytes = s.as_bytes().to_vec();
        let mut mb = s.is_multibyte();
        // GNU: if buffer is unibyte but table is multibyte, convert table to
        // unibyte (string_make_unibyte). Our mapping below indexes by byte
        // for unibyte tables; flatten by taking the byte view, which already
        // happens for unibyte-only tables. For a multibyte table on a unibyte
        // buffer we set mb=false and let the byte index lookup take over.
        if !multibyte && mb {
            mb = false;
        }
        // In the unibyte-buffer × multibyte-table case, leave bytes alone:
        // the unibyte-source path indexes by byte so it stays consistent.
        let _ = &mut bytes;
        (bytes, mb)
    });
    let translatable_chars: i64 = if let Some((bytes, _)) = table_string_info.as_ref() {
        std::cmp::min(MAX_CHAR as i64 + 1, bytes.len() as i64)
    } else {
        MAX_CHAR as i64 + 1
    };

    // ----- Walk the region, build the translated bytes -----------------------
    let mut out: Vec<u8> = Vec::with_capacity(source.len());
    let mut characters_changed: i64 = 0;
    let mut p: usize = 0;
    // GNU per-character point relocation: track the current source character
    // position and accumulate the net character-count delta of every
    // replacement that ends at or before point.  `replace_range` relocates
    // point when `from < PT || PT == to`, i.e. when the replaced character span
    // ends at or before point's character position (`cur_char + old_chars <=
    // point_char`).
    let mut cur_char: i64 = region_start_char;
    let mut point_char_delta: i64 = 0;
    while p < source.len() {
        let (oc, len) = if multibyte {
            let mut q = p;
            let c = string_char_advance(&source, &mut q);
            (c as i64, q - p)
        } else {
            (source[p] as i64, 1)
        };

        // Default: no translation.
        let mut nc: i64 = oc;
        let mut new_bytes: Option<Vec<u8>> = None;

        if oc < translatable_chars {
            if let Some((tt, table_mb)) = table_string_info.as_ref() {
                if *table_mb {
                    // Find char index `oc` within the multibyte table bytes.
                    let mut bp = 0usize;
                    let mut idx: i64 = 0;
                    while idx < oc && bp < tt.len() {
                        let (_c, l) = super::emacs_char::string_char(&tt[bp..]);
                        bp += l.max(1);
                        idx += 1;
                    }
                    if bp < tt.len() {
                        let mut qq = bp;
                        let c = string_char_advance(tt, &mut qq);
                        nc = c as i64;
                        new_bytes = Some(tt[bp..qq].to_vec());
                    }
                } else if (oc as usize) < tt.len() {
                    let b = tt[oc as usize];
                    nc = b as i64;
                    if b >= 0x80 && multibyte {
                        // BYTE8_STRING: encode raw byte as a 2-byte multibyte.
                        let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
                        let n = char_string(byte8_to_char(b), &mut buf);
                        new_bytes = Some(buf[..n].to_vec());
                    } else {
                        new_bytes = Some(vec![b]);
                    }
                }
            } else {
                // char-table case.
                let val = ct_lookup(table, oc)?;
                if let Some(c) = val.as_fixnum() {
                    if (0..=MAX_CHAR as i64).contains(&c) {
                        nc = c;
                        let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
                        let n = char_string(c as u32, &mut buf);
                        new_bytes = Some(buf[..n].to_vec());
                    }
                } else if val.is_vector() {
                    // [TO_CHAR ...] — concatenate the chars.
                    nc = -1;
                    if let Some(items) = val.as_vector_data() {
                        let mut bytes = Vec::new();
                        for ch in items.iter() {
                            if let Some(c) = ch.as_fixnum()
                                && (0..=MAX_CHAR as i64).contains(&c)
                            {
                                let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
                                let n = char_string(c as u32, &mut buf);
                                bytes.extend_from_slice(&buf[..n]);
                            }
                        }
                        new_bytes = Some(bytes);
                    }
                } else if val.is_cons() {
                    // (([FROM-CHAR ...] . TO) ...) — multi-char source
                    // pattern. Mirror GNU `check_translation` (editfns.c:2448).
                    if let Some((consumed_bytes, consumed_chars, to_val)) =
                        check_translation(&source, p, multibyte, &val)
                    {
                        let to_bytes = encode_translation_to(&to_val, multibyte);
                        out.extend_from_slice(&to_bytes);
                        let added_chars = if multibyte {
                            chars_in_multibyte(&to_bytes) as i64
                        } else {
                            to_bytes.len() as i64
                        };
                        characters_changed += added_chars;
                        // GNU `replace_range (from, to, TO)` with `from =
                        // cur_char`, `to = cur_char + consumed`, `inschars =
                        // added` relocates point via
                        //   adjust_point (from + inschars - min (PT, to))
                        // when `from < PT || PT == to`.  This multi-character
                        // source span is the only branch where point can fall
                        // strictly inside the consumed text, so handle both
                        // cases here.
                        let from = cur_char;
                        let to = cur_char + consumed_chars as i64;
                        if to <= point_char {
                            // Replacement ends at or before point: shift point
                            // by the net character-count delta.
                            point_char_delta += added_chars - consumed_chars as i64;
                        } else if from < point_char {
                            // Point is strictly inside the consumed span; GNU
                            // clamps it to the end of the replacement.  In the
                            // running (already-shifted) coordinate space that
                            // is `from + point_char_delta + added`, so re-express
                            // the total delta relative to the original point.
                            point_char_delta = from + point_char_delta + added_chars - point_char;
                        }
                        cur_char += consumed_chars as i64;
                        p += consumed_bytes.max(1);
                        continue;
                    }
                    nc = oc;
                    new_bytes = None;
                }
            }
        }

        if nc != oc && nc >= 0 {
            // Single-char-to-something replacement.
            if let Some(b) = new_bytes {
                out.extend_from_slice(&b);
            } else {
                out.extend_from_slice(&source[p..p + len]);
            }
            characters_changed += 1;
        } else if nc < 0 {
            // Vector form: one source char → multiple chars.
            if let Some(b) = new_bytes {
                let added = if multibyte {
                    chars_in_multibyte(&b) as i64
                } else {
                    b.len() as i64
                };
                out.extend_from_slice(&b);
                characters_changed += added;
                // GNU `replace_range (pos, pos + 1, [TO ...])` relocates point
                // when the single replaced char ends at or before point.
                if cur_char < point_char {
                    point_char_delta += added - 1;
                }
            } else {
                out.extend_from_slice(&source[p..p + len]);
            }
        } else {
            // Identity.
            out.extend_from_slice(&source[p..p + len]);
        }
        // Each source character consumed here is exactly one buffer character
        // (the multi-character `check_translation` case advances `cur_char`
        // itself and `continue`s before reaching this point).  char->char and
        // identity translations preserve the count, so they never move point.
        cur_char += 1;
        p += len.max(1);
    }

    // ----- Write back if anything changed ------------------------------------
    if characters_changed > 0 {
        let replacement = if multibyte {
            crate::heap_types::LispString::from_emacs_bytes(out)
        } else {
            crate::heap_types::LispString::from_unibyte(out)
        };
        super::fns::replace_buffer_emacs_byte_range_lisp_string(
            eval,
            buffer_id,
            byte_range,
            &replacement,
        )?;

        // Restore point to the GNU-faithful character position.  The
        // whole-region replace dragged point to the end of the inserted
        // replacement; GNU instead leaves point at its original character
        // position shifted only by replacements ending at or before it.
        let target_char = point_char + point_char_delta;
        if let Some(byte_pos) = eval
            .buffers
            .get(buffer_id)
            .map(|b| b.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(target_char)))
        {
            let _ = eval.buffers.goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
        }
    }

    Ok(Value::fixnum(characters_changed))
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
