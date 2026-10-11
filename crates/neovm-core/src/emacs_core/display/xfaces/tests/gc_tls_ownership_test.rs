use super::*;
use crate::emacs_core::eval::Context;

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_font_registry_gc_roots(&ctx.font_registry, &mut roots);
    roots
}

#[test]
fn gc_tls_ownership_font_reset_discards_a_dropped_heap() {
    let old = {
        let ctx = Context::new();
        let value = Value::vector(vec![Value::fixnum(42)]);
        set_face_override("default", LFaceAttr::Font, value, false);
        assert!(
            roots_for(&ctx)
                .iter()
                .any(|root| root.bits() == value.bits())
        );
        value
    };
    let mut next = Context::new();
    assert!(
        !roots_for(&next)
            .iter()
            .any(|root| root.bits() == old.bits())
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_font_roots_exclude_another_live_heap() {
    let mut first = Context::new();
    let mut second = Context::new();
    let value = Value::vector(vec![Value::fixnum(42)]);
    set_face_override("default", LFaceAttr::Font, value, false);
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    first.setup_thread_locals();
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| first.tagged_heap.owns_heap_value_for_test(*root)),
        "font overrides from the second Context are roots of the first heap"
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert_eq!(value.as_vector_data().unwrap()[0], Value::fixnum(42));
}

#[test]
fn gc_tls_ownership_font_registry_names_do_not_retain_unrooted_properties() {
    let mut ctx = Context::new();
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
    let state = alternative_font_registry_alist().read().unwrap();
    assert!(
        state
            .iter()
            .all(|(name, aliases)| !name.to_lisp_string().has_intervals()
                && aliases
                    .iter()
                    .all(|alias| !alias.to_lisp_string().has_intervals())),
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

#[test]
fn gc_tls_ownership_font_overrides_follow_context_swaps() {
    let mut first = Context::new();
    let a = Value::vector(vec![Value::fixnum(1)]);
    set_face_override("default", LFaceAttr::Font, a, false);
    let mut second = Context::new();
    let b = Value::vector(vec![Value::fixnum(2)]);
    set_face_override("default", LFaceAttr::Font, b, false);
    first.setup_thread_locals();
    assert_eq!(
        get_face_override("default", LFaceAttr::Font, false)
            .unwrap()
            .bits(),
        a.bits()
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    assert_eq!(
        get_face_override("default", LFaceAttr::Font, false)
            .unwrap()
            .bits(),
        b.bits()
    );
    second.gc_collect_exact();
}
