use super::pure::{
    KEY_CHAR_CTRL, describe_single_key_value, event_modifier_bit, event_modifier_prefix,
    key_sequence_values, reorder_event_symbol_modifiers, resolve_control_code,
};
use crate::emacs_core::value::Value;

#[test]
fn describe_int_key_succeeds() {
    crate::test_utils::init_test_tracing();
    let value = Value::fixnum(97);
    assert_eq!(
        describe_single_key_value(&value, false, crate::tagged::value::SymbolMode::Bare).unwrap(),
        b"a"
    );
}

#[test]
fn key_sequence_values_accept_string_and_list() {
    crate::test_utils::init_test_tracing();
    let string = Value::string("abc");
    let list: Value =
        crate::emacs_core::value::Value::list(vec![Value::fixnum(97), Value::fixnum(98)]);
    assert_eq!(
        key_sequence_values(&string, crate::tagged::value::SymbolMode::Bare).unwrap(),
        vec![Value::fixnum(97), Value::fixnum(98), Value::fixnum(99)]
    );
    assert_eq!(
        key_sequence_values(&list, crate::tagged::value::SymbolMode::Bare).unwrap(),
        vec![Value::fixnum(97), Value::fixnum(98)]
    );
}

#[test]
fn symbol_modifier_helpers() {
    crate::test_utils::init_test_tracing();
    assert_eq!(event_modifier_bit("control"), Some(KEY_CHAR_CTRL));
    assert!(event_modifier_prefix(KEY_CHAR_CTRL).starts_with("C-"));

    // The production consumer of the modifier-prefix recognizer is
    // `reorder_event_symbol_modifiers` (keymap_lookup.rs:468,2246). A
    // reversed prefix order must canonicalize to GNU's A,C,H,M,S,s order,
    // which exercises every prefix the old test-only predicate listed.
    let bare = crate::tagged::value::SymbolMode::Bare;
    for (input, canonical) in [("M-C-a", "C-M-a"), ("S-H-a", "H-S-a"), ("s-A-a", "A-s-a")] {
        assert_eq!(
            reorder_event_symbol_modifiers(Value::symbol(input), bare),
            Value::symbol(canonical),
            "{input} must reorder to the canonical modifier spelling"
        );
    }
    for name in ["C-x", "M-x", "S-left", "s-a", "H-a", "A-a", "foo", "left"] {
        assert_eq!(
            reorder_event_symbol_modifiers(Value::symbol(name), bare),
            Value::symbol(name),
            "{name} is already canonical and must pass through"
        );
    }
}

#[test]
fn control_code_resolution() {
    crate::test_utils::init_test_tracing();
    assert_eq!(resolve_control_code(65), Some(1));
    assert_eq!(resolve_control_code(97), Some(1));
    assert!(resolve_control_code(999).is_none());
}

#[test]
fn event_basic_type_matches_gnu_for_integer_events() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();

    // The old value-side `basic_char_code` model was a copy of GNU
    // `event-basic-type`'s numeric branch (lisp/subr.el:1865-1883). Exercise
    // the production path instead: the loaded Elisp function itself, with
    // expectations read from the pinned GNU 31.1 oracle:
    // (event-basic-type 0/1/26/27/31/65/999) => (64 97 122 91 95 97 999).
    for (arg, expected) in [
        (0, 64),
        (1, 97),
        (26, 122),
        (27, 91),
        (31, 95),
        (65, 97),
        (999, 999),
    ] {
        assert_eq!(
            eval.eval_str(&format!("(event-basic-type {arg})"))
                .expect("event-basic-type"),
            Value::fixnum(expected),
            "(event-basic-type {arg}) must match GNU"
        );
    }
}

#[test]
fn tty_erase_char_defaults_to_nil_and_is_special_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

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
