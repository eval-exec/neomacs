//! Unarmed background-compile knobs after startup match production.
//!
//! Core tests compile neovm-eval with `test-support` and without `cfg(test)`.
//! An unarmed read must therefore take the production mode selection and the
//! production inline threshold, not the eval unit-test defaults.

#[test]
fn bg_unarmed_startup_matches_production_defaults_on_linux_x86_64() {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return;
    }
    let _context = crate::test_utils::runtime_startup_context();
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let expected_mode = match std::env::var("NEOVM_JIT_BG").ok().as_deref().map(str::trim) {
        Some("legacy") => "legacy",
        Some("sync") => "sync",
        Some("on") => "on",
        _ if cpus >= 2 => "on",
        _ => "sync",
    };
    assert_eq!(
        neovm_eval::test_support::authority::bg_mode_name_for_test(),
        expected_mode,
        "unarmed mode after startup is the production selection"
    );
    let expected_insts = std::env::var("NEOVM_JIT_BG_INLINE_INSTS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(8192);
    assert_eq!(
        neovm_eval::test_support::authority::in_line_insts_for_test(),
        expected_insts,
        "unarmed inline threshold after startup is the production knob"
    );
}
