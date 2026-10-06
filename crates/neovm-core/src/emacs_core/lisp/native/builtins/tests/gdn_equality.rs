use super::*;

#[test]
fn gdn_delete_retains_input_string_encoding() {
    let source = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(vec![
        b'a', b'b', 0xc3, 0xa9,
    ]));
    let result = builtin_delete_with_symbols(vec![Value::fixnum(233), source], false).unwrap();
    assert!(result.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(result.as_lisp_string().unwrap().as_bytes(), b"ab");
    let raw = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(vec![
        0xc3, 0xa9, 0xc1, 0xaa,
    ]));
    let result = builtin_delete_with_symbols(vec![Value::fixnum(233), raw], false).unwrap();
    assert!(result.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(
        super::super::lisp_string_char_codes(result.as_lisp_string().unwrap()),
        vec![0x3fffea]
    );
}

#[test]
fn gdn_reverse_ascii_multibyte_becomes_unibyte() {
    let source = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        b"ab".to_vec(),
    ));
    let result = builtin_reverse(vec![source]).unwrap();
    assert!(!result.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(result.as_lisp_string().unwrap().as_bytes(), b"ba");
}

#[test]
fn gdn_equal_car_cycle_terminates() {
    let a = Value::list(vec![Value::fixnum(1)]);
    let b = Value::list(vec![Value::fixnum(1)]);
    a.set_car(a);
    b.set_car(b);
    assert_eq!(
        crate::emacs_core::value::try_equal_value_swp(&a, &b, 0, false).unwrap(),
        true
    );
    let c = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    let d = Value::list(vec![Value::fixnum(1), Value::fixnum(3)]);
    c.set_car(c);
    d.set_car(d);
    assert_eq!(
        crate::emacs_core::value::try_equal_value_swp(&c, &d, 0, false).unwrap(),
        false
    );
}

#[test]
fn gdn_sequence_equal_depth_error_is_not_false() {
    let mut a = Value::fixnum(0);
    let mut b = Value::fixnum(0);
    for _ in 0..300 {
        a = Value::list(vec![a]);
        b = Value::list(vec![b]);
    }
    assert!(builtin_member_values(a, Value::list(vec![b]), false).is_err());
    assert!(
        assoc_values(
            a,
            Value::list(vec![Value::cons(b, Value::fixnum(1))]),
            false
        )
        .is_err()
    );
    assert!(builtin_delete_with_symbols(vec![a, Value::list(vec![b])], false).is_err());
    assert!(builtin_delete_with_symbols(vec![a, Value::vector(vec![b])], false).is_err());
}
