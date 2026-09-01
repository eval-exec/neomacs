//! Oracle parity tests for GNU file lock primitives.
//!
//! GNU implements `lock-file`, `unlock-file`, and `file-locked-p` in
//! `src/filelock.c`.  Observable behavior includes nil return values for
//! lock/unlock, `file-locked-p` returning t for a lock owned by this Emacs,
//! and `create-lockfiles` disabling lock-file creation while still returning
//! nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_lock_state_transitions_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-lock-" t))
       (file (expand-file-name "alpha.txt" dir))
       (missing (expand-file-name "missing.txt" dir)))
  (unwind-protect
      (progn
        (write-region "alpha" nil file nil 'silent)
        (list
         (file-locked-p file)
         (lock-file file)
         (file-locked-p file)
         (unlock-file file)
         (file-locked-p file)
         (let ((create-lockfiles nil))
           (list (lock-file file)
                 (file-locked-p file)
                 (unlock-file file)
                 (file-locked-p file)))
         (file-locked-p missing)
         (condition-case err
             (lock-file)
           (error (list (car err) (cdr err))))
         (condition-case err
             (lock-file 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (unlock-file 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-locked-p 42)
           (error (list (car err) (cdr err))))))
    (ignore-errors (unlock-file file))
    (delete-directory dir t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil t nil nil (nil nil nil nil) nil (wrong-number-of-arguments (lock-file 0)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
