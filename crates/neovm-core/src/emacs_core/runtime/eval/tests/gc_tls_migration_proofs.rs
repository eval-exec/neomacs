use super::*;
use crate::emacs_core::{builtins, casetab, category, ccl, charset, syntax, xfaces};

fn collect_on_worker_and_return(mut ctx: Context) -> Context {
    // Retire both raw allocation views before transferring Context storage.
    crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);
    let mut ctx = std::thread::spawn(move || {
        ctx.setup_thread_locals();
        ctx.gc_collect_exact();
        crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);
        ctx
    })
    .join()
    .expect("collect the exclusively moved Context on the worker");
    ctx.setup_thread_locals();
    ctx
}

#[test]
fn gc_tls_ownership_canonical_tables_survive_worker_collection_and_return() {
    let mut ctx = Context::new();
    let case = casetab::builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    casetab::builtin_set_standard_case_table(&mut ctx, vec![case]).unwrap();
    let expected = [
        ctx.standard_syntax_table.bits(),
        ctx.syntax_code_objects.bits(),
        ctx.standard_category_table.bits(),
        case.bits(),
    ];
    let descriptor_bits = ctx.syntax_code_objects.as_vector_data().unwrap()[0].bits();

    let mut ctx = collect_on_worker_and_return(ctx);
    let restored = [
        syntax::builtin_standard_syntax_table(vec![]).unwrap(),
        syntax::ensure_syntax_code_objects(),
        category::ensure_standard_category_table_object().unwrap(),
        casetab::builtin_standard_case_table(&mut ctx, vec![]).unwrap(),
    ];
    for (value, bits) in restored.into_iter().zip(expected) {
        assert_eq!(value.bits(), bits, "activation replaced a canonical object");
        assert!(
            ctx.tagged_heap.owns_heap_value_for_test(value),
            "worker collection swept a Context-owned canonical object"
        );
    }
    let descriptor = ctx.syntax_code_objects.as_vector_data().unwrap()[0];
    assert_eq!(descriptor.bits(), descriptor_bits);
    // The heap ownership probe only accepts non-cons objects. The stable
    // descriptor and its contents prove that the rooted vector traced it.
    assert_eq!(descriptor.cons_car(), Value::fixnum(0));
    assert!(descriptor.cons_cdr().is_nil());
}

fn registry_roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    ccl::collect_ccl_registry_gc_roots(&ctx.ccl_registry, &mut roots);
    charset::collect_charset_registry_gc_roots(&ctx.charset_registry, &mut roots);
    xfaces::collect_font_registry_gc_roots(&ctx.font_registry, &mut roots);
    builtins::collect_hash_table_test_registry_gc_roots(&ctx.hash_table_test_registry, &mut roots);
    super::super::error::collect_in_flight_registry_gc_roots(
        &mut roots,
        &ctx.in_flight_registry,
        ctx.tagged_heap.identity(),
    );
    roots
}

#[test]
fn gc_tls_ownership_context_registries_survive_worker_collection_and_return() {
    let mut ctx = Context::new();
    let program_name = Value::symbol("gc-tls-roundtrip-program");
    let program = Value::vector(vec![Value::fixnum(0); 3]);
    ccl::builtin_register_ccl_program_impl(vec![program_name, program]).unwrap();
    let map = Value::vector(vec![Value::fixnum(71)]);
    ccl::builtin_register_code_conversion_map_impl(vec![
        Value::symbol("gc-tls-roundtrip-map"),
        map,
    ])
    .unwrap();
    let charset_payload = Value::vector(vec![Value::fixnum(72)]);
    charset::set_charset_plist_registry(
        intern("ascii"),
        vec![(intern("gc-tls-roundtrip-plist"), charset_payload)],
    );
    let family = Value::string("gc-tls-roundtrip-family");
    xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ctx,
        vec![Value::symbol("default"), Value::symbol(":family"), family],
    )
    .unwrap();
    ctx.eval_str(
        "(define-hash-table-test 'gc-tls-roundtrip-custom \
         (lambda (a b) (eq a b)) (lambda (a) 1))",
    )
    .unwrap();
    let hash_alias = builtins::lookup_hash_table_test_alias("gc-tls-roundtrip-custom").unwrap();
    let comparator = hash_alias.user_cmp_function.unwrap();
    let hasher = hash_alias.user_hash_function.unwrap();
    let error = ctx
        .eval_str("(signal 'error (list (vector 73)))")
        .expect_err("retain a public error on the source thread during migration");
    let error_payload = match &error {
        EvalError::Signal { data, .. } => data[0],
        other => panic!("expected a signal, received {other:?}"),
    };
    let expected = [
        program.bits(),
        map.bits(),
        charset_payload.bits(),
        family.bits(),
        comparator.bits(),
        hasher.bits(),
        error_payload.bits(),
    ];
    let roots = registry_roots(&ctx);
    assert!(
        expected
            .iter()
            .all(|bits| roots.iter().any(|v| v.bits() == *bits))
    );

    let mut ctx = collect_on_worker_and_return(ctx);
    let roots = registry_roots(&ctx);
    for bits in expected {
        let value = roots
            .iter()
            .find(|value| value.bits() == bits)
            .copied()
            .expect("return activation lost a Context-owned registry value");
        assert!(
            ctx.tagged_heap.owns_heap_value_for_test(value),
            "worker collection swept a Context-owned registry value"
        );
    }
    assert!(
        ccl::builtin_ccl_program_p_impl(vec![program_name])
            .unwrap()
            .is_t()
    );
    assert_eq!(
        charset_payload.as_vector_data().unwrap()[0],
        Value::fixnum(72)
    );
    assert_eq!(
        charset::builtin_charset_plist(vec![Value::symbol("ascii")])
            .unwrap()
            .cons_cdr()
            .cons_car()
            .bits(),
        charset_payload.bits()
    );
    assert_eq!(
        xfaces::builtin_internal_get_lisp_face_attribute(
            &mut ctx,
            vec![Value::symbol("default"), Value::symbol(":family")],
        )
        .unwrap()
        .bits(),
        family.bits()
    );
    assert_eq!(
        ctx.eval_str(
            "(let ((h (make-hash-table :test 'gc-tls-roundtrip-custom))) \
             (puthash 'x 74 h) (gethash 'x h))",
        )
        .unwrap(),
        Value::fixnum(74)
    );
    assert_eq!(
        error_payload.as_vector_data().unwrap()[0],
        Value::fixnum(73)
    );
    drop(error);
}
