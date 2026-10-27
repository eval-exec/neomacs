use super::*;

#[test]
fn lisp_fwd_type_codes_match_gnu_lisp_fwd_type() {
    let cases = [
        (LispFwdType::Int, 0),
        (LispFwdType::Bool, 1),
        (LispFwdType::Obj, 2),
        (LispFwdType::BufferObj, 3),
        (LispFwdType::KboardObj, 4),
    ];

    for (ty, code) in cases {
        assert_eq!(ty.gnu_code(), code);
        assert_eq!(LispFwdType::from_gnu_code(code), Some(ty));
    }
    assert_eq!(LispFwdType::from_gnu_code(5), None);
}

#[test]
fn selected_keyboard_parked_heap_values_survive_gc_and_clone_independently() {
    let mut eval = crate::emacs_core::Context::new();
    let id = crate::emacs_core::intern::intern("window-system");
    let keyboard = eval
        .obarray
        .forwarder(id)
        .unwrap()
        .as_kboard_obj_fwd()
        .unwrap();
    keyboard.select_keyboard(1, Value::NIL);
    keyboard.set(Value::string("parked keyboard heap value"));
    keyboard.select_keyboard(0, Value::symbol("neo"));
    eval.eval_str("(garbage-collect)").unwrap();

    let copy = keyboard
        .header()
        .clone_stateful()
        .unwrap()
        .as_kboard_obj_fwd()
        .unwrap();
    keyboard.select_keyboard(1, Value::NIL);
    assert_eq!(
        keyboard.get().as_runtime_string_owned().as_deref(),
        Some("parked keyboard heap value")
    );
    keyboard.set(Value::string("changed original"));
    copy.select_keyboard(1, Value::NIL);
    assert_eq!(
        copy.get().as_runtime_string_owned().as_deref(),
        Some("parked keyboard heap value")
    );
}

#[test]
fn selected_keyboard_deleted_owner_cannot_restore_a_reused_terminal_number() {
    let keyboard = alloc_kboard_objfwd(Value::NIL);
    keyboard.use_selected_keyboard();
    keyboard.select_keyboard(0, Value::symbol("neo"));
    let deleted = keyboard.binding_owner().unwrap();
    keyboard.select_keyboard(1, Value::NIL);
    keyboard.forget_keyboard(0);
    keyboard.select_keyboard(0, Value::symbol("neo"));
    assert_ne!(keyboard.binding_owner(), Some(deleted));
    assert_eq!(
        keyboard.exchange_for_owner(deleted, Value::symbol("w32")),
        None
    );
    assert_eq!(keyboard.get(), Value::symbol("neo"));
}
