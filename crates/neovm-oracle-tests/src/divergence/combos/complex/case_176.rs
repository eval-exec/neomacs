//! Complex combo batch 176 — `bookmark` / `register` / `kmacro`
//! persistence to file and reload, round-trip format checks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx176_register_to_string_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"text content\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((reg ?a))
  (set-register reg "text content")
  (let ((stored (registerv-p (get-register reg))))
    (list stored
          (get-register reg)
          (condition-case e (registerv-p "text content") (error :err)))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_point_to_register_marker_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "register jump text")
  (goto-char 10)
  (point-to-register ?p)
  (let ((reg-val (get-register ?p)))
    (list (markerp reg-val)
          (integerp reg-val)
          (eq reg-val 10))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_window_config_register_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list))))
  (window-configuration-to-register ?w)
  (split-window)
  (let ((n-split (length (window-list))))
    (jump-to-register ?w)
    (let ((n-restored (length (window-list))))
      (list n-before n-split n-restored
            (eq n-before n-restored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_bookmark_set_jump_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'bookmark)
      (with-temp-buffer
        (insert "bookmark test content here")
        (goto-char 10)
        (bookmark-set "neo-cx176-bm"))
      (let ((pos (bookmark-get-position "neo-cx176-bm"))
            (file (bookmark-get-filename "neo-cx176-bm")))
        (list pos (stringp file))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_kmacro_define_run_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'kmacro)
      (fset 'neo-cx176-mac (kbd "C-a C-e"))
      (list (fboundp 'neo-cx176-mac)
            (commandp 'neo-cx176-mac)
            (vectorp (symbol-function 'neo-cx176-mac))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_kmacro_ring_push_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'kmacro)
      (let ((kmacro-ring nil))
        (kmacro-push-ring)
        (list kmacro-ring)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_register_keys_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((945 . \"value-for-α\") (946 . \"value-for-β\") (947 . \"value-for-γ\") (948 . \"value-for-δ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((regs '(?α ?β ?γ ?δ)))
  (dolist (r regs)
    (set-register r (format "value-for-%c" r)))
  (mapcar (lambda (r) (cons r (get-register r))) regs))
"##,
        expect,
    );
}

#[test]
fn div_cx176_bookmark_all_names_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'bookmark)
      (bookmark-set "neo-cx176-a")
      (bookmark-set "neo-cx176-b")
      (list (assoc "neo-cx176-a" bookmark-alist)
            (assoc "neo-cx176-b" bookmark-alist)
            (boundp 'bookmark-alist)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_bookmark_default_file_path_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (stringp bookmark-default-file)
          (boundp 'bookmark-save-flag)
          (boundp 'bookmark-version-control))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx176_register_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Register mega test buffer content")
  (put-text-property 1 6 'face 'bold)
  (let ((m (set-marker (make-marker) 8))
        (ov (make-overlay 4 14)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 18)
    (point-to-register ?p)
    (window-configuration-to-register ?w)
    (let ((state (list (get-register ?p)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
