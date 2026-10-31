use super::builtin_frame_or_buffer_changed_p;
use crate::emacs_core::builtins::symbols::{builtin_set_2, builtin_symbol_value_1};
use crate::emacs_core::error::{FlowKind, FlowResultExt as _};
use crate::emacs_core::{Context, Value};

fn lambda() -> Value {
    Value::symbol("lambda")
}

fn visible_buffer_name(buffer: &crate::buffer::Buffer) -> bool {
    buffer
        .name_value()
        .as_lisp_string()
        .is_none_or(|name| name.as_bytes().first() != Some(&b' '))
}

#[test]
fn internal_state_is_changed_once_then_stable() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);

    assert_eq!(
        builtin_frame_or_buffer_changed_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
    assert!(
        eval.frame_and_buffer_state.is_some(),
        "nil VARIABLE must populate the per-Context internal state"
    );
    assert_eq!(
        builtin_frame_or_buffer_changed_p(&mut eval, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_frame_or_buffer_changed_p(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );
}

#[test]
fn user_variable_tracks_buffer_lifecycle() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let var = Value::symbol("neb-unit-state");
    builtin_set_2(&mut eval, var, Value::NIL).unwrap();

    let mut results = Vec::new();
    let check = |eval: &mut Context, results: &mut Vec<Value>| {
        results.push(builtin_frame_or_buffer_changed_p(eval, vec![var]).unwrap());
    };

    check(&mut eval, &mut results);
    check(&mut eval, &mut results);
    let visible = eval.buffers.create_buffer("neb-unit-visible");
    check(&mut eval, &mut results);
    check(&mut eval, &mut results);
    eval.buffers
        .get_mut(visible)
        .expect("visible buffer")
        .insert("x");
    check(&mut eval, &mut results);
    check(&mut eval, &mut results);
    eval.buffers
        .get_mut(visible)
        .expect("visible buffer")
        .set_read_only_value(true);
    check(&mut eval, &mut results);
    check(&mut eval, &mut results);
    let hidden = eval.buffers.create_buffer(" *neb-unit-hidden*");
    check(&mut eval, &mut results);
    eval.buffers
        .get_mut(hidden)
        .expect("hidden buffer")
        .insert("h");
    check(&mut eval, &mut results);
    eval.buffers.kill_buffer(visible);
    check(&mut eval, &mut results);
    check(&mut eval, &mut results);
    builtin_set_2(&mut eval, var, Value::fixnum(5)).unwrap();
    check(&mut eval, &mut results);
    check(&mut eval, &mut results);

    let expected = [
        Value::T,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::T,
        Value::NIL,
    ];
    assert_eq!(results, expected);
}

#[test]
fn state_vector_records_frames_then_visible_buffers_then_lambda() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let var = Value::symbol("neb-unit-layout");
    builtin_set_2(&mut eval, var, Value::NIL).unwrap();
    assert_eq!(
        builtin_frame_or_buffer_changed_p(&mut eval, vec![var]).unwrap(),
        Value::T
    );

    let state = builtin_symbol_value_1(&mut eval, var).unwrap();
    let slots = state.as_vector_data().expect("state vector");
    let frame = eval.frames.get(frame_id).expect("selected frame");
    assert_eq!(slots[0].bits(), Value::make_frame(frame_id.0).bits());
    assert_eq!(slots[1].bits(), frame.name_value().bits());

    let mut index = 2;
    for buffer_id in eval.buffers.buffer_list() {
        let buffer = eval.buffers.get(buffer_id).expect("live buffer");
        if !visible_buffer_name(buffer) {
            continue;
        }
        assert_eq!(
            slots[index].bits(),
            Value::make_buffer(buffer_id).bits(),
            "buffer identity at slot {index}"
        );
        assert_eq!(
            slots[index + 1].bits(),
            buffer.read_only_value().bits(),
            "buffer read-only at slot {}",
            index + 1
        );
        assert_eq!(
            slots[index + 2].bits(),
            buffer.modified_state_value().bits(),
            "buffer modified state at slot {}",
            index + 2
        );
        index += 3;
    }
    assert_eq!(slots[index].bits(), lambda().bits(), "terminating lambda");
    assert!(
        slots[index..]
            .iter()
            .all(|slot| slot.bits() == lambda().bits()),
        "every slot after the terminator stays lambda"
    );
}

#[test]
fn unchanged_state_keeps_the_same_vector_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let var = Value::symbol("neb-unit-reuse");
    builtin_set_2(&mut eval, var, Value::NIL).unwrap();
    let buffer = eval.buffers.create_buffer("neb-unit-reuse-buffer");
    assert_eq!(
        builtin_frame_or_buffer_changed_p(&mut eval, vec![var]).unwrap(),
        Value::T
    );
    let before = builtin_symbol_value_1(&mut eval, var).unwrap();
    assert!(before.as_vector_data().is_some());

    eval.buffers
        .get_mut(buffer)
        .expect("reuse buffer")
        .insert("x");
    assert_eq!(
        builtin_frame_or_buffer_changed_p(&mut eval, vec![var]).unwrap(),
        Value::T
    );
    let after = builtin_symbol_value_1(&mut eval, var).unwrap();
    assert_eq!(
        before.bits(),
        after.bits(),
        "GNU rewrites the existing state vector in place when the size fits"
    );
}

#[test]
fn unbound_and_non_symbol_variables_signal_gnu_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);

    let unbound =
        builtin_frame_or_buffer_changed_p(&mut eval, vec![Value::symbol("neb-unit-never-bound")]);
    match unbound.kinded() {
        Err(FlowKind::Signal(signal)) => assert_eq!(signal.symbol_name(), "void-variable"),
        other => panic!("expected void-variable, got {other:?}"),
    }

    let non_symbol = builtin_frame_or_buffer_changed_p(&mut eval, vec![Value::fixnum(1)]);
    match non_symbol.kinded() {
        Err(FlowKind::Signal(signal)) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data[0], Value::symbol("symbolp"));
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}
