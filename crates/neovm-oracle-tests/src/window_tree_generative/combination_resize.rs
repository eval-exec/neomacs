//! Deterministic normal-size regressions found by the generative window-tree
//! property.
//!
//! A split that splices into an existing combination must leave the other
//! siblings' normal sizes alone: GNU `Fsplit_window_internal` commits only the
//! `new_normal`s that `split-window` staged.  Later `delete-window` arithmetic
//! under `window-combination-resize` distributes the freed space by those
//! normal sizes, so a sibling whose share was restated from its pixel width
//! (27/80 instead of 1/3) ends up one column off.  Expectations are populated
//! from GNU only (`NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`).

use super::{Resize, Side, WindowOp, program};
use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

/// window-oracle-nightly 2026-10-29 (seed
/// `b31de55a2e01826950c7e467f8fa0ccea715dfab03261b3d71f9692bff50acdd`).
#[test]
fn oracle_delete_with_combination_resize_uses_unrestated_sibling_normals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = program(&[
        WindowOp::Split {
            window: 0,
            side: Side::Left,
            limit: false,
            resize: Resize::Nil,
        },
        WindowOp::Split {
            window: 0,
            side: Side::Left,
            limit: false,
            resize: Resize::T,
        },
        WindowOp::Split {
            window: 2,
            side: Side::Left,
            limit: false,
            resize: Resize::Nil,
        },
        WindowOp::Delete {
            window: 4,
            resize: Resize::T,
        },
    ]);
    assert_oracle_parity_expect(
        &form,
        expect_test::expect![[
            r#""OK ((ok (h (\"*scratch*\" 0 0 40 24) (\"*scratch*\" 40 0 80 24)) ((1.0 1.0) ((1.0 0.5)) ((1.0 0.5)))) (ok (h (\"*scratch*\" 0 0 27 24) (\"*scratch*\" 27 0 53 24) (\"*scratch*\" 53 0 80 24)) ((1.0 1.0) ((1.0 0.33333333333333337)) ((1.0 0.3333333333333333)) ((1.0 0.33333333333333337)))) (ok (h (\"*scratch*\" 0 0 27 24) (\"*scratch*\" 27 0 40 24) (\"*scratch*\" 40 0 53 24) (\"*scratch*\" 53 0 80 24)) ((1.0 1.0) ((1.0 0.33333333333333337)) ((1.0 0.16666666666666666)) ((1.0 0.16666666666666666)) ((1.0 0.33333333333333337)))) (ok (h (\"*scratch*\" 0 0 38 24) (\"*scratch*\" 38 0 59 24) (\"*scratch*\" 59 0 80 24)) ((1.0 1.0) ((1.0 0.5000000000000001)) ((1.0 0.25)) ((1.0 0.25)))))""#
        ]],
    );
}
