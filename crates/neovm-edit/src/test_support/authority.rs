//! Test-only authority fence for core-owned tests of the moved edit tests.

/// P1-CLASSES-ALL-CRATES FENCE: typed write of GNU `last_window_start`.
/// A redisplay cache reset is a write, so the fence is the only cross-crate
/// route; the raw setter stays pub(crate) and cfg(any(test, feature)).
pub fn set_last_window_start_for_test(
    buffer: &mut crate::buffer::buffer::Buffer,
    value: crate::buffer::LispCharPos1,
) {
    buffer.set_last_window_start_raw(value);
}

/// P1-CLASSES-ALL-CRATES FENCE (C1 write): splice a live marker onto this
/// text's marker chain head for the relocated marker-unchain test. The chain
/// link is heap MarkerObj state, so the write is unsafe; the raw method stays
/// pub(crate).
///
/// # Safety
/// `marker` must be a live marker owned by this text's heap and not already
/// on a chain, and the serial fixture must hold the heap/owner exclusively.
pub unsafe fn chain_splice_at_head_for_test(
    text: &crate::buffer::buffer_text::BufferText,
    marker: *mut neovm_value::tagged::header::MarkerObj,
) {
    text.chain_splice_at_head(marker);
}

/// P1-CLASSES-ALL-CRATES FENCE (N2 counter reset): zero the buffer-local
/// value-lookup probe counter for the moved eval fixtures. The counter stays
/// a thread-local in `buffer::buffer`; the increment site and this reset are
/// both under the `cfg(any(test, feature = "test-support"))` gate.
pub fn reset_buffer_local_value_lookup_probes() {
    crate::buffer::buffer::reset_buffer_local_value_lookup_probes();
}

/// P1-CLASSES-ALL-CRATES FENCE (LEAD 10:1xZ retraction): zero the screen-line
/// scan-step counter for the display fixtures. A reset is a write, so the
/// fence is the only cross-crate route; the original stays pub(crate) with
/// no visibility widening.
pub fn reset_screen_line_scan_steps_for_test() {
    crate::emacs_core::indent::reset_screen_line_scan_steps_for_test();
}

/// P1-CLASSES-ALL-CRATES FENCE (LEAD 10:1xZ retraction): zero the
/// display-stop recompute counter for the display fixtures. A reset is a
/// write; the original stays pub(crate) with no visibility widening.
pub fn reset_display_stop_recomputes_for_test() {
    crate::emacs_core::indent::reset_display_stop_recomputes_for_test();
}

/// P1-CLASSES-ALL-CRATES FENCE (LEAD 10:1xZ retraction): zero the char/byte
/// conversion call counter for the display fixtures. A reset is a write; the
/// original stays pub(crate) with no visibility widening.
pub fn reset_char_pos_to_emacs_byte_pos_call_count() {
    crate::buffer::buffer_text::reset_char_pos_to_emacs_byte_pos_call_count();
}

/// FENCE (N2 TLS reset): zero the local-var alist probe counter on the test
/// thread. The original stays pub(crate) with no visibility widening.
pub fn reset_local_var_alist_entry_probes() {
    crate::buffer::buffer::reset_local_var_alist_entry_probes();
}

/// Setter fence for `Buffer::undo_state` (peer #816 write arm): the moved
/// indirect-buffer dump/restore test replaces the whole undo state. The
/// field stays a read surface; the write routes through this fence.
pub fn set_buffer_undo_state_for_test(
    buffer: &mut crate::buffer::buffer::Buffer,
    state: crate::buffer::SharedUndoState,
) {
    buffer.undo_state = state;
}

/// Constructor fence for `CasingPropertyControls` (peer #816 write arm):
/// every field explicit, including the private `mutator` marker. The moved
/// casing tests build the controls with literals; this fence is the only test
/// construction path.
pub fn casing_property_controls_for_test(
    equality: crate::buffer::text_props::casing_insertion::CasingPropertyEquality,
    nonsticky: neovm_value::value::Value,
    aliases: neovm_value::value::Value,
    defaults: neovm_value::value::Value,
    mutator: std::marker::PhantomData<*const ()>,
) -> crate::buffer::text_props::CasingPropertyControls {
    crate::buffer::text_props::CasingPropertyControls {
        equality,
        nonsticky,
        aliases,
        defaults,
        mutator,
    }
}

/// Constructor fence for `CasingPropertyContext` (peer #816 write arm):
/// every field explicit; the moved casing tests' literals switch to it.
pub fn casing_property_context_for_test<'a>(
    obarray: &'a neovm_value::symbol::Obarray,
    controls: crate::buffer::text_props::CasingPropertyControls,
) -> crate::buffer::text_props::casing_insertion::CasingPropertyContext<'a> {
    crate::buffer::text_props::casing_insertion::CasingPropertyContext { obarray, controls }
}

/// LEAD #1052 FENCE: write `BufferText::set_modified_tick` through the
/// buffer.  The raw method stays pub(crate) and cfg(any(test, feature));
/// this fence is the only cross-crate write route (the `text` field is
/// pub(crate) in every arm).
pub fn set_text_modified_tick_for_test(buffer: &mut crate::buffer::buffer::Buffer, tick: i64) {
    buffer.set_text_modified_tick_raw(tick);
}

/// LEAD #1052 FENCE: write `BufferText::set_modification_state` through the
/// buffer.  The raw method stays pub(crate) and cfg(any(test, feature)); this
/// fence is the only cross-crate write route.
pub fn set_text_modification_state_for_test(
    buffer: &mut crate::buffer::buffer::Buffer,
    modified_tick: i64,
    chars_modified_tick: i64,
    save_modified_tick: i64,
) {
    buffer.set_text_modification_state_raw(modified_tick, chars_modified_tick, save_modified_tick);
}

/// LEAD #1052 FENCE: record the editor-global saved point through the
/// buffer's `SavedPointBeforeCommand` cell.  The raw method stays pub(crate)
/// and cfg(any(test, feature)); this fence is the only cross-crate write
/// route and the interior `Rc<Cell<_>>` never escapes this crate.
pub fn save_point_before_command_for_test(
    buffer: &mut crate::buffer::buffer::Buffer,
    buffer_id: crate::buffer::BufferId,
    point: crate::buffer::CharPos0,
) {
    buffer.save_point_before_command_raw(buffer_id, point);
}
