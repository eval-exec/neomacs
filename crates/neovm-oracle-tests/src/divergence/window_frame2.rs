//! Window & frame divergence probes (batch 2): make-frame in batch + frame ops.
//!
//! Confirmed root cause: Neomacs ALLOWS `make-frame` under `--batch` (a frame
//! is created, frame-list grows), whereas GNU Emacs ERRORS ("Cannot make
//! frame"). Each make-frame-related scenario therefore diverges. Also adds
//! structural window tests reframed to compare behaviorally (counts, edges, eq)
//! rather than raw window-object printed ids.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_wf2_make_frame_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (let ((f (make-frame))) (list (frame-live-p f) (length (frame-list)))) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_make_frame_with_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (frame-parameter (make-frame '((name . "test"))) 'name) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_make_frame_then_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (let ((f (make-frame))) (delete-frame f) (length (frame-list))) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_select_frame_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let* ((orig (selected-frame))
           (f (make-frame))
           (sel (select-frame f)))
      (select-frame orig)
      (list (frame-live-p f) (frame-live-p sel)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_frame_root_window_of_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (windowp (frame-root-window (make-frame))) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_frame_selected_window_of_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (windowp (frame-selected-window (make-frame))) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_frame_first_window_of_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (windowp (frame-first-window (make-frame))) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_multiple_make_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (let ((f1 (make-frame)) (f2 (make-frame))) (length (frame-list))) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_make_frame_visible_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (frame-visible-p (make-frame)) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_make_frame_width_height_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (list (frame-parameter (make-frame '((width . 40))) 'width)
                          (frame-parameter (make-frame '((height . 12))) 'height))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_make_frame_fullscreen_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (frame-parameter (make-frame '((fullscreen . fullboth))) 'fullscreen) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_next_frame_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (list (frame-live-p f) (memq f (frame-list))))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_modify_frame_parameters_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (modify-frame-parameters f '((name . "renamed")))
      (frame-parameter f 'name))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_redesignate_display_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (list (eq (window-frame (frame-selected-window f)) f)
            (frame-parameter f 'name)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

// --- structural window tests (behavioral, no raw window-object ids) ---------

#[test]
fn div_wf2_split_window_count_growth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n0 (count-windows)))
  (split-window)
  (split-window nil nil 'right)
  (list n0 (count-windows) (- (count-windows) n0)))
"##,
        expect,
    );
}

#[test]
fn div_wf2_window_buffer_eq_after_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *wf2-eq*")))
  (set-window-buffer (selected-window) buf)
  (prog1 (eq (window-buffer (selected-window)) buf)
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_wf2_window_edges_after_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 0 40 24) (40 0 80 24))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((w (split-window nil nil 'right)))
      (list (window-edges (selected-window))
            (window-edges w)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_window_combined_p_after_splits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<window 5> nil #<window 1 on *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((wv (split-window nil nil 'below))
          (wh (split-window nil nil 'right)))
      (list (window-combined-p wv)
            (window-combined-p wv 'vertical)
            (window-combined-p wh 'horizontal)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf2_display_type_param_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (mono dark nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (frame-parameter nil 'display-type)
      (frame-parameter nil 'background-mode)
      (frame-parameter nil 'window-system))
"##,
        expect,
    );
}
