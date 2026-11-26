//! Dormant admission on a fresh context, and unregister on drop after admit.

use super::super::{ConditionSnapshotAdmission, ConditionSnapshotAdmitError, Context};

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new(crate::test_utils::unit_runtime())
}

#[test]
fn fresh_context_is_dormant_and_registers_no_payload_id() {
    let ctx = context();
    assert!(ctx.condition_snapshots.is_dormant());
    assert!(matches!(
        ctx.condition_snapshots,
        ConditionSnapshotAdmission::Dormant
    ));
    assert_eq!(ctx.tagged_heap.payload_publisher_count_for_test(), 0);
}

#[test]
fn dropping_a_dormant_context_leaves_the_root_map_empty() {
    let ctx = context();
    assert_eq!(ctx.tagged_heap.payload_publisher_count_for_test(), 0);
    drop(ctx);
}

#[test]
fn admit_registers_one_id_and_drop_unregisters_it() {
    let mut ctx = context();
    ctx.admit_condition_snapshots()
        .expect("the first admit mints one publisher");
    assert!(!ctx.condition_snapshots.is_dormant());
    assert_eq!(ctx.tagged_heap.payload_publisher_count_for_test(), 1);
    assert_eq!(
        ctx.admit_condition_snapshots(),
        Err(ConditionSnapshotAdmitError::AlreadyActive)
    );
    assert_eq!(ctx.tagged_heap.payload_publisher_count_for_test(), 1);
    drop(ctx);
}
