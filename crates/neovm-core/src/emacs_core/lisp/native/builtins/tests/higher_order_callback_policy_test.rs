use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::value::LambdaParams;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

fn callback(car: bool) -> Value {
    let mut code =
        ByteCodeFunction::new(LambdaParams::simple(vec![intern("capture-callback-arg")]));
    code.lexical = true;
    code.ops = vec![Op::StackRef(0)];
    if car {
        code.ops.push(Op::Car);
    }
    code.ops.push(Op::Return);
    code.max_stack = 8;
    Value::make_bytecode(code)
}

#[test]
fn higher_order_callback_policy_observes_bytecode_under_an_outer_capture() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let function = callback(false);
    let roots = eval.save_vm_roots();
    eval.push_vm_frame_root(function);
    let list = Value::list(vec![Value::fixnum(41), Value::fixnum(42)]);
    eval.push_vm_frame_root(list);
    let (result, reads) = capture(|| {
        assert!(matches!(
            MapCallee::resolve(&mut eval, function),
            MapCallee::Generic(_)
        ));
        builtin_mapcar_2(&mut eval, function, list)
    });
    assert_eq!(
        list_to_vec(&result.unwrap()),
        Some(vec![Value::fixnum(41), Value::fixnum(42)])
    );
    let reads = reads.expect("mapping a read-only callback preserves its certificate");
    assert!(reads.unchanged());
    LispCollectionRevision::changed(function);
    assert!(
        !reads.unchanged(),
        "the active map observes callback metadata"
    );
    eval.restore_vm_roots(roots);
}

#[test]
fn higher_order_callback_policy_preserves_gc_and_signal_unwind() {
    crate::test_utils::init_test_tracing();
    for gc_stress in [false, true] {
        let mut eval = crate::emacs_core::eval::Context::new();
        eval.gc_stress = gc_stress;
        let roots = eval.save_vm_roots();
        let function = callback(true);
        eval.push_vm_frame_root(function);
        let arg = Value::cons(Value::fixnum(42), Value::NIL);
        eval.push_vm_frame_root(arg);
        let callee = MapCallee::resolve(&mut eval, function);
        assert_eq!(
            matches!(callee, MapCallee::UnobservedByteCode(_)),
            crate::tagged::collection_reads::hoist_reads()
        );
        let depth = eval.depth;
        let specpdl = eval.specpdl.len();
        assert_eq!(callee.call(&mut eval, arg).unwrap(), Value::fixnum(42));
        assert_eq!((eval.depth, eval.specpdl.len()), (depth, specpdl));
        assert!(callee.call(&mut eval, Value::fixnum(7)).is_err());
        assert_eq!((eval.depth, eval.specpdl.len()), (depth, specpdl));
        assert!(!crate::tagged::collection_reads::is_active());
        eval.restore_vm_roots(roots);
    }
}
