//! List selection distinguishes a loop's work from numeric helpers and glue.
//! Threading: immutable op fixtures and invocation-owned scalar overrides only.

use super::frontend_tests::Settings;
use super::*;
use crate::emacs_core::jit::compile::{OptProfitMode, force_opt_max_ops_for_test};

#[test]
fn opt_profit_lists_is_explicit_and_keeps_the_default_off() {
    assert_eq!(OptProfitMode::parse(Some(" lists ")), OptProfitMode::Lists);
    assert_eq!(OptProfitMode::parse(None), OptProfitMode::Off);
    assert_eq!(OptProfitMode::parse(Some("list")), OptProfitMode::Off);
}

#[test]
fn opt_profit_lists_accepts_existing_list_access_and_setter_loops() {
    let _settings = Settings::enter();
    for list_op in [
        Op::Car,
        Op::Cdr,
        Op::CarSafe,
        Op::CdrSafe,
        Op::Setcar,
        Op::Setcdr,
    ] {
        for edge in [
            Op::Goto(0),
            Op::GotoIfNil(0),
            Op::GotoIfNotNil(0),
            Op::GotoIfNilElsePop(0),
            Op::GotoIfNotNilElsePop(0),
        ] {
            for hot in [false, true] {
                assert!(body_admitted(
                    OptProfitMode::Lists,
                    &[list_op.clone(), edge.clone()],
                    false,
                    hot,
                ));
            }
        }
    }
}

#[test]
fn opt_profit_lists_declines_numeric_predicate_and_allocation_only_loops() {
    let _settings = Settings::enter();
    for work in [Op::Add1, Op::Mul, Op::Eq, Op::Consp, Op::Listp, Op::Cons] {
        let ops = [work, Op::Goto(0)];
        assert!(!body_admitted(OptProfitMode::Lists, &ops, false, true));
        for mode in [OptProfitMode::Loops, OptProfitMode::Kernels] {
            assert!(body_admitted(mode, &ops, false, false));
        }
        assert!(body_admitted(OptProfitMode::Off, &ops, true, false));
    }
    let helper = [Op::Car, Op::Add1, Op::Return];
    assert!(!body_admitted(OptProfitMode::Lists, &helper, false, true));
    assert!(body_admitted(OptProfitMode::Kernels, &helper, false, true));
}

#[test]
fn opt_profit_lists_requires_list_work_inside_a_backward_edge_span() {
    let _settings = Settings::enter();
    // Both list operations are outside the numeric loop at PCs 1..=2.
    let mut ops = vec![Op::Car, Op::Add1, Op::Goto(1), Op::Cdr, Op::Return];
    assert!(!body_admitted(OptProfitMode::Lists, &ops, false, true));
    assert!(body_admitted(OptProfitMode::Loops, &ops, false, true));
    // A later independent backedge includes the Cdr, which now qualifies.
    ops[4] = Op::Goto(3);
    assert!(body_admitted(OptProfitMode::Lists, &ops, false, false));
}

#[test]
fn opt_profit_lists_retains_call_heavy_unsupported_and_size_rejections() {
    let _settings = Settings::enter();
    let ops = [Op::Setcar, Op::Add1, Op::Goto(0)];
    assert!(body_admitted(OptProfitMode::Lists, &ops, false, true));
    assert!(!body_admitted(OptProfitMode::Lists, &ops, true, true));
    let unsupported = [Op::Setcar, Op::Throw, Op::Goto(0)];
    assert!(!body_admitted(
        OptProfitMode::Lists,
        &unsupported,
        false,
        true
    ));
    force_opt_max_ops_for_test(Some(ops.len() - 1));
    assert!(!body_admitted(OptProfitMode::Lists, &ops, false, true));
    assert!(body_admitted(OptProfitMode::Off, &unsupported, true, false));
}
