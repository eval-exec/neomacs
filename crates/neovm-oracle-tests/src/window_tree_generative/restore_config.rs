//! Deterministic configuration round trips retain full window edges after each step.
//!
//! Reuse the generative program rather than the shape-only renderer: losing a
//! vertical row on restore must remain visible in both the saved GNU expectation
//! and the live engine comparison. Expectations are populated from GNU only.

use super::{Side, WindowOp, program};
use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_restore_window_configuration_preserves_top_then_bottom_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 18) (\"*side-1*\" 0 18 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_bottom_then_top_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*scratch*\" 0 1 80 19) (\"*side-0*\" 0 19 80 25))) (ok (v (\"*side-1*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-0*\" 0 19 80 25))) (ok (v (\"*side-1*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-0*\" 0 19 80 25))) (ok (v (\"*side-1*\" 0 1 80 7) (\"*scratch*\" 0 7 80 18) (\"*side-0*\" 0 18 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_left_then_right_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Left,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Right,
            slot: 0,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (h (\"*side-0*\" 0 0 20 24) (\"*scratch*\" 20 0 80 24))) (ok (h (\"*side-0*\" 0 0 20 24) (\"*scratch*\" 20 0 60 24) (\"*side-1*\" 60 0 80 24))) (ok (h (\"*side-0*\" 0 0 20 24) (\"*scratch*\" 20 0 60 24) (\"*side-1*\" 60 0 80 24))) (ok (h (\"*side-0*\" 0 1 20 24) (\"*scratch*\" 20 1 60 24) (\"*side-1*\" 60 1 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_right_then_left_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Right,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Left,
            slot: 0,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (h (\"*scratch*\" 0 0 60 24) (\"*side-0*\" 60 0 80 24))) (ok (h (\"*side-1*\" 0 0 20 24) (\"*scratch*\" 20 0 60 24) (\"*side-0*\" 60 0 80 24))) (ok (h (\"*side-1*\" 0 0 20 24) (\"*scratch*\" 20 0 60 24) (\"*side-0*\" 60 0 80 24))) (ok (h (\"*side-1*\" 0 1 20 24) (\"*scratch*\" 20 1 60 24) (\"*side-0*\" 60 1 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_two_top_slots_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 1,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (h (\"*side-0*\" 0 1 40 7) (\"*side-1*\" 40 1 80 7)) (\"*scratch*\" 0 7 80 25))) (ok (v (h (\"*side-0*\" 0 1 40 7) (\"*side-1*\" 40 1 80 7)) (\"*scratch*\" 0 7 80 25))) (ok (v (h (\"*side-0*\" 0 1 40 7) (\"*side-1*\" 40 1 80 7)) (\"*scratch*\" 0 7 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_two_bottom_slots_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 1,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*scratch*\" 0 1 80 19) (\"*side-0*\" 0 19 80 25))) (ok (v (\"*scratch*\" 0 1 80 19) (h (\"*side-0*\" 0 19 40 25) (\"*side-1*\" 40 19 80 25)))) (ok (v (\"*scratch*\" 0 1 80 19) (h (\"*side-0*\" 0 19 40 25) (\"*side-1*\" 40 19 80 25)))) (ok (v (\"*scratch*\" 0 1 80 18) (h (\"*side-0*\" 0 18 40 24) (\"*side-1*\" 40 18 80 24)))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_two_left_slots_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Left,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Left,
            slot: 1,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (h (\"*side-0*\" 0 0 20 24) (\"*scratch*\" 20 0 80 24))) (ok (h (v (\"*side-0*\" 0 1 20 13) (\"*side-1*\" 0 13 20 25)) (\"*scratch*\" 20 1 80 25))) (ok (h (v (\"*side-0*\" 0 1 20 13) (\"*side-1*\" 0 13 20 25)) (\"*scratch*\" 20 1 80 25))) (ok (h (v (\"*side-0*\" 0 1 20 13) (\"*side-1*\" 0 13 20 24)) (\"*scratch*\" 20 1 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_two_right_slots_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::SideWindow {
            side: Side::Right,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Right,
            slot: 1,
        },
        WindowOp::SaveConfiguration,
        WindowOp::RestoreConfiguration,
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (h (\"*scratch*\" 0 0 60 24) (\"*side-0*\" 60 0 80 24))) (ok (h (\"*scratch*\" 0 1 60 25) (v (\"*side-0*\" 60 1 80 13) (\"*side-1*\" 60 13 80 25)))) (ok (h (\"*scratch*\" 0 1 60 25) (v (\"*side-0*\" 60 1 80 13) (\"*side-1*\" 60 13 80 25)))) (ok (h (\"*scratch*\" 0 1 60 24) (v (\"*side-0*\" 60 1 80 13) (\"*side-1*\" 60 13 80 24)))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_frame_geometry_budget() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let side_windows = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
    ]);
    // Append observations before the program consumes its reverse-order log.
    let prefix = side_windows
        .strip_suffix("(nreverse oracle--log)\n")
        .expect("the generative program ends by returning its operation log");
    let form = format!(
        r#"{prefix}
(let ((geometry
       (lambda ()
         (let* ((frame (selected-frame))
                (root (frame-root-window frame))
                (mini (minibuffer-window frame)))
           (list (frame-height frame)
                 (frame-pixel-height frame)
                 (frame-parameter frame 'menu-bar-lines)
                 (window-pixel-height root)
                 (window-total-height root)
                 (window-pixel-height mini)
                 (window-total-height mini)
                 (window-edges root)
                 (window-edges mini)
                 (oracle--wt (car (window-tree frame))))))))
  (let ((before (funcall geometry)))
    (oracle--step (setq oracle--config (current-window-configuration)))
    (oracle--step (when oracle--config
                   (set-window-configuration oracle--config)))
    (list before (funcall geometry) (nreverse oracle--log))))
"#,
    );
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((25 25 1 24 24 1 1 (0 1 80 25) (0 25 80 26) (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (25 25 1 23 23 1 1 (0 1 80 24) (0 24 80 25) (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 18) (\"*side-1*\" 0 18 80 24))) ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 18) (\"*side-1*\" 0 18 80 24)))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_fixed_normal_window_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let side_windows = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
    ]);
    let prefix = side_windows
        .strip_suffix("(nreverse oracle--log)\n")
        .expect("the generative program ends by returning its operation log");
    let form = format!(
        r#"{prefix}
(oracle--step
 (with-current-buffer "*scratch*"
   (setq-local window-size-fixed 'height)))
(oracle--step (setq oracle--config (current-window-configuration)))
(oracle--step (when oracle--config
               (set-window-configuration oracle--config)))
(nreverse oracle--log)
"#,
    );
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_preserves_normal_window_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let side_windows = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
    ]);
    let prefix = side_windows
        .strip_suffix("(nreverse oracle--log)\n")
        .expect("the generative program ends by returning its operation log");
    let form = format!(
        r#"{prefix}
(oracle--step
 (window-preserve-size (get-buffer-window "*scratch*" (selected-frame)) nil t))
(oracle--step (setq oracle--config (current-window-configuration)))
(oracle--step (when oracle--config
               (set-window-configuration oracle--config)))
(nreverse oracle--log)
"#,
    );
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 24))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_propagates_root_resize_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let saved_windows = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
        WindowOp::SaveConfiguration,
    ]);
    let prefix = saved_windows
        .strip_suffix("(nreverse oracle--log)\n")
        .expect("the generative program ends by returning its operation log");
    let form = format!(
        r#"{prefix}
(advice-add 'window--resize-root-window :before
            (lambda (&rest _args) (error "stop root resize")))
(oracle--step (when oracle--config
               (set-window-configuration oracle--config)))
(nreverse oracle--log)
"#,
    );
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (err (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))))""#
        ]],
    );
}

#[test]
fn oracle_restore_window_configuration_propagates_cell_reconciliation_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let saved_windows = program(&[
        WindowOp::SideWindow {
            side: Side::Above,
            slot: 0,
        },
        WindowOp::SideWindow {
            side: Side::Below,
            slot: 0,
        },
        WindowOp::SaveConfiguration,
    ]);
    let prefix = saved_windows
        .strip_suffix("(nreverse oracle--log)\n")
        .expect("the generative program ends by returning its operation log");
    let form = format!(
        r#"{prefix}
(advice-add 'window--pixel-to-total :before
            (lambda (&rest _args) (error "stop cell reconciliation")))
(oracle--step (when oracle--config
               (set-window-configuration oracle--config)))
(nreverse oracle--log)
"#,
    );
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (ok (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 19) (\"*side-1*\" 0 19 80 25))) (err (v (\"*side-0*\" 0 1 80 7) (\"*scratch*\" 0 7 80 18) (\"*side-1*\" 0 18 80 24))))""#
        ]],
    );
}
