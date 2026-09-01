use super::*;

fn key_description(vec: Vec<Value>) -> String {
    builtin_key_description(vec![Value::vector(vec)])
        .expect("key-description should succeed")
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
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
    let m_a = crate::emacs_core::keyboard::pure::KEY_CHAR_META | 97;
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
