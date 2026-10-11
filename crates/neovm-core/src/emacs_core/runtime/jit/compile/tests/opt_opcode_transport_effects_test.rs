//! Generic arithmetic dispatches signal hooks before returning to generated
//! code. Its selected transport must invalidate heap facts and preserve roots
//! even though the primitive body itself cannot run Lisp.

use super::*;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::opt::mem::{AliasClass, Effects};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses::default()));
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
    }
}

fn arithmetic_plan(op: Op, feedback: NumericFeedback) -> ir::InstData {
    let _settings = Settings::enter();
    let arity = crate::emacs_core::bytecode::ArithGenericKind::from_op(&op)
        .unwrap()
        .arity();
    let mut ops = vec![Op::Nil; arity];
    let pc = ops.len();
    ops.push(op);
    ops.push(Op::Return);
    let mut feedbacks = vec![NumericFeedback::FixnumOnly; ops.len()];
    feedbacks[pc] = feedback;
    let _feedback = publish_numeric_feedback_vec(feedbacks);
    let cfg = analyze_cfg(&ops, &[], None, 0).unwrap();
    let plan = build_plan(&ops, &[], &cfg, ir::ParamShape::default(), 0, None).unwrap();
    plan.insts
        .iter()
        .find(|inst| inst.pc == pc as u32 && matches!(inst.op, ir::Opcode::Opaque(_)))
        .unwrap()
        .clone()
}

#[test]
fn opt_generic_arithmetic_transport_keeps_signal_hook_safepoints() {
    for op in [
        Op::Add,
        Op::Sub,
        Op::Mul,
        Op::Div,
        Op::Rem,
        Op::Add1,
        Op::Sub1,
        Op::Negate,
        Op::Max,
        Op::Min,
        Op::Eqlsign,
        Op::Lss,
        Op::Gtr,
        Op::Leq,
        Op::Geq,
    ] {
        let inst = arithmetic_plan(op.clone(), NumericFeedback::Other);
        assert!(inst.eff.contains(Effects::MAY_REENTER), "{op:?}");
        assert!(inst.eff.contains(Effects::MAY_GC), "{op:?}");
        assert!(inst.eff.contains(Effects::MAY_SIGNAL), "{op:?}");
        assert!(inst.op.is_safepoint(inst.eff), "{op:?}");
        assert!(AliasClass::ConsCar.clobbered_by(inst.eff), "{op:?}");
        assert!(AliasClass::Bindings.clobbered_by(inst.eff), "{op:?}");
    }
}

#[test]
fn opt_primitive_float_transport_keeps_sink_allocation_effects() {
    for op in [Op::Add, Op::Sub, Op::Mul, Op::Div] {
        let inst = arithmetic_plan(op.clone(), NumericFeedback::Float);
        assert!(inst.eff.contains(Effects::ALLOCATES), "{op:?}");
        assert!(
            !inst
                .eff
                .intersects(Effects::MAY_REENTER.with(Effects::MAY_GC)),
            "{op:?}"
        );
        assert!(!inst.op.is_safepoint(inst.eff), "{op:?}");
    }
    // These opcodes have no float-native arm, so Float selects the generic
    // transport and its signal-hook protocol as well.
    for op in [Op::Rem, Op::Max, Op::Min, Op::Add1, Op::Sub1, Op::Negate] {
        let inst = arithmetic_plan(op.clone(), NumericFeedback::Float);
        assert!(inst.eff.contains(Effects::MAY_REENTER), "{op:?}");
        assert!(inst.eff.contains(Effects::MAY_GC), "{op:?}");
        assert!(inst.op.is_safepoint(inst.eff), "{op:?}");
    }
}
