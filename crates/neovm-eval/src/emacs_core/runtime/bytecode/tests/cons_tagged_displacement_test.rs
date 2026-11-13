//! Exact live-cons slot values through both dispatch copies, including a
//! capture scope. The existing collection_capture_test checks dependencies;
//! this additionally detects swapped or incorrectly displaced car/cdr reads.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::decode::parse_arglist_descriptor;
use crate::emacs_core::eval::Context;
use neovm_value::tagged::collection_reads::capture;

fn execute(ctx: &mut Context, code: &ByteCodeFunction, cell: Value) -> Value {
    let mut vm = Vm::from_context(ctx);
    #[cfg(feature = "jit")]
    vm.force_interpreter_only_for_test();
    vm.execute_with_func_value(code, vec![cell], Value::NIL)
        .expect("one live-cons read")
}

#[test]
fn bytecode_cons_tagged_displacements_preserve_exact_slots_and_capture() {
    for verified in [false, true] {
        let mut ctx = Context::new_vm_runtime_harness(crate::test_utils::unit_runtime());
        for op in [Op::Car, Op::Cdr, Op::CarSafe, Op::CdrSafe] {
            let roots = ctx.save_vm_roots();
            // Distinct heap-backed values make a wrong slot obvious and are
            // rooted before construction of the next slot or the live cons.
            let car = Value::string("car-slot");
            ctx.push_vm_frame_root(car);
            let cdr = Value::vector(vec![Value::fixnum(19), Value::fixnum(-7)]);
            ctx.push_vm_frame_root(cdr);
            let cell = Value::cons(car, cdr);
            ctx.push_vm_frame_root(cell);
            assert_eq!(cell.cons_car_unobserved().bits(), car.bits());
            assert_eq!(cell.cons_cdr_unobserved().bits(), cdr.bits());
            let is_car = matches!(op, Op::Car | Op::CarSafe);
            let expected = if is_car { car } else { cdr };
            let mut code = ByteCodeFunction::new(parse_arglist_descriptor(1 | (1 << 8)));
            code.ops = vec![op.clone(), Op::Return];
            code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(1);
            code.seal_hand_assembled_ops_for_test();
            assert!(code.executes_sealed_ops());
            assert!(
                code.executes_verified_ops(),
                "the real verifier admits this body"
            );
            if !verified {
                // Keep the dispatch seal, revoke only the optional stack
                // proof, and execute the actual checked monomorphization.
                code.stack_verified = false;
            }
            assert_eq!(code.executes_verified_ops(), verified);
            let plain = execute(&mut ctx, &code, cell);
            assert_eq!(plain.bits(), expected.bits(), "{op:?}, verified={verified}");
            let (observed, reads) = capture(|| execute(&mut ctx, &code, cell));
            assert_eq!(
                observed.bits(),
                expected.bits(),
                "{op:?}: capture parity, verified={verified}",
            );
            let reads = reads.expect("the observed opcode records its live cons");
            assert!(reads.unchanged());
            if is_car {
                cell.set_car(Value::T);
            } else {
                cell.set_cdr(Value::NIL);
            }
            assert!(!reads.unchanged(), "{op:?}: selected slot mutation");
            assert!(ctx.bc_buf.is_empty());
            assert!(ctx.bc_frames.is_empty());
            assert!(ctx.specpdl.is_empty());
            ctx.restore_vm_roots(roots);
        }
    }
}
