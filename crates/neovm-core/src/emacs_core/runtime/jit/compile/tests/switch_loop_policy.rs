use super::*;
use crate::emacs_core::jit::compile::knobs::{
    parse_switch_loop_policy, switch_loop_policy_scope_for_test,
};

#[test]
fn switch_loop_policy_defaults_off_and_parses_explicit_opt_in() {
    for text in [None, Some("off"), Some("invalid")] {
        assert_eq!(parse_switch_loop_policy(text), SwitchLoopPolicy::DirectOnly);
    }
    for text in [Some("on"), Some("1")] {
        assert_eq!(
            parse_switch_loop_policy(text),
            SwitchLoopPolicy::Conservative
        );
    }
}

#[test]
fn both_switch_policies_preserve_direct_and_handler_classification() {
    for policy in [SwitchLoopPolicy::DirectOnly, SwitchLoopPolicy::Conservative] {
        assert!(!has_back_edge_with_policy(
            &[Op::Goto(1), Op::Return],
            policy
        ));
        assert!(has_back_edge_with_policy(&[Op::Goto(0)], policy));
        assert!(has_back_edge_with_policy(
            &[Op::Nil, Op::GotoIfNil(0)],
            policy
        ));
        for op in [
            Op::PushConditionCase(0),
            Op::PushConditionCaseRaw(0),
            Op::PushCatch(0),
        ] {
            assert!(!has_back_edge_with_policy(&[op], policy));
        }
    }
}

#[test]
fn scoped_switch_loop_policy_restores_previous_setting() {
    let _outer = switch_loop_policy_scope_for_test(SwitchLoopPolicy::DirectOnly);
    assert!(!has_back_edge(&[Op::Switch]));
    {
        let _inner = switch_loop_policy_scope_for_test(SwitchLoopPolicy::Conservative);
        assert!(has_back_edge(&[Op::Switch]));
    }
    assert!(!has_back_edge(&[Op::Switch]));
}
