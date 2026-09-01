//! Divergence tests: window + buffer + point + marker spatial combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_split_window_point_per_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 15 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LINE-1\nLINE-2\nLINE-3\nLINE-4\nLINE-5")
  (let ((w1 (selected-window))
        (w2 (split-window nil 3)))
    (set-window-point w1 1)
    (set-window-point w2 15)
    (set-window-start w1 1)
    (set-window-start w2 15)
    (list (window-point w1) (window-point w2)
          (eq (window-buffer w1) (window-buffer w2))
          (delete-window w2)
          (= (length (window-list)) 1)))) "#,
        expect,
    );
}

#[test]
fn divergence_walk_windows_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((windows nil))
  (walk-windows (lambda (w) (push (list (window-buffer w) (window-point w)) windows)))
  (list (length windows)
        (>= (length windows) 1)
        (cl-every #'windowp (window-list)))) "#,
        expect,
    );
}

#[test]
fn divergence_set_window_buffer_then_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (#<killed buffer> 7 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((buf (generate-new-buffer "*test-wbuf*"))
       (w (selected-window)))
  (with-current-buffer buf
    (insert "Hello World in special buffer"))
  (set-window-buffer w buf)
  (set-window-point w 7)
  (list (window-buffer w)
        (window-point w)
        (>= (window-point w) 7)
        (set-window-buffer w (get-buffer "*scratch*"))
        (kill-buffer buf))) "#,
        expect,
    );
}

#[test]
fn divergence_save_selected_window_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w1 (selected-window)))
  (save-selected-window
    (let ((w2 (split-window nil nil 'right)))
      (select-window w2)
      (let ((inside-selected (selected-window)))
        (list (eq inside-selected w2)
              (not (eq inside-selected w1))))))
  (list (eq (selected-window) w1)
        (delete-other-windows)
        (= (length (window-list)) 1))) "#,
        expect,
    );
}

#[test]
fn divergence_window_configuration_full_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 15 45)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((wc1 (current-window-configuration)))
  (split-window nil nil 'right)
  (let ((wc2 (current-window-configuration))
        (n2 (length (window-list))))
    (split-window nil nil 'below)
    (let ((wc3 (current-window-configuration))
          (n3 (length (window-list))))
      (set-window-configuration wc1)
      (list (= (length (window-list)) 1)
            n2 n3
            (>= n3 n2)
            (set-window-configuration wc2)
            (= (length (window-list)) n2)
            (set-window-configuration wc1)
            (= (length (window-list)) 1)))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_scroll_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 t nil 200 nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (dotimes (i 50) (insert (format "Line %02d\n" i)))
  (goto-char 1)
  (let ((w (selected-window)))
    (set-window-start w 1)
    (list (window-start w)
          (>= (window-end w) (window-start w))
          (pos-visible-in-window-p (window-start w))
          (goto-char 200)
          (recenter 0)
          (>= (window-start w) 1)))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_in_two_windows_simultaneously() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 15 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (let ((w1 (selected-window))
        (w2 (split-window nil nil 'right)))
    (set-window-buffer w2 (current-buffer))
    (set-window-point w1 1)
    (set-window-point w2 15)
    (set-window-start w1 1)
    (set-window-start w2 15)
    (let ((p1 (window-point w1))
          (p2 (window-point w2)))
      (delete-window w2)
      (list p1 p2
            (/= p1 p2)
            (eq (window-buffer w1) (current-buffer)))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_dedicated_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 6 39)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w (selected-window)))
  (list (not (window-dedicated-p w))
        (set-window-dedicated-p w t)
        (window-dedicated-p w)
        (set-window-dedicated-p w nil)
        (not (window-dedicated-p w))))) "#,
        expect,
    );
}

#[test]
fn divergence_temp_buffer_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((orig-buf (current-buffer)))
  (with-temp-buffer
    (insert "temp content for display")
    (goto-char 1)
    (list (buffer-string)
          (= (point) 1)
          (not (eq (current-buffer) orig-buf))))
  (eq (current-buffer) orig-buf)) "#,
        expect,
    );
}

#[test]
fn divergence_window_margins_fringes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function window-left-margin)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((w (selected-window)))
  (list (>= (window-width w) 1)
        (>= (window-height w) 1)
        (>= (window-body-width w) 1)
        (>= (window-body-height w) 1)
        (window-left-margin w)
        (window-right-margin w)
        (window-left-fringe w)
        (window-right-fringe w)
        (>= (+ (window-left-fringe w) (window-right-fringe w)) 0)))) "#,
        expect,
    );
}
