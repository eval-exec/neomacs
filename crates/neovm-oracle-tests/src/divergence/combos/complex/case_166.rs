//! Complex combo batch 166 — `byte-compile` warnings / `byte-compile-log` /
//! `bytecomp-load-repurposed` / `with-no-warnings` / `with-suppressed-warnings`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx166_byte_compile_warnings_var_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable byte-compile-warnings)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'byte-compile-warnings)
      (consp byte-compile-warnings)
      (memq 'free-vars byte-compile-warnings)
      (memq 'unresolved byte-compile-warnings)
      (memq 'obsolete byte-compile-warnings))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_log_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'byte-compile-log)
      (fboundp 'byte-compile-log-warning)
      (fboundp 'byte-compile-warn)
      (boundp 'byte-compile-current-functions))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_simple_lambda_no_warnings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" byte-compile-error-on-warn)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (let ((lexical-binding t)) (lambda (x) (* x x))))
       (bc (let ((byte-compile-warnings nil)
                 (byte-compile-error-on-warn nil))
             (byte-compile lex))))
  (list (byte-code-function-p bc)
        (compiled-function-p bc)
        (funcall bc 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_with_no_warnings_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lex (lambda (x) (+ x 1))))
  (with-no-warnings
    (let ((bc (byte-compile lex)))
      (list (byte-code-function-p bc)
            (funcall bc 41)))))
"##,
        expect,
    );
}

#[test]
fn div_cx166_with_suppressed_warnings_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'with-suppressed-warnings)
          (fboundp 'with-no-warnings)
          (boundp 'byte-compile--suppressed-warnings))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_dest_file_extension_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((el (expand-file-name "neo-cx166-test.el" temporary-file-directory))
           (elc (byte-compile-dest-file el)))
      (list (stringp elc)
            (string-suffix-p ".elc" elc)
            (string-prefix-p (file-name-sans-extension el) elc)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_function_make_byte_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 257 \"��_�\" [2] 3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (lambda (x) (* x 2)))
       (bc (byte-compile lex)))
  (list (byte-code-function-p bc)
        (aref bc 0)
        (aref bc 1)
        (aref bc 2)
        (aref bc 3)
        (compiled-function-p bc)))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_error_on_warn_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable byte-compile-error-on-warn)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'byte-compile-error-on-warn)
      (booleanp byte-compile-error-on-warn)
      (boundp 'byte-compile-generate-call-tree))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_dynamic_flag_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'byte-compile-dynamic)
      (boundp 'byte-compile-dynamic-bound-vars)
      (boundp 'byte-compile-verbose)
      (boundp 'byte-optimize))
"##,
        expect,
    );
}

#[test]
fn div_cx166_native_comp_async_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'native-compile-async)
          (fboundp 'native-compile)
          (boundp 'native-comp-jit-compilation)
          (boundp 'native-comp-driver-options))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx166_byte_compile_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (let ((lexical-binding t)) (lambda (x) (* x x))))
       (bc (with-no-warnings (byte-compile lex))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Byte-compile mega: %S" (funcall bc 7)))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (byte-code-function-p bc)
                         (funcall bc 5)
                         (boundp 'byte-compile-warnings)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
