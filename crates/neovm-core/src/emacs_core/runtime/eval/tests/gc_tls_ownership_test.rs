use super::*;

#[test]
fn gc_tls_ownership_exact_collection_uses_the_collecting_heap() {
    let mut first = Context::new();
    let mut second = Context::new();
    // Deliberately leave the second allocation heap installed. This exercises
    // the real collector rather than prechecking a side-table root vector.
    first.gc_collect_exact();
    second.gc_collect_exact();
    // A second cycle also checks values allocated by post-GC bookkeeping.
    first.gc_collect_exact();
    second.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_safe_point_uses_the_collecting_heap() {
    let mut first = Context::new();
    let mut second = Context::new();
    first.gc_collect_from_current_roots();
    second.gc_collect_from_current_roots();
}

#[test]
fn gc_tls_ownership_semantic_registries_have_independent_thread_owners() {
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut first = Context::new();
        first
            .eval_str("(register-ccl-program 'gc-tls-worker-program [0 0 0])")
            .unwrap();
        receive.recv().unwrap();
        assert!(super::super::ccl::is_registered_ccl_program(intern(
            "gc-tls-worker-program"
        )));
        first.gc_collect_exact();
    });
    let mut second = Context::new();
    send.send(()).unwrap();
    second.gc_collect_exact();
    worker.join().unwrap();
    assert!(!super::super::ccl::is_registered_ccl_program(intern(
        "gc-tls-worker-program"
    )));
}

fn registered_public_entry_contexts() -> (Context, Context) {
    let mut first = Context::new();
    first
        .eval_str("(register-ccl-program 'gc-tls-public-first [0 0 0])")
        .unwrap();
    let mut second = Context::new();
    second
        .eval_str("(register-ccl-program 'gc-tls-public-second [0 0 0])")
        .unwrap();
    (first, second)
}

fn assert_ccl_registry_programs(registered: &[&str], missing: &[&str]) {
    for name in registered {
        assert!(
            super::super::ccl::is_registered_ccl_program(intern(name)),
            "public evaluation left the active CCL registry without {name}"
        );
    }
    for name in missing {
        assert!(
            !super::super::ccl::is_registered_ccl_program(intern(name)),
            "public evaluation left another Context's {name} in the active CCL registry"
        );
    }
}

#[test]
fn gc_tls_ownership_eval_str_activates_its_context_registries() {
    let (mut first, mut second) = registered_public_entry_contexts();
    assert!(
        first
            .eval_str("(ccl-program-p 'gc-tls-public-first)")
            .unwrap()
            .is_t(),
        "eval_str used the previously active Context's CCL registry"
    );
    assert_ccl_registry_programs(&["gc-tls-public-first"], &["gc-tls-public-second"]);
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
    first.gc_collect_exact();
    second.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_eval_form_activates_its_context_registries() {
    let mut first = Context::new();
    first
        .eval_str("(register-ccl-program 'gc-tls-public-first [0 0 0])")
        .unwrap();
    let form = first
        .eval_str("'(ccl-program-p 'gc-tls-public-first)")
        .unwrap();
    let mut second = Context::new();
    second
        .eval_str("(register-ccl-program 'gc-tls-public-second [0 0 0])")
        .unwrap();
    assert!(
        first.eval_form(form).unwrap().is_t(),
        "eval_form used the previously active Context's CCL registry"
    );
    assert_ccl_registry_programs(&["gc-tls-public-first"], &["gc-tls-public-second"]);
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
    first.gc_collect_exact();
    second.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_eval_value_activates_its_context_registries() {
    let mut first = Context::new();
    first
        .eval_str("(register-ccl-program 'gc-tls-public-first [0 0 0])")
        .unwrap();
    let form = first
        .eval_str("'(ccl-program-p 'gc-tls-public-first)")
        .unwrap();
    let mut second = Context::new();
    second
        .eval_str("(register-ccl-program 'gc-tls-public-second [0 0 0])")
        .unwrap();
    assert!(first.eval_value(&form).unwrap().is_t());
    assert_ccl_registry_programs(&["gc-tls-public-first"], &["gc-tls-public-second"]);
    first.gc_collect_exact();
    second.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_eval_str_each_activates_its_context_registries() {
    let (mut first, mut second) = registered_public_entry_contexts();
    let results = first.eval_str_each(
        "(ccl-program-p 'gc-tls-public-first)
         (register-ccl-program 'gc-tls-public-extra [0 0 0])
         (ccl-program-p 'gc-tls-public-extra)",
    );
    assert_eq!(results.len(), 3);
    assert!(
        results[0].as_ref().unwrap().is_t(),
        "eval_str_each used the previously active Context's CCL registry"
    );
    assert!(results[2].as_ref().unwrap().is_t());
    assert_ccl_registry_programs(
        &["gc-tls-public-first", "gc-tls-public-extra"],
        &["gc-tls-public-second"],
    );
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
    first.gc_collect_exact();
    second.gc_collect_exact();
}
