/// Batch 489: display-buffer-alist, display-buffer-base-action, window-combine.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx489_display_buffer_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<window 1 on *cx489*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((display-buffer-alist '(("\\*cx489\\*" . (display-buffer-same-window)))))
  (let ((buf (get-buffer-create "*cx489*")))
    (display-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_display_buffer_action() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<window 1 on *cx489-action*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((display-buffer-base-action '(display-buffer-same-window)))
  (let ((buf (get-buffer-create "*cx489-action*")))
    (display-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_combination_resize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function set-window-combination-resize)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-combination-resize w t)
  (window-combination-resize w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_combination_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Combination limit is meaningful for internal windows only\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-combination-limit w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_splits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-splits)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-splits w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_use_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (integerp (window-use-time w)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_new_total() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-new-total w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_new_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-new-pixel w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (list (window-pixel-left w) (window-pixel-top w)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_resize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (window-resize (selected-window) 1)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_resize_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (window-resize-apply (selected-window))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_edges_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 0 80 23) (0 0 80 24))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (list (window-edges w t) (window-pixel-edges w)))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-absolute-pixel-edges w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_inside() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 80 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-inside-pixel-edges w))
"##,
        expect,
    );
}

#[test]
fn div_cx489_window_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (list (window-parameters w) (window-prev-buffers w)))
"##,
        expect,
    );
}
