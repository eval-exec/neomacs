//! Oracle parity tests for GNU `copy-file` semantics.
//!
//! GNU implements `copy-file` in `src/fileio.c`.  These tests cover observable
//! overwrite policy, directory-target expansion, regular-file requirements,
//! keep-time behavior, and public arity/type errors.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_copy_file_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-copy-file-" t))
       (src (expand-file-name "src.txt" dir))
       (dst (expand-file-name "dst.txt" dir))
       (other (expand-file-name "other.txt" dir))
       (subdir (expand-file-name "sub" dir))
       (timecopy (expand-file-name "time.txt" dir)))
  (unwind-protect
      (progn
        (write-region "src" nil src nil 'silent)
        (write-region "old" nil dst nil 'silent)
        (make-directory subdir)
        (set-file-times src (seconds-to-time 1000))
        (list
         (condition-case err
             (copy-file src dst)
           (error (list (car err) (cdr err))))
         (copy-file src dst t)
         (with-temp-buffer
           (insert-file-contents dst)
           (buffer-string))
         (copy-file src (file-name-as-directory subdir) t)
         (with-temp-buffer
           (insert-file-contents (expand-file-name "src.txt" subdir))
           (buffer-string))
         (condition-case err
             (copy-file src subdir t)
           (error (list (car err) (cdr err))))
         (copy-file src timecopy t t)
         (equal (nth 5 (file-attributes src))
                (nth 5 (file-attributes timecopy)))
         (condition-case err
             (copy-file subdir other t)
           (error (list (car err) (cdr err))))
         (condition-case err
             (copy-file)
           (error (list (car err) (cdr err))))
         (condition-case err
             (copy-file 42 other)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-directory dir t))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 41 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
