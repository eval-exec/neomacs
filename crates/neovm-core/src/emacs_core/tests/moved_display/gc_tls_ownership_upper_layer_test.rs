//! Xfaces GC/TLS-ownership tests that need the core upper layers.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xfaces::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn gc_tls_ownership_font_registry_names_do_not_retain_unrooted_properties() {
    let mut ctx = crate::test_utils::new_context();
    let normalized = ctx.eval_str("(let ((name (propertize \"ISO-10646-1\" 'gc-tls-payload (vector 42)))) (internal-set-alternative-font-registry-alist (list (list name))))").unwrap();
    assert!(
        normalized
            .cons_car()
            .cons_car()
            .as_lisp_string()
            .unwrap()
            .has_intervals(),
        "the normalized Lisp result must preserve GNU-visible properties"
    );
    let state = crate::emacs_core::font::alternative_font_registry_alist_snapshot_for_test();
    assert!(
        state.iter().all(|(name, aliases)| !name.has_intervals()
            && aliases.iter().all(|alias| !alias.has_intervals())),
        "the static font name registry copied heap Values in string text properties"
    );
    drop(state);
    ctx.gc_collect_exact();
    assert_eq!(
        crate::emacs_core::font::alternative_font_registries("iso-10646-1"),
        vec!["iso-10646-1"]
    );
    builtin_internal_set_alternative_font_registry_alist(vec![Value::NIL]).unwrap();
}
