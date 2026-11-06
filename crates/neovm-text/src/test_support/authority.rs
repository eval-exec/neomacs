//! Test-only authority fence for core-owned tests of the moved regex probes.
//!
//! Writes and resets stay in this module: the originals are plain
//! `pub(crate)` with no visibility widening, and this fence is their only
//! cross-crate route.  Reads use the TEST-SUPPORT-VIS rows instead.

/// P1-CLASSES-ALL-CRATES FENCE: run `f` with the syntax-class fusion
/// peephole disabled and restore the previous policy afterwards.  This is a
/// scoped thread-local write, so the fence is the only cross-crate route;
/// the original `with_syntax_fusion_disabled` stays `pub(crate)`.
pub fn with_syntax_fusion_disabled_for_test<R>(f: impl FnOnce() -> R) -> R {
    crate::emacs_core::regex_emacs::with_syntax_fusion_disabled(f)
}

/// P1-CLASSES-ALL-CRATES FENCE: zero the debug-only publish/read counters
/// used by the lazy-match-data probe.  A counter reset is a write, so the
/// fence is the only cross-crate route; the original `reset` stays
/// `pub(crate)`.  The counters exist only in debug builds, where the probe
/// runs; optimized builds have nothing to reset.
pub fn reset_regex_match_stats_for_test() {
    #[cfg(debug_assertions)]
    crate::emacs_core::regex::match_stats::reset();
}
