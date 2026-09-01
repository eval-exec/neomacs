//! Oracle parity tests for GNU `executable-find`.
//!
//! GNU implements `executable-find` in `lisp/files.el` using `locate-file`,
//! `exec-path`, `exec-suffixes`, and an integer executable-access predicate for
//! local paths.  These tests keep the executable search path isolated so the
//! result does not depend on host-installed programs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_executable_find_exec_path_modes_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(let* ((root (make-temp-file "neomacs-oracle-executable-find-" t))
       (d1 (expand-file-name "d1" root))
       (d2 (expand-file-name "d2" root))
       (cmd1 (expand-file-name "cmd" d1))
       (cmd2 (expand-file-name "cmd" d2))
       (data (expand-file-name "data" d1)))
  (unwind-protect
      (progn
        (make-directory d1)
        (make-directory d2)
        (write-region "#!/bin/sh\nexit 0\n" nil cmd1 nil 'silent)
        (write-region "#!/bin/sh\nexit 0\n" nil cmd2 nil 'silent)
        (write-region "not executable\n" nil data nil 'silent)
        (set-file-modes cmd1 #o755)
        (set-file-modes cmd2 #o755)
        (set-file-modes data #o644)
        (let ((exec-path (list d2 d1))
              (default-directory (file-name-as-directory root))
              (rel (lambda (file)
                     (and file (file-relative-name file root)))))
          (list
           (funcall rel (executable-find "cmd"))
           (funcall rel (let ((exec-path (list d1 d2)))
                          (executable-find "cmd")))
           (executable-find "data")
           (executable-find "missing-neomacs-oracle-command")
           (funcall rel (executable-find cmd1))
           (condition-case err
               (executable-find)
             (error (list (car err) (cdr err))))
           (condition-case err
               (executable-find 42)
             (error (list (car err) (cdr err))))
           (condition-case err
               (let ((exec-path 42))
                 (executable-find "cmd"))
             (error (list (car err) (cdr err)))))))
    (delete-directory root t)))
"##;

    let expect = expect_test::expect![[
        r#""OK (\"d2/cmd\" \"d1/cmd\" nil nil \"d1/cmd\" (wrong-number-of-arguments ((1 . 2) 0)) (wrong-type-argument (stringp 42)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
