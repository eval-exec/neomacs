use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::LambdaParams;
use crate::tagged::collection_reads::capture;

#[test]
fn interpreter_collection_opcodes_keep_capture_dependencies() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_vm_runtime_harness();
    for (op, prefix) in [
        (Op::Car, 1),
        (Op::Cdr, 1),
        (Op::CarSafe, 1),
        (Op::CdrSafe, 1),
        (Op::Nth, 3),
        (Op::Nthcdr, 2),
    ] {
        for index in 0..prefix {
            let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
            let mut source = list;
            for _ in 0..index {
                source = source.cons_cdr();
            }
            let mut code = ByteCodeFunction::new(LambdaParams {
                required: Vec::new(),
                optional: Vec::new(),
                rest: None,
            });
            code.constants = vec![list, Value::fixnum(2)].into();
            code.ops = if matches!(op, Op::Nth | Op::Nthcdr) {
                vec![Op::Constant(1), Op::Constant(0), op.clone(), Op::Return]
            } else {
                vec![Op::Constant(0), op.clone(), Op::Return]
            };
            code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(2);
            let mut vm = Vm::from_context(&mut eval);
            #[cfg(feature = "jit")]
            vm.force_interpreter_only_for_test();
            let (result, reads) = capture(|| vm.execute(&code, Vec::new()));
            assert!(result.is_ok(), "{op:?}: {result:?}");
            let reads = reads.expect("an interpreter read produces a coherent certificate");
            assert!(reads.unchanged());
            source.set_car(Value::T);
            assert!(!reads.unchanged(), "{op:?}, visited cell {index}");
        }
    }
}
