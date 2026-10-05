//! No CFG/SSA construction or native execution: reject unmodeled control early.
use super::*;
use crate::emacs_core::jit::compile::OptProfitMode;

#[test]
fn opt_profit_refuses_unmodeled_handlers_and_throw_even_in_dead_source_ranges() {
    for unsupported in [
        Op::PushConditionCase(0),
        Op::PushConditionCaseRaw(0),
        Op::PushCatch(0),
        Op::PopHandler,
        Op::Throw,
    ] {
        // Numeric work and a backedge would otherwise select this body for
        // early T1, upgrades, and OSR through the shared body_admitted policy.
        let loop_ops = [Op::Add1, unsupported.clone(), Op::Goto(0)];
        for mode in [OptProfitMode::Loops, OptProfitMode::Kernels] {
            for hot in [false, true] {
                assert!(!body_admitted(
                    mode,
                    &loop_ops,
                    CallDensity::Sparse,
                    KernelHeat::from(hot)
                ));
            }
        }
        assert!(body_admitted(
            OptProfitMode::Off,
            &loop_ops,
            CallDensity::Heavy,
            KernelHeat::Cold
        ));

        // Source scan deliberately avoids CFG work. Builder can discard this
        // unreachable operation, but the selective policy declines it cheaply.
        let dead_ops = [
            Op::Goto(3),
            unsupported.clone(),
            Op::Return,
            Op::Add1,
            Op::Goto(3),
        ];
        assert!(!body_admitted(
            OptProfitMode::Loops,
            &dead_ops,
            CallDensity::Sparse,
            KernelHeat::Hot
        ));
        assert!(body_admitted(
            OptProfitMode::Off,
            &dead_ops,
            CallDensity::Sparse,
            KernelHeat::Cold
        ));

        let helper_ops = [Op::Car, Op::Add1, unsupported, Op::Return];
        assert!(!body_admitted(
            OptProfitMode::Kernels,
            &helper_ops,
            CallDensity::Sparse,
            KernelHeat::Hot,
        ));
    }

    // UnwindProtectPop is modeled as an opaque bind operation, unlike the
    // five handler/throw cases. Keep its existing builder/admission decision.
    assert!(body_admitted(
        OptProfitMode::Loops,
        &[Op::UnwindProtectPop, Op::Add1, Op::Goto(0)],
        CallDensity::Sparse,
        KernelHeat::Hot,
    ));
}
