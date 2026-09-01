//! Complex combo batch 139 — `bytecomp` / `byte-compile` / `nativecomp` /
//! `byte-compile-warnings` / `loaddefs` generation, with edge cases on
//! warning categories.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx139_byte_compile_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'byte-compile)
      (fboundp 'byte-compile-file)
      (fboundp 'byte-recompile-directory)
      (boundp 'byte-compile-warnings)
      (boundp 'byte-compile-error-on-warn))
"##,
        expect,
    );
}

#[test]
fn div_cx139_native_comp_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'native-compile)
          (fboundp 'native-compile-async)
          (boundp 'native-comp-jit-compilation)
          (boundp 'native-comp-deferred-compilation-deny-list))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_native_comp_available_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'native-comp-available-p)
          (when (fboundp 'native-comp-available-p)
            (native-comp-available-p)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_byte_compile_lambda_to_byte_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 257 \"�\u{1}_�\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lex (let ((lexical-binding t)) (lambda (x) (* x x)))))
  (let ((bc (byte-compile lex)))
    (list (byte-code-function-p bc)
          (compiled-function-p bc)
          (aref bc 0)
          (aref bc 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_byte_compile_simple_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fn (defun neo-cx139-simple (x) "doc" (+ x 1))))
  (let ((bc (byte-compile (symbol-function 'neo-cx139-simple))))
    (list (byte-code-function-p bc)
          (funcall bc 41))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_byte_compile_warning_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable byte-compile-warnings)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((categories '(free-vars unresolved callargs redefine obsolete
                    noruntime cl-functions interactive make-local)))
  (mapcar (lambda (cat)
            (list cat (memq cat byte-compile-warnings)))
          categories))
"##,
        expect,
    );
}

#[test]
fn div_cx139_disassemble_byte_compiled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (let ((lexical-binding t)) (lambda (x y) (+ (* x x) (* y y)))))
       (bc (byte-compile lex)))
  (condition-case e
      (let ((disassembled (disassemble bc)))
        (list (consp disassembled)
              (stringp (car disassembled))))
    (error (list :errored (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_byte_compile_constant_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (let ((lexical-binding t)) (lambda () (+ 1 2 3))))
       (bc (byte-compile lex)))
  (list (byte-code-function-p bc)
        (funcall bc)))
"##,
        expect,
    );
}

#[test]
fn div_cx139_loaddefs_update_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'loaddefs-generate--parse-file)
          (fboundp 'update-file-autoloads)
          (boundp 'generated-autoload-file)
          (boundp 'autoload-computed-prefixes))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_compiled_function_p_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (lambda (x) x))
       (bc (byte-compile lex)))
  (list (compiled-function-p bc)
        (compiled-function-p lex)
        (byte-code-function-p bc)
        (byte-code-function-p lex)
        (subrp (symbol-function 'car))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_byte_optimize_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (2 4 6 8 10))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (let ((lexical-binding t)) (lambda (lst) (mapcar (lambda (x) (* x 2)) lst))))
       (bc (byte-compile lex)))
  (list (byte-code-function-p bc)
        (funcall bc '(1 2 3 4 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx139_byte_compile_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lex (let ((lexical-binding t)) (lambda (x) (* x x))))
       (bc (byte-compile lex)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Byte-comp result: %S" (funcall bc 7)))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (byte-code-function-p bc)
                         (funcall bc 7)
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
