//! R-access reads real copied symbol/forwarder channels through the current owner.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::forward::{alloc_kboard_objfwd, alloc_objfwd};

#[test]
fn world_bases_admission_distinguishes_direct_nil_and_dedicated_selection() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let nonnil = intern("world-bases-admission-nonnil");
    let nil = intern("world-bases-admission-nil");
    context
        .obarray
        .set_symbol_value_id(nonnil, Value::fixnum(17));
    context.obarray.set_symbol_value_id(nil, Value::NIL);
    let undo = intern("buffer-undo-list");
    let mut vm = Vm::from_context(&mut context);
    for (symbol, expected) in [(nonnil, Value::fixnum(17)), (nil, Value::NIL)] {
        assert_eq!(
            vm.fast_path_var_ref(symbol).expect("bound copied cell"),
            expected
        );
    }
    assert_eq!(
        vm.fast_path_var_ref(undo).expect("default undo state"),
        Value::NIL
    );
    vm.ctx.obarray.set_symbol_value_id(undo, Value::fixnum(29));
    assert_eq!(
        vm.fast_path_var_ref(undo).expect("nonnil undo cell"),
        Value::fixnum(29)
    );
}

#[test]
fn world_bases_admission_keeps_missing_and_unbound_slot_errors() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let missing_keyword = intern(":world-bases-admission-missing-keyword");
    let missing_plain = intern("world-bases-admission-missing-plain");
    let unbound = intern("world-bases-admission-unbound");
    context.obarray.ensure_symbol_id(unbound);
    assert!(context.obarray.get_by_id(missing_keyword).is_none());
    assert!(context.obarray.get_by_id(missing_plain).is_none());
    let mut vm = Vm::from_context(&mut context);
    for symbol in [missing_keyword, missing_plain, unbound] {
        match vm.fast_path_var_ref(symbol).kinded() {
            Err(FlowKind::Signal(signal)) => {
                assert_eq!(signal.symbol, intern("void-variable"));
                assert_eq!(signal.data, vec![Value::from_sym_id(symbol)]);
            }
            Err(FlowKind::Throw(_) | FlowKind::ThreadBlocked(_) | FlowKind::Shutdown(_)) => {
                panic!("VarRef must signal void-variable")
            }
            Ok(_) => panic!("unbound VarRef unexpectedly returned"),
        }
    }
}

#[test]
fn world_bases_admission_does_not_steal_forwarded_unbound_as_control() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let object = intern("world-bases-admission-forwarded-object-unbound");
    let keyboard = intern("world-bases-admission-forwarded-keyboard-unbound");
    context
        .obarray
        .install_objfwd(object, alloc_objfwd(Value::UNBOUND));
    context
        .obarray
        .install_kboard_objfwd(keyboard, alloc_kboard_objfwd(Value::UNBOUND));
    let mut vm = Vm::from_context(&mut context);
    for symbol in [object, keyboard] {
        assert!(
            vm.fast_path_var_ref(symbol)
                .expect("descriptor owns its value")
                .is_unbound()
        );
    }
}

#[test]
fn world_bases_admission_dedicated_nil_keeps_the_no_current_buffer_result() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.buffers = crate::buffer::BufferManager::new();
    let undo = intern("buffer-undo-list");
    let mut code = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    code.ops = vec![Op::VarRef(0), Op::Return];
    code.constants = vec![Value::from_sym_id(undo)].into();
    code.max_stack = super::super::function_slots::StackDepth::try_from(4usize)
        .expect("valid fixture stack depth");
    let mut vm = Vm::from_context(&mut context);
    #[cfg(feature = "jit")]
    vm.force_interpreter_only_for_test();
    assert_eq!(
        vm.execute(&code, vec![])
            .expect("nil without a selected buffer"),
        Value::NIL
    );
}
