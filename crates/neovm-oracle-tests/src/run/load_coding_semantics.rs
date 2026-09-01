//! Oracle parity tests for source-file coding selection during `load`.
//!
//! GNU's `load-with-code-conversion` inserts source into a multibyte buffer,
//! where `find-auto-coding` gives an explicit `coding:` cookie precedence
//! before the Lisp reader sees any forms.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_load_decodes_iso_2022_source_from_first_line_cookie() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"日本語\" (26085 26412 35486))""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
         (file (expand-file-name "legacy-source.el" dir))
         (prefix
          (string-as-unibyte
           ";;; legacy-source.el --- fixture -*- coding: iso-2022-7bit; -*-
(setq neovm--legacy-source-value \""))
         (encoded-japanese
          (apply #'unibyte-string
                 '(27 36 66 70 124 75 92 56 108 27 40 66)))
         (suffix (string-as-unibyte "\")
"))
         (source (concat prefix encoded-japanese suffix)))
    (unwind-protect
        (progn
          (let ((coding-system-for-write 'no-conversion))
            (write-region source nil file nil 'silent))
          (makunbound 'neovm--legacy-source-value)
          ;; The oracle harness binds this to UTF-8 while it reads the test
          ;; form.  Ordinary package loading leaves it nil so the source
          ;; cookie can select the coding system.
          (let ((coding-system-for-read nil))
            (list (load file nil t t)
                  neovm--legacy-source-value
                  (append neovm--legacy-source-value nil))))
      (when (file-exists-p file)
        (delete-file file))))"#,
        expect,
    );
}

#[test]
fn oracle_load_auto_detects_iso_2022_source_without_a_valid_cookie() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"こん\" (12371 12435) iso-2022-7bit-unix)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
         (file (expand-file-name "auto-detected-legacy-source.el" dir))
         ;; Like navi2ch-multibbs.el, the first line closes an empty -*- block
         ;; and the second line's coding tag is therefore not a valid cookie.
         (prefix
          (string-as-unibyte
           ";;; auto-detected-legacy-source.el --- fixture -*-
;;; coding: iso-2022-7bit; -*-
(setq neovm--auto-detected-source-value \""))
         (encoded-japanese
          (apply #'unibyte-string
                 '(27 36 66 36 51 36 115 27 40 66)))
         (suffix (string-as-unibyte "\")
"))
         (source (concat prefix encoded-japanese suffix)))
    (unwind-protect
        (progn
          (let ((coding-system-for-write 'no-conversion))
            (write-region source nil file nil 'silent))
          (makunbound 'neovm--auto-detected-source-value)
          (let ((coding-system-for-read nil))
            (list (load file nil t t)
                  neovm--auto-detected-source-value
                  (append neovm--auto-detected-source-value nil)
                  last-coding-system-used)))
      (when (file-exists-p file)
        (delete-file file))
      (makunbound 'neovm--auto-detected-source-value)))"#,
        expect,
    );
}

#[test]
fn oracle_nested_source_load_restores_the_outer_reader() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (a-start b-start c-start d c-end b-end a-end) (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
         (files
          '(("neovm-reader-a.el"
             . "(provide 'neovm-reader-a)
(push 'a-start neovm--reader-trace)
(require 'neovm-reader-b)
(push 'a-end neovm--reader-trace)
")
            ("neovm-reader-b.el"
             . "(provide 'neovm-reader-b)
(push 'b-start neovm--reader-trace)
(require 'neovm-reader-c)
(push 'b-end neovm--reader-trace)
")
            ("neovm-reader-c.el"
             . "(provide 'neovm-reader-c)
(push 'c-start neovm--reader-trace)
(require 'neovm-reader-a)
(require 'neovm-reader-d)
(push 'c-end neovm--reader-trace)
")
            ("neovm-reader-d.el"
             . "(provide 'neovm-reader-d)
(push 'd neovm--reader-trace)
")))
         (paths
          (mapcar (lambda (entry)
                    (expand-file-name (car entry) dir))
                  files))
         (load-path (cons dir load-path))
         (load-suffixes '(".el")))
    (unwind-protect
        (progn
          (defvar neovm--reader-trace)
          (setq neovm--reader-trace nil)
          (mapc (lambda (entry)
                  (let ((coding-system-for-write 'utf-8-unix))
                    (write-region
                     (cdr entry) nil
                     (expand-file-name (car entry) dir)
                     nil 'silent)))
                files)
          (let ((coding-system-for-read nil))
            (list
             (load (car paths) nil t t)
             (nreverse neovm--reader-trace)
             (mapcar #'featurep
                     '(neovm-reader-a neovm-reader-b
                       neovm-reader-c neovm-reader-d)))))
      (mapc (lambda (file)
              (when (file-exists-p file)
                (delete-file file)))
            paths)
      (makunbound 'neovm--reader-trace)))"#,
        expect,
    );
}
