//! Test-only authority fence for core-owned tests of the moved system tests.

/// FENCE (N2 TLS reset): zero the per-thread new-child-command call counter.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_new_child_command_calls_for_test() {
    crate::emacs_core::callproc::reset_new_child_command_calls_for_test();
}
