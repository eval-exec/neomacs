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
fn an_idle_sweep_stops_for_a_key_still_in_the_host_input_channel() {
    use super::command_loop::CommandLoopHost;
    use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
    // GNU `detect_input_pending_run_timers` reads the OS input queue
    // (`get_input_pending` -> `gobble_input`, keyboard.c), so a key the
    // frontend has sent but the loop has not yet staged is pending input.
    let (mut context, completed) = context_with_a_paced_deferred_sweep();
    let publisher = crate::host_input::HostInputPublisher::install(&mut context);
    publisher
        .send(InputEvent::key_press(KeyEvent::char('x')))
        .unwrap();
    context.command_loop_1_idle_sweep();
    assert!(
        context.tagged_heap().sweep_in_progress(),
        "a key still in the host input channel did not stop the idle sweep"
    );
    assert_eq!(context.gc_count_for_test(), completed);
}

#[test]
fn an_idle_sweep_leaves_a_finished_input_stream_to_the_read() {
    use super::command_loop::CommandLoopHost;
    use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
    // The read after the sweep takes the queued key before it can see the
    // closed channel; the sweep must not reach that end first.
    let (mut context, _) = context_with_a_paced_deferred_sweep();
    let publisher = crate::host_input::HostInputPublisher::install(&mut context);
    publisher
        .send(InputEvent::key_press(KeyEvent::char('x')))
        .unwrap();
    drop(publisher);
    context.command_loop_1_idle_sweep();
    let receiver = context.input_rx().as_ref();
    assert!(
        receiver.is_some(),
        "the idle sweep closed the input terminal"
    );
    assert_eq!(
        receiver.map(|rx| rx.len()),
        Some(1),
        "the idle sweep took the key the read is owed"
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

#[test]
fn a_key_read_finishes_an_idle_sweep_after_a_mouse_motion_it_consumes() {
    use crate::emacs_core::eval::keyboard::{InputEvent, Modifiers};
    // With `track-mouse` nil a motion is no pending input in GNU
    // (`some_mouse_moved`, keyboard.c:1296), so `read_char` collects before
    // it waits (keyboard.c:2976-2977).  The pre-read drain stops for the
    // motion still in the channel; the read consumes it without returning a
    // key and must finish the sweep before it waits.
    let (mut context, completed) = context_with_a_paced_deferred_sweep();
    let publisher = crate::host_input::HostInputPublisher::install(&mut context);
    publisher
        .send(InputEvent::MouseMove {
            x: 1.0,
            y: 1.0,
            modifiers: Modifiers::none(),
            target_frame_id: 0,
        })
        .unwrap();
    // A zero-delay idle timer ends the read once it waits: idle timers run
    // only after the read has started its idle epoch.
    context
        .eval_str(
            r#"(progn
                 (fset 'timer-event-handler
                       (lambda (timer)
                         (setq timer-idle-list (delq timer timer-idle-list))
                         (apply (aref timer 5) (aref timer 6))))
                 (fset 'idle-sweep-test-end-read
                       (lambda () (throw 'idle-sweep-test 'waited)))
                 (setq timer-idle-list
                       (list (vector nil 0 0 0 nil 'idle-sweep-test-end-read
                                     nil t 0 nil))))"#,
        )
        .expect("install the idle timer that ends the read");
    assert!(
        context.tagged_heap().sweep_in_progress(),
        "installing the idle timer finished the paced sweep"
    );
    let result =
        context.eval_str(r#"(catch 'idle-sweep-test (read-key-sequence "key: ") 'read-returned)"#);
    assert_eq!(crate::emacs_core::format_eval_result(&result), "OK waited");
    assert!(
        !context.tagged_heap().sweep_in_progress(),
        "the read waited with the deferred sweep unfinished"
    );
    assert_eq!(context.gc_count_for_test(), completed + 1);
    drop(publisher);
}

#[test]
fn an_idle_sweep_leaves_a_ready_keyboard_macro_key_to_the_read() {
    use super::command_loop::CommandLoopHost;
    // GNU `read_char` returns the next key of an executing keyboard macro
    // (keyboard.c:2659-2680) and asks for a collection only when INTERACTIVE,
    // which excludes an executing macro (commands.h:28, keyboard.c:2932).
    let (mut context, completed) = context_with_a_paced_deferred_sweep();
    context.begin_executing_kbd_macro_runtime(vec![Value::fixnum('x' as i64)]);
    context.command_loop_1_idle_sweep();
    assert!(
        context.tagged_heap().sweep_in_progress(),
        "a keyboard macro key ready for the read did not stop the idle sweep"
    );
    assert_eq!(context.gc_count_for_test(), completed);
    assert_eq!(
        context.command_loop().keyboard.kboard.kbd_macro_index,
        0,
        "the idle sweep consumed the macro key the read is owed"
    );
}
