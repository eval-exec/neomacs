use super::*;

#[test]
fn vm_named_fillarray_redefinition_keeps_original_argument() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let concat = eval.eval_str("(symbol-function 'concat)").unwrap();
    eval.obarray
        .set_symbol_function_id(intern("fillarray"), concat);
    let original = Value::string("abc");
    let caller = string_call_retaining_argument(
        Value::from_sym_id(intern("fillarray")),
        original,
        Value::string("!"),
    );
    let result = new_vm(&mut eval).execute(&caller, vec![]).unwrap();
    // GNU Ffillarray mutates only when the actual builtin runs. A redefined
    // function can return another string without replacing the old argument.
    assert_eq!(result, original);
    assert_eq!(result.as_utf8_str(), Some("abc"));
}

#[test]
fn vm_redefined_fillarray_preserves_global_nested_and_hash_aliases() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let original = eval
        .eval_str(
            r#"(progn
                 (setq vm-p65-original (copy-sequence "abc"))
                 (setq vm-p65-list (list vm-p65-original))
                 (setq vm-p65-vector (vector vm-p65-original))
                 (setq vm-p65-table (make-hash-table :test 'eq))
                 (puthash vm-p65-original 'kept vm-p65-table)
                 (fset 'fillarray (lambda (array _item) (concat "NEW" array)))
                 vm-p65-original)"#,
        )
        .unwrap();
    let caller = string_call_retaining_argument(
        Value::from_sym_id(intern("fillarray")),
        original,
        Value::fixnum('x' as i64),
    );
    let retained = new_vm(&mut eval).execute(&caller, vec![]).unwrap();
    assert_eq!(retained, original);
    assert_eq!(retained.as_utf8_str(), Some("abc"));
    // Check observable object identities, including an Eq-table key. GNU's
    // ordinary call does not walk the global object graph or rekey the table.
    let aliases = eval
        .eval_str(
            r#"(list vm-p65-original
                    (eq vm-p65-original (car vm-p65-list))
                    (eq vm-p65-original (aref vm-p65-vector 0))
                    (gethash vm-p65-original vm-p65-table)
                    (hash-table-count vm-p65-table))"#,
        )
        .unwrap();
    assert_eq!(aliases.cons_car(), original);
    let mut tail = aliases.cons_cdr();
    assert_eq!(tail.cons_car(), Value::T);
    tail = tail.cons_cdr();
    assert_eq!(tail.cons_car(), Value::T);
    tail = tail.cons_cdr();
    assert_eq!(tail.cons_car(), Value::from_sym_id(intern("kept")));
    assert_eq!(tail.cons_cdr().cons_car(), Value::fixnum(1));
}
