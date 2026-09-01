/// Batch 528: window-state-get deep, window-state-put with various parameters.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx528_window_state_get_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((min-height . 4) (min-width . 10) (min-height-ignore . 2) (min-width-ignore . 2) (min-height-safe . 1) (min-width-safe . 2) (min-pixel-height . 4) (min-pixel-width . 10) (min-pixel-height-ignore . 2) (min-pixel-width-ignore . 2) (min-pixel-height-safe . 1) (min-pixel-width-safe . 2)) leaf (pixel-width . 80) (pixel-height . 24) (total-width . 80) (total-height . 24) (normal-height . 1.0) (normal-width . 1.0) (parameters (clone-of . #<window 1 on *scratch*>)) (buffer #<buffer *scratch*> (selected . t) (hscroll . 0) (fringes 0 0 nil nil) (margins nil) (scroll-bars nil 0 t nil 0 t nil) (vscroll . 0) (dedicated) (point . #<marker at 1 in *scratch*>) (start . #<marker at 1 in *scratch*>)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-state-get w))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_get_ignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((min-height . 4) (min-width . 10) (min-height-ignore . 2) (min-width-ignore . 2) (min-height-safe . 1) (min-width-safe . 2) (min-pixel-height . 4) (min-pixel-width . 10) (min-pixel-height-ignore . 2) (min-pixel-width-ignore . 2) (min-pixel-height-safe . 1) (min-pixel-width-safe . 2)) leaf (pixel-width . 80) (pixel-height . 24) (total-width . 80) (total-height . 24) (normal-height . 1.0) (normal-width . 1.0) (buffer \"*scratch*\" (selected . t) (hscroll . 0) (fringes 0 0 nil nil) (margins nil) (scroll-bars nil 0 t nil 0 t nil) (vscroll . 0) (dedicated) (point . 1) (start . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (window-state-get w t))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_put_safe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "content")
  (let ((state (window-state-get (selected-window))))
    (window-state-put state nil 'safe)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#<buffer *scratch*> #<buffer *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "buffer-content")
  (let ((state (window-state-get (selected-window))))
    (window-state-buffers state)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_put_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(fboundp 'window-state-put)
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_with_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (set-window-parameter w 'test-param 'test-value)
  (let ((state (window-state-get w)))
    (window-state-put state nil 'safe)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((state (window-state-get (selected-window) t)))
  (list (listp state) (> (length (flatten-tree state)) 10)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<buffer *scratch*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "roundtrip")
  (let ((w (selected-window))
        (buf (current-buffer)))
    (let ((state (window-state-get w t)))
      (window-state-put state w 'safe)
      (window-buffer w))))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_usable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-state-usable-state)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((state (window-state-get (selected-window))))
    (window-state-usable-state (selected-window) state)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_ignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-state-ignored-parameters)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(window-state-ignored-parameters)
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_swap_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "swapped")
  (window-swap-states nil nil))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_put_noignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "no-ignore")
  (let ((state (window-state-get (selected-window) t)))
    (window-state-put state nil 'safe)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((state (window-state-get (selected-window))))
    (delete-other-windows)
    (window-state-put state nil 'safe)))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_get_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((state (window-state-get (selected-window))))
    (eq (length state) 0))
"##,
        expect,
    );
}

#[test]
fn div_cx528_window_state_norecord() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "content")
  (let ((state (window-state-get (selected-window) t)))
    (window-state-put state nil 'norecord)))
"##,
        expect,
    );
}
