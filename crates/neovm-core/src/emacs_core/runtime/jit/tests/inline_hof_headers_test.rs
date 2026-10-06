//! A serviced poll can change captures before the next checked callback.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode, NativeRun};
use crate::emacs_core::jit::inline;
use crate::emacs_core::value::LambdaParams;

struct Knobs;

impl Knobs {
    fn enter() -> Self {
        inline::force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::Hof));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        Self
    }
}

impl Drop for Knobs {
    fn drop(&mut self) {
        inline::force_inline_for_test(None);
        compile::force_inline2_for_test(None);
    }
}

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![intern("hof-header-arg")]));
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    function.seal_hand_assembled_ops_for_test();
    function.jit_runtime().set_hot_for_test();
    function
}

#[test]
fn inline_hof_headers_refresh_captured_cell_after_a_serviced_poll() {
    let _knobs = Knobs::enter();
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    ctx.set_variable("hof-header-cell", cell);
    let prototype = Value::make_bytecode(function(
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Return,
        ],
        vec![Value::symbol("V0")],
    ));
    let callback = ctx
        .apply2(Value::symbol("make-closure"), prototype, cell)
        .expect("capture is patched before native entry");
    let caller = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapc"), callback],
    );
    let sequence = Value::list((0..300).map(Value::make_int).collect());
    ctx.push_vm_root_frame();
    for value in [prototype, callback, sequence] {
        ctx.push_vm_frame_root(value);
    }
    ctx.eval_str(
        "(setq post-gc-hook (list (lambda () (setq post-gc-hook nil) (setcar hof-header-cell 1000))))",
    )
    .unwrap();
    let leaf = compile::compile_bytecode_function_with(&caller, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    ctx.gc_stress = true;
    reset_bytecode_branch_poll_count();
    let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence]) else {
        panic!("guarded capture stores remain native across a serviced poll")
    };
    assert_eq!(Value::from_bits(bits), sequence);
    assert_eq!(bytecode_branch_poll_count(), 1);
    assert_eq!(cell.cons_car(), Value::make_int(1045));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_frames.is_empty() && ctx.bc_buf.is_empty());
    assert_eq!(ctx.save_vm_frame_roots(), 3);
    ctx.pop_vm_root_frame();
}
