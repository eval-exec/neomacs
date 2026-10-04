//! Compare the complete old/new synchronization plan before cost assertions.
//! Fixtures own protocol rows and numeric snapshots; no Lisp state is shared.
use super::*;
use crate::incremental_layout::edit_sync::dense_index_test_support::{self as probes, Guard};
use neovm_core::buffer::position::LispCharPos1;
use neovm_core::window::{
    DisplayPointRole, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
};

fn snapshot_matrix() -> RetainedWindowMatrix {
    let mut matrix = monospace_matrix(6);
    let snapshot = std::sync::Arc::make_mut(&mut matrix.display_snapshot);
    for (index, row) in matrix.matrix.rows.iter().enumerate() {
        snapshot.rows.push(DisplayRowSnapshot {
            row: index as i64,
            y: row.pixel_y as i64,
            height: row.height_px as i64,
            start_buffer_pos: Some(LispCharPos1::from_one_based_usize(row.start_charpos + 1)),
            end_buffer_pos: Some(LispCharPos1::from_one_based_usize(row.end_charpos + 1)),
            end_col: 2,
            end_x: 16,
            ..DisplayRowSnapshot::default()
        });
        snapshot.points.push(DisplayPointSnapshot {
            buffer_pos: LispCharPos1::from_one_based_usize(row.start_charpos + 1),
            role: DisplayPointRole::Glyph,
            x: 0,
            y: row.pixel_y as i64,
            width: 8,
            height: 16,
            row: index as i64,
            col: 0,
        });
    }
    // Duplicate snapshots/points retain order and multiplicity; negative and
    // chrome/outside keys must remain excluded by either membership policy.
    snapshot.rows.push(snapshot.rows[3].clone());
    snapshot.points.push(snapshot.points[3].clone());
    let mut outside = snapshot.rows[0].clone();
    outside.row = -1;
    snapshot.rows.push(outside);
    let mut outside = snapshot.points[0].clone();
    outside.row = -1;
    snapshot.points.push(outside);
    snapshot.point_rows = Some(DisplayPointRows::from_points(snapshot.points.clone()));
    matrix
}
fn observed_plan(
    matrix: &RetainedWindowMatrix,
    body: &[(usize, &MatrixRow)],
    delta: i64,
    on: bool,
) -> (Option<edit_sync::EditSyncPlan>, probes::Counts) {
    let _guard = Guard::set(on);
    let plan = edit_sync::plan_positions(matrix, body, 1, EditReplayPositions::new(15, 16, delta));
    (plan, probes::counts())
}
fn equal_plan(left: &Option<edit_sync::EditSyncPlan>, right: &Option<edit_sync::EditSyncPlan>) {
    assert_eq!(
        format!("{left:#?}"),
        format!("{right:#?}"),
        "complete rows, sources and all snapshot streams"
    );
}
fn body(matrix: &RetainedWindowMatrix) -> Vec<(usize, &MatrixRow)> {
    matrix
        .matrix
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.enabled && !RetainedWindowMatrix::is_chrome_role(row.role))
        .collect()
}

#[test]
fn dense_plan_avoids_legacy_snapshot_membership_after_complete_plan_comparison() {
    let matrix = snapshot_matrix();
    let body = body(&matrix);
    let off = observed_plan(&matrix, &body, 1, false);
    let on = observed_plan(&matrix, &body, 1, true);
    equal_plan(&on.0, &off.0);
    assert!(on.0.is_some());
    assert!(off.1.legacy_plans > 0 && off.1.plan_insertions > 0);
    assert_eq!(
        on.1.plan_insertions, 0,
        "dense planning constructed legacy index set"
    );
    assert_eq!(on.1.legacy_plans, 0);
    assert!(on.1.dense_plans > 0);
}

#[test]
fn dense_planning_matches_legacy_signed_positions_and_duplicate_snapshots() {
    let matrix = snapshot_matrix();
    let body = body(&matrix);
    for delta in [-1, 0, 1] {
        let off = observed_plan(&matrix, &body, delta, false);
        let on = observed_plan(&matrix, &body, delta, true);
        equal_plan(&on.0, &off.0);
        let plan = on.0.unwrap();
        assert_eq!(
            plan.row_snapshots.iter().filter(|row| row.row == 3).count(),
            2
        );
        assert_eq!(plan.points.iter().filter(|point| point.row == 3).count(), 2);
        assert!(
            plan.row_snapshots
                .iter()
                .all(|row| (2..6).contains(&row.row))
        );
        assert!(
            plan.point_rows
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .all(|row| (2..6).contains(&row.row()))
        );
    }
}

#[test]
fn sparse_and_duplicate_candidate_indices_keep_legacy_selection() {
    let matrix = snapshot_matrix();
    let original = body(&matrix);
    for tail in [[2, 4, 5, 6], [2, 3, 3, 4], [3, 2, 4, 5]] {
        let mut body = original.clone();
        for (entry, index) in body[2..].iter_mut().zip(tail) {
            entry.0 = index;
        }
        let off = observed_plan(&matrix, &body, 0, false);
        let on = observed_plan(&matrix, &body, 0, true);
        equal_plan(&on.0, &off.0);
        assert!(on.0.is_some());
        assert!(on.1.plan_insertions > 0 && on.1.legacy_plans > 0);
        assert_eq!(on.1.dense_plans, 0);
    }
}
