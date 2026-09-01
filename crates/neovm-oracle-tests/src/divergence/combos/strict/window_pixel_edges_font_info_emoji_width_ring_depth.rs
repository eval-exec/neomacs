//! Strict combo oracle probes, batch 120: window pixel geometry, font-info,
//! emoji/ZWJ string-width, ring operations, and depth/stack limits.
//! Uses assert_oracle_parity_expect with expect_test snapshots.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t4_window_pixel_geometry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 0 80 24) (0 0 80 23) 80 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((b (get-buffer-create " *probe-pixel*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (list (window-pixel-edges)
              (window-inside-pixel-edges)
              (window-pixel-width)
              (window-pixel-height)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"####,
        expect,
    );
}

#[test]
fn div_t4_font_info_metrics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (err wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(let ((font-name (face-attribute 'default :font nil 'default)))
  (condition-case err
      (let ((info (font-info font-name)))
        (list (vectorp info)
              (aref info 0)
              (aref info 3)
              (aref info 4)
              (> (aref info 3) 0)))
    (error (list 'err (car err)))))
"####,
        expect,
    );
}

#[test]
fn div_t4_emoji_zwj_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 1 0 2 2 4 6 4 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list (char-width 128077)
      (char-width 129309)
      (char-width 9879)
      (char-width 8205)
      (string-width "👍")
      (string-width "🤝")
      (string-width "a👍b")
      (string-width "👨‍👩‍👧")
      (string-width (string 97 128077 98))
      (char-width 8986))
"####,
        expect,
    );
}

#[test]
fn div_t4_ring_operations_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ring-insert+expand)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r####"
(let ((r (make-ring 5)))
  (ring-insert r 'a)
  (ring-insert r 'b)
  (ring-insert r 'c)
  (ring-insert r 'd)
  (ring-insert r 'e)
  (ring-insert r 'f)
  (list (ring-length r)
        (ring-size r)
        (ring-ref r 0)
        (ring-ref r 1)
        (ring-elements r)
        (ring-remove r 1)
        (ring-length r)
        (ring-ref r 0)
        (progn (ring-insert+expand r 'g) (ring-size r))))
"####,
        &["emacs-lisp/ring.el"],
        expect,
    );
}

#[test]
fn div_t4_depth_and_stack_limits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1600 2500 t t excessive-lisp-nesting)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(list max-lisp-eval-depth
      max-specpdl-size
      (> max-lisp-eval-depth 100)
      (> max-specpdl-size 100)
      (condition-case err
          (let ((max-lisp-eval-depth 50))
            (defun deep-rec (n) (if (<= n 0) 'done (deep-rec (1- n))))
            (deep-rec 100))
        (error (car err))))
"####,
        expect,
    );
}
