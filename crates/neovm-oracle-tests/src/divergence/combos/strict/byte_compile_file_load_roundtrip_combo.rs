//! Strict combo oracle probes, batch 343: byte-compile-file + load-file deep
//! (shared tempdir). Compile a .el to .elc, load the compiled file, call the
//! compiled function, and verify load-history entry.
//! Uses assert_oracle_parity_with_shared_tempdir_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_byte_compile_file_load_call_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (src (expand-file-name "probe-bcf.el" dir))
       (elc (expand-file-name "probe-bcf.elc" dir)))
  (when (file-exists-p src) (delete-file src))
  (when (file-exists-p elc) (delete-file elc))
  (write-region "(defun probe-bcf-fn (x) (* x 42))\n(provide 'probe-bcf)" nil src)
  (byte-compile-file src)
  (prog1
      (list (file-exists-p elc)
            (load-file elc)
            (fboundp 'probe-bcf-fn)
            (funcall 'probe-bcf-fn 2)
            (featurep 'probe-bcf))
    (when (file-exists-p src) (delete-file src))
    (when (file-exists-p elc) (delete-file elc))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t 84 t)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_byte_compile_file_macro_defun_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (src (expand-file-name "probe-bcm.el" dir))
       (elc (expand-file-name "probe-bcm.elc" dir)))
  (when (file-exists-p src) (delete-file src))
  (when (file-exists-p elc) (delete-file elc))
  (write-region "(defmacro probe-bcm-mac (x) `(* ,x 10))\n(defun probe-bcm-use () (probe-bcm-mac 5))\n(provide 'probe-bcm)" nil src)
  (byte-compile-file src)
  (prog1
      (list (file-exists-p elc)
            (load-file elc)
            (fboundp 'probe-bcm-mac)
            (fboundp 'probe-bcm-use)
            (probe-bcm-use)
            (featurep 'probe-bcm))
    (when (file-exists-p src) (delete-file src))
    (when (file-exists-p elc) (delete-file elc))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t 50 t)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_load_file_eval_after_load_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (src (expand-file-name "probe-lf.el" dir)))
  (when (file-exists-p src) (delete-file src))
  (write-region "(defvar probe-lf-val 'loaded)\n(provide 'probe-lf)" nil src)
  (prog1
      (list (load src nil t)
            (boundp 'probe-lf-val)
            probe-lf-val
            (featurep 'probe-lf))
    (when (file-exists-p src) (delete-file src))))
"##;
    let expect = expect_test::expect![[r#""OK (t t loaded t)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}
