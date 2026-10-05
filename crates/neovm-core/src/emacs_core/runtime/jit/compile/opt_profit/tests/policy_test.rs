//! Profitability choices without native execution or runtime recording.

use super::*;
use crate::emacs_core::jit::compile::OptProfitMode;

#[test]
fn opt_profit_mode_is_default_off_and_explicit() {
    assert_eq!(OptProfitMode::parse(None), OptProfitMode::Off);
    assert_eq!(OptProfitMode::parse(Some("invalid")), OptProfitMode::Off);
    assert_eq!(OptProfitMode::parse(Some("loops")), OptProfitMode::Loops);
    assert_eq!(
        OptProfitMode::parse(Some(" kernels ")),
        OptProfitMode::Kernels
    );
}

#[test]
fn opt_profit_off_keeps_call_glue_and_empty_shapes() {
    assert!(body_admitted(
        OptProfitMode::Off,
        &[],
        CallDensity::Heavy,
        KernelHeat::Cold
    ));
    assert!(body_admitted(
        OptProfitMode::Off,
        &[Op::Call(0)],
        CallDensity::Heavy,
        KernelHeat::Cold
    ));
}

#[test]
fn opt_profit_loops_admit_numeric_list_and_predicate_work() {
    for useful in [Op::Add1, Op::Cdr, Op::Eq, Op::Consp, Op::Setcar] {
        assert!(body_admitted(
            OptProfitMode::Loops,
            &[useful, Op::Goto(0)],
            CallDensity::Sparse,
            KernelHeat::Cold,
        ));
    }
}

#[test]
fn opt_profit_loop_glue_and_cached_call_heavy_verdict_are_rejected() {
    assert!(!body_admitted(
        OptProfitMode::Loops,
        &[Op::Nil, Op::Pop, Op::Goto(0)],
        CallDensity::Sparse,
        KernelHeat::Hot,
    ));
    assert!(!body_admitted(
        OptProfitMode::Kernels,
        &[Op::Add1, Op::Call(0), Op::Goto(0)],
        CallDensity::Heavy,
        KernelHeat::Hot,
    ));
}

#[test]
fn opt_profit_helpers_need_kernel_mode_heat_and_multiple_useful_ops() {
    let helper = [Op::Car, Op::Add1, Op::Return];
    assert!(!body_admitted(
        OptProfitMode::Loops,
        &helper,
        CallDensity::Sparse,
        KernelHeat::Hot
    ));
    assert!(!body_admitted(
        OptProfitMode::Kernels,
        &helper,
        CallDensity::Sparse,
        KernelHeat::Cold
    ));
    assert!(body_admitted(
        OptProfitMode::Kernels,
        &helper,
        CallDensity::Sparse,
        KernelHeat::Hot
    ));
    assert!(!body_admitted(
        OptProfitMode::Kernels,
        &[Op::Add1, Op::Return],
        CallDensity::Sparse,
        KernelHeat::Hot,
    ));
    let mut long = vec![Op::Nil; 65];
    long[0] = Op::Car;
    long[1] = Op::Add1;
    assert!(!body_admitted(
        OptProfitMode::Kernels,
        &long,
        CallDensity::Sparse,
        KernelHeat::Hot
    ));
}
