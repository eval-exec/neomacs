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

/// FENCE (N2 TLS reset): clear this thread's regex pattern caches.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn clear_regex_caches() {
    crate::emacs_core::regex::clear_regex_caches();
}

/// FENCE (N2 TLS reset): reset this thread's existence-DFA filter counters.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_dfa_stats() {
    crate::emacs_core::regex_emacs::dfa::reset_dfa_stats();
}

/// FENCE (N2 TLS reset): reset this thread's syntax parse-cache stats.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_parse_cache_stats() {
    crate::emacs_core::syntax::parse_cache::reset_parse_cache_stats();
}

/// FENCE (N2 TLS reset): zero the per-thread syntax-table decode counter.
/// The original stays pub(crate) with no visibility widening; this fence is
/// its only cross-crate route (LEAD 18:1xZ).
pub fn reset_syntax_table_decodes_for_test() {
    crate::emacs_core::syntax::reset_syntax_table_decodes_for_test();
}

/// FENCE (R4-F2, peer P5): arm this process's existence-DFA filter counters.
/// The runtime stats enable stays `cfg!(test)`; without this explicit arming
/// a test-support dependency build records nothing (own-build discriminator).
/// Inert unless called.  Arming persists for the rest of the process (there
/// is no disarm); the moved DFA tests rely on nextest running every test in
/// its own process, so a later test in the same process would still observe
/// the counters armed.
pub fn enable_dfa_stats_for_test() {
    crate::emacs_core::regex_emacs::dfa::enable_dfa_stats_for_test();
}

/// FENCE (global charset-state write): reorder the process charset registry,
/// as `set-charset-priority` does.  The fontset ordering tests in display need
/// this write to prove the charset-preference substrate hook republishes the
/// rendering order.  Lisp is unreachable there: the subr is registered by the
/// upper core layer, which display's unit runtime replaces with the signed
/// absent-layer stand-in, so the global write routes through this fence.  The
/// original builtin stays `pub(crate)`; its pre-existing test-support widening
/// is the signed TEST-SUPPORT-VIS-TEXT row used by the moved core tests.
pub fn set_charset_priority_for_test(
    args: Vec<neovm_value::value::Value>,
) -> neovm_value::error::EvalResult {
    crate::emacs_core::charset::builtin_set_charset_priority(args)
}

/// FENCE (global charset-state write): define or re-point a charset alias, as
/// `define-charset-alias` does.  Same reachability argument as
/// [`set_charset_priority_for_test`]: the alias mutation is global charset
/// state and the original builtin stays `pub(crate)`.
pub fn define_charset_alias_for_test(
    args: Vec<neovm_value::value::Value>,
) -> neovm_value::error::EvalResult {
    crate::emacs_core::charset::builtin_define_charset_alias(args)
}
