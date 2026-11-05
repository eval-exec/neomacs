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
