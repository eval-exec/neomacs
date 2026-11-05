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
