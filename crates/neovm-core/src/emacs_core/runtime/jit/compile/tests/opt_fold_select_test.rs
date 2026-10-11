//! Bool Select must accept local I8 and incoming I64 compiler flags together.
//! Threading: all plans and runtime contexts belong to one test invocation;
//! overrides hold compiler settings only and are restored on scope exit.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::function;
use super::*;
use crate::emacs_core::jit::opt::mem::{AliasClass, Effects};
use crate::emacs_core::jit::opt::types::TypeSet;
use crate::emacs_core::jit::opt::{eval, ir};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            fold: true,
            ..OptPasses::default()
        }));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_deopt_for_test(false);
    }
}

fn emit(
    func: &mut ir::Func,
    block: ir::Block,
    op: ir::Opcode,
    args: Vec<ir::Value>,
    rep: ir::Rep,
    ty: TypeSet,
) -> ir::Value {
    let inst = ir::Inst(func.insts.len() as u32);
    let value = ir::Value(func.values.len() as u32);
    func.values.push(ir::ValueData {
        def: ir::ValueDef::Inst(inst),
        rep,
        ty,
    });
    func.insts.push(ir::InstData {
        op,
        args,
        result: Some(value),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: func.blocks[block.index()].pc,
    });
    func.blocks[block.index()].insts.push(inst);
    value
}

fn bool_param(func: &mut ir::Func, block: ir::Block) -> ir::Value {
    let value = ir::Value(func.values.len() as u32);
    let index = func.blocks[block.index()].params.len() as u32;
    func.values.push(ir::ValueData {
        def: ir::ValueDef::Param { block, index },
        rep: ir::Rep::Bool,
        ty: TypeSet::BOOLEAN,
    });
    func.blocks[block.index()].params.push(value);
    value
}

fn mixed_select(local_value: bool, local_on_true: bool) -> ir::Func {
    let entry = ir::Block(0);
    let join = ir::Block(1);
    let mut func = ir::Func::new(
        Vec::new().into_boxed_slice(),
        ir::ParamShape {
            required: 2,
            ..ir::ParamShape::default()
        },
        0,
    );
    func.blocks = vec![ir::BlockData::new(0), ir::BlockData::new(1)];
    let mut flags = Vec::new();
    for argument in 0..2 {
        let input = emit(
            &mut func,
            entry,
            ir::Opcode::Arg(argument),
            vec![],
            ir::Rep::Tagged,
            TypeSet::TOP,
        );
        flags.push(emit(
            &mut func,
            entry,
            ir::Opcode::IsNonNil,
            vec![input],
            ir::Rep::Bool,
            TypeSet::BOOLEAN,
        ));
    }
    func.blocks[entry.index()].term = ir::Term::Jump(ir::Edge {
        target: join,
        args: flags,
    });
    func.blocks[join.index()].preds = vec![entry];
    let condition = bool_param(&mut func, join);
    let incoming = bool_param(&mut func, join);
    // The incoming Bool parameter is transported as I64. This BoolConst stays
    // local I8, so either arm order exercises the mixed-width native boundary.
    let local = emit(
        &mut func,
        join,
        ir::Opcode::BoolConst(local_value),
        vec![],
        ir::Rep::Bool,
        if local_value {
            TypeSet::T
        } else {
            TypeSet::NIL
        },
    );
    let (yes, no) = if local_on_true {
        (local, incoming)
    } else {
        (incoming, local)
    };
    let selected = emit(
        &mut func,
        join,
        ir::Opcode::Select,
        vec![condition, yes, no],
        ir::Rep::Bool,
        TypeSet::BOOLEAN,
    );
    let tagged = emit(
        &mut func,
        join,
        ir::Opcode::BoolToLisp,
        vec![selected],
        ir::Rep::Tagged,
        TypeSet::BOOLEAN,
    );
    func.blocks[join.index()].term = ir::Term::Return(tagged);
    func.verify()
        .expect("mixed-width Bool Select plan verifies");
    func
}

#[test]
fn opt_fold_select_mixed_local_and_incoming_bool_materializes_gnu_boolean() {
    let _settings = Settings::enter();
    let source = function(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Eq, Op::Return],
        vec![],
        2,
    );
    let mut ctx = Context::new();
    for local_value in [false, true] {
        for local_on_true in [false, true] {
            let plan = mixed_select(local_value, local_on_true);
            let leaf = lower_opt_ir_for_test(
                source.executable_ops(),
                &source.constants,
                2,
                source.executable_gnu_byte_offset_map(),
                &plan,
            )
            .expect("native lowering accepts mixed-width Bool Select");
            assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
            for condition in [Value::NIL, Value::T] {
                // Zero is truthy in GNU Lisp; the incoming flag is not a raw
                // numeric zero test and still must return the exact T word.
                for incoming in [Value::NIL, Value::fixnum(0)] {
                    let args = [condition, incoming];
                    let run = eval::evaluate(
                        &plan,
                        &mut ctx,
                        eval::Inputs {
                            args: &args,
                            ..eval::Inputs::default()
                        },
                    )
                    .expect("reference Bool Select");
                    let eval::Outcome::Returned(expected) = run.outcome else {
                        panic!("reference Bool Select must return")
                    };
                    let expected = expected.to_value();
                    assert!(expected.is_nil() || expected.is_t());
                    let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &args)
                    else {
                        panic!("native Bool Select must return")
                    };
                    assert_eq!(Value::from_bits(bits), expected);
                }
            }
        }
    }
}
