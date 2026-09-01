//! Oracle parity tests for GNU `rename-file` semantics.
//!
//! GNU implements `rename-file` in `src/fileio.c`.  It distinguishes
//! no-replace and overwrite modes, recognizes directory targets only when the
//! new name ends in a slash, and renames symbolic links as links.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_rename_file_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-rename-file-" t))
       (src (expand-file-name "src.txt" dir))
       (dst (expand-file-name "dst.txt" dir))
       (old (expand-file-name "old.txt" dir))
       (subdir (expand-file-name "sub" dir))
       (subfile nil)
       (sym (expand-file-name "sym.txt" dir))
       (symdst (expand-file-name "symdst.txt" dir)))
  (unwind-protect
      (progn
        (write-region "src" nil src nil 'silent)
        (write-region "dst" nil dst nil 'silent)
        (write-region "old" nil old nil 'silent)
        (make-directory subdir)
        (make-symbolic-link "old.txt" sym)
        (list
         (condition-case err
             (rename-file src dst)
           (error (list (car err) (cdr err))))
         (file-exists-p src)
         (with-temp-buffer
           (insert-file-contents dst)
           (buffer-string))
         (rename-file src dst t)
         (file-exists-p src)
         (with-temp-buffer
           (insert-file-contents dst)
           (buffer-string))
         (rename-file dst (file-name-as-directory subdir) t)
         (setq subfile (expand-file-name "dst.txt" subdir))
         (file-exists-p subfile)
         (with-temp-buffer
           (insert-file-contents subfile)
           (buffer-string))
         (condition-case err
             (rename-file subfile subdir t)
           (error (list (car err) (cdr err))))
         (rename-file sym symdst)
         (file-symlink-p symdst)
         (file-exists-p old)
         (file-exists-p sym)
         (condition-case err
             (rename-file)
           (error (list (car err) (cdr err))))
         (condition-case err
             (rename-file 42 dst)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-directory dir t))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 49 46)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
