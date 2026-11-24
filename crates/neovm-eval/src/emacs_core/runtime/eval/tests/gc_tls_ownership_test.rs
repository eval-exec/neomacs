use super::*;

#[test]
fn storefast_world_certificate_survives_another_worlds_journal_wrap() {
    use neovm_value::tagged::collection_reads::capture;

    let mut first = Context::new(crate::test_utils::unit_runtime());
    let source = Value::cons(Value::NIL, Value::NIL);
    first.set_variable("storefast-source", source);
    let (_, certificate) = capture(|| source.cons_car());
    let certificate = certificate.expect("a stable source can be certified");

    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    assert!(
        certificate.unchanged(),
        "validation uses its originating World's sequence space"
    );
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    second.set_variable("storefast-unrelated", unrelated);
    // Exceed one complete journal: identities alone cannot hide accidental
    // sharing of the revision/ring between these independent Worlds.
    for _ in 0..8193 {
        unrelated.set_car(Value::T);
    }

    let _parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert!(
        certificate.unchanged(),
        "another World's writes must not consume this World's journal"
    );
    source.set_cdr(Value::T);
    assert!(
        !certificate.unchanged(),
        "the owner's write still invalidates"
    );
}

#[test]
fn storefast_world_capture_rejects_a_local_world_switch_without_stranding_its_guard() {
    use neovm_value::tagged::collection_reads::capture;

    let mut first = Context::new(crate::test_utils::unit_runtime());
    let source = Value::cons(Value::NIL, Value::NIL);
    first.set_variable("storefast-source", source);
    let mut first_slot = Some(first);
    let (_, certificate) = capture(|| {
        source.cons_car();
        let parked = first_slot.take().unwrap().park_for_test();
        let mut second = Context::new(crate::test_utils::unit_runtime());
        let second_source = Value::cons(Value::NIL, Value::NIL);
        let (_, local) = capture(|| second_source.cons_car());
        assert!(local.unwrap().unchanged());
        second.gc_collect_exact();
        drop(second);
        first_slot = Some(parked.resume_for_test().expect("resume first"));
    });
    assert!(certificate.is_none());
    let mut first = first_slot.take().unwrap();
    let (_, certificate) = capture(|| source.cons_car());
    assert!(certificate.unwrap().unchanged());
}

#[test]
fn storefast_world_capture_rejects_foreign_reads_after_return_to_its_owner() {
    use neovm_value::tagged::collection_reads::capture;

    let mut first = Context::new(crate::test_utils::unit_runtime());
    let first_source = Value::cons(Value::NIL, Value::NIL);
    first.set_variable("storefast-source", first_source);
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    let second_source = Value::cons(Value::NIL, Value::NIL);
    second.set_variable("storefast-source", second_source);
    let mut second_slot = Some(second.park_for_test());
    let mut first_slot = Some(parked_first.resume_for_test().expect("resume first"));

    let (_, certificate) = capture(|| {
        first_source.cons_car();
        let parked_first = first_slot.take().unwrap().park_for_test();
        let mut second = second_slot
            .take()
            .unwrap()
            .resume_for_test()
            .expect("resume second");
        second_source.cons_car();
        second_slot = Some(second.park_for_test());
        first_slot = Some(parked_first.resume_for_test().expect("resume first"));
    });
    let mut first = first_slot.take().unwrap();
    assert!(
        certificate.is_none(),
        "a World-local certificate must not omit foreign dependency reads"
    );
    let (_, local) = capture(|| first_source.cons_car());
    assert!(local.unwrap().unchanged());
}

#[test]
fn gc_tls_ownership_exact_collection_uses_the_collecting_heap() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    // Each collection runs with that Context installed. Parking is what
    // lets the other constructor run; the collector still sees its own heap.
    second.gc_collect_exact();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    first.gc_collect_exact();
    let parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    second.gc_collect_exact();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    first.gc_collect_exact();
    drop(parked_second);
}

#[test]
fn gc_tls_ownership_safe_point_uses_the_collecting_heap() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    second.gc_collect_from_current_roots();
    let _parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    first.gc_collect_from_current_roots();
}

#[test]
fn gc_tls_ownership_semantic_registries_have_independent_thread_owners() {
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut first = Context::new(crate::test_utils::unit_runtime());
        first
            .eval_str("(register-ccl-program 'gc-tls-worker-program [0 0 0])")
            .unwrap();
        receive.recv().unwrap();
        assert!(neovm_text::emacs_core::ccl::is_registered_ccl_program(
            intern("gc-tls-worker-program")
        ));
        first.gc_collect_exact();
    });
    let mut second = Context::new(crate::test_utils::unit_runtime());
    send.send(()).unwrap();
    second.gc_collect_exact();
    worker.join().unwrap();
    assert!(!neovm_text::emacs_core::ccl::is_registered_ccl_program(
        intern("gc-tls-worker-program")
    ));
}

fn registered_public_entry_contexts() -> (super::parked_context::ParkedContext, Context) {
    let mut first = Context::new(crate::test_utils::unit_runtime());
    first
        .eval_str("(register-ccl-program 'gc-tls-public-first [0 0 0])")
        .unwrap();
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    second
        .eval_str("(register-ccl-program 'gc-tls-public-second [0 0 0])")
        .unwrap();
    (parked_first, second)
}

fn assert_ccl_registry_programs(registered: &[&str], missing: &[&str]) {
    for name in registered {
        assert!(
            neovm_text::emacs_core::ccl::is_registered_ccl_program(intern(name)),
            "public evaluation left the active CCL registry without {name}"
        );
    }
    for name in missing {
        assert!(
            !neovm_text::emacs_core::ccl::is_registered_ccl_program(intern(name)),
            "public evaluation left another Context's {name} in the active CCL registry"
        );
    }
}

#[test]
fn gc_tls_ownership_eval_str_activates_its_context_registries() {
    let (parked_first, second) = registered_public_entry_contexts();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert!(
        first
            .eval_str("(ccl-program-p 'gc-tls-public-first)")
            .unwrap()
            .is_t(),
        "eval_str used the resumed Context's CCL registry"
    );
    assert_ccl_registry_programs(&["gc-tls-public-first"], &["gc-tls-public-second"]);
    let parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert!(
        second
            .eval_str("(ccl-program-p 'gc-tls-public-second)")
            .unwrap()
            .is_t()
    );
    assert_ccl_registry_programs(
        &["gc-tls-public-second"],
        &["gc-tls-public-first", "gc-tls-public-extra"],
    );
    assert!(
        second
            .eval_str("(ccl-program-p 'gc-tls-public-first)")
            .unwrap()
            .is_nil()
    );
    second.gc_collect_exact();
    let _parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    first.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_eval_form_activates_its_context_registries() {
    let mut first = Context::new(crate::test_utils::unit_runtime());
    first
        .eval_str("(register-ccl-program 'gc-tls-public-first [0 0 0])")
        .unwrap();
    let form = first
        .eval_str("'(ccl-program-p 'gc-tls-public-first)")
        .unwrap();
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    second
        .eval_str("(register-ccl-program 'gc-tls-public-second [0 0 0])")
        .unwrap();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert!(
        first.eval_form(form).unwrap().is_t(),
        "eval_form used the resumed Context's CCL registry"
    );
    assert_ccl_registry_programs(&["gc-tls-public-first"], &["gc-tls-public-second"]);
    let parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert!(
        second
            .eval_str("(ccl-program-p 'gc-tls-public-second)")
            .unwrap()
            .is_t()
    );
    assert_ccl_registry_programs(
        &["gc-tls-public-second"],
        &["gc-tls-public-first", "gc-tls-public-extra"],
    );
    second.gc_collect_exact();
    drop(second);
    let mut first = parked_first.resume_for_test().expect("resume first");
    first.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_eval_value_activates_its_context_registries() {
    let mut first = Context::new(crate::test_utils::unit_runtime());
    first
        .eval_str("(register-ccl-program 'gc-tls-public-first [0 0 0])")
        .unwrap();
    let form = first
        .eval_str("'(ccl-program-p 'gc-tls-public-first)")
        .unwrap();
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    second
        .eval_str("(register-ccl-program 'gc-tls-public-second [0 0 0])")
        .unwrap();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert!(first.eval_value(&form).unwrap().is_t());
    assert_ccl_registry_programs(&["gc-tls-public-first"], &["gc-tls-public-second"]);
    first.gc_collect_exact();
    let _parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    second.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_eval_str_each_activates_its_context_registries() {
    let (parked_first, second) = registered_public_entry_contexts();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    let results = first.eval_str_each(
        "(ccl-program-p 'gc-tls-public-first)
         (register-ccl-program 'gc-tls-public-extra [0 0 0])
         (ccl-program-p 'gc-tls-public-extra)",
    );
    assert_eq!(results.len(), 3);
    assert!(
        results[0].as_ref().unwrap().is_t(),
        "eval_str_each used the resumed Context's CCL registry"
    );
    assert!(results[2].as_ref().unwrap().is_t());
    assert_ccl_registry_programs(
        &["gc-tls-public-first", "gc-tls-public-extra"],
        &["gc-tls-public-second"],
    );
    let parked_first = first.park_for_test();
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert!(
        second
            .eval_str("(ccl-program-p 'gc-tls-public-second)")
            .unwrap()
            .is_t()
    );
    assert_ccl_registry_programs(
        &["gc-tls-public-second"],
        &["gc-tls-public-first", "gc-tls-public-extra"],
    );
    assert!(
        second
            .eval_str("(ccl-program-p 'gc-tls-public-extra)")
            .unwrap()
            .is_nil(),
        "eval_str_each registered a program in another Context"
    );
    second.gc_collect_exact();
    let _parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    first.gc_collect_exact();
}
