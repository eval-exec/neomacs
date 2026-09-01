//! Window & frame divergence probes (batch 3): more make-frame-gated frame
//! operations. Because Neomacs permits `make-frame` under `--batch` and GNU
//! errors, every operation that creates then inspects/modifies a frame
//! diverges (GNU returns errored, Neomacs returns the value).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_wf3_make_frame_foreground_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((foreground-color . "red"))) 'foreground-color) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_background_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((background-color . "blue"))) 'background-color) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_minibuffer_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((minibuffer))) 'minibuffer) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_scroll_bars_param() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((vertical-scroll-bars . nil))) 'vertical-scroll-bars) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_menu_bar_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((menu-bar-lines . 2))) 'menu-bar-lines) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_cursor_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((cursor-color . "green"))) 'cursor-color) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case err (frame-parameter (make-frame '((internal-border-width . 3))) 'internal-border-width) (error (cons 'errored (car err))))"##,
        expect,
    );
}

#[test]
fn div_wf3_raise_frame_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (let ((f (make-frame))) (raise-frame f) (frame-live-p f)) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_iconify_frame_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (let ((f (make-frame))) (iconify-frame f) (frame-visible-p f)) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_make_frame_invisible_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (make-frame-invisible f)
      (let ((v1 (frame-visible-p f)))
        (make-frame-visible f)
        (list v1 (frame-visible-p f))))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_focus_frame_after_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err (let ((f (make-frame))) (focus-frame f) (frame-live-p f)) (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_frame_configuration_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (let ((cfg (current-frame-configuration)))
        (list (consp cfg) (length cfg))))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_frames_on_display_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (progn (make-frame) (make-frame) (length (frames-on-display-list)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_filtered_frame_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (progn (make-frame) (length (filtered-frame-list (lambda (f) (frame-live-p f)))))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_minibuffer_window_of_new_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (list (window-minibuffer-p (minibuffer-window f))
            (eq (minibuffer-window f) (minibuffer-window))))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_set_frame_selected_window_new_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let* ((f (make-frame))
           (w (frame-selected-window f)))
      (list (windowp w) (eq (window-frame w) f)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_frame_name_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f1 (make-frame)) (f2 (make-frame)))
      (list (frame-parameter f1 'name) (frame-parameter f2 'name)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_window_list_count_on_new_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((f (make-frame)))
      (length (window-list f 'nomini)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}

#[test]
fn div_wf3_select_window_on_new_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let* ((f (make-frame))
           (w (split-window (frame-root-window f) nil 'below)))
      (list (windowp w) (eq (window-frame w) f) (count-windows)))
  (error (cons 'errored (car err))))
"##,
        expect,
    );
}
