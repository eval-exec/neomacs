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

/// A context whose paced deferred sweep is in progress and owes no slice to
/// a mutator that does not allocate, with the `gcs-done` count before the
/// cycle completes.
fn context_with_a_paced_deferred_sweep() -> (crate::emacs_core::Context, u64) {
    use neovm_value::heap_types::LispString;
    // Select the concurrent (non-generational) collector for this heap only:
    // the knob is read per heap, when the constructor runs, and the previous
    // value is restored right after it.
    let previous = std::env::var_os("NEOVM_GC_GENERATIONAL");
    // SAFETY: nextest isolates each test in its own process; the variable is
    // restored before any other code of this test runs.
    unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "0") };
    let mut context = crate::test_utils::new_context();
    // SAFETY: as above.
    unsafe {
        match previous {
            Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
            None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
        }
    }
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut context, false);
    context.gc_collect_exact();
    // Unrooted strings fill arena pages the sweep visits a few per slice.
    for i in 0..200_000u32 {
        // SAFETY: test-only owner-thread allocation through the non-swappable
        // mutable accessor; the view is used in place and never moved.
        let _ = unsafe { context.tagged_heap_mut() }
            .alloc_string(LispString::from_utf8(&format!("idle garbage {i:08}")));
    }
    // SAFETY: as above.
    unsafe { context.tagged_heap_mut() }.set_gc_threshold(1);
    // SAFETY: as above.
    unsafe { context.tagged_heap_mut() }.alloc_cons(Value::NIL, Value::NIL);
    let mut sweeping = false;
    for _ in 0..100_000 {
        context.gc_safe_point();
        if context.tagged_heap().sweep_in_progress() {
            sweeping = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(sweeping, "concurrent marking did not enter deferred sweep");
    // A threshold far above anything allocated from here on: the paced
    // sweep owes no slice to a mutator that does not allocate.
    context.set_gc_threshold(1 << 40);
    let completed = context.gc_count_for_test();
    for _ in 0..1_000 {
        context.gc_safe_point();
    }
    assert!(
        context.tagged_heap().sweep_in_progress(),
        "idle safe points finished the paced sweep"
    );
    assert_eq!(context.gc_count_for_test(), completed);
    (context, completed)
}

#[test]
fn an_idle_command_loop_finishes_a_paced_deferred_sweep() {
    use super::command_loop::CommandLoopHost;
    let (mut context, completed) = context_with_a_paced_deferred_sweep();
    context.command_loop_1_idle_sweep();
    assert!(!context.tagged_heap().sweep_in_progress());
    assert_eq!(
        context.gc_count_for_test(),
        completed + 1,
        "the idle command loop completes the cycle"
    );
}

#[test]
fn the_command_loop_finishes_a_paced_deferred_sweep_before_its_first_input_wait() {
    use super::command_loop::CommandLoopHost;
    // GNU `read_char` collects before it waits when no input is pending
    // (keyboard.c:2975-2977), so a sweep already in progress when the loop
    // starts (after startup, say) must not wait for the first command.
    let (mut context, completed) = context_with_a_paced_deferred_sweep();
    context.command_loop_mut().running = true;
    // No input source: the first read reaches end of input and the loop
    // returns without running a command.
    let result = context.command_loop_1();
    assert!(
        result.is_ok(),
        "the command loop failed before its first read"
    );
    assert!(
        !context.tagged_heap().sweep_in_progress(),
        "the first input wait left the deferred sweep unfinished"
    );
    assert_eq!(context.gc_count_for_test(), completed + 1);
}
