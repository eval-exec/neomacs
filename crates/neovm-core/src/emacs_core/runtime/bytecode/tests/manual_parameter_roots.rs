//! Manual VM execution has no heap bytecode owner to trace its children.
use super::*;
use crate::emacs_core::bytecode::{FunctionParams, StackDepth};
use crate::emacs_core::eval::Context;

#[test]
fn p6_manual_execute_roots_parameters_after_formal_binding_and_body_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    let formal = intern("p6-manual-parameter-root");
    let parameter_list = Value::list(vec![Value::from_sym_id(formal)]);
    let observable_arglist = Value::list(vec![Value::fixnum(91)]);
    let mut code = ByteCodeFunction::new(FunctionParams::try_from(parameter_list).unwrap());
    // Both fields are independently observable public Rust API. The code is
    // never published as a heap function, nor are its children scratch roots.
    code.arglist = observable_arglist;
    let garbage_collect = code.add_constant(Value::from_sym_id(intern("garbage-collect")));
    let answer = code.add_constant(Value::fixnum(42));
    code.ops = vec![
        Op::Constant(garbage_collect),
        Op::Call(0),
        Op::Pop,
        Op::Constant(answer),
        Op::Return,
    ];
    code.max_stack = StackDepth::for_test(1);
    let collections_before = eval.gc_count;
    let result = {
        let mut vm = Vm::from_context(&mut eval);
        #[cfg(feature = "jit")]
        vm.force_interpreter_only_for_test();
        vm.execute(&code, vec![Value::fixnum(42)])
    };
    assert_eq!(result.unwrap(), Value::fixnum(42));
    assert!(
        eval.gc_count > collections_before,
        "the body must collect after binding the formal"
    );
    // Check allocated ownership before reading either cons: missing roots
    // must fail safely even when GC has returned its slot to the free list.
    assert!(
        eval.tagged_heap.owns_heap_value_for_test(parameter_list),
        "manual execution lost the parameter child"
    );
    assert!(
        eval.tagged_heap
            .owns_heap_value_for_test(observable_arglist),
        "manual execution lost the original arglist"
    );
    // The statistics result may allocate after GC, so also rule out a freed
    // slot being recycled for one of its conses before the ownership probes.
    assert_eq!(parameter_list.cons_car(), Value::from_sym_id(formal));
    assert_eq!(parameter_list.cons_cdr(), Value::NIL);
    assert_eq!(observable_arglist.cons_car(), Value::fixnum(91));
    assert_eq!(observable_arglist.cons_cdr(), Value::NIL);
}
