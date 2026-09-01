//! Strict combo oracle probes, batch 278: window-tree / frame-list behavioral.
//! window-tree structure across splits, window-list, window-at, and frame-list
//! / next-frame iteration.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_tree_split_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wt*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b)
          (let ((w3 (split-window nil nil 'below)))
            (set-window-buffer w3 b)
            (list (window-tree)
                  (length (window-list))
                  (window-at 0 0)
                  (windowp (window-at 0 0))
                  (eq (window-root) (selected-window))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function window-root)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_child_parent_siblings_behavioral() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wcp*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w2 (split-window nil nil 'below)))
          (let ((w3 (split-window w2 nil 'right)))
            (list (eq (window-parent w2) (window-parent (selected-window)))
                  (window-child-count (window-parent w2))
                  (window-next-sibling w2)
                  (window-prev-sibling w3)
                  (eq (window-child (window-parent w2) 0) (selected-window))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_frame_list_next_frame_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((f (selected-frame)))
  (list (consp (frame-list))
        (memq f (frame-list))
        (eq (next-frame f) f)
        (eq (next-frame f nil (selected-frame)) (selected-frame))
        (length (frame-list))
        (eq (car (frame-list)) (selected-frame))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments next-frame 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
