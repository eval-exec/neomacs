//! Oracle parity tests for the *shape* of the window tree after splits and
//! deletions.
//!
//! These probe the seam between `lisp/window.el` (which neomacs ships verbatim
//! from GNU) and the Rust re-implementation of `src/window.c`. Every decision
//! window.el stages — `window-combination-limit`, which sibling absorbs a
//! deleted window's space — has to be honored by the primitive rather than
//! re-derived from the tree, and a wrong answer shows up as a differently
//! shaped tree rather than as an error.
//!
//! Each case renders the tree as combination direction + buffer names + column
//! spans, which is stable across GNU `--batch` and neomacs.
//!
//! Regression origin: a `right` side window landed mid-frame when a `left` side
//! window already existed, because the split path read the per-window
//! `combination_limit` slot instead of the dynamic variable of the same name.

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Renders `(window-tree)` as `(h|v CHILD...)` with `(BUFFER LEFT RIGHT)` for
/// each live window. Column spans only: GNU `--batch` and neomacs place the
/// root at different y origins in some configurations, and the tree *shape* is
/// what these cases are about.
const RENDERER: &str = r#"
(defun oracle--wt (node)
  (cond
   ((windowp node)
    (list (buffer-name (window-buffer node))
          (nth 0 (window-edges node))
          (nth 2 (window-edges node))))
   ((consp node)
    (cons (if (car node) 'v 'h) (mapcar #'oracle--wt (cddr node))))))
(defun oracle--tree () (oracle--wt (car (window-tree))))
(defun oracle--side (name s w)
  (display-buffer (get-buffer-create name)
                  (list 'display-buffer-in-side-window
                        (cons 'side s) (cons 'window-width w))))
"#;

fn tree_form(body: &str) -> String {
    format!("{RENDERER}\n(progn {body} (oracle--tree))")
}

// ---------------------------------------------------------------------------
// `window-combination-limit` — GNU src/window.c:5423-5431.
//
//   combination_limit = (EQ (Vwindow_combination_limit, Qt)
//                        || NILP (o->parent)
//                        || parent is ortho-combined);
//
// The *dynamic variable* drives this; the per-window `combination_limit` slot
// is only read by `recombine_windows` on the delete path (src/window.c:2616).
// ---------------------------------------------------------------------------

/// Binding the variable to `t` must interpose a fresh parent even though the
/// target's parent is iso-combined.
#[test]
fn oracle_split_window_honors_dynamic_window_combination_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let ((window-combination-limit t))
           (split-window nil nil 'right)
           (split-window nil nil 'right))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40)) (\"*scratch*\" 40 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// The control case: with the variable nil an iso-combined parent is reused and
/// the combination stays flat.
#[test]
fn oracle_split_window_reuses_iso_combined_parent_when_limit_is_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let ((window-combination-limit nil))
           (split-window nil nil 'right)
           (split-window nil nil 'right))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// Only the symbol `t` forces a new parent — the other `display-buffer`-related
/// values behave like nil by the time the split runs.
#[test]
fn oracle_split_window_treats_non_t_combination_limit_like_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let ((window-combination-limit 'window-size))
           (split-window nil nil 'right)
           (split-window nil nil 'right))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// An ortho-combined parent cannot hold the new sibling, so a parent is
/// interposed regardless of the variable.
#[test]
fn oracle_split_window_nests_when_parent_is_ortho_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form("(split-window nil nil 'right) (split-window nil nil 'below)");
    let expect = expect_test::expect![[
        r#""OK (h (v (\"*scratch*\" 0 40) (\"*scratch*\" 0 40)) (\"*scratch*\" 40 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// Reusing a parent must not be gated on the split target being a LEAF: GNU
/// splices a sibling next to an internal node just the same, which is how a
/// side window is attached beside the frame's main-window group.
#[test]
fn oracle_split_window_reuses_iso_combined_parent_when_target_is_internal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(split-window nil nil 'right)
         (let ((window-combination-limit t))
           (split-window nil nil 'right))
         (let ((window-combination-limit nil)
               (ignore-window-parameters t))
           (split-window (window-parent (selected-window)) nil 'right))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (h (\"*scratch*\" 0 10) (\"*scratch*\" 10 20)) (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// The new parent's stored `combination_limit` slot is set only when the
/// dynamic variable was `t` (GNU src/window.c:5557-5560).
#[test]
fn oracle_new_parent_is_sealed_only_when_the_variable_was_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        "{RENDERER}
(list
 (progn (let ((window-combination-limit t)) (split-window nil nil 'right))
        (window-combination-limit (window-parent (selected-window))))
 (progn (delete-other-windows)
        (split-window nil nil 'right)
        (split-window nil nil 'below)
        (window-combination-limit (window-parent (selected-window)))))"
    );
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// Side windows.
//
// `split-window` binds `window-combination-limit` to t when the split target
// has a side-window sibling (lisp/window.el), which keeps the frame's main area
// in its own combination so later side windows attach at the frame edge.
// ---------------------------------------------------------------------------

/// A `left` side window alone.
#[test]
fn oracle_left_side_window_occupies_the_frame_left_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form("(oracle--side \"*L*\" 'left 20)");
    let expect = expect_test::expect![[r#""OK (h (\"*L*\" 0 20) (\"*scratch*\" 20 80))""#]];
    assert_oracle_parity_expect(&form, expect);
}

/// Splitting the main window while a side window is its sibling must wrap the
/// main area in its own combination rather than flatten it into the root.
#[test]
fn oracle_splitting_next_to_a_side_window_wraps_the_main_area() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*L*\" 'left 20)
         (split-window (window-main-window) nil 'right)",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*L*\" 0 20) (h (\"*scratch*\" 20 50) (\"*scratch*\" 50 80)))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// The reported bug: with a `left` side window and a split main area, a `right`
/// side window must land at the frame's far right as the LAST child of the root
/// combination — not between the main windows.
#[test]
fn oracle_right_side_window_lands_at_far_right_when_left_side_window_exists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*L*\" 'left 20)
         (split-window (window-main-window) nil 'right)
         (oracle--side \"*R*\" 'right 20)",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*L*\" 0 20) (h (\"*scratch*\" 20 40) (\"*scratch*\" 40 60)) (\"*R*\" 60 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// The mirror image: a `left` side window added when a `right` one exists.
#[test]
fn oracle_left_side_window_lands_at_far_left_when_right_side_window_exists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*R*\" 'right 20)
         (split-window (window-main-window) nil 'right)
         (oracle--side \"*L*\" 'left 20)",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*L*\" 0 20) (h (\"*scratch*\" 20 40) (\"*scratch*\" 40 60)) (\"*R*\" 60 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// Side windows on all four edges nest as vertical-outer / horizontal-inner.
#[test]
fn oracle_side_windows_on_all_four_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*L*\" 'left 20) (oracle--side \"*R*\" 'right 20)
         (oracle--side \"*T*\" 'top 4) (oracle--side \"*B*\" 'bottom 4)",
    );
    let expect = expect_test::expect![[
        r#""OK (v (\"*T*\" 0 80) (h (\"*L*\" 0 20) (\"*scratch*\" 20 60) (\"*R*\" 60 80)) (\"*B*\" 0 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// Splitting the main group again, orthogonally, after both side windows exist.
#[test]
fn oracle_orthogonal_split_of_the_main_group_between_side_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*L*\" 'left 20)
         (split-window (window-main-window) nil 'right)
         (oracle--side \"*R*\" 'right 20)
         (split-window (window-main-window) nil 'below)",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*L*\" 0 20) (v (h (\"*scratch*\" 20 40) (\"*scratch*\" 40 60)) (\"*scratch*\" 20 60)) (\"*R*\" 60 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

// ---------------------------------------------------------------------------
// `delete-window` space reclamation.
//
// `lisp/window.el` picks ONE sibling — `(or (window-left w) (window-right w))`,
// i.e. previous if present else next — and stages its size with
// `window--resize-this-window`. `Fdelete_window_internal` commits the staged
// `new_pixel` values via `window_resize_apply`; the primitive must not invent a
// layout of its own.
// ---------------------------------------------------------------------------

/// Deleting the middle of three siblings gives its columns to the one on its
/// left; the third window must not move.
#[test]
fn oracle_deleting_a_middle_window_gives_its_space_to_the_previous_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let ((window-combination-limit nil))
           (split-window nil nil 'right) (split-window nil nil 'right))
         (delete-window (nth 1 (window-list nil 'no-minibuf nil)))",
    );
    let expect = expect_test::expect![[r#""OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))""#]];
    assert_oracle_parity_expect(&form, expect);
}

/// Deleting the last of three siblings: the first window must not move.
#[test]
fn oracle_deleting_the_last_window_gives_its_space_to_the_previous_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let ((window-combination-limit nil))
           (split-window nil nil 'right) (split-window nil nil 'right))
         (delete-window (nth 2 (window-list nil 'no-minibuf nil)))",
    );
    let expect = expect_test::expect![[r#""OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 80))""#]];
    assert_oracle_parity_expect(&form, expect);
}

/// Deleting the first sibling has no previous sibling, so the NEXT one absorbs
/// the space and slides left; the third window must not move.
#[test]
fn oracle_deleting_the_first_window_gives_its_space_to_the_next_sibling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let ((window-combination-limit nil))
           (split-window nil nil 'right) (split-window nil nil 'right))
         (delete-window (nth 0 (window-list nil 'no-minibuf nil)))",
    );
    let expect = expect_test::expect![[r#""OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))""#]];
    assert_oracle_parity_expect(&form, expect);
}

/// Deleting a window out of a nested combination promotes the sole survivor
/// into its parent's slot, with the parent's geometry.
#[test]
fn oracle_deleting_from_a_nested_combination_promotes_the_survivor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(split-window nil nil 'right)
         (let ((window-combination-limit t)) (split-window nil nil 'right))
         (delete-window (selected-window))",
    );
    let expect = expect_test::expect![[r#""OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))""#]];
    assert_oracle_parity_expect(&form, expect);
}

/// Deleting a side window must not resize the *other* side window: the freed
/// columns belong to the main group, not to every child of the root.
#[test]
fn oracle_deleting_a_right_side_window_keeps_the_left_side_window_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*L*\" 'left 20)
         (split-window (window-main-window) nil 'right)
         (oracle--side \"*R*\" 'right 20)
         (delete-window (get-buffer-window \"*R*\"))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*L*\" 0 20) (h (\"*scratch*\" 20 50) (\"*scratch*\" 50 80)))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// Deleting the only side window hands the whole frame back to the main group.
#[test]
fn oracle_deleting_the_only_side_window_returns_its_space_to_the_main_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(oracle--side \"*L*\" 'left 20)
         (split-window (window-main-window) nil 'right)
         (delete-window (get-buffer-window \"*L*\"))",
    );
    let expect = expect_test::expect![[r#""OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))""#]];
    assert_oracle_parity_expect(&form, expect);
}

/// GNU `recombine_windows` (src/window.c:2606-2650), called on the window
/// promoted into its parent's slot (src/window.c:5801): an UNSEALED
/// combination along the same axis as its new parent dissolves into it.
///
/// The nesting is built with *orthogonal* splits on purpose — a
/// `window-combination-limit t` split seals the parent it creates, and GNU
/// skips sealed nodes, so a sealed reproducer silently passes without ever
/// exercising the merge.
#[test]
fn oracle_deleting_recombines_the_promoted_child_into_an_iso_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let* ((w (selected-window)) (n1 (split-window w nil 'right)))
           (select-window n1)
           (let ((n2 (split-window n1 nil 'below)))
             (select-window n2)
             (split-window n2 nil 'right)
             (delete-window n1)))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 60) (\"*scratch*\" 60 80))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}

/// The same shape with the inner combination SEALED must survive intact —
/// which is what `set-window-combination-limit` is for, and what
/// `window--make-major-side-window` relies on (Bug#80665).
#[test]
fn oracle_deleting_does_not_recombine_a_sealed_promoted_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = tree_form(
        "(let* ((w (selected-window)) (n1 (split-window w nil 'right)))
           (select-window n1)
           (let ((n2 (split-window n1 nil 'below)))
             (select-window n2)
             (split-window n2 nil 'right)
             (set-window-combination-limit (window-parent (selected-window)) t)
             (delete-window n1)))",
    );
    let expect = expect_test::expect![[
        r#""OK (h (\"*scratch*\" 0 40) (h (\"*scratch*\" 40 60) (\"*scratch*\" 60 80)))""#
    ]];
    assert_oracle_parity_expect(&form, expect);
}
