//! Complex combo batch 145 — `python` / `ruby` / `go` / `rust` / `js`
//! / `sh` / `c` / `css` / `html` major-mode parsing and font-lock.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx145_python_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'python)
      (list (fboundp 'python-mode)
            (boundp 'python-indent-offset)
            (boundp 'python-shell-interpreter)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_python_basic_buffer_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t font-lock-keyword-face font-lock-function-name-face)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "def hello():\n    return 42\n\nclass Foo:\n    pass\n")
      (python-mode)
      (font-lock-fontify-buffer)
      (list (eq major-mode 'python-mode)
            (get-text-property 1 'face)
            (get-text-property 5 'face)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_ruby_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ruby-mode)
      (list (fboundp 'ruby-mode)
            (boundp 'ruby-indent-level)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_go_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'go-mode)
          (featurep 'go-mode)
          (boundp 'gofmt-command))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_rust_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'rust-mode)
          (featurep 'rust-mode)
          (boundp 'rust-format-on-save))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_js_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'js)
      (list (fboundp 'js-mode)
            (boundp 'js-indent-level)
            (boundp 'js-enabled-frameworks)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_sh_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'sh-script)
      (list (fboundp 'sh-mode)
            (boundp 'sh-basic-offset)
            (boundp 'sh-indentation)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_c_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cc-mode)
      (list (fboundp 'c-mode)
            (fboundp 'c++-mode)
            (boundp 'c-basic-offset)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_html_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'sgml-mode)
      (list (fboundp 'html-mode)
            (boundp 'sgml-basic-offset)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_css_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'css-mode)
      (list (fboundp 'css-mode)
            (boundp 'css-indent-offset)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_yaml_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'yaml-mode)
          (featurep 'yaml-mode))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx145_python_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "def alpha():\n    return 'hello'\n\ndef beta(x):\n    return x * 2\n")
      (python-mode)
      (put-text-property 1 9 'face 'bold)
      (let ((m (set-marker (make-marker) 14))
            (ov (make-overlay 4 22)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 30)
        (let ((state (list (eq major-mode 'python-mode)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
