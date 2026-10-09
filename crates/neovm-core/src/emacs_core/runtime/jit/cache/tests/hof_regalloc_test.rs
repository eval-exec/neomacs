//! OSR shares admitted HOF allocation without widening call-heavy policy.

use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::jit::inline::force_inline_for_test;
use crate::emacs_core::value::LambdaParams;

struct Knobs;

impl Knobs {
    fn enter() -> Self {
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::Hof));
        compile::force_gate_relax_for_test(false);
        compile::force_deopt_for_test(false);
        Self
    }
}

impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        compile::force_inline2_for_test(None);
    }
}

fn lexical(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("osr-hof-regalloc-arg")]));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn caller(extra_call: bool) -> ByteCodeFunction {
    let callback = Value::make_bytecode(lexical(
        vec![Op::Constant(0), Op::StackRef(1), Op::Add, Op::Return],
        vec![Value::symbol("V0")],
    ));
    let mut ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Constant(3),
        Op::Call(2),
        Op::StackRef(2),
        Op::Call(2),
    ];
    if extra_call {
        ops.extend([Op::Pop, Op::Constant(4), Op::StackRef(1), Op::Call(1)]);
    }
    ops.push(Op::Return);
    lexical(
        ops,
        vec![
            Value::symbol("mapcar"),
            Value::symbol("make-closure"),
            callback,
            Value::make_int(9),
            Value::symbol("identity"),
        ],
    )
}

#[test]
fn osr_hof_regalloc_admitted_native_loop_uses_full() {
    let _knobs = Knobs::enter();
    let ctx = Context::new();
    let f = caller(false);
    assert!(compile::body_is_call_heavy(
        f.executable_ops(),
        &f.constants
    ));
    let id = f.jit_runtime().compiled_id_or_assign();
    let entry = compile_osr_leaf(&ctx.obarray, &f, 0, id, None).unwrap();
    assert_eq!(
        entry.leaf.regalloc,
        forced_regalloc().unwrap_or(RegallocChoice::Full)
    );
    assert_eq!(entry.leaf.obs.osr_pc, Some(0));
    assert!(
        !entry.leaf.chains.is_empty(),
        "OSR lowered the admitted HOF"
    );
}

#[test]
fn osr_hof_regalloc_remaining_calls_keep_source_policy() {
    let _knobs = Knobs::enter();
    let ctx = Context::new();
    let f = caller(true);
    let expected = compile::regalloc_for(RegallocPolicy::Full, f.executable_ops(), &f.constants);
    let id = f.jit_runtime().compiled_id_or_assign();
    let entry = compile_osr_leaf(&ctx.obarray, &f, 0, id, None).unwrap();
    assert_eq!(entry.leaf.regalloc, expected);
    assert!(
        !entry.leaf.chains.is_empty(),
        "remaining calls did not reject HOF admission"
    );
}
