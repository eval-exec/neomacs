use super::*;

#[test]
fn inlining_rejects_switch_tables_and_exceptional_resume_edges() {
    for op in [
        Op::Switch,
        Op::PushConditionCase(0),
        Op::PushConditionCaseRaw(0),
        Op::PushCatch(0),
    ] {
        assert!(!op_is_inlinable(&op, NumericFeedback::FixnumOnly), "{op:?}");
        assert_eq!(jump_target(&op), None);
    }
    assert_eq!(jump_target(&Op::Goto(9)), Some(9));
}
