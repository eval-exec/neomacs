//! Editing-function builtins — point/mark queries, insertion, deletion,
//! substring extraction, and miscellaneous user/system info.
//!
//! Emacs Lisp uses **1-based character positions** while the internal
//! `Buffer` stores **0-based Emacs-byte positions**.  Every Lisp↔Buffer boundary
//! must convert:
//!
//! - Lisp char pos  →  byte pos:  `LispCharPos1::to_byte_pos`
//! - byte pos       →  Lisp char: `EmacsBytePos::to_lisp`

use crate::buffer::overlay::OverlayModificationHook;
use crate::buffer::{
    Buffer, BufferManager, CharLen, EmacsByteLen, EmacsBytePos, EmacsByteRange, LispCharPos1,
    TextChange, TextEditRange, TextExtent,
};
use crate::emacs_core::edit_host::EditVariableRead;
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::heap_types::LispString;
use neovm_value::value::ValueKind;
use neovm_value::value::*;
#[cfg(unix)]
use std::ffi::CStr;
use strum::IntoStaticStr;

mod text_edit;
#[doc(hidden)]
pub use text_edit::{DeletedText, PendingTextEdit, RemeasureRule, delete_text_range};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

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
    ($vis:vis $fn_name:ident, $name:literal) => {
        $vis fn $fn_name() -> neovm_value::intern::SymId {
            static S: std::sync::OnceLock<neovm_value::intern::SymId> =
                std::sync::OnceLock::new();
            *S.get_or_init(|| neovm_value::intern::intern($name))
        }
    };
}
editfns_cached_symbol!(inhibit_read_only_symbol, "inhibit-read-only");
#[doc(hidden)]
editfns_cached_symbol!(
    pub
    inhibit_modification_hooks_symbol,
    "inhibit-modification-hooks"
);
editfns_cached_symbol!(deactivate_mark_symbol, "deactivate-mark");
editfns_cached_symbol!(
    undo_auto_undoable_change_symbol,
    "undo-auto--undoable-change"
);
editfns_cached_symbol!(first_change_hook_symbol, "first-change-hook");
editfns_cached_symbol!(undo_auto_amalgamate_symbol, "undo-auto-amalgamate");
editfns_cached_symbol!(kill_forward_chars_symbol, "kill-forward-chars");
editfns_cached_symbol!(evaporate_symbol, "evaporate");
editfns_cached_symbol!(before_change_functions_symbol, "before-change-functions");
editfns_cached_symbol!(after_change_functions_symbol, "after-change-functions");
editfns_cached_symbol!(
    combine_after_change_calls_symbol,
    "combine-after-change-calls"
);

// `buffer-read-only` is an UNCONDITIONAL buffer slot (`local_flags_idx: -1`),
// so `get_buffer_local_by_sym_id_gated` cannot take its `local_flags_idx >= 0`
// early return for this symbol and always answers with the very slot word
// `Buffer::get_read_only` reads. That is what lets the read-only test below be
// a single slot read. The equivalence is a property of the slot TABLE, not of
// the call site, so give the slot a local flag and this fails the build rather
// than quietly turning the check into a read-only bypass.
const _: () = {
    let mut i = 0;
    while i < crate::buffer::buffer::BUFFER_SLOT_INFO.len() {
        let info = &crate::buffer::buffer::BUFFER_SLOT_INFO[i];
        if info.offset.index() == crate::buffer::buffer::BUFFER_SLOT_READ_ONLY.index() {
            assert!(
                info.local_flags_idx < 0,
                "buffer-read-only gained a local flag: buffer_read_only_active_in_state \
                 must go back to consulting get_buffer_local_by_sym_id_gated"
            );
        }
        i += 1;
    }
};

/// GNU `Fbarf_if_buffer_read_only`'s test. Inlined so a writable buffer
/// (the common one) costs its callers one slot test and no variable view.
#[inline(always)]
#[doc(hidden)]
pub fn buffer_read_only_active_in_state<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buf: &Buffer,
) -> bool {
    // GNU `Fbarf_if_buffer_read_only' reads the buffer's `read_only' slot
    // first and looks at `inhibit-read-only' only when it is set. Every
    // insertion and deletion runs this check twice, and a writable buffer --
    // the common one -- should not pay for the two `inhibit-read-only'
    // lookups to learn that it is writable.
    let _ = dynamic;
    // One slot read, as GNU does: `!NILP (BVAR (current_buffer, read_only))'
    // (src/buffer.c:2464). The second lookup this used to perform could only
    // ever return the very same word -- see the assertion below -- so it asked
    // the same question twice on a path that runs four times per keystroke.
    buf.get_read_only() && !inhibit_read_only_in_state(vars, buf)
}

/// Whether `inhibit-read-only` lets a read-only BUF be changed.
#[inline(never)]
fn inhibit_read_only_in_state<'a, Vars: EditVariableRead<'a>>(vars: Vars, buf: &Buffer) -> bool {
    let iro = inhibit_read_only_symbol();
    if let Some(value) = buf.get_buffer_local_by_sym_id_gated(iro, vars.obarray().is_localized(iro))
        && value.is_truthy()
    {
        return true;
    }
    vars.value_or_nil(iro).is_truthy()
}

#[doc(hidden)]
pub fn ensure_current_buffer_writable_in_state<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &BufferManager,
) -> Result<(), Flow> {
    if let Some(buf) = buffers.current_buffer()
        && buffer_read_only_active_in_state(vars, dynamic, buf)
    {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(buf.id)],
        ));
    }
    Ok(())
}

#[doc(hidden)]
pub fn buffer_edit_range_for_byte_range_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<TextEditRange, Flow> {
    buffers
        .edit_range_for_buffer_emacs_byte_range(buffer_id, byte_range)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))
}

#[doc(hidden)]
pub fn lisp_string_text_extent(text: &LispString) -> TextExtent {
    TextExtent::new(
        CharLen::new(text.schars()),
        EmacsByteLen::new(text.sbytes()),
    )
}

// The current-buffer region trio (stage-3 C12, relocated from the lisp
// layer's `fns`): every edit-side region consumer -- base64/zlib/xml
// region subrs, translate-region-internal -- resolves, reads, and replaces
// through the buffer substrate here; `fns` keeps `pub(crate) use`
// re-exports for its own subr wrappers. The replace member runs the
// modification callbacks, so it takes the host.
#[doc(hidden)]
pub fn normalize_current_buffer_region_bounds_in_manager(
    buffers: &BufferManager,
    start_arg: &Value,
    end_arg: &Value,
) -> Result<(crate::buffer::BufferId, EmacsByteRange), Flow> {
    let region = super::position::LispRegionArgs::from_values(buffers, *start_arg, *end_arg)?;
    let buffer_id = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .id;

    let buf = buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;

    Ok((buffer_id, region.accessible_byte_range(buf)?))
}

#[doc(hidden)]
pub fn read_buffer_region_bytes_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<Vec<u8>, Flow> {
    let buf = buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    Ok(buf.buffer_substring_bytes_range(byte_range))
}

#[doc(hidden)]
pub fn replace_buffer_emacs_byte_range_lisp_string<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
    replacement: &neovm_value::heap_types::LispString,
) -> Result<(), Flow> {
    let range = buffer_edit_range_for_byte_range_in_manager(
        &host.edit_and_vars().0.buffers,
        buffer_id,
        byte_range,
    )?;
    // The modification callbacks may edit the buffer: replace the range they
    // leave, re-measured like GNU `replace_range` (insdel.c:1502-1513).
    let pending = PendingTextEdit::new(range, RemeasureRule::ReplaceRange);
    let Some(prepared) = pending.prepare(host)? else {
        return Ok(());
    };
    // Don't inherit text properties from neighbors here.
    // GNU's replace path (del_range + insert_from_gap in decode_coding)
    // also skips adjust_intervals_for_insertion.  Property inheritance
    // belongs to insert-and-inherit (insert_pieces_in_state with
    // inherit=true), not general-purpose replace operations.
    let new_extent = prepared
        .lease(&mut host.edit_and_vars().0.buffers)
        .and_then(|lease| lease.replace(replacement))
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    signal_after_text_change(
        host,
        crate::buffer::TextChange::new(prepared.range(), new_extent),
    )?;
    Ok(())
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

#[doc(hidden)]
pub fn text_change_for_empty_insertion_at_emacs_byte_pos(
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

#[doc(hidden)]
pub fn text_change_for_lisp_string_replacement_in_manager(
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

#[doc(hidden)]
pub fn text_change_for_deletion_in_manager(
    buffers: &BufferManager,
    buffer_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
) -> Result<TextChange, Flow> {
    let old_range = buffer_edit_range_for_byte_range_in_manager(buffers, buffer_id, byte_range)?;
    Ok(TextChange::deletion(old_range))
}

#[doc(hidden)]
pub fn text_change_for_unchanged_extent_in_manager(
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
/// The first C13 EditHost conversion: pure hook reads, no evaluator
/// handle, monomorphized at `H = Context` (PEER-RULING-STAGE3 R0).
fn hook_symbol_value_truthy<H: crate::emacs_core::edit_host::EditHost>(
    host: &H,
    sym: neovm_value::intern::SymId,
) -> bool {
    let sym = host.hook_symbol_by_id(sym);
    host.hook_value_by_id(sym).is_some_and(|v| v.is_truthy())
}

/// Check whether `inhibit-modification-hooks` is non-nil. GNU `insdel.c`
/// reads the `DEFVAR_BOOL` flag in one load; the read's fast paths over
/// the forwarding are an eval service on the host (C2 EditHost
/// conversion — this body names no evaluator type).
pub(crate) fn inhibit_modification_hooks<H: crate::emacs_core::edit_host::EditHost>(
    host: &H,
) -> bool {
    host.inhibit_modification_hooks()
}

/// Run a named hook, resetting it to nil if running it signals — GNU
/// `signal_before_change`/`signal_after_change` arming
/// `reset_var_on_error` (`insdel.c`) around the `run_hook_with_args`
/// call. The reset deliberately bypasses watchers and alias resolution
/// (the hook symbol is resolved above), so it is the raw host store,
/// never `set_variable`. C2 EditHost conversion.
fn run_named_hook_reset_on_error<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    hook_name: neovm_value::intern::SymId,
    hook_args: &[Value],
) -> Result<(), Flow> {
    let hook_sym = host.hook_symbol_by_id(hook_name);
    let hook_value = host.hook_value_by_id(hook_sym).unwrap_or(Value::NIL);
    if hook_value.is_nil() {
        return Ok(());
    }
    match host.run_hook_value(hook_sym, hook_value, hook_args, true) {
        Ok(_) => Ok(()),
        Err(flow) => {
            host.reset_variable_on_error(hook_sym, Value::NIL);
            Err(flow)
        }
    }
}

/// C13 EditHost conversion: this body needs hooks alone, so it never
/// names the evaluator (`H = Context` at every call site today).
fn run_named_hook_without_reset<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    hook_name: neovm_value::intern::SymId,
    hook_args: &[Value],
) -> Result<(), Flow> {
    let hook_sym = host.hook_symbol_by_id(hook_name);
    let hook_value = host.hook_value_by_id(hook_sym).unwrap_or(Value::NIL);
    if hook_value.is_nil() {
        return Ok(());
    }
    let _ = host.run_hook_value(hook_sym, hook_value, hook_args, true)?;
    Ok(())
}

// Local argument extractors (stage-3 C12): the builtins copies stay above
// the split with the subr wrappers; the edit layer keeps its own, exactly
// as `editing::buffer` already does.

fn maybe_trace_characterp_nil(value: &Value, source: &str) {
    if !value.is_nil() {
        return;
    }
    if std::env::var("NEOVM_TRACE_CHARACTERP_NIL").unwrap_or_default() != "1" {
        return;
    }
    eprintln!(
        "NEOVM_TRACE_CHARACTERP_NIL source={source}\n{}",
        std::backtrace::Backtrace::force_capture()
    );
}

fn expect_character_code(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(c) if (0..=0x3F_FFFF).contains(&c) => Ok(c),
        _other => {
            maybe_trace_characterp_nil(value, "expect_character_code");
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), *value],
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BufferChangeKind {
    Characters,
    PropertiesOnly,
}

/// Verify intervals at the preparation seam. This only reads the owning
/// host's edit state and publishes no process-global or thread-local state.
#[inline(always)]
fn verify_change_text_read_only<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    byte_range: EmacsByteRange,
) -> Result<(), Flow> {
    let (edit, vars) = host.edit_and_vars();
    if let Some(current_id) = edit.buffers.current_buffer_id() {
        if byte_range.is_empty() {
            crate::emacs_core::textprop::verify_text_read_only_for_insert_in_state(
                vars,
                &edit.buffers,
                current_id,
                byte_range.start(),
            )?;
        } else {
            crate::emacs_core::textprop::verify_text_read_only_emacs_byte_range_in_state(
                vars,
                &edit.buffers,
                current_id,
                byte_range,
            )?;
        }
    }

    Ok(())
}

/// Where a pending change starts, through the callbacks its preparation runs.
///
/// GNU `prepare_to_modify_buffer (start, end, preserve_ptr)`: a deletion or a
/// replacement passes `&from`, and edits made by the callbacks move FROM the
/// way they move a marker (insdel.c:2149-2156 around
/// `verify_interval_modification`, PRESERVE_VALUE in `signal_before_change`).
/// An insertion or a property change passes NULL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangeStartPolicy {
    Fixed,
    Preserved,
}

/// The range a change is being prepared for, kept usable across Lisp.
///
/// Native preparation leaves measured bytes authoritative. Capture the text
/// only immediately before a boundary that can run Lisp; consume that snapshot
/// on return so subsequent native stages require no buffer/tick probes.
#[derive(Clone, Copy, Debug)]
struct LiveChangeRange {
    range: TextEditRange,
    before_callbacks: Option<ChangeTextSnapshot>,
}

#[derive(Clone, Copy, Debug)]
struct ChangeTextSnapshot {
    buffer: Option<crate::buffer::BufferId>,
    chars_tick: i64,
}

impl LiveChangeRange {
    #[inline]
    fn new(range: TextEditRange) -> Self {
        Self {
            range,
            before_callbacks: None,
        }
    }

    #[inline]
    fn capture_for_callbacks(&mut self, buffers: &BufferManager) {
        let buffer = buffers.current_buffer_id();
        self.before_callbacks = Some(ChangeTextSnapshot {
            buffer,
            chars_tick: current_chars_tick(buffers, buffer),
        });
    }

    /// Re-measure only when callback Lisp could have invalidated the bytes.
    #[inline(always)]
    fn refresh(&mut self, buffers: &BufferManager) -> TextEditRange {
        if let Some(before) = self.before_callbacks.take() {
            let buffer = buffers.current_buffer_id();
            let chars_tick = current_chars_tick(buffers, buffer);
            if buffer != before.buffer || chars_tick != before.chars_tick {
                if let Some(buf) = buffer.and_then(|id| buffers.get(id)) {
                    self.range = live_edit_range_for_chars(buf, self.range.char_range());
                }
            }
        }
        self.range
    }
}

fn current_chars_tick(buffers: &BufferManager, buffer: Option<crate::buffer::BufferId>) -> i64 {
    buffer
        .and_then(|id| buffers.get(id))
        .map_or(0, Buffer::chars_modified_tick)
}

/// Measure character positions in the live text, clamped to its end: a
/// position computed before a callback shrank the buffer must not reach past
/// the text (GNU would read beyond Z there; neomacs must not panic).
fn live_edit_range_for_chars(buf: &Buffer, chars: crate::buffer::CharRange) -> TextEditRange {
    let end = buf.total_char_end_pos();
    buf.edit_range_for_char_range(crate::buffer::CharRange::new(
        chars.start().min(end),
        chars.end().min(end),
    ))
}

/// CHARS measured in the current buffer's live text, clamped to its end:
/// for callers that, like GNU, reuse numeric character positions computed
/// before Lisp ran. None when there is no current buffer.
pub(crate) fn current_buffer_live_range(
    buffers: &BufferManager,
    chars: crate::buffer::CharRange,
) -> Option<TextEditRange> {
    buffers
        .current_buffer()
        .map(|buf| live_edit_range_for_chars(buf, chars))
}

/// Run modification callbacks with the change's start carried across them.
///
/// For `ChangeStartPolicy::Preserved` this is GNU's PRESERVE_VALUE /
/// RESTORE_VALUE pair: a temporary marker (insertion type nil) holds FROM
/// while Lisp runs and is read back and unchained afterwards. Only callers
/// that know Lisp is about to run call this, so the callback-free path does
/// no marker work. The marker is a GC root of this mutator's specpdl for
/// exactly the callbacks' extent.
fn run_preserving_change_start<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    policy: ChangeStartPolicy,
    start: crate::buffer::CharPos0,
    callbacks: impl FnOnce(&mut H) -> Result<(), Flow>,
) -> Result<crate::buffer::CharPos0, Flow> {
    let current_buffer = host.edit_and_vars().0.buffers.current_buffer_id();
    let marker = match (policy, current_buffer) {
        (ChangeStartPolicy::Preserved, Some(buffer)) => {
            Some(crate::emacs_core::marker::make_registered_buffer_marker(
                &mut host.edit_and_vars().0.buffers,
                buffer,
                start.to_lisp(),
                false,
            ))
        }
        (ChangeStartPolicy::Preserved, None) | (ChangeStartPolicy::Fixed, _) => None,
    };
    let Some(marker) = marker else {
        callbacks(host)?;
        return Ok(start);
    };
    let roots = host.save_specpdl_roots();
    host.push_specpdl_root(marker);
    let result = callbacks(host);
    // GNU `marker_position` signals when the callbacks killed the marker's
    // buffer; a nonlocal exit from the callbacks takes precedence.
    let preserved = crate::emacs_core::marker::marker_logical_fields(&marker).and_then(
        |(buffer, position, _)| {
            buffer
                .and_then(|buffer| host.edit_and_vars().0.buffers.get(buffer))
                .and(position)
        },
    );
    crate::emacs_core::marker::unchain_marker(&mut host.edit_and_vars().0.buffers, &marker);
    host.restore_specpdl_roots(roots);
    result?;
    preserved
        .map(crate::buffer::CharPos0::from_lisp)
        .ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("Marker does not point anywhere")],
            )
        })
}

/// GNU buffer-modification preparation, with an explicit distinction between
/// character input consumed by Tree-sitter and property-only modifications.
/// `range` is measured in the current buffer; its 1-based character positions
/// are what the Lisp hooks receive. Returns the change's start after the
/// callbacks ran (moved only for `ChangeStartPolicy::Preserved`).
fn prepare_buffer_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    range: TextEditRange,
    kind: BufferChangeKind,
    start_policy: ChangeStartPolicy,
) -> Result<crate::buffer::CharPos0, Flow> {
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
    {
        let (edit, vars) = host.edit_and_vars();
        ensure_current_buffer_writable_in_state(vars, &[], &edit.buffers)?;
    }
    let mut start = range.char_start();
    let mut live = LiveChangeRange::new(range);

    let gnu_hooks = super::redisplay_policy::gnu_redisplay_hooks_enabled();
    if !gnu_hooks {
        verify_change_text_read_only(host, range.byte_range())?;
    }

    let mut range = range;
    if let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() {
        let undo_enabled = host
            .edit_and_vars()
            .0
            .buffers
            .get(current_id)
            .is_some_and(|buf| !buf.get_undo_list().is_t());
        let undoable_change = undo_auto_undoable_change_symbol();
        let undoable_hook_bound = host.edit_and_vars().1.obarray().fboundp_id(undoable_change);
        if undo_enabled && undoable_hook_bound {
            live.capture_for_callbacks(&host.edit_and_vars().0.buffers);
            host.apply(Value::from_sym_id(undoable_change), &[])?;
            range = live.refresh(&host.edit_and_vars().0.buffers);
        }
    }

    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(start);
    };
    // GNU insdel.c:2145 publishes after undoable-change and before any
    // interval callback, including the inhibited-modification-hooks arm.
    if gnu_hooks {
        host.mark_buffer_redisplay(current_id);
        verify_change_text_read_only(host, range.byte_range())?;
    }

    if kind == BufferChangeKind::Characters && host.treesit_has_editable_tree(current_id) {
        host.treesit_begin_buffer_edit(current_id, range.byte_range());
    }

    if inhibit_modification_hooks(host) {
        return Ok(start);
    }

    // GNU `prepare_to_modify_buffer_1` locks a clean file-visiting base buffer
    // at this exact chokepoint, before first-change-hook and
    // before-change-functions.  Text edits already converge here, so the lock
    // transition remains complete without being duplicated across producers.
    // The clean/file-target selection is native edit state; publishing the
    // range roots and taking the lock (which can run Lisp) keep GNU's order.
    if let Some(filename) =
        super::file_name::clean_current_buffer_file_lock_target(&host.edit_and_vars().0.buffers)
    {
        live.capture_for_callbacks(&host.edit_and_vars().0.buffers);
        host.lock_file(&filename)?;
    }
    let range = live.refresh(&host.edit_and_vars().0.buffers);
    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(start);
    };

    // GNU preserves FROM around `verify_interval_modification` only when the
    // buffer has intervals; a range with no properties runs no hook Lisp.
    let preserve_interval_start = start_policy == ChangeStartPolicy::Preserved
        && !range.is_empty()
        && host
            .edit_and_vars()
            .0
            .buffers
            .get(current_id)
            .is_some_and(|buf| !buf.text_props_is_empty());
    let bytes = range.byte_range();
    let mut prepare_intervals = |host: &mut H| {
        crate::emacs_core::textprop::prepare_interval_modification_for_change(
            host,
            current_id,
            bytes.start(),
            bytes.end(),
            |host| live.capture_for_callbacks(&host.edit_and_vars().0.buffers),
        )
    };
    if preserve_interval_start {
        start = run_preserving_change_start(host, start_policy, start, prepare_intervals)?;
    } else {
        prepare_intervals(host)?;
    }
    let range = live.refresh(&host.edit_and_vars().0.buffers);
    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(start);
    };
    if before_change_hooks_quiet(host, current_id) {
        host.edit_and_vars().0.last_overlay_modification_hooks = Vec::new();
    } else if start_policy == ChangeStartPolicy::Preserved {
        start = run_preserving_change_start(host, start_policy, start, |host| {
            run_before_change_hooks(host, current_id, range)
        })?;
    } else {
        run_before_change_hooks(host, current_id, range)?;
    }
    deactivate_mark_after_preparing_change(host)?;
    Ok(start)
}

/// True when no before-change callback can run: no first-change hook due,
/// `before-change-functions` nil, and no overlays.
///
/// GNU binds `inhibit-modification-hooks` unconditionally (insdel.c
/// signal_before_change), but its bind is a C specpdl push; ours was ~590 Ir
/// of bind+unbind per modification, and with nothing to run under it the
/// binding is unobservable.
#[inline(always)]
fn before_change_hooks_quiet<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    current_id: crate::buffer::BufferId,
) -> bool {
    let first_change_pending = host
        .edit_and_vars()
        .0
        .buffers
        .get(current_id)
        .is_some_and(|buf| buf.modified_state_value().is_nil());
    let first_change_due =
        first_change_pending && hook_symbol_value_truthy(host, first_change_hook_symbol());
    !first_change_due
        && !hook_symbol_value_truthy(host, before_change_functions_symbol())
        && !buffer_has_overlays(host, current_id)
}

/// Run callbacks only. Preparation owns mark deactivation so the inhibited
/// path and the callback-free fast path cannot accidentally have the same
/// selection side effects (GNU insdel.c: prepare_to_modify_buffer_1).
fn run_before_change_hooks<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    current_id: crate::buffer::BufferId,
    range: TextEditRange,
) -> Result<(), Flow> {
    let byte_range = range.byte_range();
    let lisp_beg = range.char_start().to_lisp().as_i64();
    let lisp_end = range.char_end().to_lisp().as_i64();

    let hook_args = vec![Value::fixnum(lisp_beg), Value::fixnum(lisp_end)];
    let run_first_change = host
        .edit_and_vars()
        .0
        .buffers
        .get(current_id)
        .is_some_and(|buf| buf.modified_state_value().is_nil());
    let specpdl_count = host.specpdl_index();
    host.try_specbind_or_unwind_to(specpdl_count, inhibit_modification_hooks_symbol(), Value::T)?;
    let result = (|| -> Result<(), Flow> {
        if run_first_change {
            run_named_hook_without_reset(host, first_change_hook_symbol(), &[])?;
        }
        run_named_hook_reset_on_error(host, before_change_functions_symbol(), &hook_args)?;

        host.edit_and_vars().0.last_overlay_modification_hooks = collect_overlay_change_hooks(
            host,
            byte_range.start().get(),
            byte_range.end().get(),
            byte_range.is_empty(),
        );
        run_recorded_overlay_change_hooks(host, Value::NIL, lisp_beg, lisp_end, None)?;

        Ok(())
    })();
    host.unbind_to_with_result(specpdl_count, result.map(|()| Value::NIL))
        .map(|_| ())
}

/// Run the before-change protocol for an edit whose range the caller will
/// not re-measure: an insertion, or a change whose callbacks cannot move it.
#[doc(hidden)]
pub fn signal_before_text_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    change: TextChange,
) -> Result<(), Flow> {
    prepare_buffer_change(
        host,
        change.old_range(),
        BufferChangeKind::Characters,
        ChangeStartPolicy::Fixed,
    )
    .map(|_| ())
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
pub(crate) fn signal_before_insertion_at_emacs_byte_pos<
    H: crate::emacs_core::edit_host::EditHost,
>(
    host: &mut H,
    byte_pos: EmacsBytePos,
) -> Result<(), Flow> {
    // Insertions happen at point, whose character position the buffer keeps
    // beside its byte position, so this is not a text walk.
    let char_pos = host
        .edit_and_vars()
        .0
        .buffers
        .current_buffer()
        .map_or(crate::buffer::CharPos0::ZERO, |buf| {
            buf.emacs_byte_pos_to_char_pos_clamped(byte_pos)
        });
    prepare_buffer_change(
        host,
        TextEditRange::empty_at(byte_pos, char_pos),
        BufferChangeKind::Characters,
        ChangeStartPolicy::Fixed,
    )
    .map(|_| ())
}

/// Run GNU's modification-hook protocol for a text-property-only change.
/// Tree-sitter consumes characters, not properties, so this deliberately does
/// not create an incremental parser edit.
pub(crate) fn signal_before_property_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    change: TextChange,
) -> Result<(), Flow> {
    prepare_buffer_change(
        host,
        change.old_range(),
        BufferChangeKind::PropertiesOnly,
        ChangeStartPolicy::Fixed,
    )
    .map(|_| ())
}

#[inline(always)]
fn deactivate_mark_after_preparing_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
) -> Result<(), Flow> {
    // GNU `prepare_to_modify_buffer_1` (insdel.c), with hooks enabled, runs
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
    if deactivate_mark_set_is_noop(host, sym) {
        return Ok(());
    }
    // `Fset` notifies variable watchers, and a watcher's signal aborts the
    // modification before any text changes (insdel.c:2189 runs before the
    // caller's mutation).
    host.set_variable(Value::from_sym_id(sym), Value::T)
        .map(|_| ())
}

/// True when `(set 'deactivate-mark t)` would change no binding and notify no
/// watcher: the symbol is untrapped and the binding the store would hit — the
/// current buffer's local one for a localized symbol, the value cell otherwise —
/// already holds `t`.
fn deactivate_mark_set_is_noop<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    sym: neovm_value::intern::SymId,
) -> bool {
    use neovm_value::symbol::{SymbolRedirect, SymbolTrappedWrite};
    let (trapped_write, redirect) = {
        let Some(symbol) = host.edit_and_vars().1.obarray().get_by_id(sym) else {
            return false;
        };
        (symbol.trapped_write(), symbol.redirect())
    };
    if trapped_write != SymbolTrappedWrite::Untrapped {
        return false;
    }
    match redirect {
        SymbolRedirect::Plainval => host.forwarded_variable_value(sym) == Some(Value::T),
        SymbolRedirect::Localized => {
            host.edit_and_vars()
                .0
                .buffers
                .current_buffer()
                .and_then(|buf| buf.get_buffer_local_binding_by_sym_id_gated(sym, true))
                .and_then(|binding| binding.as_value())
                == Some(Value::T)
        }
        _ => false,
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// GNU `signal_after_change(beg, end, old_len)` — run `after-change-functions`
/// and overlay hooks after a buffer modification.
///
/// `byte_range` is a 0-based Emacs-byte range in the new buffer state.
/// `old_len` is the character length of the old text that was replaced.
pub(crate) fn signal_after_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    byte_range: EmacsByteRange,
    old_len: CharLen,
) -> Result<(), Flow> {
    signal_after_change_with_kind(host, byte_range, old_len, BufferChangeKind::Characters)
}

fn signal_after_change_with_kind<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    byte_range: EmacsByteRange,
    old_len: CharLen,
    kind: BufferChangeKind,
) -> Result<(), Flow> {
    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(());
    };

    // GNU window positions are markers, so their Lisp-visible values already
    // reflect this edit before after-change hooks run.  Neomacs keeps typed
    // position caches beside the authoritative markers for layout; refresh all
    // derived caches at this common post-edit boundary, including when
    // modification hooks are inhibited.  Only a character change moves a
    // marker: a property-only change (font-lock's `put-text-property`, tens
    // of thousands per fontification pass) leaves every position where it
    // was, so it has no caches to refresh.
    if kind == BufferChangeKind::Characters {
        host.sync_window_positions(current_id);
    }

    // GNU `adjust_overlays_for_delete_in_buffer` queries only the deletion
    // boundary after shifting the interval tree.  Do the category-resolving
    // evaporation pass at that same boundary; enumerating the whole buffer
    // here makes every localized deletion O(number of overlays).
    if old_len.get() > 0 && buffer_has_overlays(host, current_id) {
        evaporate_emptied_overlays_at(host, current_id, byte_range.start());
    }

    if kind == BufferChangeKind::Characters {
        finish_treesit_after_buffer_change(host, current_id, byte_range.start(), byte_range.end());
    }

    if inhibit_modification_hooks(host) {
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
    if combine_after_change_calls_active(host) && !buffer_has_overlays(host, current_id) {
        // If the pending deferred list belongs to a different buffer, GNU
        // flushes it via `Fcombine_after_change_execute` before recording
        // the new change.
        let needs_flush = {
            let (edit, _vars) = host.edit_and_vars();
            !edit.combine_after_change_list.is_empty()
                && edit.combine_after_change_buffer != Some(current_id)
        };
        if needs_flush {
            execute_combined_after_change(host)?;
        }

        let deferred_fields = {
            let (edit, _vars) = host.edit_and_vars();
            edit.buffers.get(current_id).map(|buf| {
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
                (beg_field, end_field, change)
            })
        };
        if let Some((beg_field, end_field, change)) = deferred_fields {
            host.edit_and_vars()
                .0
                .combine_after_change_list
                .push((beg_field, end_field, change));
            host.edit_and_vars().0.combine_after_change_buffer = Some(current_id);
        }
        return Ok(());
    }

    // Not deferring: any pending coalesced changes must run first so their
    // hooks observe the buffer state from before this new edit's after-pass.
    if !host.edit_and_vars().0.combine_after_change_list.is_empty() {
        execute_combined_after_change(host)?;
    }

    // Quiet fast path (twin of the one in `signal_before_change_hooks`):
    // with `after-change-functions` nil, no recorded or live overlay hooks,
    // no interval insert hooks, and no text-property intervals to report,
    // nothing can run under the bind.
    let edit_state_quiet = {
        let (edit, _vars) = host.edit_and_vars();
        edit.last_overlay_modification_hooks.is_empty()
            && edit.interval_insert_behind_hooks.is_nil()
            && edit.interval_insert_in_front_hooks.is_nil()
            && edit
                .buffers
                .get(current_id)
                .is_some_and(|buf| buf.text_props_is_empty())
    };
    if !hook_symbol_value_truthy(host, after_change_functions_symbol())
        && edit_state_quiet
        && !buffer_has_overlays(host, current_id)
    {
        return Ok(());
    }

    // Convert byte positions to 1-based character positions.
    let (lisp_beg, lisp_end, lisp_old_len) = {
        let Some(buf) = host.edit_and_vars().0.buffers.get(current_id) else {
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
    let saved_interval_insert_behind_hooks = host.edit_and_vars().0.interval_insert_behind_hooks;
    let saved_interval_insert_in_front_hooks =
        host.edit_and_vars().0.interval_insert_in_front_hooks;

    let specpdl_count = host.specpdl_index();
    // The saved hook lists live only in the Rust locals above while
    // after-change-functions run arbitrary Lisp; if a hook clears the
    // context fields (their previous root), a GC frees the lists before
    // they are reinstated below. GcRoot entries unwind with the specpdl,
    // so unbind_to(specpdl_count) pops them with the specbind. Roots are
    // pushed BEFORE the specbind at one count — the order
    // PEER-RULING-STAGE3 C3 pins ("that order is kept exactly").
    host.push_specpdl_root(saved_interval_insert_behind_hooks);
    host.push_specpdl_root(saved_interval_insert_in_front_hooks);
    host.try_specbind_or_unwind_to(specpdl_count, inhibit_modification_hooks_symbol(), Value::T)?;
    let result = (|| -> Result<(), Flow> {
        run_named_hook_reset_on_error(host, after_change_functions_symbol(), &hook_args)?;

        host.edit_and_vars().0.interval_insert_behind_hooks = saved_interval_insert_behind_hooks;
        host.edit_and_vars().0.interval_insert_in_front_hooks =
            saved_interval_insert_in_front_hooks;

        // --- Run overlay hooks ---
        // insert-in-front-hooks: overlays whose start == beg
        // insert-behind-hooks:   overlays whose end == beg (before insertion point)
        // modification-hooks:    overlays covering [beg, end)
        run_overlay_after_change_hooks(
            host,
            byte_range.start().get(),
            byte_range.end().get(),
            lisp_beg,
            lisp_end,
            lisp_old_len,
        )?;

        if lisp_old_len == 0 {
            crate::emacs_core::textprop::report_interval_modification(host, lisp_beg, lisp_end)?;
        }

        Ok(())
    })();
    host.unbind_to_with_result(specpdl_count, result.map(|()| Value::NIL))
        .map(|_| ())
}

#[doc(hidden)]
pub fn signal_after_text_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    change: TextChange,
) -> Result<(), Flow> {
    signal_after_change_with_kind(
        host,
        change.after_byte_range(),
        change.old_char_len(),
        BufferChangeKind::Characters,
    )
}

/// GNU `signal_after_change (charpos, lendel, lenins)` in character terms,
/// for callers whose positions were computed before Lisp ran: the inserted
/// range is measured in the live text (clamped to its end), so a stale
/// position can neither reach past the text nor split a character.
#[doc(hidden)]
pub fn signal_after_change_chars<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    start: crate::buffer::CharPos0,
    deleted: CharLen,
    inserted: CharLen,
) -> Result<(), Flow> {
    let Some(after) = host.edit_and_vars().0.buffers.current_buffer().map(|buf| {
        live_edit_range_for_chars(
            buf,
            crate::buffer::CharRange::from_start_len(start, inserted),
        )
    }) else {
        return Ok(());
    };
    signal_after_change_with_kind(
        host,
        after.byte_range(),
        deleted,
        BufferChangeKind::Characters,
    )
}

pub(crate) fn signal_after_property_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    change: TextChange,
) -> Result<(), Flow> {
    signal_after_change_with_kind(
        host,
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
#[doc(hidden)]
pub fn insert_lisp_string_with_change_hooks_in_buffer<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    buffer_id: crate::buffer::BufferId,
    text: &LispString,
) -> Result<(), Flow> {
    if text.is_empty() {
        return Ok(());
    }

    let saved_current = host.edit_and_vars().0.buffers.current_buffer_id();
    let result = (|| {
        host.set_current_buffer_unrecorded(buffer_id)?;
        let insert_pos = host
            .edit_and_vars()
            .0
            .buffers
            .get(buffer_id)
            .map(|buffer| buffer.point_emacs_byte_pos())
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
        let change = text_change_for_empty_insertion_at_emacs_byte_pos(
            &host.edit_and_vars().0.buffers,
            buffer_id,
            insert_pos,
            lisp_string_text_extent(text),
        )?;

        signal_before_text_change(host, change)?;
        // A before-change function may select another buffer. The mutation and
        // after-change notification still belong to the declared edit target.
        host.set_current_buffer_unrecorded(buffer_id)?;
        host.edit_and_vars()
            .0
            .buffers
            .insert_lisp_string_into_buffer(buffer_id, text)
            .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
        signal_after_text_change(host, change)
    })();

    if let Some(saved) = saved_current {
        host.restore_current_buffer_if_live(saved);
    }
    result
}

/// Delete overlays that a deletion left empty and whose `evaporate` property is
/// non-nil. Unlike the low-level direct-plist evaporation in
/// `OverlayList::adjust_for_delete_emacs_byte_range`, this resolves `evaporate`
/// through the overlay's `category` symbol (GNU `overlay-get` semantics), so a
/// category-inherited evaporate flag is honored. Mirrors GNU deleting empty
/// evaporate overlays during a buffer deletion.
fn evaporate_emptied_overlays_at<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    buffer_id: crate::buffer::BufferId,
    position: EmacsBytePos,
) {
    let empty: Vec<Value> = {
        let Some(buf) = host.edit_and_vars().0.buffers.get(buffer_id) else {
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
            let (edit, vars) = host.edit_and_vars();
            crate::emacs_core::textprop::lookup_overlay_property(
                vars,
                &edit.buffers,
                *overlay,
                Value::from_sym_id(evaporate_symbol()),
            )
            .is_truthy()
        })
        .collect();
    for overlay in to_delete {
        host.edit_and_vars()
            .0
            .buffers
            .delete_buffer_overlay(buffer_id, overlay);
    }
}

fn finish_treesit_after_buffer_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    buffer_id: crate::buffer::BufferId,
    beg: EmacsBytePos,
    end: EmacsBytePos,
) {
    host.treesit_note_buffer_change(buffer_id, beg);
    // The liveness guard lives inside the host method: the pending edit is
    // dropped exactly when the buffer is gone, as before.
    if host.treesit_has_pending_edit(buffer_id) {
        host.treesit_finish_buffer_edit(buffer_id, end);
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum BeforeChangeSpecialFunction {
    SyntaxPpssFlushCache,
}

impl BeforeChangeSpecialFunction {
    fn is_lisp_value(self, value: Value) -> bool {
        value == Value::symbol(self.name())
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn name(self) -> &'static str {
        self.into()
    }
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// Mirrors GNU's deferral predicate for `signal_after_change`
/// (`insdel.c:2393`). True when `combine-after-change-calls` is non-nil and
/// `before-change-functions` is either nil or the well-known
/// `(t syntax-ppss-flush-cache)` special case. C2 EditHost conversion:
/// hook reads plus one `default_value`, no evaluator type named.
fn combine_after_change_calls_active<H: crate::emacs_core::edit_host::EditHost>(host: &H) -> bool {
    let combine_sym = host.hook_symbol_by_id(combine_after_change_calls_symbol());
    let combine_val = host.hook_value_by_id(combine_sym).unwrap_or(Value::NIL);
    if combine_val.is_nil() {
        return false;
    }

    let before_sym = host.hook_symbol_by_id(before_change_functions_symbol());
    let before_val = host.hook_value_by_id(before_sym).unwrap_or(Value::NIL);
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
                && second.is_bare_symbol()
                && BeforeChangeSpecialFunction::SyntaxPpssFlushCache.is_lisp_value(second)
            {
                let default_val = host.default_value(before_sym).unwrap_or(Value::NIL);
                return default_val.is_nil();
            }
        }
    }
    false
}

fn buffer_has_overlays<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    buffer_id: crate::buffer::BufferId,
) -> bool {
    host.edit_and_vars()
        .0
        .buffers
        .get(buffer_id)
        .is_some_and(|buf| !buf.overlays.is_empty())
}

/// GNU `Fcombine_after_change_execute` (insdel.c:2475). Merges the deferred
/// per-change records into a single (begpos, lendel, lenins) triple and
/// dispatches one `signal_after_change` call.
#[doc(hidden)]
pub fn execute_combined_after_change<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
) -> Result<(), Flow> {
    if host.edit_and_vars().0.combine_after_change_list.is_empty() {
        return Ok(());
    }

    let Some(target_id) = host.edit_and_vars().0.combine_after_change_buffer else {
        host.edit_and_vars().0.combine_after_change_list.clear();
        return Ok(());
    };

    if host.edit_and_vars().0.buffers.get(target_id).is_none() {
        host.edit_and_vars().0.combine_after_change_list.clear();
        host.edit_and_vars().0.combine_after_change_buffer = None;
        return Ok(());
    }

    // GNU temporarily switches to the recording buffer.
    let saved_buffer = host.edit_and_vars().0.buffers.current_buffer_id();
    if saved_buffer != Some(target_id) {
        let _ = host.set_current_buffer_unrecorded(target_id);
    }

    let merged = {
        let (edit, _vars) = host.edit_and_vars();
        match edit.buffers.get(target_id) {
            Some(buf) => {
                let z = buf.z_lisp_char_pos().as_i64();
                let init = z - 1;
                let mut beg = init;
                let mut end = init;
                let mut change: i64 = 0;
                for (thisbeg, thisend, thischange) in &edit.combine_after_change_list {
                    change += *thischange;
                    if *thisbeg < beg {
                        beg = *thisbeg;
                    }
                    if *thisend < end {
                        end = *thisend;
                    }
                }
                Some((
                    1 + beg,
                    z - end,
                    change,
                    edit.combine_after_change_list.len(),
                ))
            }
            None => None,
        }
    };
    let Some((begpos, endpos, change_total, list_len)) = merged else {
        host.edit_and_vars().0.combine_after_change_list.clear();
        host.edit_and_vars().0.combine_after_change_buffer = None;
        if let Some(prev) = saved_buffer {
            let _ = host.set_current_buffer_unrecorded(prev);
        }
        return Ok(());
    };

    host.edit_and_vars().0.combine_after_change_list.clear();
    host.edit_and_vars().0.combine_after_change_buffer = None;

    let _ = list_len;

    // Convert merged 1-based char range back into byte positions for our
    // signal_after_change typed byte range.
    let byte_range = {
        let (edit, _vars) = host.edit_and_vars();
        let buf = edit.buffers.get(target_id).expect("target buffer");
        EmacsByteRange::new(
            buf.char_pos_to_emacs_byte_pos_clamped(LispCharPos1::new(begpos).to_char_pos()),
            buf.char_pos_to_emacs_byte_pos_clamped(LispCharPos1::new(endpos).to_char_pos()),
        )
    };
    let old_len = CharLen::new((endpos - begpos - change_total).max(0) as usize);

    let result = signal_after_change(host, byte_range, old_len);

    if let Some(prev) = saved_buffer
        && prev != target_id
    {
        let _ = host.set_current_buffer_unrecorded(prev);
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
fn collect_overlay_change_hooks<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    beg: usize,
    end: usize,
    insertion: bool,
) -> Vec<OverlayModificationHook> {
    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Vec::new();
    };

    // GNU widens the search by one on each side for insertions so that
    // overlays whose endpoints touch the insertion point are included.
    let search_beg = if insertion && beg > 0 { beg - 1 } else { beg };
    let search_end = if insertion { end + 1 } else { end.max(beg) };
    // Collect the overlay ids and endpoints in one edit phase, so the
    // category-resolving property reads below can open their own phases.
    let overlay_ids: Vec<(Value, EmacsBytePos, EmacsBytePos)> = {
        let Some(buf) = host.edit_and_vars().0.buffers.get(current_id) else {
            return Vec::new();
        };
        buf.overlays
            .overlays_in_emacs_byte_range(EmacsByteRange::new(
                EmacsBytePos::new(search_beg),
                EmacsBytePos::new(search_end),
            ))
            .into_iter()
            .filter_map(|ov_id| {
                let start = buf.overlays.overlay_start_emacs_byte_pos(ov_id)?;
                let end = buf.overlays.overlay_end_emacs_byte_pos(ov_id)?;
                Some((ov_id, start, end))
            })
            .collect()
    };
    let beg_pos = EmacsBytePos::new(beg);
    let end_pos = EmacsBytePos::new(end);

    let mut result = Vec::new();
    for (ov_id, ov_start, ov_end) in overlay_ids {
        // GNU `report_overlay_modification` reads these hook lists with
        // `Foverlay_get`, which resolves through `lookup_char_property` -- so a
        // `category' overlay property whose value is a symbol contributes the
        // symbol's `insert-in-front-hooks' / `insert-behind-hooks' /
        // `modification-hooks' property.  Use the same category-resolving
        // lookup here instead of a plain plist read.
        let mut overlay_hook = |prop: &str| -> Option<Value> {
            let (edit, vars) = host.edit_and_vars();
            let value = crate::emacs_core::textprop::lookup_overlay_property(
                vars,
                &edit.buffers,
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
fn run_overlay_after_change_hooks<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    beg: usize,
    end: usize,
    lisp_beg: i64,
    lisp_end: i64,
    lisp_old_len: i64,
) -> Result<(), Flow> {
    let _ = (beg, end);
    // GNU passes `t` for AFTER in the after-change phase and replays the
    // hook-list/overlay pairs recorded by the before-change scan.
    run_recorded_overlay_change_hooks(host, Value::T, lisp_beg, lisp_end, Some(lisp_old_len))
}

fn run_recorded_overlay_change_hooks<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    after_flag: Value,
    lisp_beg: i64,
    lisp_end: i64,
    lisp_old_len: Option<i64>,
) -> Result<(), Flow> {
    let hooks = host
        .edit_and_vars()
        .0
        .last_overlay_modification_hooks
        .clone();
    if hooks.is_empty() {
        return Ok(());
    }
    let roots = host.save_specpdl_roots();
    for hook in &hooks {
        host.push_specpdl_root(hook.hook_list);
        host.push_specpdl_root(hook.overlay);
    }
    let apply_result = (|| -> Result<(), Flow> {
        for hook in &hooks {
            if !overlay_belongs_to_current_buffer(host, hook.overlay) {
                continue;
            }
            call_overlay_hook_list(
                host,
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
    host.restore_specpdl_roots(roots);
    apply_result
}

fn overlay_belongs_to_current_buffer<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    overlay: Value,
) -> bool {
    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return false;
    };
    overlay
        .as_overlay_data()
        .is_some_and(|data| data.buffer == Some(current_id))
}

/// C3 EditHost conversion: the whole body needs only the specpdl-root
/// API and `apply`, so it names no evaluator type (`H = Context` at
/// every caller today).
fn call_overlay_hook_list<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
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
    let root_scope = host.save_specpdl_roots();
    let cursor_slot = host.push_specpdl_root_slot(Value::NIL);
    let result = (|| -> Result<(), Flow> {
        let mut cursor = hook_list;
        while cursor.is_cons() {
            host.set_specpdl_root_slot(&cursor_slot, cursor);
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
            host.apply(func, &args)?;
            cursor = cursor.cons_cdr();
        }
        Ok(())
    })();
    host.restore_specpdl_roots(root_scope);
    result
}

#[doc(hidden)]
pub fn expect_integer_or_marker_in_buffers(
    buffers: &BufferManager,
    value: &Value,
) -> Result<LispCharPos1, Flow> {
    Ok(LispCharPos1::new(
        super::position::fix_position_with_buffers(buffers, value)?,
    ))
}

#[doc(hidden)]
pub fn current_buffer_accessible_char_region_in_buffers(
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
    use neovm_value::emacs_char;
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
                let code = expect_character_code(arg)? as u32;
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

/// `(delete-char N &optional KILLFLAG)` — delete N characters forward.
#[doc(hidden)]
pub fn builtin_delete_char<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: &[Value],
) -> EvalResult {
    expect_min_args("delete-char", &args, 1)?;
    expect_max_args("delete-char", &args, 2)?;
    // GNU cmds.c:233 uses CHECK_FIXNUM, including for bignums.
    let n = neovm_value::error::expect_fixnum(&args[0])?;
    let killflag = args.get(1).is_some_and(|v| v.is_truthy());
    if n.unsigned_abs() < 2 {
        // GNU `Fdelete_char' calls this too, but does not intern its name
        // on the way: every single-character deletion runs this line.
        host.apply(Value::from_sym_id(undo_auto_amalgamate_symbol()), &[])?;
    }
    // GNU `Fdelete_char` (cmds.c:221) dispatches to `kill-forward-chars`
    // when KILLFLAG is non-nil, saving the deleted text in the kill ring.
    if killflag {
        return host.apply(Value::from_sym_id(kill_forward_chars_symbol()), &[args[0]]);
    }
    if let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() {
        let Some(byte_range) = ({
            let Some(buf) = host.edit_and_vars().0.buffers.get(current_id) else {
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
        let delete_range = buffer_edit_range_for_byte_range_in_manager(
            &host.edit_and_vars().0.buffers,
            current_id,
            byte_range,
        )?;
        {
            let (edit, vars) = host.edit_and_vars();
            crate::emacs_core::textprop::verify_text_read_only_in_state(
                vars,
                &edit.buffers,
                current_id,
                start,
                end,
            )?;
        }
        // GNU `Fdelete_char` -> `del_range (PT, pos)` -> `del_range_1`.
        delete_text_range(host, delete_range, DeletedText::Discard)?;
    }
    Ok(Value::NIL)
}

/// `(delete-region START END)` — delete text in the accessible current buffer.
#[doc(hidden)]
pub fn builtin_delete_region<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args("delete-region", &args, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_delete_region_2(host, arg(0), arg(1))
}
/// `delete-region` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_delete_region_2<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    start: Value,
    end: Value,
) -> EvalResult {
    let args: [Value; 2] = [start, end];
    expect_args("delete-region", &args, 2)?;
    let Some(byte_range) = current_buffer_accessible_char_region_in_buffers(
        &host.edit_and_vars().0.buffers,
        &args[0],
        &args[1],
    )?
    else {
        return Ok(Value::NIL);
    };
    let start_byte = byte_range.start().get();
    let end_byte = byte_range.end().get();
    if start_byte == end_byte {
        return Ok(Value::NIL);
    }

    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };
    let read_only = {
        let (edit, vars) = host.edit_and_vars();
        edit.buffers
            .get(current_id)
            .is_some_and(|buf| buffer_read_only_active_in_state(vars, &[], buf))
    };
    if read_only {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }
    {
        let (edit, vars) = host.edit_and_vars();
        crate::emacs_core::textprop::verify_text_read_only_in_state(
            vars,
            &edit.buffers,
            current_id,
            start_byte,
            end_byte,
        )?;
    }

    let delete_range = buffer_edit_range_for_byte_range_in_manager(
        &host.edit_and_vars().0.buffers,
        current_id,
        byte_range,
    )?;
    // GNU `Fdelete_region` -> `del_range` -> `del_range_1`.
    delete_text_range(host, delete_range, DeletedText::Discard)?;
    Ok(Value::NIL)
}

/// `(delete-and-extract-region START END)` — delete text and return it.
#[doc(hidden)]
pub fn builtin_delete_and_extract_region<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-and-extract-region", &args, 2)?;
    let Some(byte_range) = current_buffer_accessible_char_region_in_buffers(
        &host.edit_and_vars().0.buffers,
        &args[0],
        &args[1],
    )?
    else {
        return Ok(Value::string(""));
    };
    let start_byte = byte_range.start().get();
    let end_byte = byte_range.end().get();
    if start_byte == end_byte {
        return Ok(Value::string(""));
    }

    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(Value::string(""));
    };
    {
        let (edit, vars) = host.edit_and_vars();
        let Some(buf) = edit.buffers.get(current_id) else {
            return Ok(Value::string(""));
        };
        if buffer_read_only_active_in_state(vars, &[], buf) {
            return Err(signal(
                LispCondition::BufferReadOnly,
                vec![Value::make_buffer(current_id)],
            ));
        }
    }
    {
        let (edit, vars) = host.edit_and_vars();
        crate::emacs_core::textprop::verify_text_read_only_in_state(
            vars,
            &edit.buffers,
            current_id,
            start_byte,
            end_byte,
        )?;
    }

    let delete_range = buffer_edit_range_for_byte_range_in_manager(
        &host.edit_and_vars().0.buffers,
        current_id,
        byte_range,
    )?;
    // GNU `del_range_1 (from, to, true, true)`: the string is made by
    // `del_range_2` AFTER `prepare_to_modify_buffer` ran the before-change
    // hooks, and it is the same string `record_delete` gets -- one
    // `make_buffer_string_both`, not a substring plus a second copy for undo.
    Ok(delete_text_range(host, delete_range, DeletedText::Return)?
        .map_or_else(|| Value::string(""), Value::heap_string))
}

/// `(erase-buffer)` — delete all text and remove any narrowing restriction.
#[doc(hidden)]
pub fn builtin_erase_buffer<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("erase-buffer", &args, 0)?;
    let Some(current_id) = host.edit_and_vars().0.buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };
    let byte_range = host
        .edit_and_vars()
        .0
        .buffers
        .get(current_id)
        .map(|buf| buf.full_emacs_byte_range())
        .unwrap_or(EmacsByteRange::EMPTY);
    let delete_range = buffer_edit_range_for_byte_range_in_manager(
        &host.edit_and_vars().0.buffers,
        current_id,
        byte_range,
    )?;
    let change = TextChange::deletion(delete_range);
    if !byte_range.is_empty() {
        signal_before_text_change(host, change)?;
    }
    // Edit phase: the read view and the edit runtime come from the one
    // disjoint-borrow pair; while `edit`/`vars` live, no host call runs.
    let (edit, vars) = host.edit_and_vars();
    erase_buffer_impl(vars, &[], &mut edit.buffers, vec![])?;
    if !byte_range.is_empty() {
        signal_after_text_change(host, change)?;
    }
    Ok(Value::NIL)
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn erase_buffer_impl<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("erase-buffer", &args, 0)?;
    let Some(current_id) = buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };

    let should_signal_read_only = buffers.get(current_id).is_some_and(|buf| {
        !buf.is_text_empty() && buffer_read_only_active_in_state(vars, dynamic, buf)
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
#[doc(hidden)]
pub fn builtin_buffer_substring_no_properties<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("buffer-substring-no-properties", &args, 2)?;
    // No Lisp runs between resolving the region and copying it (the
    // fontify hook belongs to `buffer-substring`, not this one), so the
    // character count the resolver computed still describes the bytes.
    let Some((byte_range, chars)) = current_buffer_accessible_char_region_with_chars(
        &host.edit_and_vars().0.buffers,
        &args[0],
        &args[1],
    )?
    else {
        return Ok(Value::heap_string(
            neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };
    let (edit, _vars) = host.edit_and_vars();
    let Some(buf) = edit.buffers.current_buffer() else {
        return Ok(Value::heap_string(
            neovm_value::heap_types::LispString::from_emacs_bytes(Vec::new()),
        ));
    };
    let mut bytes = Vec::new();
    buf.copy_emacs_byte_range_to(byte_range, &mut bytes);
    Ok(Value::heap_string(if buf.get_multibyte() {
        neovm_value::heap_types::LispString::from_emacs_bytes_with_chars(bytes, chars)
    } else {
        super::buffer::lisp_string_from_buffer_bytes(bytes, false)
    }))
}

/// `(following-char)` — return character after point (0 if at end).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_following_char<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("following-char", &args, 0)?;
    following_char_value(&host.edit_and_vars().0.buffers)
}

#[doc(hidden)]
pub fn builtin_following_char_0<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
) -> EvalResult {
    following_char_value(&host.edit_and_vars().0.buffers)
}

fn following_char_value(buffers: &BufferManager) -> EvalResult {
    match buffers.current_buffer() {
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
#[doc(hidden)]
pub fn builtin_preceding_char<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("preceding-char", &args, 0)?;
    match host.edit_and_vars().0.buffers.current_buffer() {
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

/// `(file-user-uid)` — return the UID used for file ownership.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up

/// `(file-group-gid)` — return the GID used for file ownership.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up

/// `(group-name GID)` — return the group name for numeric GID.
#[doc(hidden)]
pub fn builtin_group_name(args: Vec<Value>) -> EvalResult {
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
#[doc(hidden)]
pub fn builtin_load_average(args: Vec<Value>) -> EvalResult {
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
#[doc(hidden)]
pub fn builtin_logcount(args: Vec<Value>) -> EvalResult {
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
    use neovm_value::emacs_char::string_char_advance;

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
                c.code() as i64
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
    use neovm_value::emacs_char::{MAX_CHAR, MAX_MULTIBYTE_LENGTH, char_string};

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
#[doc(hidden)]
pub fn builtin_translate_region_internal<H: crate::emacs_core::edit_host::EditHost>(
    host: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    use neovm_value::chartab::{builtin_char_table_subtype, ct_lookup_with_symbols, is_char_table};
    use neovm_value::emacs_char::{
        MAX_CHAR, MAX_MULTIBYTE_LENGTH, byte8_to_char, char_string, chars_in_multibyte,
        string_char_advance,
    };

    expect_args("translate-region-internal", &args, 3)?;
    let table = &args[2];

    // ----- Validate TABLE ----------------------------------------------------
    // The epoch-checked borrow is materialized to owned bytes up front: its
    // validation order is kept (a non-table signals before the region is
    // resolved), while no string borrow spans the edit phases below.
    let table_bytes = host
        .lisp_string(*table)
        .map(|s| (s.as_bytes().to_vec(), s.is_multibyte()));
    let is_str_table = table_bytes.is_some();
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
        if !neovm_value::value::eq_value_swp(
            &purpose,
            &translation_sym,
            matches!(
                host.symbol_mode(),
                neovm_value::tagged::SymbolMode::Positioned
            ),
        ) {
            return Err(signal(
                "error",
                vec![Value::string("Not a translation table")],
            ));
        }
    }

    // ----- Resolve region in the current buffer ------------------------------
    let (buffer_id, byte_range) = normalize_current_buffer_region_bounds_in_manager(
        &host.edit_and_vars().0.buffers,
        &args[0],
        &args[1],
    )?;
    if byte_range.is_empty() {
        return Ok(Value::fixnum(0));
    }
    let multibyte = host
        .edit_and_vars()
        .0
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
    let (point_char, region_start_char) = host
        .edit_and_vars()
        .0
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
    let source = read_buffer_region_bytes_in_manager(
        &host.edit_and_vars().0.buffers,
        buffer_id,
        byte_range,
    )?;

    // ----- String-table prep -------------------------------------------------
    let table_string_info: Option<(Vec<u8>, bool)> = table_bytes.map(|(mut bytes, mut mb)| {
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
            (c.code() as i64, q - p)
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
                        let (_c, l) = neovm_value::emacs_char::string_char(&tt[bp..]);
                        bp += l.max(1);
                        idx += 1;
                    }
                    if bp < tt.len() {
                        let mut qq = bp;
                        let c = string_char_advance(tt, &mut qq);
                        nc = c.code() as i64;
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
                let val = ct_lookup_with_symbols(table, oc, host.symbol_mode())?;
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
            neovm_value::heap_types::LispString::from_emacs_bytes(out)
        } else {
            neovm_value::heap_types::LispString::from_unibyte(out)
        };
        replace_buffer_emacs_byte_range_lisp_string(host, buffer_id, byte_range, &replacement)?;

        // Restore point to the GNU-faithful character position.  The
        // whole-region replace dragged point to the end of the inserted
        // replacement; GNU instead leaves point at its original character
        // position shifted only by replacements ending at or before it.
        let target_char = point_char + point_char_delta;
        if let Some(byte_pos) = host
            .edit_and_vars()
            .0
            .buffers
            .get(buffer_id)
            .map(|b| b.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(target_char)))
        {
            let _ = host
                .edit_and_vars()
                .0
                .buffers
                .goto_buffer_emacs_byte_pos(buffer_id, byte_pos);
        }
    }

    Ok(Value::fixnum(characters_changed))
}

#[cfg(test)]
#[path = "tests/editfns_test.rs"]
mod tests;
