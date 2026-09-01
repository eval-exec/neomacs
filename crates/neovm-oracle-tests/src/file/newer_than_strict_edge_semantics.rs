//! Oracle parity tests for GNU `file-newer-than-file-p` semantics.
//!
//! GNU implements this in `src/fileio.c`: missing FILE1 returns nil, missing
//! FILE2 returns t, and existing files compare their last modification times.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_newer_than_file_p_missing_and_timestamp_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-newer-" t))
       (old (expand-file-name "old" dir))
       (new (expand-file-name "new" dir))
       (old-link (expand-file-name "old-link" dir))
       (new-link (expand-file-name "new-link" dir))
       (missing (expand-file-name "missing" dir)))
  (unwind-protect
      (progn
        (write-region "old" nil old nil 'silent)
        (write-region "new" nil new nil 'silent)
        (set-file-times old 100)
        (set-file-times new 200)
        (make-symbolic-link "old" old-link)
        (make-symbolic-link "new" new-link)
        (list
         (file-newer-than-file-p new old)
         (file-newer-than-file-p old new)
         (file-newer-than-file-p old old)
         (file-newer-than-file-p missing old)
         (file-newer-than-file-p old missing)
         (file-newer-than-file-p missing missing)
         ;; GNU follows symlinks here, just like `stat`.
         (file-newer-than-file-p new-link old-link)
         (file-newer-than-file-p old-link new-link)
         (condition-case err
             (file-newer-than-file-p old)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-newer-than-file-p old 42)
           (error (list (car err) (cdr err))))))
    (ignore-errors (delete-file old-link))
    (ignore-errors (delete-file new-link))
    (ignore-errors (delete-file old))
    (ignore-errors (delete-file new))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil nil nil t nil t nil (wrong-number-of-arguments (file-newer-than-file-p 1)) (wrong-type-argument (stringp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
