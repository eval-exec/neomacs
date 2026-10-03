//! The bytecode-size cap precedes shape policy but never a forced allocator.

use super::lowering::{RegallocChoice, RegallocPolicy, choose_regalloc};

#[test]
fn regalloc_small_policy_table() {
    use RegallocChoice::{Fast, Full};
    use RegallocPolicy::{Auto, Full as RequestedFull};
    let shapes = [
        (Auto, false, false, Fast),
        (Auto, true, false, Full),
        (RequestedFull, false, false, Full),
        (RequestedFull, true, false, Full),
        (Auto, false, true, Fast),
        (Auto, true, true, Fast),
        (RequestedFull, false, true, Fast),
        (RequestedFull, true, true, Fast),
    ];
    // Columns are caps 0, 128, 256, 384, 512: literal admission expectations
    // include empty bodies, inclusive boundaries and the first excluded op.
    let sizes = [
        (0, [false, true, true, true, true]),
        (1, [false, true, true, true, true]),
        (127, [false, true, true, true, true]),
        (128, [false, true, true, true, true]),
        (129, [false, false, true, true, true]),
        (256, [false, false, true, true, true]),
        (384, [false, false, false, true, true]),
        (512, [false, false, false, false, true]),
        (513, [false, false, false, false, false]),
    ];
    for forced in [None, Some(Fast), Some(Full)] {
        for (policy, has_back_edge, call_heavy, original) in shapes {
            for (op_count, admissions) in sizes {
                for (small_max, admitted) in [0, 128, 256, 384, 512].into_iter().zip(admissions) {
                    let expected = forced.unwrap_or(if admitted { Full } else { original });
                    assert_eq!(
                        choose_regalloc(
                            forced,
                            policy,
                            has_back_edge,
                            call_heavy,
                            op_count,
                            small_max,
                        ),
                        expected,
                        "forced={forced:?} policy={policy:?} back_edge={has_back_edge} \
                         call_heavy={call_heavy} ops={op_count} cap={small_max}",
                    );
                }
            }
        }
    }
}

#[test]
fn regalloc_small_knob_parsing() {
    use super::knobs::parse_regalloc_small_max;

    for (value, expected) in [
        (None, 0),
        (Some("0"), 0),
        (Some("128"), 128),
        (Some(" 384 "), 384),
        (Some(""), 0),
        (Some("off"), 0),
        (Some("-1"), 0),
        (Some("1.5"), 0),
        (Some("99999999999999999999999999999999999999"), 0),
    ] {
        assert_eq!(parse_regalloc_small_max(value), expected, "{value:?}");
    }
}

#[test]
fn regalloc_small_selection_covers_baseline_and_mir() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    use super::*;
    use crate::emacs_core::value::LambdaParams;

    force_profit_gate_for_test(false);
    force_deopt_for_test(false);
    let expected = lowering::forced_regalloc().unwrap_or(if jit_regalloc_small_max() >= 4 {
        RegallocChoice::Full
    } else {
        RegallocChoice::Fast
    });
    for (baseline, tier) in [(true, LeafTier::Baseline), (false, LeafTier::Mir)] {
        let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![
            crate::emacs_core::intern::intern("regalloc-small-arg"),
        ]));
        f.lexical = true;
        if baseline {
            // An optional argument keeps this body out of the MIR tier.
            f.params
                .optional
                .push(crate::emacs_core::intern::intern("regalloc-small-optional"));
        }
        f.ops = vec![
            Op::StackRef(u16::from(baseline)),
            Op::Constant(0),
            Op::Add,
            Op::Return,
        ];
        f.constants = vec![Value::make_int(1)].into();
        f.max_stack = 4;
        f.seal_hand_assembled_ops();
        let leaf =
            compile_bytecode_function_with(&f, None).expect("small arithmetic body compiles");
        assert_eq!(leaf.tier, tier);
        assert_eq!(leaf.regalloc, expected);
        assert_eq!(
            leaf.call_for_test(&[Value::make_int(41), Value::NIL][..if baseline { 2 } else { 1 }]),
            Some(Value::make_int(42).bits()),
        );
        assert_eq!(lowering::active_regalloc_choice(), RegallocChoice::Full);
    }
}
