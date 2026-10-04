//! Contracts for the new private FnOnce seam. These require the production
//! API and are first-isolation GREEN controls, not literal-bd0e regressions.
use super::*;
use crate::incremental_layout::{EditReplayPositions, EditReplaySourceProof};
use std::cell::Cell;

fn monospace_matrix(rows: usize) -> RetainedWindowMatrix {
    let mut matrix = synthetic_matrix(0, rows);
    for index in 0..rows {
        let row = MatrixRow::make_mut(&mut matrix.matrix.rows[index]);
        for offset in 0..9 {
            let mut glyph = neomacs_display_protocol::glyph_matrix::Glyph::char(
                'a',
                FaceId::new(0),
                index * 10 + offset,
            );
            glyph.pixel_width = 8.0;
            row.glyphs[GlyphArea::Text.index()].push(glyph);
        }
    }
    matrix
}
fn current(matrix: &RetainedWindowMatrix, delta: i64) -> RetainedWindowKey {
    let mut key = matrix.key.clone();
    key.buffer_size += delta;
    key.chars_modified_tick += 1;
    key.point = 15;
    key
}
fn eager(
    matrix: &RetainedWindowMatrix,
    key: &RetainedWindowKey,
    damage: EditDamage,
    proved: bool,
) -> Option<ScrollReplay> {
    matrix.edit_replay_with(
        key,
        damage,
        edit_sync::BelowReuse::Sync {
            prove_fallback: proved,
        },
    )
}
fn complete_replay_equal(left: &Option<ScrollReplay>, right: &Option<ScrollReplay>) {
    // ScrollReplay/MatrixRow/point rows derive Debug across their complete
    // numeric/row fields; no active heap/source Value exists in this fixture.
    assert_eq!(format!("{left:#?}"), format!("{right:#?}"));
}

#[test]
fn admitted_sync_skips_fn_once_and_keeps_complete_eager_replay() {
    let matrix = monospace_matrix(5);
    let key = current(&matrix, 1);
    for damage in [EditDamage::new(15, 16, 1, 0), EditDamage::new(20, 21, 1, 0)] {
        let lazy = matrix.edit_replay_sync_lazy(&key, damage.into(), || {
            panic!("accepted Sync must not request source proof")
        });
        assert!(lazy.as_ref().unwrap().sync.is_some());
        complete_replay_equal(&lazy, &eager(&matrix, &key, damage, true));
    }
}

#[test]
fn rejected_sync_invokes_one_proof_and_keeps_sync_prefix_bounded_fallback() {
    // The only tail begins at end_old=20; general Sync requires >=21, while
    // bounded Prove admits it after one old/new newline proof. Each synthetic
    // source row covers nine ASCII glyphs followed by its terminating newline.
    let matrix = monospace_matrix(3);
    let key = current(&matrix, 1);
    let damage = EditDamage::new(15, 21, 1, 1);
    let calls = Cell::new(0);
    let lazy = matrix.edit_replay_sync_lazy(&key, damage.into(), || {
        calls.set(calls.get() + 1);
        EditReplaySourceProof::Simple { newlines: 1 }
    });
    assert_eq!(calls.get(), 1);
    assert!(lazy.as_ref().unwrap().bound_walk);
    assert!(lazy.as_ref().unwrap().sync.is_none());
    complete_replay_equal(&lazy, &eager(&matrix, &key, damage, true));
}

#[test]
fn rejected_or_mismatched_source_keeps_exact_above_only_replay() {
    let matrix = monospace_matrix(3);
    let key = current(&matrix, 1);
    for proof in [
        EditReplaySourceProof::Rejected,
        EditReplaySourceProof::Simple { newlines: 0 },
    ] {
        let calls = Cell::new(0);
        let damage = EditDamage::new(15, 21, 1, 0);
        let lazy = matrix.edit_replay_sync_lazy(&key, damage.into(), || {
            calls.set(calls.get() + 1);
            proof
        });
        assert_eq!(calls.get(), 1);
        assert!(!lazy.as_ref().unwrap().bound_walk);
        complete_replay_equal(&lazy, &eager(&matrix, &key, damage, false));
    }
}

#[test]
fn impossible_tail_width_and_common_admission_do_not_read_source_proof() {
    let matrix = monospace_matrix(2);
    let key = current(&matrix, 1);
    let damage = EditDamage::new(15, 16, 1, 0);
    let no_tail = matrix.edit_replay_sync_lazy(&key, damage.into(), || {
        panic!("no tail makes bounded proof impossible")
    });
    complete_replay_equal(&no_tail, &eager(&matrix, &key, damage, true));
    let matrix = monospace_matrix(3);
    let key = current(&matrix, 1000);
    let damage = EditDamage::new(15, 1020, 1000, 1);
    let too_wide = matrix.edit_replay_sync_lazy(&key, damage.into(), || {
        panic!("failed bounded width gate must skip proof")
    });
    complete_replay_equal(&too_wide, &eager(&matrix, &key, damage, true));
    for invalid in [true, false] {
        let matrix = monospace_matrix(3);
        let mut key = current(&matrix, 1);
        if invalid {
            key.buffer_id += 1;
        } else {
            key.vscroll = 1;
        }
        let lazy = matrix.edit_replay_sync_lazy(&key, EditReplayPositions::new(15, 21, 1), || {
            panic!("common admission rejected before proof")
        });
        assert!(lazy.is_none());
    }
}

#[test]
fn positions_and_eager_plan_preserve_coordinate_conversion() {
    let damage = EditDamage::new(15, 21, 1, 1);
    let positions = EditReplayPositions::from(damage);
    assert_eq!(positions.start(), damage.start());
    assert_eq!(positions.end_old(), damage.end_old());
    assert_eq!(positions.delta(), damage.delta());
    let matrix = monospace_matrix(5);
    let body: Vec<_> = matrix
        .matrix
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.enabled && !RetainedWindowMatrix::is_chrome_role(row.role))
        .collect();
    assert_eq!(
        format!("{:?}", edit_sync::plan(&matrix, &body, 1, damage)),
        format!(
            "{:?}",
            edit_sync::plan_positions(&matrix, &body, 1, positions)
        )
    );
}

#[cfg(test)]
#[path = "edit_sync_dense_index_plan_test.rs"]
mod dense_index_plan_tests;
