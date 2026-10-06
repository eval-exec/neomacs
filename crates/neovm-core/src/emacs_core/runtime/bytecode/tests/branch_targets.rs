use super::*;

#[test]
fn direct_and_handler_targets_preserve_instruction_indices() {
    for op in [
        Op::Goto(7),
        Op::GotoIfNil(7),
        Op::GotoIfNotNil(7),
        Op::GotoIfNilElsePop(7),
        Op::GotoIfNotNilElsePop(7),
    ] {
        let BranchTargets::Direct(target) = op.branch_targets() else {
            panic!("{op:?}");
        };
        assert_eq!(target.get(), 7);
    }
    for op in [
        Op::PushConditionCase(7),
        Op::PushConditionCaseRaw(7),
        Op::PushCatch(7),
    ] {
        let BranchTargets::Handler(target) = op.branch_targets() else {
            panic!("{op:?}");
        };
        assert_eq!(target.get(), 7);
    }
}

#[test]
fn switch_requires_a_table_while_nonbranches_have_no_target() {
    assert_eq!(Op::Switch.branch_targets(), BranchTargets::SwitchTable);
    for op in [
        Op::Constant(0),
        Op::Return,
        Op::Throw,
        Op::PopHandler,
        Op::UnwindProtectPop,
        Op::SaveCurrentBuffer,
        Op::Call(0),
    ] {
        assert_eq!(op.branch_targets(), BranchTargets::None, "{op:?}");
    }
}
