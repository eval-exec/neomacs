use super::loop_walk::has_back_edge_with_policy;
use super::*;

// The baseline's ops-only predicate missed this possible-loop classification.
// Production keeps its original policy by default; the pure policy is explicit.
#[test]
fn unresolved_switch_may_loop() {
    assert!(has_back_edge_with_policy(
        &[Op::Switch],
        SwitchLoopPolicy::Conservative
    ));
    assert!(!has_back_edge_with_policy(
        &[Op::Switch],
        SwitchLoopPolicy::DirectOnly
    ));
}

#[test]
fn direct_backedges_and_handler_resumes_are_distinct() {
    assert!(!has_back_edge(&[Op::Goto(1), Op::Return]));
    assert!(has_back_edge(&[Op::Goto(0), Op::Return]));
    assert!(has_back_edge(&[Op::Nil, Op::GotoIfNil(0), Op::Return]));
    for op in [
        Op::PushConditionCase(0),
        Op::PushConditionCaseRaw(0),
        Op::PushCatch(0),
    ] {
        assert!(!has_back_edge(&[op, Op::Return]));
    }
}

#[test]
fn baseline_uses_resolved_switch_targets_not_raw_gnu_byte_offsets() {
    let ops = [Op::Constant(0), Op::Switch, Op::Return];
    let mut cfg = Cfg {
        leaders: Vec::new(),
        entry_depth: HashMap::default(),
        entry_binds: HashMap::default(),
        entry_handlers: HashMap::default(),
        switch_targets: HashMap::default(),
        max_depth: 0,
    };
    cfg.switch_targets.insert(1, vec![(500, 2)]);
    assert!(!baseline_has_backedge(&ops, &cfg));
    cfg.switch_targets.insert(1, vec![(500, 1)]);
    assert!(baseline_has_backedge(&ops, &cfg));
    cfg.switch_targets.insert(1, vec![(500, 0)]);
    assert!(baseline_has_backedge(&ops, &cfg));
}
