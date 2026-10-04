//! Real placement/remapping API controls, with complete OFF/ON snapshots.
//! Plans and their immutable row shares are exclusively owned by each test.
use super::*;
use crate::incremental_layout::edit_sync::dense_index_test_support::{self as probes, Guard};

fn observed(
    plan: EditSyncPlan,
    on: bool,
    reached: EditSyncReached,
    bottom: f32,
    limit: usize,
) -> (EditSyncInstall, probes::Counts) {
    let _guard = Guard::set(on);
    let placed = plan.install_edit(reached, bottom, limit);
    (placed, probes::counts())
}
fn equal(left: &EditSyncInstall, right: &EditSyncInstall) {
    assert_eq!(
        format!("{:?}", left.rows),
        format!("{:?}", right.rows),
        "complete row placement/source/pointer metadata"
    );
    assert_eq!(left.row_snapshots, right.row_snapshots);
    assert_eq!(left.points, right.points);
    assert_eq!(left.point_rows, right.point_rows);
    assert_eq!(left.dy, right.dy);
}
fn reached(index: usize, y: f32) -> EditSyncReached {
    EditSyncReached {
        display_row_index: index,
        y,
    }
}
fn duplicated_snapshots(mut plan: EditSyncPlan) -> EditSyncPlan {
    plan.row_snapshots.push(plan.row_snapshots[1].clone());
    plan.points.push(plan.points[2].clone());
    let duplicated = plan.point_rows.as_ref().unwrap().rows[1].clone();
    plan.point_rows.as_mut().unwrap().rows.push(duplicated);
    plan
}

#[test]
fn dense_install_avoids_legacy_map_after_complete_snapshot_comparison() {
    let plan = duplicated_snapshots(populated_plan());
    for (target, bottom, limit) in [
        (reached(5, 50.0), 80.0, 10),
        (reached(6, 60.0), 80.0, 10),
        (reached(6, 50.0), 80.0, 8),
    ] {
        let off = observed(plan.clone(), false, target, bottom, limit);
        let on = observed(plan.clone(), true, target, bottom, limit);
        equal(&on.0, &off.0);
        assert!(!on.0.rows.is_empty());
        assert!(off.1.legacy_installs > 0 && off.1.install_insertions > 0);
        assert_eq!(
            on.1.install_insertions, 0,
            "dense installation constructed legacy row map"
        );
        assert_eq!(on.1.legacy_installs, 0);
        assert!(on.1.dense_installs > 0);
        if target.y == 50.0 {
            assert!(std::ptr::eq(
                on.0.rows[0].1.as_ref(),
                plan.rows[0].1.as_ref()
            ));
        }
    }
}

#[test]
fn sparse_and_duplicate_survivors_keep_original_map_and_snapshot_multiplicity() {
    for indices in [[5, 7, 8], [5, 5, 6], [6, 5, 7]] {
        let mut plan = duplicated_snapshots(populated_plan());
        for ((index, _), replacement) in plan.rows.iter_mut().zip(indices) {
            *index = replacement;
        }
        let off = observed(plan.clone(), false, reached(5, 50.0), 200.0, usize::MAX);
        let on = observed(plan, true, reached(5, 50.0), 200.0, usize::MAX);
        equal(&on.0, &off.0);
        assert_eq!(
            on.0.rows.len(),
            3,
            "do not deduplicate retained row vectors"
        );
        assert!(on.1.install_insertions > 0);
        assert_eq!(on.1.dense_installs, 0);
    }
}

#[test]
fn non_monotone_clipping_and_all_dropped_rows_match_legacy_installer() {
    for ys in [
        [50.0, 500.0, 70.0],
        [500.0, 500.0, 500.0],
        [50.0, 60.0, 70.0],
    ] {
        let mut plan = populated_plan();
        for ((_, row), y) in plan.rows.iter_mut().zip(ys) {
            MatrixRow::make_mut(row).pixel_y = y;
        }
        for (bottom, limit) in [(80.0, usize::MAX), (80.0, 6), (0.0, usize::MAX)] {
            let off = observed(plan.clone(), false, reached(5, 50.0), bottom, limit);
            let on = observed(plan.clone(), true, reached(5, 50.0), bottom, limit);
            equal(&on.0, &off.0);
            assert!(on.0.point_rows.is_some(), "Some(empty) is not None");
            if ys[1] > 80.0 && ys[0] < 80.0 && ys[2] < 80.0 && limit == usize::MAX && bottom > 0.0 {
                assert_eq!(
                    on.1.dense_installs, 0,
                    "interior clipping leaves sparse keys"
                );
                assert!(on.1.install_insertions > 0);
            }
        }
    }
}

#[test]
fn signed_index_and_exclusive_end_boundaries_keep_legacy_installer() {
    // The endpoint case exists on 64-bit hosts without allocating any giant
    // vector. i64::MAX itself is a legacy-valid key but has no signed end+1.
    let Some(max_signed) = usize::try_from(i64::MAX).ok() else {
        return;
    };
    for index in [max_signed, usize::MAX] {
        let mut plan = populated_plan();
        plan.rows.truncate(1);
        plan.rows[0].0 = index;
        let off = observed(plan.clone(), false, reached(5, 50.0), 200.0, usize::MAX);
        let on = observed(plan, true, reached(5, 50.0), 200.0, usize::MAX);
        equal(&on.0, &off.0);
        assert_eq!(on.1.dense_installs, 0);
        assert!(on.1.legacy_installs > 0);
    }
}

#[test]
fn standalone_and_backward_installers_keep_original_hash_policy() {
    let plan = populated_plan();
    let off = observed(plan.clone(), false, reached(6, 60.0), 80.0, usize::MAX);
    let _on = Guard::set(true);
    // Renderer uses this original entry for scroll.edit=false; standalone
    // consumers keep this entry even while the new process selector is ON.
    let unchanged = plan.install(reached(6, 60.0), 80.0, usize::MAX);
    equal(&unchanged, &off.0);
    assert!(probes::counts().legacy_installs > 0 && probes::counts().install_insertions > 0);
    assert_eq!(probes::counts().dense_installs, 0);
}
