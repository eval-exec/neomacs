use super::pure::{
    KEY_CHAR_CODE_MASK, KEY_CHAR_CTRL, basic_char_code, describe_single_key_value,
    event_modifier_bit, event_modifier_prefix, key_sequence_values, resolve_control_code,
    symbol_has_modifier_prefix,
};
use crate::emacs_core::value::Value;

#[test]
fn describe_int_key_succeeds() {
    crate::test_utils::init_test_tracing();
    let value = Value::fixnum(97);
    assert_eq!(describe_single_key_value(&value, false).unwrap(), b"a");
}

#[test]
fn key_sequence_values_accept_string_and_list() {
    crate::test_utils::init_test_tracing();
    let string = Value::string("abc");
    let list: Value =
        crate::emacs_core::value::Value::list(vec![Value::fixnum(97), Value::fixnum(98)]);
    assert_eq!(
        key_sequence_values(&string).unwrap(),
        vec![Value::fixnum(97), Value::fixnum(98), Value::fixnum(99)]
    );
    assert_eq!(
        key_sequence_values(&list).unwrap(),
        vec![Value::fixnum(97), Value::fixnum(98)]
    );
}

#[test]
fn symbol_modifier_helpers() {
    crate::test_utils::init_test_tracing();
    assert!(symbol_has_modifier_prefix("C-x"));
    assert!(!symbol_has_modifier_prefix("foo"));
    assert_eq!(event_modifier_bit("control"), Some(KEY_CHAR_CTRL));
    assert!(event_modifier_prefix(KEY_CHAR_CTRL).starts_with("C-"));
}

#[test]
fn control_code_resolution() {
    crate::test_utils::init_test_tracing();
    assert_eq!(resolve_control_code(65), Some(1));
    assert_eq!(resolve_control_code(97), Some(1));
    assert!(resolve_control_code(999).is_none());
}

#[test]
fn basic_char_code_masks() {
    crate::test_utils::init_test_tracing();
    let bits = 0x123456;
    assert!(basic_char_code(bits) <= KEY_CHAR_CODE_MASK);
}

#[test]
fn tty_erase_char_defaults_to_nil_and_is_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // GNU declares this with DEFVAR_LISP in `syms_of_keyboard'
    // (src/keyboard.c:13925) and leaves the value to `init_sys_modes'
    // (src/sysdep.c:1112), which starts it at Qnil and only assigns
    // c_cc[VERASE] for a live tty. A batch session therefore reads nil, not a
    // number: `normal-erase-is-backspace-setup-frame' (lisp/simple.el) tests
    // (eq tty-erase-char ?\^H), so a hardcoded 0 is a value GNU never has.
    assert_eq!(
        eval.eval_str("tty-erase-char").expect("tty-erase-char"),
        Value::NIL,
        "tty-erase-char starts nil, as init_sys_modes leaves it off a tty"
    );

    // DEFVAR_LISP is also what makes the symbol special, so a `let' around it
    // in a lexical-binding file binds dynamically and callees observe it.
    let seen = eval
        .eval_str("(let ((tty-erase-char 8)) (funcall (lambda () tty-erase-char)))")
        .expect("let over tty-erase-char");
    assert_eq!(
        seen,
        Value::fixnum(8),
        "a let of tty-erase-char must bind dynamically, as DEFVAR_LISP makes it special"
    );
}
