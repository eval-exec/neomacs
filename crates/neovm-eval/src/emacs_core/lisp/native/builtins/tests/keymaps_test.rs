use super::*;

fn key_description(vec: Vec<Value>) -> String {
    builtin_key_description(
        vec![Value::vector(vec)],
        neovm_value::tagged::SymbolMode::Bare,
    )
    .expect("key-description should succeed")
    .as_lisp_string()
    .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
    .expect("key-description returns a string")
}

// GNU `Fkey_description` folds a lone ESC (meta_prefix_char, 27) into the meta
// modifier of the FOLLOWING ordinary event.
#[test]
fn key_description_folds_esc_into_meta() {
    crate::test_utils::init_test_tracing();
    // [27 97] -> M-a
    assert_eq!(
        key_description(vec![Value::fixnum(27), Value::fixnum(97)]),
        "M-a"
    );
    // [27 1] (ESC C-a) -> C-M-a
    assert_eq!(
        key_description(vec![Value::fixnum(27), Value::fixnum(1)]),
        "C-M-a"
    );
    // [27 32] (ESC SPC) -> M-SPC
    assert_eq!(
        key_description(vec![Value::fixnum(27), Value::fixnum(32)]),
        "M-SPC"
    );
}

// Boundary cases must keep the ESC literal.
#[test]
fn key_description_esc_boundary_cases() {
    crate::test_utils::init_test_tracing();
    // [27 27] -> ESC ESC (second ESC cannot absorb meta)
    assert_eq!(
        key_description(vec![Value::fixnum(27), Value::fixnum(27)]),
        "ESC ESC"
    );
    // [27 27 97] -> ESC M-a (first ESC stays pending, folds into a)
    assert_eq!(
        key_description(vec![
            Value::fixnum(27),
            Value::fixnum(27),
            Value::fixnum(97)
        ]),
        "ESC M-a"
    );
    // [27 M-a] -> ESC M-a (already-meta next key cannot absorb another meta)
    let m_a = neovm_value::keyboard_pure::KEY_CHAR_META | 97;
    assert_eq!(
        key_description(vec![Value::fixnum(27), Value::fixnum(m_a)]),
        "ESC M-a"
    );
    // [27 <f5>] -> ESC <f5> (non-fixnum next key cannot absorb meta)
    assert_eq!(
        key_description(vec![Value::fixnum(27), Value::symbol("f5")]),
        "ESC <f5>"
    );
    // [27] alone -> ESC
    assert_eq!(key_description(vec![Value::fixnum(27)]), "ESC");
}

// #590: define-key stores the definition object; it must not publish the
// canonical symbols reachable only from DEF. The canary id exists in the
// name registry but is not an obarray member, so only a definition walk
// could make it one.
#[test]
fn define_key_does_not_publish_a_definition_symbol() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let keymap = eval.eval_str("(make-sparse-keymap)").expect("fresh keymap");
    let canary = neovm_value::intern::intern("neovm--def-canary");
    let definition = Value::cons(Value::from_sym_id(canary), Value::NIL);
    builtin_define_key(
        &mut eval,
        vec![keymap, Value::vector(vec![Value::fixnum(1)]), definition],
    )
    .expect("define-key");
    assert!(
        !eval.obarray_view().is_global_member(canary),
        "a symbol reachable only from DEF must stay a non-member"
    );
}
