use super::*;
use neovm_eval::emacs_core::eval::Context;

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    ctx.collect_font_registry_gc_roots_for_test(&mut roots);
    roots
}

#[test]
fn gc_tls_ownership_font_reset_discards_a_dropped_heap() {
    let old = {
        let ctx = Context::new(crate::test_utils::unit_runtime());
        let value = Value::vector(vec![Value::fixnum(42)]);
        set_face_override("default", LFaceAttr::Font, value, false);
        assert!(
            roots_for(&ctx)
                .iter()
                .any(|root| root.bits() == value.bits())
        );
        value
    };
    let mut next = Context::new(crate::test_utils::unit_runtime());
    assert!(
        !roots_for(&next)
            .iter()
            .any(|root| root.bits() == old.bits())
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_font_roots_exclude_another_live_heap() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let parked_first = first.park_for_test();
    let second = Context::new(crate::test_utils::unit_runtime());
    let value = Value::vector(vec![Value::fixnum(42)]);
    set_face_override("default", LFaceAttr::Font, value, false);
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| first.tagged_heap().owns_heap_value_for_test(*root)),
        "font overrides from the second Context are roots of the first heap"
    );
    first.gc_collect_exact();
    let _parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    second.gc_collect_exact();
    assert_eq!(value.as_vector_data().unwrap().at(0), Value::fixnum(42));
}

#[test]
fn gc_tls_ownership_font_overrides_follow_context_swaps() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let a = Value::vector(vec![Value::fixnum(1)]);
    set_face_override("default", LFaceAttr::Font, a, false);
    let parked_first = first.park_for_test();
    let second = Context::new(crate::test_utils::unit_runtime());
    let b = Value::vector(vec![Value::fixnum(2)]);
    set_face_override("default", LFaceAttr::Font, b, false);
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert_eq!(
        get_face_override("default", LFaceAttr::Font, false)
            .unwrap()
            .bits(),
        a.bits()
    );
    first.gc_collect_exact();
    let _parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert_eq!(
        get_face_override("default", LFaceAttr::Font, false)
            .unwrap()
            .bits(),
        b.bits()
    );
    second.gc_collect_exact();
}
